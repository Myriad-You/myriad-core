//! What a look at the owner's screen may become of hers, through the C
//! entry point: every case in the shared conformance files
//! (`crates/conformance/v1/screen`).
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-core-ffi screen`
//! writes the outputs again after a deliberate change.

use std::path::PathBuf;

use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::ffi;
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &["screen.fact", "screen.notes", "screen.unquoted"];

#[test]
fn every_screen_case_answers_through_the_c_entry_point() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/screen");
    let bless = std::env::var_os("MYRIAD_CONFORMANCE_BLESS").is_some();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut calls = Vec::new();
    for path in files {
        let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap().to_string();
        for case in file["cases"].as_array_mut().unwrap() {
            let (status, out) = ffi(&call, &case["input"]);
            let got = match status {
                STATUS_OK => out,
                STATUS_BAD_INPUT => json!({ "error": "bad_input" }),
                other => panic!("{call}: status {other}: {out}"),
            };
            if bless {
                case["output"] = got;
                continue;
            }
            assert_eq!(
                canonical_bytes(&got),
                canonical_bytes(&case["output"]),
                "{} › {}",
                path.display(),
                case["name"]
            );
        }
        if bless {
            let mut written = serde_json::to_string_pretty(&file).unwrap();
            written.push('\n');
            std::fs::write(&path, written).unwrap();
        }
        calls.push(call);
    }
    calls.sort();
    assert_eq!(calls, TESTED);
}
