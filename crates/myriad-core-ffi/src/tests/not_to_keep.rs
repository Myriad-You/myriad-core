//! What they asked her not to keep, through the C entry point: every case in
//! the shared conformance files (`crates/conformance/v1/not_to_keep`), as
//! another platform runs them, and the same answers as the crate. The cases
//! of `chat_remember.extract_system` and `extract_schema` that do not ask
//! for it were recorded before they could (2026-11-24, 57fd42be4): what was
//! asked as ever is asked as ever.
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-core-ffi not_to_keep`
//! rewrites their `output` from the crate, only with a change that says why.

use std::path::PathBuf;

use myriad_merope::{chat_remember, not_to_keep};
use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "chat_remember.extract_schema",
    "chat_remember.extract_system",
    "chat_remember.read_not_to_keep",
    "not_asked.input",
    "not_asked.read",
    "not_asked.schema",
    "not_asked.system",
    "not_to_keep.input",
    "not_to_keep.parts",
    "not_to_keep.read",
    "not_to_keep.schema",
    "not_to_keep.sources",
    "not_to_keep.system",
];

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/not_to_keep")
}

#[test]
fn every_not_to_keep_case_answers_through_the_c_entry_point() {
    let bless = std::env::var("MYRIAD_CONFORMANCE_BLESS").as_deref() == Ok("1");
    let mut calls = std::collections::BTreeSet::new();
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
        }
        if bless {
            let mut text = serde_json::to_string_pretty(&file).unwrap();
            text.push('\n');
            std::fs::write(&path, text).unwrap();
        }
    }
    assert_eq!(calls.into_iter().collect::<Vec<_>>(), TESTED);
}

/// What is asked with and without `asked` is the crate's; left out, it is
/// false, and then nothing is asked that was not.
#[test]
fn what_they_ask_not_to_keep_is_asked_for_as_the_crate_does() {
    use chat_remember::Examples;
    let known = vec!["养了一只猫".to_string(), "在上海工作".to_string()];
    for (theirs, examples) in [
        ("体重的事以后都别记", Examples::Chinese),
        ("don't keep anything about my weight", Examples::English),
    ] {
        let system = |more: Value| {
            let mut input = json!({ "existing": known, "theirs": theirs });
            (input.as_object_mut().unwrap()).extend(more.as_object().unwrap().clone());
            ok("chat_remember.extract_system", input)["text"].clone()
        };
        for (dated, asked, want) in [
            (
                false,
                false,
                chat_remember::extract_system_prompt(&known, examples),
            ),
            (
                true,
                false,
                chat_remember::extract_system_prompt_dated(&known, examples),
            ),
            (
                false,
                true,
                chat_remember::extract_system_prompt_asked(&known, examples),
            ),
            (
                true,
                true,
                chat_remember::extract_system_prompt_dated_asked(&known, examples),
            ),
        ] {
            assert_eq!(system(json!({ "dated": dated, "asked": asked })), want);
            if !asked {
                assert_eq!(system(json!({ "dated": dated })), want);
                assert!(!want.contains("notToKeep"));
            }
        }
    }
    for (input, want) in [
        (json!({}), chat_remember::extract_schema()),
        (json!({ "asked": false }), chat_remember::extract_schema()),
        (
            json!({ "dated": true }),
            chat_remember::extract_schema_dated(),
        ),
        (
            json!({ "asked": true }),
            chat_remember::extract_schema_asked(),
        ),
        (
            json!({ "dated": true, "asked": true }),
            chat_remember::extract_schema_dated_asked(),
        ),
    ] {
        assert_eq!(
            ok("chat_remember.extract_schema", input)["schema"],
            want,
            "{want}"
        );
    }
}

/// Read as the crate reads it, and a refusal quotes nothing of what they
/// said.
#[test]
fn what_they_asked_is_read_as_the_crate_does_and_not_quoted_back() {
    let theirs = "体重的事你以后都别记\n好\n前任的事也别记";
    let by = [true, false, true];
    let good = json!({ "fact": "不要记前任的事", "evidence": "前任的事也别记", "said": 2,
        "replaces": ["不要记前任的生日"] });
    let before = ["不要记前任的生日".to_string()];
    for list in [
        json!([]),
        json!(null),
        json!([good]),
        json!([{ "fact": "不要记体重" }, good]),
        json!([{ "fact": "不要记体重" }]),
    ] {
        let answer = json!({ "notToKeep": list });
        // Of one line alone, none of what they asked before is given.
        for (by, before) in [(None, &[][..]), (Some(&by[..]), &before[..])] {
            let mut input = json!({ "answer": answer, "theirs": theirs, "askedBefore": before });
            if let Some(by) = by {
                input["saidByThem"] = json!(by);
            }
            let got = ok("chat_remember.read_not_to_keep", input);
            let asked = chat_remember::read_not_to_keep(&answer, theirs, by, before);
            assert_eq!(got["asked"], serde_json::to_value(&asked).unwrap());
            assert_eq!(
                got["unread"],
                chat_remember::not_to_keep_unread(&answer, theirs, by)
            );
        }
    }
    let secret = "不要记体重的事";
    for (call, input) in [
        (
            "chat_remember.read_not_to_keep",
            json!({ "answer": {}, "theirs": "", "askedBefore": secret }),
        ),
        (
            "not_to_keep.input",
            json!({ "asked": secret, "items": [secret] }),
        ),
        ("not_to_keep.parts", json!({ "items": secret })),
        ("not_to_keep.read", json!({ "raw": "", "items": secret })),
        ("not_asked.input", json!({ "line": "", "asked": secret })),
        ("not_asked.read", json!({ "raw": "", "asked": secret })),
    ] {
        let (status, out) = ffi(call, &input);
        assert_eq!(status, STATUS_BAD_INPUT, "{call}");
        assert!(!out.to_string().contains("体重"), "{call}: {out}");
    }
}

#[test]
fn the_step_that_holds_what_is_kept_to_it_is_the_crates() {
    let asked = vec!["不要记体重的事".to_string()];
    let items = vec!["周六爬山".to_string(), "瘦了两公斤".to_string()];
    for who in [None, Some("阿北")] {
        assert_eq!(
            ok(
                "not_to_keep.input",
                json!({ "asked": asked, "who": who, "items": items })
            )["input"],
            not_to_keep::not_to_keep_input(&asked, who, &items)
        );
    }
    assert_eq!(
        ok("not_to_keep.system", json!({}))["text"],
        not_to_keep::not_to_keep_system()
    );
    assert_eq!(
        ok("not_to_keep.schema", json!({})),
        json!({
            "schemaName": not_to_keep::NOT_TO_KEEP_SCHEMA_NAME,
            "schema": not_to_keep::not_to_keep_schema(),
        })
    );
    assert_eq!(
        ok(
            "not_asked.input",
            json!({ "line": "体重别记了", "asked": asked })
        )["input"],
        not_to_keep::not_asked_input("体重别记了", &asked)
    );
    assert_eq!(
        ok("not_asked.system", json!({}))["text"],
        not_to_keep::not_asked_system()
    );
    assert_eq!(
        ok("not_asked.schema", json!({})),
        json!({
            "schemaName": not_to_keep::NOT_ASKED_SCHEMA_NAME,
            "schema": not_to_keep::not_asked_schema(),
        })
    );
    // Not read is null, and never an empty list.
    assert_eq!(
        ok("not_to_keep.read", json!({ "raw": "", "items": 1 })),
        json!({ "drop": null })
    );
    assert_eq!(
        ok("not_asked.read", json!({ "raw": "", "asked": 1 })),
        json!({ "notAsked": null })
    );
    // Each part is the items it names, and together they are all of them.
    let many: Vec<String> = (0..60).map(|one| "说".repeat(one * 20)).collect();
    let parts = ok("not_to_keep.parts", json!({ "items": many }));
    let mut next = 0;
    for part in parts["parts"].as_array().unwrap() {
        assert_eq!(part["from"], next);
        next = part["to"].as_u64().unwrap();
    }
    assert_eq!(next, 60);
    assert_eq!(
        ok("not_to_keep.sources", json!({})),
        json!({ "asked": "asked", "leftAlone": "left_alone" })
    );
}
