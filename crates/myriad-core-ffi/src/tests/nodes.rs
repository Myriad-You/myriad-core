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
    "nodes.file",
    "nodes.fold",
    "nodes.id",
    "nodes.place",
    "nodes.said",
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
                    assert!(!message.contains('它'), "{call}: {message}");
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
    assert!(cases >= 50, "{cases}");
}
