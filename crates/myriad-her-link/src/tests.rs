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
    assert_eq!(raw["schemaVersion"], 3);
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

#[test]
fn every_call_receive_case_answers_as_recorded() {
    let path = conformance().join("call_receive.json");
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let pairs = |list: &Value| -> Vec<(String, u64)> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|one| {
                (
                    one["name"].as_str().unwrap().to_string(),
                    one["version"].as_u64().unwrap(),
                )
            })
            .collect()
    };
    for case in file["cases"].as_array().unwrap() {
        let input = &case["input"];
        let granted = (!input["granted"].is_null()).then(|| pairs(&input["granted"]));
        let refused = input.get("refused").map(pairs).unwrap_or_default();
        let got = call_receive(
            input["action"].as_str().unwrap(),
            input["version"].as_u64().unwrap(),
            &input["for"],
            input["deadlineAt"].as_u64().unwrap(),
            input["takenAt"].as_u64().unwrap(),
            &pairs(&input["offers"]),
            &refused,
            granted.as_deref(),
        );
        let (verdict, code) = match got.verdict {
            CallVerdict::Hold => ("hold", Value::Null),
            CallVerdict::Do => ("do", Value::Null),
            CallVerdict::Fail(code) => ("fail", json!(code)),
        };
        let got = json!({ "verdict": verdict, "code": code, "version": got.version,
            "deadlineAt": got.deadline_at });
        assert_eq!(got, case["output"], "{}", case["name"]);
        // Every code it fails with is one the contract's calls name.
        if let Some(code) = got["code"].as_str() {
            assert!(CONTRACT_JSON.contains(&format!("\"{code}\"")), "{code}");
        }
    }
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

