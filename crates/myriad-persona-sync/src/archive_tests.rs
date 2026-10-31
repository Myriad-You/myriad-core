//! The export file's frame: what opens and what does not.

use super::*;
use crate::tests::{SAMPLE_PASSPHRASE, archive_sample};

fn sealed() -> (ArchiveHeader, usize, Vec<u8>) {
    let file = archive_sample().seal();
    let (header, start) = read_header(&file).unwrap();
    (header, start, file)
}

fn refused(file: &[u8]) -> String {
    open(SAMPLE_PASSPHRASE, file).unwrap_err().to_string()
}

#[test]
fn the_sample_spans_several_chunks_and_opens() {
    let (header, start, file) = sealed();
    let chunk = header.chunk_bytes as usize + TAG_BYTES;
    assert!(file.len() - start > 2 * chunk, "{}", file.len());
    let opened = open(SAMPLE_PASSPHRASE, &file).unwrap();
    assert_eq!(opened.manifest.digest, header.digest);
    assert_eq!(opened.manifest.kinds["memory"].count, 1);
    assert_eq!(opened.manifest.embeddings.count, 1);
    assert_eq!(opened.manifest.assets.count, 2);
}

#[test]
fn a_wrong_passphrase_opens_nothing_and_says_nothing_of_it() {
    let (_, _, file) = sealed();
    let error = open("雨天的伞 umbrella 8", &file).unwrap_err().to_string();
    assert_eq!(error, DOES_NOT_OPEN);
    assert!(!error.contains("umbrella"));
    assert!(open("", &file).is_err());
}

#[test]
fn the_same_words_in_another_normal_form_are_the_same_passphrase() {
    let kdf = archive_sample().header.kdf;
    let composed = derive_key("caf\u{e9}", &kdf).unwrap();
    let decomposed = derive_key("cafe\u{301}", &kdf).unwrap();
    assert_eq!(composed.as_bytes(), decomposed.as_bytes());
    assert_ne!(
        derive_key("cafe", &kdf).unwrap().as_bytes(),
        composed.as_bytes()
    );
    assert_eq!(format!("{composed:?}"), "ArchiveKey(..)");
}

#[test]
fn a_file_cut_short_anywhere_is_refused() {
    let (header, start, file) = sealed();
    let chunk = header.chunk_bytes as usize + TAG_BYTES;
    // At a chunk's edge: the last chunk left is not sealed as the last.
    assert_eq!(refused(&file[..start + chunk]), DOES_NOT_OPEN);
    assert_eq!(refused(&file[..start + 2 * chunk]), DOES_NOT_OPEN);
    // The last chunk cut, or dropped whole.
    assert_eq!(refused(&file[..file.len() - 1]), DOES_NOT_OPEN);
    let last = (file.len() - start) % chunk;
    assert_eq!(refused(&file[..file.len() - last]), DOES_NOT_OPEN);
    // Inside the header line.
    assert!(open(SAMPLE_PASSPHRASE, &file[..start - 1]).is_err());
    // Nothing after the header.
    assert_eq!(refused(&file[..start]), DOES_NOT_OPEN);
}

#[test]
fn moved_changed_or_added_bytes_are_refused() {
    let (header, start, file) = sealed();
    let chunk = header.chunk_bytes as usize + TAG_BYTES;
    let mut swapped = file[..start].to_vec();
    swapped.extend_from_slice(&file[start + chunk..start + 2 * chunk]);
    swapped.extend_from_slice(&file[start..start + chunk]);
    swapped.extend_from_slice(&file[start + 2 * chunk..]);
    assert_eq!(refused(&swapped), DOES_NOT_OPEN);
    let mut flipped = file.clone();
    flipped[start + 5] ^= 1;
    assert_eq!(refused(&flipped), DOES_NOT_OPEN);
    let mut longer = file.clone();
    longer.push(0);
    assert_eq!(refused(&longer), DOES_NOT_OPEN);
    // A header changed in a way that still parses: another epoch.
    let mut other = header.clone();
    other.epoch = Some(4);
    let mut rewritten = other.line().unwrap();
    rewritten.push(b'\n');
    rewritten.extend_from_slice(&file[start..]);
    assert_eq!(refused(&rewritten), DOES_NOT_OPEN);
    // A header not in its canonical form.
    let mut spaced = b"{ ".to_vec();
    spaced.extend_from_slice(&file[1..]);
    assert!(open(SAMPLE_PASSPHRASE, &spaced).is_err());
}

