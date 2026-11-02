//! Handing work off through the C entry point: every case in the shared
//! conformance files (`crates/conformance/v1/handoff`), as another platform
//! runs them, and the same answers as the crate.
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-core-ffi handoff`
//! rewrites their `output` from the crate, only with a change that says why.

use std::path::PathBuf;

use myriad_merope::handoff::{self, CutOffBy, HandOff, Piece, Standing};
use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "handoff.act",
    "handoff.ended_line",
    "handoff.her_choice",
    "handoff.labelled",
    "handoff.line",
    "handoff.next_number",
    "handoff.route",
    "handoff.section",
    "handoff.shown",
    "handoff.split",
    "handoff.stands",
    "handoff.work_input",
];

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/handoff")
}

#[test]
fn every_hand_off_case_answers_through_the_c_entry_point() {
    let bless = std::env::var("MYRIAD_CONFORMANCE_BLESS").as_deref() == Ok("1");
    let mut calls = std::collections::BTreeSet::new();
    let mut cases = 0;
    let mut files: Vec<PathBuf> = std::fs::read_dir(conformance())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    for path in files {
        let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap().to_string();
        calls.insert(call.clone());
        for case in file["cases"].as_array_mut().unwrap() {
            let (status, out) = ffi(&call, &case["input"]);
            let got = match status {
                STATUS_OK => out,
                STATUS_BAD_INPUT => json!({ "error": "bad_input" }),
                other => panic!("{call}: status {other}: {out}"),
            };
            if bless {
                case["output"] = got;
            } else {
                assert_eq!(
                    canonical_bytes(&got),
                    canonical_bytes(&case["output"]),
                    "{} › {}",
                    path.display(),
                    case["name"]
                );
            }
            cases += 1;
        }
        if bless {
            let mut text = serde_json::to_string_pretty(&file).unwrap();
            text.push('\n');
            std::fs::write(&path, text).unwrap();
        }
    }
    assert_eq!(calls.into_iter().collect::<Vec<_>>(), TESTED);
    assert!(cases >= 70, "{cases}");
}

fn piece(number: u32, standing: Standing) -> Piece {
    Piece {
        number,
        asked: format!("第{number}件"),
        started_at: Some(60),
        standing,
    }
}

#[test]
fn hand_off_calls_answer_as_the_crate_does() {
    let reply = "好\n[[work: 查天气]]\n[[work #2: 是]]\n[[stop #3]]";
    let (spoken, hand_offs) = handoff::split(reply);
    assert_eq!(
        ok("handoff.split", json!({ "reply": reply })),
        json!({ "spoken": spoken, "handOffs": hand_offs })
    );

    let pieces = vec![
        piece(
            1,
            Standing::Doing {
                progress: Some(40),
                steps: None,
            },
        ),
        piece(
            2,
            Standing::Waiting {
                question: "要几点？".into(),
            },
        ),
        piece(
            3,
            Standing::CutOff {
                by: CutOffBy::Restart,
            },
        ),
    ];
    // Seconds in the crate, microseconds through the C entry point.
    let wire: Vec<Value> = pieces
        .iter()
        .map(|piece| {
            let mut value = serde_json::to_value(piece).unwrap();
            value["startedAt"] = json!(piece.started_at.unwrap() * 1_000_000);
            value
        })
        .collect();
    let now = 400;
    assert_eq!(
        ok(
            "handoff.line",
            json!({ "piece": wire[0], "now": now * 1_000_000 + 999_999 })
        ),
        json!({ "line": handoff::line(&pieces[0], now) })
    );
    let numbers: Vec<u32> = handoff::shown(pieces.clone(), now)
        .iter()
        .map(|piece| piece.number)
        .collect();
    assert_eq!(
        ok(
            "handoff.shown",
            json!({ "pieces": wire, "now": now * 1_000_000 })
        ),
        json!({ "numbers": numbers })
    );
    assert_eq!(
        ok("handoff.route", json!({ "pieces": wire })),
        json!({ "route": handoff::route(&pieces) })
    );
    for hand_off in &hand_offs {
        assert_eq!(
            ok(
                "handoff.act",
                json!({ "handOff": hand_off, "pieces": wire })
            ),
            json!({ "act": handoff::act(hand_off, &pieces) })
        );
    }
    let to_cut_off = HandOff::To {
        work: 3,
        text: "接着办".into(),
    };
    assert_eq!(
        ok(
            "handoff.act",
            json!({ "handOff": to_cut_off, "pieces": wire })
        )["act"]["act"],
        "pickUp"
    );
}

#[test]
fn a_wrong_shape_is_bad_input() {
    for (call, input) in [
        ("handoff.split", json!({ "reply": "x", "extra": 1 })),
        ("handoff.stands", json!({ "stoped": true })),
        (
            "handoff.section",
            json!({ "host": "phone", "lines": [], "waiting": 0 }),
        ),
        (
            "handoff.act",
            json!({ "handOff": { "kind": "new" }, "pieces": [] }),
        ),
    ] {
        assert_eq!(ffi(call, &input).0, STATUS_BAD_INPUT, "{call}");
    }
}
