//! Whether she is the same on every end: every case in the shared
//! conformance files (`crates/conformance/v1/consistency`) through the C
//! entry point, as another platform runs them, and a round trip against the
//! crate. `myriad-merope` runs the same files through the crate.

use std::path::PathBuf;

use myriad_merope::consistency::{self, Turn};
use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "consistency.apart",
    "consistency.measure",
    "consistency.measure_runs",
    "consistency.prompt_diff",
    "consistency.prompt_segments",
];

#[test]
fn every_consistency_case_answers_through_the_c_entry_point() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/consistency");
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

#[test]
fn measures_match_the_crate() {
    let turns: Vec<Turn> = (0..24)
        .map(|i| Turn {
            said: vec![format!("回来啦{i}\n今天好冷？"), "嗯嗯".to_string()],
            answering: Some("在吗".to_string()),
            opens: i % 3 == 0,
        })
        .collect();
    let crate_measures = consistency::measure(&turns);
    let out = ok("consistency.measure", json!({ "turns": turns }));
    assert_eq!(out["measures"], json!(crate_measures));
    let mut other = crate_measures.clone();
    other.ai_flavored = Some(0.5);
    assert_eq!(
        ok(
            "consistency.apart",
            json!({ "left": crate_measures, "right": other })
        )["apart"],
        json!(consistency::apart(&crate_measures, &other))
    );
}
