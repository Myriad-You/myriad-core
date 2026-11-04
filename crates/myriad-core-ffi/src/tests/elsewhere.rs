//! Her present across people through the C entry point: every case in the
//! shared conformance files (`crates/conformance/v1/elsewhere`), and the
//! calls said again through the crate.
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-core-ffi elsewhere`
//! writes the outputs again after a deliberate change.

use std::path::PathBuf;

use chrono::{DateTime, Duration, Utc};
use myriad_merope::elsewhere::{self, Cause, Facing, Heard, Moved, Place, Push};
use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok, text};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "elsewhere.own_feeling",
    "elsewhere.own_feeling_is_low",
    "elsewhere.own_feeling_words",
    "elsewhere.since_up_section",
];

#[test]
fn every_elsewhere_case_answers_through_the_c_entry_point() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/elsewhere");
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
    assert_eq!(calls, TESTED);
}

/// What the site tells (`speaking_context::since_she_got_up`) is what the
/// call tells for the same facts, people keyed by the end's own ids.
#[test]
fn the_section_is_the_crates_for_the_same_facts() {
    let up = DateTime::<Utc>::from_timestamp(1_793_750_400, 0).unwrap();
    let now = up + Duration::hours(10);
    let micros = |at: DateTime<Utc>| at.timestamp_micros();
    let talked = now - Duration::hours(2);
    let got = text(
        "elsewhere.since_up_section",
        json!({
            "heard": [
                { "person": "b", "place": { "kind": "group", "venue": "qq:1" }, "at": micros(talked) },
                { "person": "owner", "place": { "kind": "private" }, "at": micros(talked) },
            ],
            "moved": [{ "person": "b", "at": micros(talked), "cause": "user_scold", "feltAfter": -30.0 }],
            "since": micros(up),
            "facing": { "person": "owner" },
            "now": micros(now),
        }),
    );
    let heard = [
        Heard {
            person: 2,
            place: Place::Group("qq:1".into()),
            at: talked,
        },
        Heard {
            person: 1,
            place: Place::Private,
            at: talked,
        },
    ];
    let moved = [Moved {
        person: 2,
        at: talked,
        cause: Cause::Scold,
        felt_after: -30.0,
    }];
    let talks = elsewhere::talks(
        &heard,
        &moved,
        &[],
        &[],
        up,
        Facing {
            person: 1,
            group: None,
        },
    );
    assert_eq!(got, Some(elsewhere::section(&talks, now)));
    assert!(got.unwrap().contains("group"));
    let feeling = ok(
        "elsewhere.own_feeling",
        json!({ "pushes": [{ "at": micros(talked), "valence": -30.0 }], "now": micros(now) }),
    )["feeling"]
        .as_f64()
        .unwrap();
    let crate_feeling = elsewhere::own_feeling(
        &[Push {
            at: talked,
            valence: -30.0,
        }],
        now,
    );
    assert_eq!(feeling.to_bits(), crate_feeling.to_bits());
}

/// What the site reads from her feeling (`self_state` `recall_breadth` and
/// `facts_view`) is what the calls read, on and around every threshold.
#[test]
fn how_today_left_her_reads_as_the_crate_reads_it() {
    for feeling in [
        -40.0, -25.0, -24.9, -10.0, -9.9, 0.0, 9.9, 10.0, 24.9, 25.0, 40.0,
    ] {
        let low = ok(
            "elsewhere.own_feeling_is_low",
            json!({ "feeling": feeling }),
        )["low"]
            .clone();
        assert_eq!(
            low,
            json!(elsewhere::own_feeling_is_low(feeling)),
            "{feeling}"
        );
        let words =
            ok("elsewhere.own_feeling_words", json!({ "feeling": feeling }))["words"].clone();
        assert_eq!(
            words,
            json!(elsewhere::own_feeling_words(feeling)),
            "{feeling}"
        );
    }
}
