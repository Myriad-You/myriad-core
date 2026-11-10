//! Mind-wandering and what kind of event a key is, through the C entry
//! point: every case in the shared conformance files
//! (`crates/conformance/v1/wander`, `v1/notify`), and the calls said again
//! through the crates. A `notify.kind` case carries `today`, the site's and
//! the Mac's own conclusions before the core held this; where the two agreed
//! the core says the same. A case the core changed on purpose carries
//! `changed`, why, instead.
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-core-ffi wander`
//! writes the outputs again after a deliberate change.

use std::path::PathBuf;

use chrono::{DateTime, Duration, FixedOffset, Utc};
use myriad_agent_rules::{Concept, memory::recall};
use myriad_merope::{notify, wander};
use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "memory.wander",
    "notify.kind",
    "wander.quiet",
    "wander.thought",
];

fn run(family: &str) -> Vec<String> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../conformance/v1")
        .join(family);
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
            if let Some(today) = case.get("today") {
                let (site, native) = (&today["site"], &today["native"]);
                if site == native {
                    assert_eq!(
                        &got,
                        site,
                        "{} › {}: both ends agreed",
                        path.display(),
                        case["name"]
                    );
                }
            }
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
    calls
}

#[test]
fn every_wander_and_notify_case_answers_through_the_c_entry_point() {
    let mut calls = run("wander");
    calls.extend(run("notify"));
    calls.sort();
    assert_eq!(calls, TESTED);
}

fn concept(name: &str) -> Concept {
    Concept {
        name: name.into(),
        aliases: Vec::new(),
    }
}

/// The same rows and rolls land in the same place through the C entry point
/// as straight through `recall::wander`, gap and all.
#[test]
fn a_wander_is_the_crates_for_the_same_rows_and_rolls() {
    let now = DateTime::<Utc>::from_timestamp(1_793_750_400, 0).unwrap();
    let cat = [concept("猫"), concept("年糕")];
    let mochi = [concept("年糕")];
    let tea = [concept("茶")];
    let rows = [
        (
            "年糕又胖了",
            &cat[..],
            now - Duration::minutes(10),
            0.0,
            None,
        ),
        (
            "年糕偷吃了鱼",
            &mochi[..],
            now - Duration::minutes(40),
            0.4,
            None,
        ),
        (
            "喜欢喝茶",
            &tea[..],
            now - Duration::days(2),
            0.0,
            Some(now - Duration::days(1)),
        ),
        (
            "她家的猫叫年糕",
            &cat[..],
            now - Duration::days(9),
            0.0,
            None,
        ),
    ];
    let fixed = |at: DateTime<Utc>| -> DateTime<FixedOffset> { at.fixed_offset() };
    let musings: Vec<recall::Musing> = rows
        .iter()
        .map(
            |(content, concepts, at, primed, thought_at)| recall::Musing {
                content,
                concepts,
                at: fixed(*at),
                primed: *primed,
                thought_at: thought_at.map(fixed),
            },
        )
        .collect();
    for rolls in [
        [0.0, 0.0, 0.0, 0.0],
        [0.9, 0.5, 0.1, 0.7],
        [0.99, 0.99, 0.99, 0.99],
    ] {
        let mut drawn = rolls.iter().copied();
        let want = recall::wander(&musings, fixed(now), &mut || drawn.next().unwrap());
        let got = ok(
            "memory.wander",
            json!({
                "rows": rows.iter().map(|(content, concepts, at, primed, thought_at)| json!({
                    "content": content,
                    "concepts": concepts.iter().map(|concept| json!({ "name": concept.name })).collect::<Vec<_>>(),
                    "at": at.timestamp_micros(),
                    "primed": primed,
                    "thoughtAt": thought_at.map(|at| at.timestamp_micros()),
                })).collect::<Vec<_>>(),
                "now": now.timestamp_micros(),
                "rolls": rolls,
            }),
        );
        let want = want.map(|landed| {
            json!({
                "index": landed.index,
                "gap": landed.gap.map(|(name, known)| json!({ "name": name, "known": known })),
            })
        });
        assert_eq!(got["landed"], json!(want), "{rolls:?}");
        assert_ne!(got["landed"]["index"], json!(2), "thought of yesterday");
    }
}

#[test]
fn a_kind_is_the_crates() {
    for kind in [
        "agent.task_failed",
        "agent.merope.thought",
        "agent.merope.touch",
        "agent.merope.greeting",
        "mcp.connected",
    ] {
        assert_eq!(
            ok("notify.kind", json!({ "kind": kind })),
            json!({
                "inPerson": notify::said_only_in_person(kind),
                "taskOutcome": notify::is_task_outcome(kind),
                "interruptsWhenAway": notify::interrupts_when_away(kind),
                "worthNotifying": notify::worth_notifying(kind),
            }),
            "{kind}"
        );
    }
    assert_eq!(
        ok(
            "wander.thought",
            json!({ "content": "喜欢喝茶", "gap": { "name": "茶", "known": 2 } })
        )["text"],
        wander::thought("喜欢喝茶", Some(("茶", 2)))
    );
}
