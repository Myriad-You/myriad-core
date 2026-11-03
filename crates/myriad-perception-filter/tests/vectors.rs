//! The consistency vectors every client runs (fixtures/host-filter
//! `pages.json`, `others-words.json`, `terminal.json`, `secret-values.json`):
//! here straight from the crate, in `myriad-core-ffi` through the C entry point.

use myriad_perception_filter::{
    Classify, Decision, PageKind, Policy, Private, Relay, Sources, decide, decide_with, page,
    secrets,
};
use serde::Deserialize;
use serde_json::{Value, json};

fn fixture(name: &str) -> Value {
    let path = format!(
        "{}/fixtures/host-filter/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageCase {
    id: String,
    url_host: String,
    path: Option<String>,
    title: Option<String>,
    #[serde(default)]
    password_field: bool,
    market: Option<String>,
    /// `filter.decide`'s `private` for this case; `false` when not given.
    #[serde(default)]
    private: Option<Value>,
    expect: Value,
    decide: Value,
}

fn name<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn pages_place_and_decide_as_the_vectors_say() {
    let doc = fixture("pages");
    let table: Sources = serde_json::from_value(doc["table"].clone()).unwrap();
    let cases: Vec<PageCase> = serde_json::from_value(doc["cases"].clone()).unwrap();
    assert!(cases.len() >= 20);
    for case in cases {
        let placed = page(
            &case.url_host,
            case.path.as_deref(),
            case.title.as_deref(),
            case.password_field,
            case.market.as_deref(),
            &table,
        );
        let got = json!({
            "read": placed.read(),
            "page": name(placed.kind),
            "basis": name(placed.basis),
            "kind": name(placed.source),
            "category": placed.category,
            "othersWords": placed.others_words(),
        });
        assert_eq!(got, case.expect, "{}", case.id);

        let messaging = placed.kind == PageKind::Messaging;
        let decision = Decision {
            capture: Some("seen".into()),
            window: Some("seen".into()),
            password_field: Some(case.password_field),
            identifiers: Some(vec![]),
            source: Some(name(placed.source).as_str().map(str::to_string)),
            page: name(placed.kind).as_str().map(str::to_string),
            private: Some(match &case.private {
                Some(Value::String(said)) => Private::Other(said.clone()),
                Some(Value::Bool(said)) => Private::Known(*said),
                _ => Private::Known(false),
            }),
            text_hits: Some(vec![]),
            model: Some("available".into()),
            language: Some("supported".into()),
            relay: Some(messaging.then(|| Relay {
                result: "ok".into(),
                category: Some("plans".into()),
            })),
            classify: Some((!messaging).then(|| Classify {
                general: Some("not_sensitive".into()),
                tagging: Some("not_sensitive".into()),
                kind: None,
            })),
            others_text: None,
        };
        let decided = name(decide(&decision));
        for field in ["disposition", "kind", "reason", "leaves"] {
            assert_eq!(
                decided[field],
                case.decide.get(field).cloned().unwrap_or(Value::Null),
                "{} {field}",
                case.id
            );
        }
        // A page not read leaves nothing, not even the marker.
        if !placed.read() {
            assert_eq!(decided["leaves"], "nothing", "{}", case.id);
        }
    }
}

/// `filter.decide` with and without `othersWordsLeave`: `base` and the
/// case's `with` together are the call's input.
#[test]
fn others_words_leave_as_the_vectors_say() {
    let doc = fixture("others-words");
    for case in doc["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let mut input = doc["base"].clone();
        for (field, value) in case["with"].as_object().unwrap() {
            input[field] = value.clone();
        }
        let leave = input
            .as_object_mut()
            .unwrap()
            .remove("othersWordsLeave")
            .map(|on| on.as_bool().unwrap());
        let decision: Decision = serde_json::from_value(input).unwrap();
        let policy = Policy {
            others_words_leave: leave.unwrap_or(Policy::CURRENT.others_words_leave),
            ..Policy::CURRENT
        };
        let decided = name(decide_with(&decision, &policy));
        assert_eq!(decided, case["expect"], "{id}");
        if leave.is_none() {
            assert_eq!(name(decide(&decision)), decided, "{id}");
        }
    }
}

#[test]
fn terminal_credentials_are_wiped_as_the_vectors_say() {
    let doc = fixture("terminal");
    for case in doc["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let scan = secrets::scan(text);
        assert_eq!(name(&scan.wiped), case["wiped"], "{id}: {}", scan.text);
        for gone in case["gone"].as_array().unwrap() {
            assert!(!scan.text.contains(gone.as_str().unwrap()), "{id}");
        }
        for kept in case["kept"].as_array().unwrap() {
            assert!(
                scan.text.contains(kept.as_str().unwrap()),
                "{id}: {}",
                scan.text
            );
        }
        if case["wiped"].as_array().unwrap().is_empty() {
            assert_eq!(scan.text, text, "{id}");
        }
    }
}

#[test]
fn secret_values_are_where_the_vectors_say() {
    let doc = fixture("secret-values");
    for case in doc["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let text = case["text"].as_str().unwrap();
        let found: Vec<Value> = secrets::values(text)
            .into_iter()
            .map(|found| {
                json!({
                    "kind": found.kind,
                    "value": found.value,
                    "start": found.start,
                    "end": found.end,
                })
            })
            .collect();
        assert_eq!(json!(found), case["found"], "{id}");
        assert_eq!(secrets::scan(text).text, case["scanned"], "{id}");
    }
}