#[test]
fn costs_outside_the_bounds_are_refused_before_any_work() {
    let mut kdf = Kdf::new(&[1; SALT_BYTES]);
    assert!(kdf.check().is_ok());
    kdf.mem_kib = MAX_MEM_KIB + 1;
    assert!(derive_key("x", &kdf).is_err());
    kdf.mem_kib = MIN_MEM_KIB - 1;
    assert!(derive_key("x", &kdf).is_err());
    kdf.mem_kib = DEFAULT_MEM_KIB;
    kdf.iterations = 1;
    assert!(kdf.check().is_err());
    kdf.iterations = DEFAULT_ITERATIONS;
    kdf.salt = "00".into();
    assert!(kdf.check().is_err());
    let mut header = archive_sample().header;
    header.chunk_bytes = MAX_CHUNK_BYTES + 1;
    assert!(header.check().is_err());
}

#[test]
fn a_chunk_seals_its_whole_size_except_the_last() {
    let sample = archive_sample();
    let key = derive_key(sample.passphrase, &sample.header.kdf).unwrap();
    let size = sample.header.chunk_bytes as usize;
    assert!(seal_chunk(&key, &sample.header, 0, false, &vec![0; size - 1]).is_err());
    assert!(seal_chunk(&key, &sample.header, 0, true, &vec![0; size + 1]).is_err());
    let last = seal_chunk(&key, &sample.header, 7, true, b"").unwrap();
    assert_eq!(last.len(), TAG_BYTES);
    assert_eq!(
        open_chunk(&key, &sample.header, 7, true, &last).unwrap(),
        b""
    );
    assert!(open_chunk(&key, &sample.header, 7, false, &last).is_err());
    assert!(open_chunk(&key, &sample.header, 6, true, &last).is_err());
}

#[test]
fn parts_are_checked_against_the_manifest_and_their_files() {
    let sample = archive_sample();
    let key = || derive_key(sample.passphrase, &sample.header.kdf).unwrap();
    let reseal = |parts: Vec<(Part, Vec<u8>)>| {
        let mut sealer = Sealer::new(key(), sample.header.clone()).unwrap();
        let mut out = Vec::new();
        for (part, payload) in &parts {
            out.extend(sealer.push(&part.head().unwrap()).unwrap());
            out.extend(sealer.push(payload).unwrap());
        }
        // Past the sealer's own checks: a writer that lies.
        let mut hack = sealer;
        hack.ended = true;
        out.extend(hack.finish().unwrap());
        out
    };
    // A picture whose bytes are not its id.
    let mut parts = sample.parts.clone();
    parts[1].1 = b"sticker-pixelz".to_vec();
    assert!(refused(&reseal(parts)).contains("picture"));
    // A rig without its atlas.
    let mut parts = sample.parts.clone();
    parts.remove(3);
    assert!(refused(&reseal(parts)).contains("rig"));
    // A page dropped: the manifest no longer holds.
    let mut parts = sample.parts.clone();
    parts.remove(0);
    assert!(open(SAMPLE_PASSPHRASE, &reseal(parts)).is_err());
    // No end.
    let mut parts = sample.parts.clone();
    parts.pop();
    assert!(refused(&reseal(parts)).contains("end"));
    // Something after the end.
    let mut parts = sample.parts.clone();
    parts.push(parts[1].clone());
    assert!(refused(&reseal(parts)).contains("follows the end"));
    // The sealer itself will not end on another manifest than the header's.
    let mut sealer = Sealer::new(key(), sample.header.clone()).unwrap();
    let mut wrong = match &sample.parts[4].0 {
        Part::End { manifest } => manifest.clone(),
        _ => unreachable!(),
    };
    wrong.digest = "0".repeat(64);
    assert!(sealer.part(&Part::End { manifest: wrong }, b"").is_err());
}

