//! Her nodes: every case in the shared conformance files
//! (`crates/conformance/v1/nodes`) through the C entry point, as another
//! platform runs them. `myriad-nodes` runs the same files through the crate
//! and blesses them, so passing here is the C entry point answering byte
//! for byte as the crate does.

use std::path::PathBuf;

use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::ffi;
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "nodes.body",
    "nodes.fold",
    "nodes.id",
    "nodes.limits",
    "nodes.place_input",
    "nodes.place_read",
    "nodes.queue",
    "nodes.repo_key",
    "nodes.upkeep_input",
    "nodes.upkeep_read",
    "nodes.wire_send",
    "nodes.wire_take",
];

#[test]
fn every_node_case_answers_through_the_c_entry_point() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/nodes");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut calls = std::collections::BTreeSet::new();
    let mut cases = 0;
    for path in files {
        let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap();
        calls.insert(call.to_string());
        for case in file["cases"].as_array().unwrap() {
            let (status, out) = ffi(call, &case["input"]);
            let got = match status {
                STATUS_OK => out,
                STATUS_BAD_INPUT => {
                    // A refusal names a field or a rule, never her words.
                    let message = out["error"]["message"].as_str().unwrap_or("");
                    for word in ["sample-repo", "Users", "一段", "abc123"] {
                        assert!(!message.contains(word), "{call}: {message}");
                    }
                    json!({ "error": "bad_input" })
                }
                other => panic!("{call}: status {other}: {out}"),
            };
            assert_eq!(
                canonical_bytes(&got),
                canonical_bytes(&case["output"]),
                "{} › {}",
                path.display(),
                case["name"]
            );
            cases += 1;
        }
    }
    assert_eq!(calls.into_iter().collect::<Vec<_>>(), TESTED);
    assert!(cases >= 100, "{cases}");
}

#[test]
fn a_refused_row_is_named_by_its_field_never_by_its_value() {
    let row = json!({
        "audience": "personal:7", "author": "her", "dseq": 1, "epoch": 1,
        "hlc": "001760000000000-000000-dev_mac", "id": "n1", "kind": "node.created",
        "object": "node:25ae623559006437c416adbde717a853", "origin": "dev_mac",
        "payload": {"ref": "body:n1"}, "replicate": "shared", "version": 1,
    });
    let secret = "sensitive-synthetic-value";
    let mut origin = row.clone();
    origin["origin"] = json!(format!("{secret} x"));
    origin["hlc"] = json!(format!("001760000000000-000000-{secret} x"));
    let mut hlc_node = row.clone();
    hlc_node["hlc"] = json!(format!("001760000000000-000000-{secret}"));
    let mut subject = row.clone();
    subject["subject"] = json!(secret);
    let mut wrong_type = row.clone();
    wrong_type["object"] = json!(format!("mood:{secret}"));
    let mut cases = Vec::new();
    for (field, value) in [
        ("audience", json!(secret)),
        ("audience", json!(format!("personal:{secret}\u{7}"))),
        ("object", json!(secret)),
        ("kind", json!(secret)),
        ("id", json!(format!("{secret}\u{7}"))),
    ] {
        let mut bad = row.clone();
        bad[field] = value;
        cases.push(bad);
    }
    cases.extend([origin, hlc_node, subject, wrong_type]);
    let mut messages = Vec::new();
    for bad in cases {
        let (status, out) = ffi("nodes.fold", &json!({ "rows": [bad] }));
        let message = out["error"]["message"].as_str().unwrap_or("").to_string();
        assert_eq!(status, STATUS_BAD_INPUT, "{bad}: {out}");
        assert!(!message.contains("sensitive"), "{message}");
        messages.push(message);
    }
    assert_eq!(
        messages,
        [
            "a row: its audience is not personal:, own: or group:",
            "a row: its audience is not personal:, own: or group:",
            "a row: its object is not <type>:<id>",
            "a row: its kind is not one the log holds",
            "a row: its id is not a token",
            "input does not fit the call",
            "a row: its clock's node is not its origin",
            "a row: its subject is not of its audience",
            "a row: its kind is about another type of object",
        ]
    );
}

#[test]
fn every_repo_key_made_is_a_key_nodes_id_takes() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/nodes/repo_key.json");
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut made = 0;
    for case in file["cases"].as_array().unwrap() {
        let Some(key) = case["output"]["key"].as_str() else {
            continue;
        };
        let (status, out) = ffi("nodes.id", &json!({ "audience": "personal:7", "key": key }));
        assert_eq!(status, STATUS_OK, "{key}: {out}");
        let rest = key.strip_prefix("repo:").unwrap();
        let (status, out) = ffi("nodes.repo_key", &json!({ "remote": rest }));
        assert_eq!((status, out["key"].as_str()), (STATUS_OK, Some(key)));
        made += 1;
    }
    assert!(made >= 5, "{made}");
}

/// A batch `nodes.wire_send` makes is one `log.rows` frame: its limits are
/// the link contract's.
#[test]
fn a_wire_batch_fits_one_log_rows_frame() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shared/her_link_contract.json");
    let contract: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let frame = &contract["next"]["frames"]["log.rows"];
    assert_eq!(frame["maxItems"]["rows"], myriad_nodes::wire::BATCH_ROWS);
    assert_eq!(
        frame["maxItems"]["payloads"],
        myriad_nodes::wire::BATCH_PAYLOADS
    );
    let max = contract["next"]["limits"]["maxFrameBytes"]
        .as_u64()
        .unwrap() as usize;
    assert!(myriad_nodes::wire::BATCH_BYTES + (32 << 10) <= max);
}
