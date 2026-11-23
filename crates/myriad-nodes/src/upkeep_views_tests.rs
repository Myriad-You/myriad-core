//! A view that shows a memory row is hers to see in her upkeep, not to
//! change: the row is what holds, and the host keeps it.

use serde_json::json;

use super::*;

const NODE: &str = "person:7";
const AUDIENCE: &str = "personal:7";

fn row(dseq: u64, kind: &str, author: &str) -> Value {
    json!({
        "id": format!("r{dseq}"), "kind": kind, "author": author, "audience": AUDIENCE,
        "object": format!("node:{}", node_id(AUDIENCE, NODE).unwrap()),
        "origin": "dev_mac", "dseq": dseq, "epoch": 1, "version": 1,
        "hlc": format!("{:015}-000000-dev_mac", 1_760_000_000_000_u64 + dseq * 10),
        "payload": { "ref": format!("body:r{dseq}") }, "replicate": "shared",
    })
}

fn view(text: &str, base: &[&str], because: &str, said_by: &str) -> Value {
    json!({
        "version": 1, "sync": "shared", "saidBy": [said_by], "base": base,
        "view": "us", "text": text, "because": [because],
    })
}

/// 「关于你」 with one entry (`e1`) and what `views` writes to its `us`
/// (`v1`): the row's dseq, its author, its body.
fn input(views: &[(u64, &str, Value)]) -> UpkeepInput {
    let mut rows = vec![row(1, "node.created", "her"), row(2, "node.entry", "her")];
    let mut bodies = json!({
        "r1": { "version": 1, "sync": "shared", "saidBy": ["her"], "key": NODE,
                "kind": "person", "name": "关于你" },
        "r2": { "version": 1, "sync": "shared", "saidBy": ["partner"], "audience": AUDIENCE,
                "text": "喜欢慢慢走。", "refs": ["memory:m9"] },
    });
    for (dseq, author, body) in views {
        rows.push(row(*dseq, "node.view_changed", author));
        bodies[format!("r{dseq}")] = body.clone();
    }
    serde_json::from_value(json!({
        "rows": rows, "bodies": bodies, "audience": AUDIENCE,
        "node": node_id(AUDIENCE, NODE).unwrap(),
    }))
    .unwrap()
}

fn answered(input: UpkeepInput, actions: Value) -> UpkeepRead {
    read(ReadInput {
        input,
        run: "run-1".into(),
        sync: Sync::Shared,
        output: json!({ "actions": actions }).to_string(),
    })
    .unwrap()
}

fn views_written(read: &UpkeepRead) -> Vec<&Value> {
    read.writes
        .iter()
        .filter(|write| write.kind == "node.view_changed")
        .map(|write| &write.body)
        .collect()
}

#[test]
fn a_view_that_shows_a_memory_row_is_shown_so_and_never_changed() {
    let hung = || {
        input(&[(
            3,
            "her",
            view("像老朋友一样松弛。", &[], "memory:m1", "her"),
        )])
    };
    let shown = prepare(hung()).unwrap().node.unwrap().views;
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].text, "像老朋友一样松弛。");
    assert!(shown[0].kept_elsewhere);

    // On evidence that counts, a change of any other view would go through.
    let read = answered(
        hung(),
        json!([
            { "do": "view", "view": "v1", "text": "最近有点生分。", "because": ["e1"] },
            { "do": "view", "text": "走路不爱赶。", "because": ["e1"] },
        ]),
    );
    assert_eq!(read.fallback, None);
    assert_eq!(
        read.held,
        vec![Held {
            action: 1,
            why: HeldWhy::KeptElsewhere
        }]
    );
    // What she sees herself still stands, as a view of its own.
    let written = views_written(&read);
    assert_eq!(written.len(), 1);
    assert_ne!(written[0]["view"], "us");
    assert_eq!(written[0]["text"], "走路不爱赶。");
}

#[test]
fn a_view_changed_on_both_sides_is_kept_elsewhere_when_either_shows_a_row() {
    for (one, other) in [("memory:m1", "entry:r2"), ("entry:r2", "memory:m1")] {
        let both = input(&[
            (3, "her", view("像老朋友一样松弛。", &[], one, "her")),
            (4, "her", view("最近有点生分。", &[], other, "her")),
        ]);
        let read = answered(
            both,
            json!([{ "do": "view", "view": "v1", "text": "又熟起来了。", "because": ["e1"] }]),
        );
        assert_eq!(read.held[0].why, HeldWhy::KeptElsewhere, "{one} {other}");
        assert!(views_written(&read).is_empty());
    }
}

/// Only what stands now decides: a view of hers that once showed a row, and
/// no longer does, is hers again; and the partner's word comes first.
#[test]
fn only_the_current_version_decides_and_the_partner_comes_first() {
    let change = json!([{ "do": "view", "view": "v1", "text": "又熟起来了。", "because": ["e1"] }]);
    let hung = (
        3,
        "her",
        view("像老朋友一样松弛。", &[], "memory:m1", "her"),
    );
    let hers = input(&[
        hung.clone(),
        (4, "her", view("最近有点生分。", &["r3"], "entry:r2", "her")),
    ]);
    assert!(!prepare(hers.clone()).unwrap().node.unwrap().views[0].kept_elsewhere);
    let read = answered(hers, change.clone());
    assert!(read.held.is_empty());
    assert_eq!(views_written(&read).len(), 1);

    let theirs = input(&[
        hung,
        (
            4,
            "owner",
            view("别说生分。", &["r3"], "memory:m1", "partner"),
        ),
    ]);
    assert_eq!(answered(theirs, change).held[0].why, HeldWhy::PartnerSet);
}
