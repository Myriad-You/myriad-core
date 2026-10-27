//! Grants and audience through the C entry point: every case in the shared
//! conformance files (`crates/conformance/v1/grants`, `…/audience`), as
//! another platform runs them, and the same answers as the crate.
//! `myriad-agent-rules` runs the same files through the crate.

use std::path::PathBuf;

use myriad_agent_rules::{
    grants::{self, Actor, AutonomyGrant, Layers, Source},
    memory::audience::{self, Audience, Venue},
};
use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "audience.admits",
    "grants.autonomy",
    "grants.cap_still_allows",
    "grants.check",
    "grants.claim",
    "grants.effective",
    "grants.granted",
    "grants.may_enter_work",
    "grants.prepare",
    "grants.revoke",
];

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1")
}

#[test]
fn every_grant_and_audience_case_answers_through_the_c_entry_point() {
    let mut calls = std::collections::BTreeSet::new();
    let mut cases = 0;
    for area in ["audience", "grants"] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(conformance().join(area))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        for path in files {
            let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let call = file["call"].as_str().unwrap();
            calls.insert(call.to_string());
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
    }
    assert_eq!(calls.into_iter().collect::<Vec<_>>(), TESTED);
    assert!(cases >= 70, "{cases}");
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

#[test]
fn grant_calls_answer_as_the_crate_does() {
    let actor = Actor {
        id: "7".to_string(),
        personal: true,
    };
    let grant = AutonomyGrant {
        actor: "7".to_string(),
        allowed: names(&["mail:send", "calendar:read", "mcp:execute"]),
        revoked: false,
    };
    let granted = names(&["calendar:read", "mail:send", "files:write"]);
    let cap = names(&["calendar:read"]);
    let required = names(&["files:write", "calendar:read", "mail:send"]);
    let both = json!({ "actor": actor, "grant": grant, "granted": granted });

    let layers = Layers {
        declared: names(&["a", "b", "c"]),
        approved: names(&["c", "a"]),
        runtime_allowed: names(&["a", "c"]),
    };
    assert_eq!(
        ok("grants.granted", json!(layers))["granted"],
        json!(grants::granted(&layers).unwrap())
    );
    assert_eq!(
        ok(
            "grants.effective",
            json!({ "granted": granted, "cap": cap })
        )["effective"],
        json!(grants::effective(&granted, Some(&cap)))
    );
    assert_eq!(
        ok("grants.autonomy", both.clone()),
        json!(grants::evaluate(&actor, Some(&grant), &granted))
    );
    for (cap, label) in [(Some(&cap), "capped"), (None, "uncapped")] {
        let mut req = both.clone();
        req["cap"] = json!(cap);
        req["required"] = json!(required);
        assert_eq!(
            ok("grants.check", req.clone()),
            json!(grants::check(
                &actor,
                Some(&grant),
                &granted,
                cap.map(Vec::as_slice),
                &required
            )),
            "{label}"
        );
        req.as_object_mut().unwrap().remove("required");
        assert_eq!(
            ok("grants.cap_still_allows", req)["allows"],
            json!(grants::cap_still_allows(
                &actor,
                Some(&grant),
                &granted,
                cap.map(Vec::as_slice)
            )),
            "{label}"
        );
    }
    for source in [Source::User, Source::Autonomy] {
        let mut req = both.clone();
        req["source"] = json!(source);
        assert_eq!(
            ok("grants.may_enter_work", req.clone())["mayEnter"],
            json!(grants::may_enter_work(
                &actor,
                Some(&grant),
                &granted,
                source
            ))
        );
        assert_eq!(
            ok("grants.claim", req)["cap"],
            json!(grants::claim(&actor, source, Some(&grant), &granted))
        );
    }
    let prepared = grants::prepare(&actor, &[], &granted, &names(&["mail:send"])).unwrap();
    assert_eq!(
        ok(
            "grants.prepare",
            json!({ "actor": actor, "requested": [], "granted": granted, "candidates": ["mail:send"] })
        )["grant"],
        json!(prepared)
    );
    assert_eq!(
        ok(
            "grants.revoke",
            json!({ "grant": prepared, "personal": true })
        )["grant"],
        json!(grants::revoke(&prepared, true).unwrap())
    );
}

#[test]
fn audience_admits_answers_as_the_crate_does() {
    let private = |members: &[&str]| Audience {
        members: names(members),
        venue: Venue::Private,
    };
    let group = |id: &str, members: &[&str]| Audience {
        members: names(members),
        venue: Venue::Group(id.into()),
    };
    let audiences = [
        private(&["7"]),
        private(&["7", "8"]),
        private(&[]),
        group("telegram:-1", &["7"]),
        group("telegram:-2", &["8"]),
    ];
    for learned in &audiences {
        for present in &audiences {
            assert_eq!(
                ok(
                    "audience.admits",
                    json!({ "learned": learned, "present": present })
                )["admits"],
                json!(audience::admits(learned, present)),
                "{learned:?} › {present:?}"
            );
        }
    }
}

/// A refusal names no permission and no person from the input.
#[test]
fn refusals_quote_nothing_from_the_input() {
    let (status, out) = ffi(
        "grants.granted",
        &json!({ "declared": [], "approved": ["secret:name"], "runtimeAllowed": ["secret:name"] }),
    );
    assert_eq!(status, STATUS_BAD_INPUT);
    assert!(!out.to_string().contains("secret:name"), "{out}");
}
