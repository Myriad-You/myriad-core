//! The shared conformance files in `crates/conformance/v1/nodes/` run here
//! through the crate, and through the C entry point in `myriad-core-ffi`;
//! another platform runs the same files through its own runtime. Below
//! them, the properties the files only sample: a fold that does not care
//! about order or repeats, forgetting in any order, and two sides revising
//! the same field.
//!
//! `MYRIAD_CONFORMANCE_BLESS=1 cargo test -p myriad-nodes` rewrites every
//! case's `output` after a deliberate change of the rules.

use std::path::PathBuf;

use myriad_persona_log::Row;
use myriad_persona_sync::canonical::canonical_bytes;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use super::*;

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
                audience: String,
                key: String,
            }
            let In { audience, key } = input(raw)?;
            json!({ "id": node_id(&audience, &key)? })
        }
        "nodes.repo_key" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                remote: String,
            }
            let In { remote } = input(raw)?;
            json!({ "key": repo_key(&remote)? })
        }
        "nodes.limits" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {}
            let In {} = input(raw)?;
            json_of(&limits::limits())
        }
        "nodes.fold" => json_of(&fold(input(raw)?)?),
        "nodes.place_input" => json_of(&prepare(input(raw)?)?),
        "nodes.place_read" => json_of(&read(input(raw)?)?),
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
    assert_eq!(files.len(), 7, "{files:?}");
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
        // nodes.body
        ("a field the body does not know", "unknown field"),
        ("a newer body version", "body version"),
        ("a kind that is not a node's", "not a node kind"),
        ("a kind this batch does not read", "this batch reads"),
        ("a repo key as a task", "kind does not fit the key"),
        (
            "a repo key without its object",
            "a repo key has a repo object",
        ),
        (
            "a repo object on a minted key",
            "a repo key has a repo object",
        ),
        ("a person key as a topic", "kind does not fit the key"),
        ("a key that is a local path", "normalized remote"),
        ("a kind change to a person", "never becomes a person"),
        ("a sentence without evidence", "because is empty"),
        ("a relation without evidence", "because is empty"),
        ("saidBy out of order", "saidBy is not sorted"),
        ("saidBy empty", "saidBy is empty"),
        (
            "a text with a space at its end",
            "starts or ends with a space",
        ),
        ("a name too long", "longer than 80"),
        ("evidence that is not typed", "not <type>:<id>"),
        ("evidence out of order", "because is not sorted"),
        ("a quote-only entry with refs", "quote-only"),
        ("an entry without refs", "quote-only"),
        ("a span that ends before it starts", "ends before it starts"),
        ("a trigger zone that is no zone", "trigger zone"),
        ("progress without a person", "person is given for progress"),
        ("a person on a name", "person is given for progress"),
        ("an item id with a colon", "item is 1 to 64"),
        ("taking back nothing", "base is empty"),
        (
            "a relation to a key that is not one",
            "key is <space>:<value>",
        ),
        ("a status that is not one", "does not fit its kind"),
        // nodes.fold
        ("a body for no row", "for no row"),
        ("two different rows with one id", "share an id"),
        ("a row without its epoch", "lease epoch"),
        // nodes.id
        ("a repo key not normalized", "normalized remote"),
        ("an audience that is not one", "audience is not"),
        ("a minted token with a space", "minted token"),
        ("a person with a comma", "no comma"),
        ("a key in no space", "key is <space>:<value>"),
        // nodes.repo_key
        ("a local path", "local path"),
        ("a home path", "local path"),
        ("a file remote", "local path"),
        ("a windows path", "local path"),
        ("a remote without a path", "no path"),
        ("a remote that climbs", "climbs"),
        ("an unknown scheme", "scheme"),
        // nodes.place_input, nodes.place_read
        ("no items", "no items"),
        ("item ids that repeat", "item ids repeat"),
        ("no items to place", "no items"),
        ("labels that repeat", "labels repeat"),
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
            for word in [
                "sample-repo",
                "Users",
                "demo",
                "一段",
                "长长",
                "abc123",
                "happy",
            ] {
                assert!(!error.contains(word), "{name} quotes its input: {error}");
            }
            seen += 1;
        }
    }
    assert_eq!(seen, reasons.len());
}

// What the files only sample.

const AUDIENCE: &str = "personal:7";
const KEY: &str = "minted:sample-task";

fn node() -> String {
    format!("node:{}", node_id(AUDIENCE, KEY).unwrap())
}

