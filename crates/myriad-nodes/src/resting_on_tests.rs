use serde_json::json;

use super::*;

#[test]
fn a_slot_is_a_row_without_its_writer() {
    for (row, of) in [
        ("p-5f1c@dev_mac", Some("p-5f1c")),
        ("p-5f1c@dev_mac~12", Some("p-5f1c")),
        // A writer is whatever stands there, a tilde in it too.
        ("p-5f1c@dev~mac", Some("p-5f1c")),
        // An event raised again, in a row and as a slot.
        (
            "e-evt@1760000000000000@dev_mac~1",
            Some("e-evt@1760000000000000"),
        ),
        ("e-evt@1760000000000000", None),
        ("u6b99123b49c5c95ea8357268", None),
        ("h-9c21", None),
        ("@dev_mac", None),
        ("p-5f1c@", None),
        ("p-5f1c@~2", None),
    ] {
        assert_eq!(slot(row), of, "{row}");
    }
}

fn input(rows: &[(&str, &Value)], gone: &[&str]) -> RestingOnInput {
    serde_json::from_value(json!({
        "rows": rows.iter().enumerate().map(|(n, (id, _))| json!({
            "version": 1, "id": id, "origin": "dev_mac", "author": "her", "epoch": 1,
            "dseq": n + 1, "hlc": format!("{:015}-000000-dev_mac", 1_760_000_000_000_u64 + n as u64),
            "audience": "personal:local-owner",
            "object": "node:4b50145fec289d687982b9ef9d33bbf7",
            "kind": INTENT, "replicate": "shared", "payload": { "ref": format!("body:{id}") },
        })).collect::<Vec<_>>(),
        "bodies": rows.iter().map(|(id, body)| (id.to_string(), (*body).clone()))
            .collect::<BTreeMap<_, _>>(),
        "ledger": [],
        "gone": gone,
    }))
    .unwrap()
}

/// Every intent her upkeep writes in the conformance cases, a hand she
/// raised or one carried on (`m…`) when its node went into another, goes
/// with each of the spans and entries it rests on, and with nothing else.
#[test]
fn what_upkeep_writes_goes_with_what_it_rests_on() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../conformance/v2/nodes/upkeep_read.json"
    );
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let mut hands = 0;
    for case in file["cases"].as_array().unwrap() {
        for write in case["output"]["writes"].as_array().into_iter().flatten() {
            if write["kind"] != INTENT {
                continue;
            }
            let body = &write["body"];
            let with = |gone: &str| resting_on(input(&[("u1", body)], &[gone])).unwrap().intents;
            assert!(with("another-row").is_empty(), "{}", case["name"]);
            for ground in body["because"].as_array().unwrap() {
                let ground = ground.as_str().unwrap();
                let Some(row) = ground
                    .strip_prefix(SPAN_REF)
                    .or_else(|| ground.strip_prefix(ENTRY_REF))
                else {
                    continue;
                };
                assert_eq!(with(row), ["u1"], "{} › {ground}", case["name"]);
                hands += 1;
            }
        }
    }
    assert!(hands > 0);
}

/// What is left after the rows listed go stands as it did, however the
/// versions of one intent are written on each other: every way four may be.
#[test]
fn what_stands_after_is_what_stood_or_nothing() {
    let ids = ["u1", "u2", "u3", "u4"];
    let mut kept = 0;
    // A base per later version (any of those before it) and who rests on it.
    for shape in 0..(2 * 4 * 8) {
        let (second, third, fourth) = (shape & 1, (shape >> 1) & 3, shape >> 3);
        for rests in 1..16_usize {
            let bodies: Vec<Value> = [0, second, third, fourth]
                .iter()
                .enumerate()
                .map(|(n, named)| {
                    let base: Vec<&str> = (0..n)
                        .filter(|at| named >> at & 1 == 1)
                        .map(|at| ids[at])
                        .collect();
                    let ground = if rests >> n & 1 == 1 {
                        "span:s1"
                    } else {
                        "message:9"
                    };
                    json!({
                        "version": 1, "sync": "shared", "saidBy": ["her"], "base": base,
                        "intent": "h1", "text": format!("一段{n}"), "because": [ground],
                        "status": "waiting",
                    })
                })
                .collect();
            let all: Vec<(&str, &Value)> = ids.into_iter().zip(&bodies).collect();
            let stands = |rows: &[(&str, &Value)]| {
                let input = input(rows, &[]);
                standing(&input.rows.iter().collect::<Vec<_>>(), &input.bodies).unwrap()
            };
            let before = stands(&all);
            let going = resting_on(input(&all, &["s1"])).unwrap().intents;
            let left: Vec<(&str, &Value)> = all
                .iter()
                .copied()
                .filter(|(id, _)| !going.iter().any(|gone| gone == id))
                .collect();
            for (n, id) in ids.iter().enumerate() {
                assert!(rests >> n & 1 == 0 || going.iter().any(|gone| gone == id));
            }
            let rested = ids
                .iter()
                .enumerate()
                .any(|(n, id)| rests >> n & 1 == 1 && before.contains(*id));
            if rested {
                assert!(left.is_empty(), "{shape} {rests}");
            } else {
                assert_eq!(stands(&left), before, "{shape} {rests}");
                kept += 1;
            }
        }
    }
    assert!(kept > 0);
}

/// The partner's words are never quoted in a refusal.
#[test]
fn a_refusal_names_the_list_not_what_is_in_it() {
    let mut words = input(&[], &[]);
    words.messages = vec!["一段\n原话".into()];
    let error = resting_on(words).unwrap_err().to_string();
    assert_eq!(error, "messages holds a control character");
}
