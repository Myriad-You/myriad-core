//! `node.standing` and a rule's rows: the body, the fold, trust and recall.

use myriad_persona_log::{Row, row::Author};
use serde_json::{Value, json};

use crate::{
    Body, FoldInput, SaidBy, fold,
    fold::Why,
    node_id,
    recall::{PickInput, PickNode, pick},
    trust::{Tier, written_tier},
};

const AUDIENCE: &str = "personal:7";
const KEY: &str = "minted:sample-task";
const T: u64 = 1_760_000_000_000;

fn standing(extra: Value) -> Value {
    let mut body = json!({
        "version": 1, "sync": "shared", "saidBy": ["her"], "base": [],
        "item": "running", "text": "跑步：停了，膝盖半月板", "because": ["entry:e1"],
    });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    body
}

#[test]
fn a_standing_reads_in_one_byte_form() {
    let body = Body::read("node.standing", &standing(json!({ "since": "2026-10-18" }))).unwrap();
    let again = Body::read("node.standing", &body.to_value()).unwrap();
    assert_eq!(body.canonical_bytes(), again.canonical_bytes());
    // A moment, a month, a year all hold; ended is left out when false.
    for since in [
        json!(1_760_000_000_000_000_i64),
        json!("2026-10"),
        json!("2026"),
    ] {
        Body::read("node.standing", &standing(json!({ "since": since }))).unwrap();
    }
    let ended = Body::read("node.standing", &standing(json!({ "ended": false }))).unwrap();
    assert!(ended.to_value().get("ended").is_none());
}

#[test]
fn a_standing_that_does_not_hold_is_refused() {
    for (extra, why) in [
        (json!({ "since": "上个月" }), "since"),
        (json!({ "since": "2026-13" }), "since"),
        (json!({ "because": [] }), "because"),
        (json!({ "item": "有空格 的" }), "item"),
        (json!({ "text": "" }), "text"),
        (json!({ "seen": 1 }), "body: unknown field"),
    ] {
        let error = Body::read("node.standing", &standing(extra.clone())).unwrap_err();
        assert!(error.to_string().contains(why), "{extra}: {error}");
    }
}

fn row(id: &str, at: u64, kind: &str, author: &str) -> Row {
    let mut value = json!({
        "version": 1, "id": id, "origin": "site", "author": author, "dseq": at - T + 1,
        "hlc": format!("{at:015}-000000-site"), "audience": AUDIENCE,
        "object": format!("node:{}", node_id(AUDIENCE, KEY).unwrap()), "kind": kind,
        "payload": { "ref": format!("body:{id}") }, "replicate": "shared",
    });
    if author == "her" {
        value["epoch"] = json!(1);
    }
    serde_json::from_value(value).unwrap()
}

fn folded(log: &[(Row, Value)]) -> crate::Folded {
    fold(FoldInput {
        rows: log.iter().map(|(row, _)| row.clone()).collect(),
        bodies: log
            .iter()
            .map(|(row, body)| (row.id.clone(), body.clone()))
            .collect(),
        ledger: Vec::new(),
    })
    .unwrap()
}

fn created() -> Value {
    json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "kind": "task", "key": KEY, "name": "拆分" })
}

#[test]
fn a_new_standing_replaces_the_old_and_an_ended_one_stays_marked() {
    let log = vec![
        (row("a1", T, "node.created", "her"), created()),
        (
            row("a2", T + 1, "node.standing", "her"),
            standing(json!({})),
        ),
        (
            row("a3", T + 2, "node.standing", "her"),
            standing(json!({ "base": ["a2"], "text": "跑步：医生说可以慢跑了" })),
        ),
        (
            row("a4", T + 3, "node.standing", "her"),
            standing(json!({ "item": "shop", "text": "网店：关了", "ended": true })),
        ),
    ];
    let node = &folded(&log).nodes[0];
    let running = &node.standing["running"];
    assert_eq!(running.current.value.text, "跑步：医生说可以慢跑了");
    assert_eq!(running.history[0].row, "a2");
    assert!(!running.both_changed);
    assert!(node.standing["shop"].current.value.ended);
}

#[test]
fn a_rule_writes_how_things_stand_but_never_what_she_concludes() {
    let rule = |text: &str| {
        standing(
            json!({ "item": "now", "saidBy": ["third_party"], "text": text, "because": ["session:s1"] }),
        )
    };
    let done_when = json!({
        "version": 1, "sync": "shared", "saidBy": ["third_party"], "base": [],
        "text": "推送完就算做完", "because": ["session:s1"],
    });
    let log = vec![
        (row("a1", T, "node.created", "rule"), created()),
        (row("a2", T + 1, "node.standing", "rule"), rule("在等测试")),
        (row("a3", T + 2, "node.done_when", "rule"), done_when),
        (
            row("a4", T + 3, "node.kind_changed", "rule"),
            json!({
                "version": 1, "sync": "shared", "saidBy": ["third_party"], "base": ["a1"],
                "kind": "topic", "reason": "没有做完的标准", "because": ["session:s1"],
            }),
        ),
    ];
    let folded = folded(&log);
    let node = &folded.nodes[0];
    assert_eq!(node.standing["now"].current.author, Author::Rule);
    assert!(node.done_when.is_none());
    for id in ["a3", "a4"] {
        let skipped = folded
            .skipped
            .iter()
            .find(|skipped| skipped.id == id)
            .unwrap();
        assert_eq!(skipped.why, Why::AuthorMismatch, "{id}");
    }
    assert_eq!(
        node.kind.as_ref().unwrap().current.value.kind,
        crate::Kind::Task
    );
}

