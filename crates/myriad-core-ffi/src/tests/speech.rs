//! What she means to say through the C entry point: every case in the
//! shared conformance files (`crates/conformance/v1/speech`), as another
//! platform runs them. `myriad-merope`'s `speech_conformance` test runs the
//! same files through the crate (and blesses them); here the C entry point
//! must answer byte for byte the same.

use std::path::PathBuf;

use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "speech.admit",
    "speech.choice_parse",
    "speech.choice_prompt",
    "speech.markers",
    "speech.strip_markers",
    "speech.texts",
    "speech.told",
];

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/speech")
}

#[test]
fn every_speech_case_answers_through_the_c_entry_point() {
    let mut calls = std::collections::BTreeSet::new();
    let mut cases = 0;
    let mut files: Vec<PathBuf> = std::fs::read_dir(conformance())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    for path in files {
        let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap().to_string();
        calls.insert(call.clone());
        for case in file["cases"].as_array().unwrap() {
            let (status, out) = ffi(&call, &case["input"]);
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
            cases += 1;
        }
    }
    assert_eq!(calls.into_iter().collect::<Vec<_>>(), TESTED);
    assert!(cases >= 60, "{cases}");
}

#[test]
fn a_wish_with_a_fixed_offset_reads_her_night_the_same() {
    let wish = json!({
        "to": {"venue": "private", "members": ["1"]},
        "source": "reach",
        "seed": "想他了",
        "seedFrom": {"venue": "own"},
        "at": 1_793_685_600_000_000_i64,
    });
    let late = 1_793_685_600_000_000_i64 + 12 * 3_600_000_000 + 40 * 60_000_000;
    let named = ok(
        "speech.admit",
        json!({"wish": wish, "now": late, "zone": "Asia/Shanghai"}),
    );
    let fixed = ok(
        "speech.admit",
        json!({"wish": wish, "now": late, "zone": "+08:00"}),
    );
    assert_eq!(named, fixed);
    assert_eq!(named["outcome"], "ask");
    let (status, _) = ffi(
        "speech.admit",
        &json!({"wish": wish, "now": late, "zone": "Asia/Shanghai", "extra": 1}),
    );
    assert_eq!(status, STATUS_BAD_INPUT);
}