#[test]
fn the_manifest_does_not_depend_on_the_order_her_pages_came_in() {
    let opened = open(SAMPLE_PASSPHRASE, &archive_sample().seal()).unwrap();
    let envelope = &opened.envelopes[0];
    let mut split: Vec<Envelope> = envelope
        .entities
        .iter()
        .rev()
        .map(|entity| Envelope {
            entities: vec![entity.clone()],
            embeddings: Vec::new(),
            assets: Vec::new(),
            ..envelope.clone()
        })
        .collect();
    split[0].embeddings = envelope.embeddings.clone();
    split[1].assets = envelope.assets.iter().rev().cloned().collect();
    split[2].assets = envelope.assets.clone();
    assert_eq!(Manifest::of_envelopes(&split).unwrap(), opened.manifest);
    let mut changed = envelope.clone();
    changed.entities[0].rev = "001760000000124-000000-site".parse().unwrap();
    assert_ne!(
        Manifest::of_envelopes(&[changed]).unwrap().kinds["diary"],
        opened.manifest.kinds["diary"]
    );
}

#[test]
fn streaming_in_any_window_sizes_opens_the_same() {
    let sample = archive_sample();
    let (header, start, file) = sealed();
    for window in [1usize, 7, 1040, 4096] {
        let key = derive_key(sample.passphrase, &header.kdf).unwrap();
        let mut opener = Opener::new(key, header.clone()).unwrap();
        let mut reader = PartReader::new();
        let mut parts = Vec::new();
        for piece in file[start..].chunks(window) {
            parts.extend(reader.push(&opener.push(piece).unwrap()).unwrap());
        }
        parts.extend(reader.push(&opener.finish().unwrap()).unwrap());
        reader.finish().unwrap();
        assert_eq!(parts, sample.parts, "window {window}");
    }
}

/// One audience's page of her log, as `nodes.wire_send` makes a batch.
fn log_page(ids: &[&str]) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = ids
        .iter()
        .enumerate()
        .map(|(at, id)| {
            serde_json::json!({
                "version": 1, "id": id, "origin": "dev_mac", "author": "her", "epoch": 1,
                "dseq": at + 1, "hlc": format!("{:015}-000000-dev_mac", 1_760_000_000_000u64 + at as u64),
                "audience": "personal:7", "object": "node:a", "kind": "node.span",
                "payload": { "ref": format!("doc:{id}") }, "replicate": "shared",
            })
        })
        .collect();
    serde_json::json!({
        "audience": "personal:7",
        "rows": rows,
        "payloads": [{ "ref": format!("doc:{}", ids[0]), "data": "e30=" }],
    })
}

/// The sample with log pages after its envelope page, sealed as a
/// version 2 file (or, with `logs` false, wrongly as version 1).
fn with_logs(pages: &[serde_json::Value], logs: bool) -> Result<Vec<u8>, crate::SyncError> {
    let mut sample = archive_sample();
    let envelopes: Vec<Envelope> = sample
        .parts
        .iter()
        .filter(|(part, _)| matches!(part, Part::Envelope { .. }))
        .map(|(_, bytes)| serde_json::from_slice(bytes).unwrap())
        .collect();
    let manifest = Manifest::of_envelopes(&envelopes)
        .unwrap()
        .with_logs(pages, |_| true)
        .unwrap();
    let mut header = ArchiveHeader::new(
        &[0x11; SALT_BYTES],
        &[0x22; NONCE_PREFIX_BYTES],
        HeaderFields {
            lineage: sample.header.lineage.clone(),
            epoch: sample.header.epoch,
            origin: sample.header.origin.clone(),
            created_at: sample.header.created_at,
            digest: manifest.digest.clone(),
            logs,
            partner: None,
        },
    )
    .unwrap();
    header.chunk_bytes = 1024;
    sample.header = header;
    for (at, page) in pages.iter().enumerate() {
        let bytes = crate::canonical::canonical_bytes(page);
        sample.parts.insert(
            1 + at,
            (
                Part::Log {
                    bytes: bytes.len() as u64,
                },
                bytes,
            ),
        );
    }
    let end = sample.parts.len() - 1;
    sample.parts[end] = (Part::End { manifest }, Vec::new());
    sample.try_seal()
}

