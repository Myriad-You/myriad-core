//! Her log across host cores: every case in the shared conformance files
//! (`crates/conformance/v2/log`, `v2/lease`) through the C entry point, as
//! another platform runs them. `myriad-persona-log` runs the same files
//! through the crate and blesses them.

use std::path::PathBuf;

use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::ffi;
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "lease.behind",
    "lease.here",
    "lease.move",
    "lease.newer",
    "log.admit",
    "log.answer",
    "log.canonical",
    "log.clock_next",
    "log.clock_observe",
    "log.clock_offset",
    "log.clock_repair",
    "log.fold",
    "log.holdings",
    "log.ledger",
    "log.merge",
    "log.missing",
    "log.partner_wrote",
    "log.reach",
];

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance")
}

#[test]
fn every_conformance_case_answers_through_the_c_entry_point() {
    let mut calls = std::collections::BTreeSet::new();
    let mut cases = 0;
    for area in ["log", "lease"] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(conformance().join(if area == "log" {
            "v2/log"
        } else {
            "v2/lease"
        }))
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
    assert!(cases >= 40, "{cases}");
}

/// A deputy runs as long as the contract says at most, in the log rules'
/// microseconds.
#[test]
fn the_longest_deputy_is_the_contracts() {
    let contract: Value =
        serde_json::from_str(myriad_her_link::CONTRACT_JSON).expect("the contract reads");
    let most_ms = contract["timing"]["deputyMaxMs"].as_i64().unwrap();
    assert_eq!(most_ms * 1000, myriad_persona_log::lease::DEPUTY_MAX_MICROS);
}

/// The reasons a child tells its host it refused rows for (`log.refused`)
/// are the reasons `log.admit` refuses a row for, by the same names, and
/// the two of a batch refused whole; its kinds are the kinds of audience.
/// A reason or a kind added here fails to compile until the contract has it.
#[test]
fn the_reasons_rows_are_refused_for_are_the_contracts() {
    use myriad_persona_log::admit::Reason;
    use myriad_persona_log::row::Audience;
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shared/her_link_contract.json");
    let contract: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let listed = |key: &str| -> Vec<String> {
        contract[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|one| one.as_str().unwrap().to_string())
            .collect()
    };
    let reasons = [
        Reason::Envelope,
        Reason::Local,
        Reason::AudienceChanged,
        Reason::NotAMember,
        Reason::EpochBack,
        Reason::NotHeld,
    ];
    for reason in reasons {
        match reason {
            Reason::Envelope
            | Reason::Local
            | Reason::AudienceChanged
            | Reason::NotAMember
            | Reason::EpochBack
            | Reason::NotHeld => {}
        }
    }
    let mut named: Vec<String> = reasons
        .iter()
        .map(|reason| {
            serde_json::to_value(reason)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    named.extend(["clock_ahead".to_string(), "batch".to_string()]);
    assert_eq!(listed("refuseReasons"), named);
    let kinds: Vec<String> = ["personal:a", "own:", "group:g"]
        .into_iter()
        .map(|raw| match Audience::parse(raw).unwrap() {
            Audience::Personal(_) => "personal",
            Audience::Own => "own",
            Audience::Group(_) => "group",
        })
        .map(str::to_string)
        .collect();
    assert_eq!(listed("logAudiences"), kinds);
}
