//! Her acting: every case in the shared conformance files
//! (`crates/conformance/v1/motion`) through the C entry point, as another
//! platform runs them. `myriad-merope` runs the same files through the
//! crate. The cases were recorded from the site backend's director before
//! its judgments moved here (2026-11-02).

use std::path::PathBuf;

use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "motion.asked",
    "motion.chat_prompt",
    "motion.direction",
    "motion.floor",
    "motion.lay_way",
    "motion.plan",
    "motion.standing_acting",
    "motion.standing_doing",
    "motion.standing_floor",
    "motion.way",
];

#[test]
fn every_motion_case_answers_through_the_c_entry_point() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/motion");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut calls = Vec::new();
    let mut cases = 0;
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
            cases += 1;
        }
    }
    assert_eq!(calls, TESTED);
    assert!(cases >= 300, "{cases}");
}

/// The director's answer as text goes in exactly as the plan came out.
#[test]
fn a_plan_goes_back_in_as_the_directors_answer() {
    let moment = json!({"phase": "delivery",
        "mood": {"before": 60.0, "after": 60.0, "bandBefore": "calm", "bandAfter": "calm",
            "delta": 0.0, "cause": "test", "revision": 3},
        "activity": "talking", "userText": "做一下狂笑", "responseText": "好啊，看我的！",
        "motionStyle": "even"});
    let answers = json!({"askedFor": {"choice": "maniac", "confidence": 0.9}});
    let plan = ok(
        "motion.plan",
        json!({"moment": moment, "answers": answers, "direct": true}),
    )["plan"]
        .clone();
    let out = ok(
        "motion.direction",
        json!({"moment": moment, "answer": plan.to_string(), "fallbackToLocal": false}),
    );
    assert_eq!(out["outcome"], "director");
    assert_eq!(out["directive"]["plan"]["cues"][0]["intent"], "maniac");
    assert_eq!(out["directive"]["moodRevision"], 3);
    let (status, _) = ffi(
        "motion.direction",
        &json!({"moment": moment, "fallbackToLocal": false, "userId": 1}),
    );
    assert_eq!(status, STATUS_BAD_INPUT, "an unknown field is refused");
}