/// Whom a call may be for (`calls`): what shows the owner's private things
/// only in their own private talk, nothing at all for someone else's, a
/// group's actions only for a group; every action's fields and each code
/// a receiver may answer with are ones the contract has.
#[test]
fn a_call_is_allowed_only_for_whom_its_action_says() {
    let owner = json!({ "kind": "owner" });
    let own = json!({ "kind": "own" });
    let group = json!({ "kind": "group", "groupId": "g_1" });
    let other = json!({ "kind": "other" });
    for private in [
        "screen.look",
        "work.run",
        "camera.take",
        "mic.listen",
        "model.chat",
    ] {
        assert_eq!(call_allowed(private, &owner), Ok(()), "{private}");
        assert_eq!(
            call_allowed(private, &group),
            Err("wrong_audience"),
            "{private}"
        );
        assert_eq!(
            call_allowed(private, &other),
            Err("wrong_audience"),
            "{private}"
        );
    }
    for told in ["notify.post", "voice.speak"] {
        assert_eq!(call_allowed(told, &own), Ok(()), "{told}");
        assert_eq!(call_allowed(told, &group), Err("wrong_audience"), "{told}");
    }
    for said in ["group.read", "group.send"] {
        assert_eq!(call_allowed(said, &group), Ok(()), "{said}");
        assert_eq!(call_allowed(said, &owner), Err("wrong_audience"), "{said}");
    }
    assert_eq!(call_allowed("screen.record", &owner), Err("call_unknown"));
    let raw: Value = serde_json::from_str(CONTRACT_JSON).unwrap();
    let kinds: BTreeSet<&str> = raw["callAudiences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kind| kind.as_str().unwrap())
        .collect();
    let groups: BTreeSet<&str> = raw["calls"]["actions"]
        .as_object()
        .unwrap()
        .values()
        .map(|action| action["group"].as_str().unwrap())
        .collect();
    for (name, action) in raw["calls"]["actions"].as_object().unwrap() {
        let spec = call_spec(name).unwrap();
        assert!(["child", "host"].contains(&spec.by.as_str()), "{name}");
        for kind in &spec.audiences {
            assert!(kinds.contains(kind.as_str()), "{name}: {kind}");
            assert_ne!(
                kind, "other",
                "{name}: no action is for someone else's talk"
            );
        }
        assert!(spec.blob <= spec.long, "{name}: a blob waits for the owner");
        for (field, list) in spec.args.enums.iter().chain(&spec.result.enums) {
            assert!(
                raw[list.as_str()].is_array() && super::named_list(list).is_some(),
                "{name}.{field}: {list} is a list the rules read"
            );
        }
        assert!(action["description"].is_string(), "{name}");
    }
    for reason in &super::CONTRACT.drop_reasons {
        assert!(
            raw["errorCodes"][reason.as_str()].is_string(),
            "dropReasons: {reason} is an error code"
        );
    }
    for (kind, streams) in [
        ("state", &super::CONTRACT.streams.state),
        ("event", &super::CONTRACT.streams.event),
    ] {
        for (name, spec) in streams {
            assert!(super::valid_id(name), "streams.{kind}.{name}");
            assert!(spec.version >= 1, "streams.{kind}.{name}");
            assert!(
                ["child", "host", "either"].contains(&spec.from.as_str()),
                "streams.{kind}.{name}: from {}",
                spec.from
            );
            assert!(
                spec.audience == "link" || super::CONTRACT.call_audiences.contains(&spec.audience),
                "streams.{kind}.{name}: for {}",
                spec.audience
            );
            for (field, list) in &spec.fields.enums {
                assert!(
                    raw[list.as_str()].is_array() && super::named_list(list).is_some(),
                    "streams.{kind}.{name}.{field}: {list} is a list the rules read"
                );
            }
            assert!(raw["streams"][kind][name.as_str()]["description"].is_string());
        }
    }
    for group in calls_default_approved() {
        assert!(groups.contains(group.as_str()), "{group}");
    }
    for code in [
        "call_unknown",
        "version_unsupported",
        "call_not_granted",
        "wrong_audience",
        "declined",
        "not_now",
        "call_timeout",
        "call_cancelled",
        "call_unavailable",
    ] {
        assert!(CONTRACT.error_codes.contains_key(code), "{code}");
    }
}

#[test]
fn a_call_is_held_to_its_deadline_or_the_most_its_action_may_be_given() {
    let taken = 1_760_141_100_000;
    // Within the most: as asked.
    assert_eq!(
        super::call_deadline("screen.look", taken + 10_000, taken),
        taken + 10_000
    );
    // Past it: held to the most from when it was taken, not refused.
    assert_eq!(
        super::call_deadline("screen.look", taken + 120_000, taken),
        taken + CONTRACT.timing.call_deadline_max_ms
    );
    assert_eq!(
        super::call_deadline("work.run", taken + 3_600_000, taken),
        taken + CONTRACT.timing.long_call_max_ms
    );
    // An action this end does not know is given a long call's most.
    assert_eq!(
        super::call_most_ms("screen.record"),
        CONTRACT.timing.long_call_max_ms
    );
    // Already passed: the receiver answers it call_timeout.
    assert!(super::call_deadline("screen.look", taken - 1, taken) < taken);
}

/// A grant or a deputy to an end that listed `deputy` is delivered when that
/// end says it took it, never by the link's ack: the ack is of everything
/// heard, a frame the end refused too. To an end that did not list it,
/// nothing changes; no other frame waits for an answer.
#[test]
fn a_lease_frame_to_an_end_that_listed_deputy_waits_for_lease_taken() {
    let deputy = vec!["say".to_string(), "deputy".to_string()];
    let before = vec!["say".to_string(), "lend".to_string()];
    for t in TAKEN_BY_ANSWER {
        assert_eq!(CONTRACT.frames[*t].delivery, "reliable", "{t}");
        assert!(taken_by_answer(t, &deputy), "{t}");
        assert!(!taken_by_answer(t, &before), "{t}");
    }
    for t in ["say", "lease.epoch", "lease.takeover", "group.lend"] {
        assert!(!taken_by_answer(t, &deputy), "{t}");
    }
    // The answer itself names the frame and its id, and only these two.
    let taken = |of: &str, id: &str| {
        json!({
            "v": 1, "t": "lease.taken", "seq": 3, "ack": 9, "at": 1,
            "body": { "of": of, "id": id },
        })
    };
    let device = roles("device");
    assert!(check(&taken("lease.grant", "59"), &device).is_ok());
    assert!(check(&taken("lease.deputy", "dp_1"), &device).is_ok());
    assert_eq!(
        check(&taken("lease.epoch", "59"), &device)
            .unwrap_err()
            .code,
        "bad_frame"
    );
    assert_eq!(
        check(&taken("lease.grant", ""), &device).unwrap_err().code,
        "bad_id"
    );
    // Only the server gives a deputy.
    let mut deputy_frame = fixtures()["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| frame["t"] == "lease.deputy")
        .unwrap()
        .clone();
    assert!(check(&deputy_frame, &roles("server")).is_ok());
    assert_eq!(
        check(&deputy_frame, &roles("mind")).unwrap_err().code,
        "bad_frame"
    );
    deputy_frame["body"]["deputy"]["extra"] = json!(1);
    // The frame checks its fields; the deputy's own are the log rules'.
    assert!(check(&deputy_frame, &roles("server")).is_ok());
}

/// An end that did not list `deputy` (every end from before it: the Mac and
/// XR as installed today) is delivered every frame by the link's ack exactly
/// as before, whatever else it listed: no frame to it waits for a
/// `lease.taken`, so the server neither holds a grant back nor sends it
/// again, nor counts it untaken, for want of one.
#[test]
fn an_end_that_did_not_list_deputy_is_delivered_by_the_ack_as_before() {
    let raw: Value = serde_json::from_str(CONTRACT_JSON).unwrap();
    let all_but_deputy: Vec<String> = raw["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|one| one.as_str())
        .filter(|one| *one != "deputy")
        .map(str::to_string)
        .collect();
    assert!(all_but_deputy.len() + 1 == raw["capabilities"].as_array().unwrap().len());
    for said in [Vec::new(), all_but_deputy] {
        for t in CONTRACT.frames.keys() {
            assert!(!taken_by_answer(t, &said), "{t} waits for an answer");
        }
    }
    // Nor does the server send it a deputy: lease.deputy is the capability's alone.
    let description = raw["frames"]["lease.deputy"]["description"]
        .as_str()
        .unwrap();
    assert!(description.starts_with("Only to an end whose hello listed deputy"));
}
