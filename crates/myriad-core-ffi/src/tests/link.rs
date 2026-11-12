//! Her link's frames through the C entry point: the shared conformance
//! cases (`crates/conformance/v1/link`) and every protocol-2 example in
//! `shared/her_link_fixtures.json`.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::{ffi, ok};
use crate::STATUS_BAD_INPUT;

pub(super) const TESTED: &[&str] = &["link.check", "link.encode", "link.now_read"];

#[test]
fn every_link_case_answers_through_the_c_entry_point() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/link/check.json");
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    for case in file["cases"].as_array().unwrap() {
        let mut got = ok("link.check", case["input"].clone());
        // The files keep the code; the detail is for people.
        if let Some(refused) = got.get_mut("refused").and_then(Value::as_object_mut) {
            refused.remove("detail");
        }
        assert_eq!(got, case["output"], "{}", case["name"]);
    }
}

#[test]
fn every_example_encodes_to_its_canonical_text() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../../shared/her_link_fixtures.json")).unwrap();
    for frame in fixtures["frames"].as_array().unwrap() {
        let text = ok("link.encode", json!({ "frame": frame }))["text"]
            .as_str()
            .unwrap()
            .to_string();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(&back, frame);
    }
    let mut bad = fixtures["frames"][0].clone();
    bad["v"] = json!(2);
    let (status, out) = ffi("link.encode", &json!({ "frame": bad }));
    assert_eq!(status, STATUS_BAD_INPUT, "{out}");
}

/// The `now` family's bodies, read by the contract's `sessions` rules: the
/// contract's examples pass, a body against the rules is a refusal (an
/// answer, the frame came from the other end), another frame is bad input.
#[test]
fn now_bodies_are_read_by_the_sessions_rules() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../../shared/her_link_fixtures.json")).unwrap();
    let body = |t: &str| {
        fixtures["frames"]
            .as_array()
            .unwrap()
            .iter()
            .find(|frame| frame["t"] == t)
            .unwrap()["body"]
            .clone()
    };
    let session = ok(
        "link.now_read",
        json!({"t": "now.session", "body": body("now.session")}),
    );
    assert_eq!(session["session"]["change"], "asked");
    assert_eq!(session["session"]["waitingOn"], "owner");
    let drop = ok(
        "link.now_read",
        json!({"t": "now.drop", "body": body("now.drop")}),
    );
    assert_eq!(drop["drop"], body("now.drop"));
    let mut bad = body("now.session");
    bad["remote"] = json!("https://token@github.com/me/repo");
    let refused = ok("link.now_read", json!({"t": "now.session", "body": bad}));
    assert_eq!(refused["refused"]["code"], "bad_frame");
    // ownerSaidAt, errors and overwrote only where the welcome listed
    // now.sessions2.
    let mut newer = body("now.session");
    newer["ownerSaidAt"] = json!(1_760_000_000_000_i64);
    let refused = ok(
        "link.now_read",
        json!({"t": "now.session", "body": newer.clone()}),
    );
    assert_eq!(refused["refused"]["code"], "bad_frame");
    let taken = ok(
        "link.now_read",
        json!({"t": "now.session", "body": newer, "sessions2": true}),
    );
    assert_eq!(taken["session"]["ownerSaidAt"], 1_760_000_000_000_i64);
    // choices only where it listed now.choices.
    let mut asked = body("now.session");
    asked["choices"] = json!(["push", "wait"]);
    let refused = ok(
        "link.now_read",
        json!({"t": "now.session", "body": asked.clone()}),
    );
    assert_eq!(refused["refused"]["code"], "bad_frame");
    let taken = ok(
        "link.now_read",
        json!({"t": "now.session", "body": asked, "choices": true}),
    );
    assert_eq!(taken["session"]["choices"], json!(["push", "wait"]));
    let (status, _) = ffi("link.now_read", &json!({"t": "moment", "body": {}}));
    assert_eq!(status, STATUS_BAD_INPUT);
}
