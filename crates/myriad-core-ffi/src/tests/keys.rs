//! Keys at rest through the C entry point: every case in the shared
//! conformance files (`crates/conformance/v1/keys`, `v1/store_keys`), as
//! another platform runs them, and refusals that name nothing they were
//! given.
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-core-ffi keys`
//! rewrites their `output` from the crate, only with a change that says why.

use std::path::PathBuf;

use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use super::{ffi, ok};
use crate::{STATUS_BAD_INPUT, STATUS_OK};

pub(super) const TESTED: &[&str] = &[
    "keys.forget_reach",
    "keys.open",
    "keys.seal",
    "store_keys.read",
    "store_keys.settle",
    "store_keys.step",
];

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1")
}

#[test]
fn every_key_case_answers_through_the_c_entry_point() {
    let bless = std::env::var("MYRIAD_CONFORMANCE_BLESS").as_deref() == Ok("1");
    let mut calls = std::collections::BTreeSet::new();
    let mut cases = 0;
    for area in ["keys", "store_keys"] {
        let mut files: Vec<PathBuf> = std::fs::read_dir(conformance().join(area))
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
    }
    assert_eq!(calls.into_iter().collect::<Vec<_>>(), TESTED);
    assert!(cases >= 30, "{cases}");
}

#[test]
fn what_is_sealed_opens_back_and_no_refusal_names_its_input() {
    let key = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=".to_string();
    let other = "CAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAg=";
    let nonce = "AQEBAQEBAQEBAQEB";
    let bound = json!({ "kind": "payload", "holder": "41", "row": "row_secret_id" });
    let data = "c2VjcmV0IHdvcmRz"; // "secret words"
    let sealed = ok(
        "keys.seal",
        json!({ "key": key, "nonce": nonce, "bound": bound, "data": data }),
    )["sealed"]
        .as_str()
        .unwrap()
        .to_string();
    let opened = ok(
        "keys.open",
        json!({ "key": key, "sealed": sealed, "bound": bound }),
    );
    assert_eq!(opened["data"], data);
    for (call, input) in [
        (
            "keys.open",
            json!({ "key": other, "sealed": sealed, "bound": bound }),
        ),
        (
            "keys.open",
            json!({ "key": key, "sealed": sealed, "bound": { "kind": "payload", "holder": "41", "row": "row_other" } }),
        ),
        (
            "keys.open",
            json!({ "key": key, "sealed": "AAAA", "bound": bound }),
        ),
        (
            "keys.seal",
            json!({ "key": "c2hvcnQ=", "nonce": nonce, "bound": bound, "data": data }),
        ),
        (
            "keys.seal",
            json!({ "key": key, "nonce": nonce, "bound": { "kind": "payload", "holder": 41, "row": "row_secret_id" }, "data": data }),
        ),
        (
            "keys.seal",
            json!({ "key": key, "nonce": nonce, "bound": bound, "data": data, "extra": key }),
        ),
    ] {
        let (status, out) = ffi(call, &input);
        assert_eq!(status, STATUS_BAD_INPUT, "{call}: {out}");
        let message = out.to_string();
        for given in [
            key.as_str(),
            other,
            nonce,
            data,
            sealed.as_str(),
            "secret",
            "row_",
        ] {
            assert!(!message.contains(given), "{call} named {given}: {message}");
        }
    }
}
