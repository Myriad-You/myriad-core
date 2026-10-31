//! The partner's one id: every case in the shared conformance files
//! (`crates/conformance/v1/partner`) through the C entry point, as another
//! platform runs them. `myriad-nodes` runs the same files through the
//! crates and blesses them.

use std::path::PathBuf;

use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::ffi;
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "partner.file",
    "partner.link",
    "partner.migrate",
    "partner.mint",
];

#[test]
fn every_partner_case_answers_through_the_c_entry_point() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/partner");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut calls = Vec::new();
    for path in files {
        let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap();
        calls.push(call.to_string());
        for case in file["cases"].as_array().unwrap() {
            let (status, out) = ffi(call, &case["input"]);
            let got = match status {
                STATUS_OK => out,
                STATUS_BAD_INPUT => json!({ "error": "bad_input" }),
                other => panic!("{call}: status {other}: {out}"),
            };
            assert_eq!(
                canonical_bytes(&got),
                canonical_bytes(&case["output"]),
                "{} › {}",
                path.display(),
                case["name"]
            );
        }
    }
    assert_eq!(calls, TESTED);
}

/// The export header checks a partner id itself (`myriad-persona-sync`
/// cannot call `myriad-persona-log`); both must read the same ids as
/// partner ids.
#[test]
fn the_export_header_reads_partner_ids_as_the_log_does() {
    use myriad_persona_log::partner;
    use myriad_persona_sync::archive::{
        ArchiveHeader, HeaderFields, NONCE_PREFIX_BYTES, SALT_BYTES,
    };
    let minted = partner::mint(1_760_227_790_848, &[0x33; partner::RANDOM_BYTES]).unwrap();
    let ids = [
        minted.as_str(),
        "partner_0199d5c0a00022222222222222222222",
        "local-owner",
        "partner_",
        "partner_0199D5C0A00022222222222222222222",
        "partner_0199d5c0a0002222222222222222222",
        "partner_0199d5c0a000222222222222222222222",
        "partner_00000000000022222222222222222222",
        "partner_01941f297c00ffffffffffffffffffff",
        "partner_01941f297bffffffffffffffffffffff",
        "acct_7",
    ];
    for id in ids {
        let header = ArchiveHeader::new(
            &[0x11; SALT_BYTES],
            &[0x22; NONCE_PREFIX_BYTES],
            HeaderFields {
                lineage: None,
                epoch: None,
                origin: "dev_mac".into(),
                created_at: 1_760_000_000_000_000,
                digest: "0".repeat(64),
                logs: true,
                home: false,
                partner: Some(id.into()),
            },
        );
        assert_eq!(header.is_ok(), partner::check(id).is_ok(), "{id}");
    }
}
