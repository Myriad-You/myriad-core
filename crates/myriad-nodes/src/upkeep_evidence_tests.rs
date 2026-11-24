//! An answer whose evidence will not do is told by which kind, for a host
//! to count.

use serde_json::json;

use super::*;

const NODE: &str = "person:7";
const AUDIENCE: &str = "personal:7";

/// 「关于你」 with two entries, and seven inbox items resting on four grants
/// each: more evidence together than a row holds.
fn input() -> UpkeepInput {
    let mut rows = Vec::new();
    let mut bodies = json!({});
    for n in 1..=3 {
        let (kind, author, body) = match n {
            1 => (
                "node.created",
                "her",
                json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "key": NODE,
                    "kind": "person", "name": "关于你" }),
            ),
            _ => (
                "node.entry",
                "her",
                json!({ "version": 1, "sync": "shared", "saidBy": ["partner"],
                    "audience": AUDIENCE, "text": format!("第{n}次"),
                    "refs": [format!("memory:m{n}")] }),
            ),
        };
        rows.push(json!({
            "id": format!("r{n}"), "kind": kind, "author": author, "audience": AUDIENCE,
            "object": format!("node:{}", node_id(AUDIENCE, NODE).unwrap()),
            "origin": "dev_mac", "dseq": n, "version": 1, "epoch": 1,
            "hlc": format!("{:015}-000000-dev_mac", 1_760_000_000_000_u64 + n * 10),
            "payload": { "ref": format!("body:r{n}") }, "replicate": "shared",
        }));
        bodies[format!("r{n}")] = body;
    }
    let inbox: Vec<Value> = (1..=7)
        .map(|n| {
            let refs: Vec<String> = (1..=4).map(|k| format!("grant:g{n}x{k}")).collect();
            json!({ "id": format!("message:a{n}"), "saidBy": ["partner"],
                "text": format!("第{n}条"), "refs": refs })
        })
        .collect();
    serde_json::from_value(json!({
        "rows": rows, "bodies": bodies, "audience": AUDIENCE, "inbox": inbox,
        "node": node_id(AUDIENCE, NODE).unwrap(),
    }))
    .unwrap()
}

fn answered(actions: Value) -> UpkeepRead {
    read(ReadInput {
        input: input(),
        run: "run-1".into(),
        sync: Sync::Shared,
        output: json!({ "actions": actions }).to_string(),
    })
    .unwrap()
}

#[test]
fn evidence_that_will_not_do_is_told_by_which_kind() {
    let many: Vec<String> = (1..=7).map(|n| format!("i{n}")).collect();
    for (because, why) in [
        (None, NoEvidence::NotGiven),
        (Some(json!("e1")), NoEvidence::NotGiven),
        (Some(json!([])), NoEvidence::Empty),
        (Some(json!(["e1", "e99"])), NoEvidence::NotShown),
        (Some(json!([1])), NoEvidence::NotShown),
        (Some(json!(many)), NoEvidence::TooMany),
    ] {
        let mut action = json!({ "do": "open", "text": "约下次" });
        if let Some(because) = because {
            action["because"] = because;
        }
        let done = json!({ "do": "open", "text": "定路线", "because": ["e1"] });
        let read = answered(json!([done, action]));
        assert_eq!(read.fallback, Some(Fallback::NoEvidence), "{why:?}");
        assert_eq!(read.no_evidence, Some(why));
        assert_eq!(read.failed, Some(2));
        assert_eq!(read.failed_do.as_deref(), Some("open"));
    }
}

#[test]
fn nothing_is_told_of_evidence_that_will_do_nor_of_what_failed_otherwise() {
    for actions in [
        json!([{ "do": "open", "text": "约下次", "because": ["e1"] }]),
        json!([{ "do": "open", "because": ["e1"] }]),
        json!([{ "do": "她写的别的", "because": [] }]),
    ] {
        let read = answered(actions);
        assert_ne!(read.fallback, Some(Fallback::NoEvidence));
        assert_eq!((read.no_evidence, read.failed_do), (None, None));
    }
}
