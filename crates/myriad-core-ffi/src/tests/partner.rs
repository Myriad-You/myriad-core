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