/// A shared row of `origin` about the sample node: hers, or the partner's
/// forget.
fn row(id: &str, origin: &str, dseq: u64, wall_ms: u64, kind: &str) -> Row {
    let mut value = json!({
        "version": 1,
        "id": id,
        "origin": origin,
        "author": "owner",
        "dseq": dseq,
        "hlc": format!("{wall_ms:015}-000000-{origin}"),
        "audience": AUDIENCE,
        "object": node(),
        "kind": kind,
        "replicate": "shared",
    });
    if kind != "forget" {
        value["author"] = json!("her");
        value["epoch"] = json!(1);
        value["payload"] = json!({ "ref": format!("body:{id}") });
    }
    serde_json::from_value(value).unwrap()
}

fn name(value: &str, base: &[&str]) -> Value {
    json!({
        "version": 1, "sync": "shared", "saidBy": ["her"],
        "base": base, "field": "name", "value": value,
    })
}

fn sample() -> Vec<(Row, Option<Value>)> {
    vec![
        (
            row("a1", "dev_mac", 1, 1_760_000_000_000, "node.created"),
            Some(json!({
                "version": 1, "sync": "shared", "saidBy": ["her"],
                "kind": "task", "key": KEY, "name": "发版",
            })),
        ),
        (
            row("a2", "dev_mac", 2, 1_760_000_001_000, "node.revised"),
            Some(name("这周发版", &["a1"])),
        ),
        (
            row("b1", "site", 1, 1_760_000_002_000, "node.revised"),
            Some(name("发 1.2 版", &["a1"])),
        ),
        (
            row("a3", "dev_mac", 3, 1_760_000_003_000, "node.span"),
            Some(json!({
                "version": 1, "sync": "shared", "saidBy": ["her", "partner"],
                "audience": AUDIENCE, "text": "伙伴说周五发。",
            })),
        ),
        (
            row("b2", "site", 2, 1_760_000_004_000, "node.kind_changed"),
            Some(json!({
                "version": 1, "sync": "shared", "saidBy": ["her"], "base": ["a1"],
                "kind": "topic", "reason": "没有做完的标准", "because": ["span:a3"],
            })),
        ),
    ]
}

fn fold_of(log: &[(Row, Option<Value>)]) -> Folded {
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
fn order_and_repeats_do_not_change_the_fold() {
    let log = sample();
    let expected = json_of(&fold_of(&log));
    // Every rotation, each with every row given twice, the second time
    // backwards.
    for turn in 0..log.len() {
        let mut turned = log.clone();
        turned.rotate_left(turn);
        let mut twice = turned.clone();
        twice.extend(turned.iter().rev().cloned());
        assert_eq!(json_of(&fold_of(&twice)), expected, "turn {turn}");
    }
    let folded = fold_of(&log);
    let node = &folded.nodes[0];
    let name = node.name.as_ref().unwrap();
    assert!(name.both_changed);
    assert_eq!(name.current.value, "这周发版");
    assert_eq!(name.rivals[0].value, "发 1.2 版");
    assert_eq!(node.kind.as_ref().unwrap().current.value.kind, Kind::Topic);
    assert_eq!(node.history[0].from, Some(Kind::Task));
    assert_eq!(node.id, node_id(AUDIENCE, KEY).unwrap());
}

#[test]
fn forgetting_absorbs_before_or_after_the_rows() {
    let mut log = sample();
    log.push((row("f1", "dev_mac", 4, 1_760_000_000_500, "forget"), None));
    let after = fold_of(&log);
    log.rotate_right(1);
    let before = fold_of(&log);
    assert_eq!(json_of(&after), json_of(&before));
    assert!(after.nodes[0].forgotten);
    assert_eq!(after.stubbed, ["a1", "a2", "a3", "b1", "b2"]);
}

#[test]
fn a_revision_that_saw_every_current_version_settles_it() {
    let mut log = sample();
    log.push((
        row("a4", "dev_mac", 4, 1_760_000_005_000, "node.revised"),
        Some(name("周五发 1.2 版", &["a2", "b1"])),
    ));
    let folded = fold_of(&log);
    let name = folded.nodes[0].name.as_ref().unwrap();
    assert!(!name.both_changed);
    assert_eq!(name.current.value, "周五发 1.2 版");
    assert_eq!(
        name.history
            .iter()
            .map(|version| version.row.as_str())
            .collect::<Vec<_>>(),
        ["a1", "a2", "b1"]
    );
}

#[test]
fn the_limits_cut_by_scalars() {
    assert_eq!(limits::cut("一二三", 3), "一二三");
    assert_eq!(limits::cut("一二三四", 3), "一二…");
    // A combining accent is a scalar of its own.
    assert_eq!(limits::cut("e\u{301}e\u{301}", 3), "e\u{301}…");
}
