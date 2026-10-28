//! The shared conformance files in `crates/conformance/v1/nodes/` run here
//! through the crate, and through the C entry point in `myriad-core-ffi`;
//! another platform runs the same files through its own runtime. Below
//! them, the properties the files only sample: a fold that does not care
//! about order or repeats, forgetting in any order, and the server and a
//! computer writing the same node.
//!
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-nodes` rewrites every
//! case's `output` after a deliberate change of the rules.

use std::path::PathBuf;

use myriad_persona_log::Row;
use myriad_persona_sync::canonical::canonical_bytes;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use super::*;
use crate::fold::Why;

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/nodes")
}

fn blessing() -> bool {
    std::env::var_os("MYRIAD_CONFORMANCE_BLESS").is_some()
}

fn input<T: DeserializeOwned>(value: Value) -> Result<T, NodeError> {
    serde_json::from_value(value).map_err(|error| NodeError::new(error.to_string()))
}

fn json_of<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("serializes")
}

/// One call as the shared core answers it, without the C ABI.
fn run(call: &str, raw: Value) -> Result<Value, NodeError> {
    Ok(match call {
        "nodes.body" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                kind: String,
                body: Value,
            }
            let In { kind, body } = input(raw)?;
            let body = Body::read(&kind, &body)?;
            json!({
                "body": body.to_value(),
                "canonical": String::from_utf8(body.canonical_bytes()).expect("UTF-8"),
                "digest": body.digest(),
            })
        }
        "nodes.id" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                owner: String,
                #[serde(default)]
                audience: Option<String>,
                key: String,
            }
            let In {
                owner,
                audience,
                key,
            } = input(raw)?;
            let audience = audience.unwrap_or_else(|| id::private(&owner));
            json!({ "id": node_id(&owner, &audience, &key)? })
        }
        "nodes.place" => match locate(input(raw)?)? {
            Some(place) => json!({ "place": place }),
            None => json!({}),
        },
        "nodes.file" => json_of(&file(input(raw)?)?),
        "nodes.said" => json!({ "rows": said(input(raw)?)? }),
        "nodes.fold" => json_of(&fold(input(raw)?)?),
        other => return Err(NodeError::new(format!("no call {other}"))),
    })
}

fn refused() -> Value {
    json!({ "error": "bad_input" })
}

fn same(one: &Value, other: &Value) -> bool {
    canonical_bytes(one) == canonical_bytes(other)
}

fn case_files() -> Vec<PathBuf> {
    let dir = conformance();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
}

#[test]
fn every_conformance_case_answers_as_recorded() {
    let files = case_files();
    assert_eq!(files.len(), 6, "{files:?}");
    for path in files {
        let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(file["version"], 1, "{}", path.display());
        let call = file["call"].as_str().unwrap().to_string();
        let mut changed = false;
        for case in file["cases"].as_array_mut().unwrap() {
            let name = case["name"].as_str().unwrap().to_string();
            let got = run(&call, case["input"].clone()).unwrap_or_else(|_| refused());
            if blessing() {
                changed |= case.get("output") != Some(&got);
                case["output"] = got;
                continue;
            }
            assert!(
                same(&case["output"], &got),
                "{} › {name}\nexpected {}\n     got {}",
                path.display(),
                case["output"],
                got
            );
        }
        if changed {
            let mut text = serde_json::to_string_pretty(&file).unwrap();
            text.push('\n');
            std::fs::write(&path, text).unwrap();
        }
    }
}

/// The files only say `bad_input`; each refused case is refused for the
/// reason its name gives, and no message holds a value from its input.
#[test]
fn every_refused_case_is_refused_for_its_own_reason() {
    let reasons = [
        ("a field the body does not know", "unknown field"),
        ("an empty line", "line is empty"),
        ("a newer body version", "body version"),
        ("an action in capitals", "action is lowercase"),
        ("a kind that is not a node's", "not a node kind"),
        ("a key with a space at its end", "key starts or ends"),
        ("an owner with a comma", "owner holds a comma"),
        ("a kind her link does not carry", "not a moment"),
        ("too few ids", "too few ids"),
        ("the node given is another", "another node"),
        ("both a place and a kind", "give a place"),
        (
            "a together key as a repository",
            "together key is a together node",
        ),
        ("a body for no row", "for no row"),
        ("ids that repeat", "ids repeat"),
    ];
    let mut seen = 0;
    for path in case_files() {
        let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap();
        for case in file["cases"].as_array().unwrap() {
            if case["output"] != refused() {
                continue;
            }
            let name = case["name"].as_str().unwrap();
            let (_, reason) = reasons
                .iter()
                .find(|(named, _)| *named == name)
                .unwrap_or_else(|| panic!("no reason listed for {name:?}"));
            let error = run(call, case["input"].clone()).unwrap_err().to_string();
            assert!(error.contains(reason), "{name}: {error}");
            for word in ["sample-repo", "它", "Shout", "o,1"] {
                assert!(!error.contains(word), "{name} quotes its input: {error}");
            }
            seen += 1;
        }
    }
    assert_eq!(seen, reasons.len());
}

// What the files only sample.

const OWNER: &str = "7";

fn rev(wall_ms: u64, node: &str) -> String {
    format!("{wall_ms:015}-000000-{node}")
}

/// A shared node row of `origin`, its body beside it.
fn row(id: &str, origin: &str, dseq: u64, wall_ms: u64, kind: &str, node: &str) -> Row {
    let author = if kind == "node.opened" || kind == "forget" {
        "owner"
    } else {
        "her"
    };
    let mut value = json!({
        "version": 1,
        "id": id,
        "origin": origin,
        "author": author,
        "dseq": dseq,
        "hlc": rev(wall_ms, origin),
        "audience": id::private(OWNER),
        "subject": OWNER,
        "object": node,
        "kind": kind,
        "replicate": "shared",
    });
    if author == "her" {
        value["epoch"] = json!(1);
    }
    if kind != "forget" {
        value["payload"] = json!({ "ref": format!("body:{id}") });
    }
    serde_json::from_value(value).unwrap()
}

fn decision(event: &str, action: &str, line: Option<&str>) -> Value {
    json!({ "event": event, "happened": "waiting", "action": action, "reason": "asked_before", "line": line })
}

/// Files one decision the way a side does: fold what it holds, file, and
/// stamp the drafts as its own rows.
fn write(
    log: &mut Vec<(Row, Option<Value>)>,
    origin: &str,
    wall_ms: u64,
    decision: Value,
) -> Filed {
    let folded = fold_log(log);
    let place = locate(
        input(json!({
            "owner": OWNER,
            "kind": id::PEER_AGENT,
            "safeFacts": { "repo": "sample-repo", "happened": "waiting" },
        }))
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    let node = folded.nodes.into_iter().find(|node| node.id == place.id);
    let base = log.len();
    let filed = file(input(json!({
        "owner": OWNER,
        "kind": id::PEER_AGENT,
        "safeFacts": { "repo": "sample-repo", "happened": "waiting" },
        "node": node,
        "decision": decision,
        "ids": [format!("{origin}-{base}"), format!("{origin}-{}", base + 1), format!("{origin}-{}", base + 2)],
    }))
    .unwrap())
    .unwrap();
    let dseq = log.iter().filter(|(row, _)| row.origin == origin).count() as u64;
    for (offset, draft) in filed.rows.iter().enumerate() {
        let mut stamped = row(
            &draft.id,
            origin,
            dseq + offset as u64 + 1,
            wall_ms,
            &draft.kind,
            &draft.object,
        );
        stamped.hlc.counter = offset as u32;
        log.push((stamped, Some(draft.body.clone())));
    }
    filed
}

fn fold_log(log: &[(Row, Option<Value>)]) -> Folded {
    fold(FoldInput {
        rows: log.iter().map(|(row, _)| row.clone()).collect(),
        bodies: log
            .iter()
            .filter_map(|(row, body)| body.clone().map(|body| (row.id.clone(), body)))
            .collect(),
    })
    .unwrap()
}

#[test]
fn the_server_and_a_computer_write_the_same_node() {
    // The pen on the Mac, then on the server: one node, one id, the bytes
    // each side wrote folding the same way on either.
    let mut log = Vec::new();
    let first = write(
        &mut log,
        "dev_mac",
        1_760_000_000_000,
        decision("evt_a", "speak", Some(" 它在等你回答。 ")),
    );
    assert_eq!(
        first
            .rows
            .iter()
            .map(|draft| draft.kind.as_str())
            .collect::<Vec<_>>(),
        ["node.created", "node.decided", "node.raised"]
    );
    let second = write(
        &mut log,
        "site",
        1_760_000_060_000,
        decision("evt_b", "remember", Some("他喜欢先跑测试。")),
    );
    assert_eq!(
        second
            .rows
            .iter()
            .map(|draft| draft.kind.as_str())
            .collect::<Vec<_>>(),
        ["node.decided", "node.noted"]
    );
    let again = write(
        &mut log,
        "site",
        1_760_000_120_000,
        decision("evt_a", "speak", Some("again")),
    );
    assert!(again.already && again.rows.is_empty());
    let nodes = fold_log(&log).nodes;
    assert_eq!(nodes.len(), 1);
    let node = &nodes[0];
    assert_eq!(node.key.as_deref(), Some("sample-repo"));
    assert_eq!(node.id, id::digest("agent_node\u{1f}7\u{1f}sample-repo"));
    let moments: Vec<(&str, &str, bool)> = node
        .moments
        .iter()
        .map(|moment| (moment.event.as_str(), moment.line.as_str(), moment.said))
        .collect();
    assert_eq!(
        moments,
        [
            ("evt_a", "它在等你回答。", false),
            ("evt_b", "他喜欢先跑测试。", true)
        ]
    );
    assert_eq!(node.refs.len(), 2);
    assert_eq!(node.created_at, Some(1_760_000_000_000_000));
    assert_eq!(node.updated_at, Some(1_760_000_060_000_000));
}

#[test]
fn a_fold_does_not_care_about_order_or_repeats() {
    let mut log = Vec::new();
    write(
        &mut log,
        "dev_mac",
        1_760_000_000_000,
        decision("evt_a", "speak", Some("一")),
    );
    write(
        &mut log,
        "site",
        1_760_000_001_000,
        decision("evt_b", "ask", Some("二")),
    );
    write(
        &mut log,
        "dev_mac",
        1_760_000_002_000,
        decision("evt_c", "ignore", None),
    );
    let once = fold_log(&log);
    let mut shuffled = log.clone();
    shuffled.reverse();
    shuffled.rotate_left(2);
    shuffled.extend(log.iter().cloned());
    assert_eq!(fold_log(&shuffled), once);
}

#[test]
fn a_forgotten_node_reads_back_as_nothing_whichever_came_first() {
    let mut log = Vec::new();
    write(
        &mut log,
        "dev_mac",
        1_760_000_000_000,
        decision("evt_a", "speak", Some("一")),
    );
    let node = log[0].0.object.clone();
    let forget = row("f-1", "site", 1, 1_759_000_000_000, "forget", &node);
    let mut before = vec![(forget.clone(), None)];
    before.extend(log.iter().cloned());
    let mut after = log.clone();
    after.push((forget, None));
    for log in [before, after] {
        let folded = fold_log(&log);
        assert!(folded.nodes[0].forgotten && folded.nodes[0].moments.is_empty());
        assert_eq!(folded.stubbed.len(), 3);
        // Nothing more is filed under it.
        let filed = write(
            &mut log.clone(),
            "site",
            1_760_000_100_000,
            decision("evt_b", "speak", Some("二")),
        );
        assert!(filed.forgotten && filed.rows.is_empty());
    }
}

#[test]
fn a_forgotten_piece_of_evidence_takes_its_rows_and_leaves_the_rest() {
    let mut log = Vec::new();
    write(
        &mut log,
        "dev_mac",
        1_760_000_000_000,
        decision("evt_a", "speak", Some("一")),
    );
    write(
        &mut log,
        "dev_mac",
        1_760_000_001_000,
        decision("evt_b", "remember", Some("二")),
    );
    let forget = row(
        "f-1",
        "dev_mac",
        6,
        1_760_000_002_000,
        "forget",
        "event:evt_a",
    );
    log.push((forget, None));
    let folded = fold_log(&log);
    let node = &folded.nodes[0];
    assert!(!node.forgotten);
    assert_eq!(node.moments.len(), 1);
    assert_eq!(node.moments[0].event, "evt_b");
    assert_eq!(
        node.refs.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["event:evt_b"]
    );
    // decided and raised of evt_a.
    assert_eq!(folded.stubbed, ["dev_mac-1", "dev_mac-2"]);
    assert_eq!(node.updated_at, Some(1_760_000_001_000_000));
    // The same, once the writing side made them stubs itself.
    let stubbed: Vec<(Row, Option<Value>)> = log
        .into_iter()
        .map(|(row, body)| {
            if folded.stubbed.contains(&row.id) {
                (row.stub(), None)
            } else {
                (row, body)
            }
        })
        .collect();
    let again = fold_log(&stubbed);
    assert_eq!(again.nodes, folded.nodes);
    assert!(again.stubbed.is_empty() && again.skipped.is_empty());
}

#[test]
fn a_row_without_its_body_is_passed_by_and_still_counts_as_a_change() {
    let mut log = Vec::new();
    write(
        &mut log,
        "dev_mac",
        1_760_000_000_000,
        decision("evt_a", "speak", Some("一")),
    );
    log[2].1 = None;
    let folded = fold_log(&log);
    assert_eq!(folded.skipped.len(), 1);
    assert_eq!(folded.skipped[0].why, Why::NoBody);
    assert!(folded.nodes[0].moments.is_empty());
}