#[test]
fn a_rule_row_is_about_a_node_and_holds_no_lease() {
    let node_row = row("a1", T, "node.standing", "rule");
    node_row.check().unwrap();
    let mut leased = node_row.clone();
    leased.epoch = Some(1);
    assert!(
        leased
            .check()
            .unwrap_err()
            .to_string()
            .contains("lease epoch")
    );
    let mut forget = node_row.clone();
    forget.kind = "forget".into();
    forget.payload = None;
    assert!(
        forget
            .check()
            .unwrap_err()
            .to_string()
            .contains("only adds")
    );
    // Forgetting follows what an envelope names, whoever wrote it.
    let mut absorbed = node_row.clone();
    absorbed.kind = "node.absorbed".into();
    absorbed.to = Some("node:n2".into());
    let mut named = node_row.clone();
    named.to = Some("node:n2".into());
    let mut takes = node_row.clone();
    takes.takes = vec!["a0".into()];
    for refused in [absorbed, named, takes] {
        assert!(
            refused
                .check()
                .unwrap_err()
                .to_string()
                .contains("names no other node"),
            "{}",
            refused.kind
        );
    }
    let mut memory = node_row;
    memory.kind = "memory.write".into();
    memory.object = "memory:m1".into();
    assert!(
        memory
            .check()
            .unwrap_err()
            .to_string()
            .contains("about a node")
    );
}

#[test]
fn what_a_rule_wrote_is_only_seen_until_the_person_answers() {
    use crate::trust::{Answer, Provenance};
    let rule = Some(Author::Rule);
    assert_eq!(
        written_tier(rule, &[SaidBy::Partner], None, true),
        Some(Tier::Seen)
    );
    let typed = Provenance {
        typed: true,
        ..Default::default()
    };
    assert_eq!(
        written_tier(rule, &[SaidBy::Partner], Some(&typed), true),
        Some(Tier::Seen)
    );
    let answered = |answer| Provenance {
        answer: Some(answer),
        ..Default::default()
    };
    let confirmed = answered(Answer::Confirmed);
    assert_eq!(
        written_tier(rule, &[SaidBy::ThirdParty], Some(&confirmed), true),
        Some(Tier::Counts)
    );
    let denied = answered(Answer::Denied);
    assert_eq!(
        written_tier(rule, &[SaidBy::Partner], Some(&denied), true),
        None
    );
    let owner = Some(Author::Owner);
    assert_eq!(
        written_tier(owner, &[SaidBy::Partner], None, true),
        Some(Tier::Counts)
    );
    assert_eq!(
        written_tier(None, &[SaidBy::Her], None, false),
        Some(Tier::Counts)
    );
}

#[test]
fn recall_puts_how_things_stand_first_and_leaves_what_ended_out() {
    let node = |id: &str, standing: Value| -> PickNode {
        serde_json::from_value(json!({
            "id": id,
            "entries": [{ "id": format!("{id}-e1"), "at": 1 }, { "id": format!("{id}-e2"), "at": 2 }],
            "standing": standing,
        }))
        .unwrap()
    };
    let picked = pick(PickInput {
        ranked: serde_json::from_value(json!([{ "id": "n1-e1", "named": true }])).unwrap(),
        nodes: vec![
            node(
                "n1",
                json!([
                    { "item": "old", "at": 1 },
                    { "item": "new", "at": 5 },
                    { "item": "gone", "at": 9, "ended": true },
                ]),
            ),
            node("me", json!([{ "item": "home", "at": 3 }])),
        ],
        about_them: Some("me".into()),
        her: None,
        thorough: false,
        budget: Default::default(),
    })
    .unwrap();
    assert_eq!(picked.picked[0].standing, ["new", "old"]);
    // They take no entry's place.
    assert_eq!(picked.picked[0].entries, ["n1-e1", "n1-e2"]);
    assert_eq!(picked.about_them_standing, ["home"]);
    // Without any, the output is as it was.
    let none = pick(PickInput {
        ranked: Vec::new(),
        nodes: vec![node("n1", json!([]))],
        about_them: None,
        her: None,
        thorough: false,
        budget: Default::default(),
    })
    .unwrap();
    assert!(
        serde_json::to_value(&none)
            .unwrap()
            .get("aboutThemStanding")
            .is_none()
    );
}
