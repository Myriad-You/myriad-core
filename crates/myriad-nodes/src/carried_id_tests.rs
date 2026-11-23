//! The public id of what goes on when a node goes into another
//! (`nodes.carried_id`) is the one her upkeep writes.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::upkeep::{Carried, carried_id};

#[test]
fn what_upkeep_carries_on_has_the_public_id() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../conformance/v2/nodes/upkeep_read.json"
    );
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut seen = BTreeSet::new();
    for case in file["cases"].as_array().unwrap() {
        let writes = case["output"]["writes"].as_array();
        let Some(writes) =
            writes.filter(|writes| writes.iter().any(|write| write["kind"] == "node.absorbed"))
        else {
            continue;
        };
        let given = &case["input"]["input"];
        let from = given["node"].as_str().unwrap();
        for write in writes {
            let (kind, name, carried) = match write["kind"].as_str().unwrap() {
                "node.intent" => ("node.intent", "intent", Carried::Intent),
                "node.open_item" => ("node.open_item", "item", Carried::Open),
                "node.standing" => ("node.standing", "item", Carried::Standing),
                _ => continue,
            };
            // Only what is written on another node is carried on.
            if write["node"] == from {
                continue;
            }
            let rows = given["rows"].as_array().unwrap();
            let made = rows
                .iter()
                .filter(|row| row["kind"] == kind)
                .filter_map(|row| given["bodies"][row["id"].as_str()?][name].as_str())
                .any(|item| write["body"][name] == carried_id(carried, from, item));
            assert!(made, "{}", case["name"]);
            seen.insert(kind);
        }
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        ["node.intent", "node.open_item"]
    );
}