/// The sample's envelopes, for a manifest.
fn envelopes() -> Vec<Envelope> {
    archive_sample()
        .parts
        .iter()
        .filter(|(part, _)| matches!(part, Part::Envelope { .. }))
        .map(|(_, bytes)| serde_json::from_slice(bytes).unwrap())
        .collect()
}

#[test]
fn a_file_with_log_pages_is_version_2_and_opens_them() {
    let pages = [log_page(&["r1", "r2"]), log_page(&["r3"])];
    let file = with_logs(&pages, true).unwrap();
    let opened = open(SAMPLE_PASSPHRASE, &file).unwrap();
    assert_eq!(opened.header.version, ARCHIVE_LOG_VERSION);
    assert_eq!(opened.logs, pages);
    assert_eq!(opened.manifest.log.as_ref().unwrap().count, 3);
    // The same rows in other pages, in another order: the same manifest.
    let other = [log_page(&["r3"]), log_page(&["r1", "r2"])];
    let reread = open(SAMPLE_PASSPHRASE, &with_logs(&other, true).unwrap()).unwrap();
    assert_eq!(reread.manifest, opened.manifest);
    // A file with no log pages is still version 1 and its manifest unchanged.
    let (header, _, plain) = sealed();
    assert_eq!(header.version, ARCHIVE_VERSION);
    assert!(
        open(SAMPLE_PASSPHRASE, &plain)
            .unwrap()
            .manifest
            .log
            .is_none()
    );
}

#[test]
fn a_log_page_is_refused_in_a_version_1_file_or_with_a_row_twice() {
    // Not written: the sealer refuses it before a byte goes out (and a
    // reader refuses such a file all the same).
    let error = with_logs(&[log_page(&["r1"])], false).unwrap_err();
    assert!(error.to_string().contains("version 2"), "{error}");
    let twice = [log_page(&["r1"]), log_page(&["r1"])];
    assert!(
        Manifest::of_envelopes(&envelopes())
            .unwrap()
            .with_logs(&twice, |_| true)
            .is_err()
    );
}

#[test]
fn a_log_page_with_a_stray_payload_or_of_an_audience_its_reader_may_not_hold_is_refused() {
    let manifest = || Manifest::of_envelopes(&envelopes()).unwrap();
    let mut stray = log_page(&["r1"]);
    stray["payloads"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "ref": "doc:nobody", "data": "e30=" }));
    let error = manifest().with_logs(&[stray], |_| true).unwrap_err();
    assert!(
        error.to_string().contains("no row of it points at"),
        "{error}"
    );
    // A file for a reader who holds only her own audience.
    let error = manifest()
        .with_logs(&[log_page(&["r1"])], |audience| audience == "own:")
        .unwrap_err();
    assert!(error.to_string().contains("may not hold"), "{error}");
    assert!(
        manifest()
            .with_logs(&[log_page(&["r1"])], |audience| audience == "personal:7")
            .is_ok()
    );
}

#[test]
fn only_a_file_with_log_pages_names_the_partner() {
    let fields = |logs: bool| HeaderFields {
        lineage: None,
        epoch: None,
        origin: "dev_mac".into(),
        created_at: 1_760_000_000_000_000,
        digest: "0".repeat(64),
        logs,
        partner: Some("partner_0199d5c0900033333333333333333333".into()),
    };
    let salt = [0x11; SALT_BYTES];
    let nonce = [0x22; NONCE_PREFIX_BYTES];
    let header = ArchiveHeader::new(&salt, &nonce, fields(true)).unwrap();
    let line = String::from_utf8(header.line().unwrap()).unwrap();
    assert!(
        line.contains("\"partner\":\"partner_0199d5c0900033333333333333333333\""),
        "{line}"
    );
    assert!(ArchiveHeader::new(&salt, &nonce, fields(false)).is_err());
    let mut legacy = fields(true);
    legacy.partner = Some("local-owner".into());
    assert!(ArchiveHeader::new(&salt, &nonce, legacy).is_err());
    // Without one, a header is the bytes it always was.
    let mut plain = fields(true);
    plain.partner = None;
    let line = ArchiveHeader::new(&salt, &nonce, plain)
        .unwrap()
        .line()
        .unwrap();
    assert!(!String::from_utf8(line).unwrap().contains("partner"));
}
