//! The contract's examples (`shared/her_link_fixtures.json`, one of every
//! frame) all pass; the shared conformance cases
//! (`crates/conformance/v1/link/`) refuse what they name, with the code they
//! name. `myriad-core-ffi` runs the same cases through the C entry point.
//!
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-her-link` rewrites the
//! cases' `output`.

use std::{collections::BTreeSet, path::PathBuf};

use serde_json::{Value, json};

use super::*;

const FIXTURES_JSON: &str = include_str!("../../../shared/her_link_fixtures.json");

fn fixtures() -> Value {
    serde_json::from_str::<Value>(FIXTURES_JSON).unwrap()
}

fn roles(of: &str) -> Vec<String> {
    match of {
        "device" => vec!["device".into()],
        "host" => vec!["device".into(), "host".into()],
        "mind" => vec!["device".into(), "host".into(), "mind".into()],
        "server" => vec!["host".into(), "server".into()],
        _ => Vec::new(),
    }
}

/// One `link.check` as the shared core answers it.
pub(crate) fn answer(input: &Value) -> Value {
    let roles: Vec<String> = input
        .get("roles")
        .map(|roles| serde_json::from_value(roles.clone()).unwrap())
        .unwrap_or_default();
    match check(&input["frame"], &roles) {
        Ok(checked) => json!({ "accepted": checked }),
        Err(refusal) => json!({ "refused": { "code": refusal.code } }),
    }
}

#[test]
fn the_contract_reads_and_agrees_with_these_rules() {
    let raw: Value = serde_json::from_str(CONTRACT_JSON).unwrap();
    assert_eq!(raw["schemaVersion"], 2);
    assert_eq!(raw["protocol"], 2, "the only protocol spoken");
    assert_eq!(protocol(), 2);
    assert!(
        raw.get("next").is_none(),
        "protocol 1 is history, not a block beside it"
    );
    assert!(raw["history"].get("1").is_some());
    assert_eq!(CONTRACT.limits.id_pattern, "^[A-Za-z0-9_.:-]{1,64}$");
    assert_eq!(CONTRACT.envelope.required[0], "v", "every frame carries v");
    for id in ["t_1", "A.b:c-D_9", &"x".repeat(64)] {
        assert!(valid_id(id), "{id}");
    }
    for id in ["", "has space", "turn/1", "表", &"x".repeat(65)] {
        assert!(!valid_id(id), "{id}");
    }
    // Every family has frames, and each frame of a family is sent by a
    // sender the contract names.
    let declared: BTreeSet<&str> = families().keys().map(String::as_str).collect();
    let used: BTreeSet<&str> = CONTRACT.frames.keys().filter_map(|t| family(t)).collect();
    assert_eq!(declared, used);
    assert_eq!(family("say"), None, "protocol 1's say is of no family");
    assert_eq!(family("say.line"), Some("say"));
    assert_eq!(family("touch.appraise"), None);
    for (t, spec) in &CONTRACT.frames {
        assert!(raw["deliveries"].get(&spec.delivery).is_some(), "{t}");
        if family(t).is_some() {
            assert!(
                raw["senders"].get(&spec.from).is_some() || spec.from == "either",
                "{t}"
            );
        }
        for pair in &spec.one_of {
            for field in pair {
                assert!(spec.optional.contains(field), "{t}.{field}");
            }
        }
        for field in spec.sealed.iter().chain(spec.max_items.keys()) {
            assert!(
                spec.optional.contains(field) || spec.required.contains(field),
                "{t}.{field}"
            );
        }
    }
    // The sync families are families, and only their frames need sync on.
    for family in &CONTRACT.sync.families {
        assert!(declared.contains(family.as_str()), "{family}");
    }
    assert!(needs_sync("lease.grant") && needs_sync("chat.lend"));
    assert!(!needs_sync("fact.up") && !needs_sync("say") && !needs_sync("hello"));
    // The welcome says whether they flow.
    assert!(
        CONTRACT.frames["welcome"]
            .required
            .contains(&"sync".to_string())
    );
    // No close code twice.
    let mut numbers = BTreeSet::new();
    for spec in raw["closeCodes"].as_object().unwrap().values() {
        let code = spec["code"].as_u64().unwrap();
        assert!((4000..5000).contains(&code));
        assert!(numbers.insert(code), "close code {code} twice");
    }
    assert_eq!(raw["wake"]["payload"], fixtures()["wake"]);
    assert_eq!(seal_info("fact.up"), "myriad-link/2 fact.up");
    assert_eq!(
        String::from_utf8(seal_aad("dev_phone", "fact.up", "dev_mac")).unwrap(),
        r#"{"from":"dev_phone","t":"fact.up","to":"dev_mac"}"#
    );
}

#[test]
fn every_example_passes_and_every_frame_has_one() {
    let mut seen = BTreeSet::new();
    for frame in fixtures()["frames"].as_array().unwrap() {
        let t = frame["t"].as_str().unwrap();
        let from = &CONTRACT.frames[t].from;
        let checked =
            check(frame, &roles(from)).unwrap_or_else(|refusal| panic!("{t}: {refusal:?}"));
        // Its one byte form reads back to itself.
        let again = check_bytes(checked.canonical.as_bytes(), &[]).unwrap();
        assert_eq!(again, checked);
        seen.insert(t.to_string());
    }
    let declared: BTreeSet<String> = CONTRACT.frames.keys().cloned().collect();
    assert_eq!(seen, declared, "one example per frame at least");
}

/// One role dials the host: every hello and welcome example names a role
/// the contract has, and the hello it closes the link on (an app from
/// before the device she lives on dialled in as a child) is a frame like
/// any other, closed by its role with a code the contract has.
#[test]
fn one_role_dials_the_host_and_the_old_one_is_told_to_update() {
    let raw: Value = serde_json::from_str(CONTRACT_JSON).unwrap();
    let roles = raw["roles"].as_array().unwrap();
    assert_eq!(roles, &[json!("child"), json!("host")]);
    for frame in fixtures()["frames"].as_array().unwrap() {
        match frame["t"].as_str().unwrap() {
            "hello" => assert_eq!(frame["body"]["role"], "child"),
            "welcome" => assert_eq!(frame["body"]["role"], "host"),
            _ => {}
        }
    }
    let closed = fixtures()["closed"].as_array().unwrap().clone();
    assert!(!closed.is_empty());
    for example in closed {
        let close = example["close"].as_str().unwrap();
        assert!(raw["closeCodes"].get(close).is_some(), "{close}");
        check(&example["frame"], &[]).unwrap();
        assert_eq!(example["frame"]["body"]["role"], "host");
    }
    assert!(raw["closeCodes"].get("host_is_site").is_none());
}

#[test]
fn a_frame_over_the_limit_is_too_large() {
    let mut frame = fixtures()["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| frame["t"] == "fact.up")
        .unwrap()
        .clone();
    frame["body"]["facts"][0]["data"]["label"] = json!("x".repeat(270_000));
    assert_eq!(check(&frame, &[]).unwrap_err().code, "too_large");
    let bytes = serde_json::to_vec(&frame).unwrap();
    assert_eq!(check_bytes(&bytes, &[]).unwrap_err().code, "too_large");
    assert_eq!(
        check_bytes(b"{not json", &[]).unwrap_err().code,
        "bad_frame"
    );
}

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/link")
}

#[test]
fn every_link_case_answers_as_recorded() {
    let path = conformance().join("check.json");
    let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(file["call"], "link.check");
    let bless = std::env::var_os("MYRIAD_CONFORMANCE_BLESS").is_some();
    let mut codes = BTreeSet::new();
    for case in file["cases"].as_array_mut().unwrap() {
        let got = answer(&case["input"]);
        if let Some(code) = got["refused"]["code"].as_str() {
            codes.insert(code.to_string());
            // A refused case is named after its code.
            assert!(
                case["name"]
                    .as_str()
                    .unwrap()
                    .starts_with(&format!("{code}:")),
                "{}: {got}",
                case["name"]
            );
        }
        if bless {
            case["output"] = got;
        } else {
            assert_eq!(case["output"], got, "{}", case["name"]);
        }
    }
    if bless {
        let mut text = serde_json::to_string_pretty(&file).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
    }
    // Every code a frame can be refused with (each `refuse("…"` in the
    // rules) but too_large (tested above, too big for a file) has a case,
    // and each is one the contract names.
    let rules = include_str!("lib.rs");
    let all: BTreeSet<String> = rules
        .split("refuse(")
        .skip(1)
        .filter_map(|rest| rest.trim_start().strip_prefix('"')?.split('"').next())
        .filter(|code| *code != "too_large")
        .map(str::to_string)
        .collect();
    for code in &all {
        assert!(CONTRACT.error_codes.contains_key(code), "{code}");
    }
    assert_eq!(codes, all);
}
