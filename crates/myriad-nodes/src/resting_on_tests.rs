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

/// Every way `n` versions of one intent may be written on those before
/// them, each with every way they may rest on what goes; `copied` bit `k`:
/// the version after `k` says the words of the one before it.
fn each_way(n: usize, copied: usize) -> usize {
    let ids = ["u1", "u2", "u3", "u4"];
    let mut kept = 0;
    for shape in 0..1_usize << (n * (n - 1) / 2) {
        for named in 1..1_usize << n {
            let text: Vec<usize> = (0..n).fold(Vec::new(), |mut text, at| {
                let same = at > 0 && copied >> (at - 1) & 1 == 1;
                text.push(if same { text[at - 1] } else { at });
                text
            });
            let rests =
                |at: usize| (0..n).any(|other| named >> other & 1 == 1 && text[other] == text[at]);
            let bodies: Vec<Value> = (0..n)
                .map(|at| {
                    // Any of the ones before it, from its own bits.
                    let bits = shape >> (at * at.saturating_sub(1) / 2);
                    let base: Vec<&str> = (0..at)
                        .filter(|on| bits >> on & 1 == 1)
                        .map(|on| ids[on])
                        .collect();
                    let ground = if named >> at & 1 == 1 {
                        "span:s1"
                    } else {
                        "message:9"
                    };
                    json!({
                        "version": 1, "sync": "shared", "saidBy": ["her"], "base": base,
                        "intent": "h1", "text": format!("一段{}", text[at]),
                        "because": [ground], "status": "waiting",
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
            let goes = |id: &str| going.iter().any(|gone| gone == id);
            let left: Vec<(&str, &Value)> =
                all.iter().copied().filter(|(id, _)| !goes(id)).collect();
            let at = format!("{n} versions, {shape}, {named}, {copied}: {going:?}");
            assert!((0..n).all(|one| !rests(one) || goes(ids[one])), "{at}");
            if (0..n).any(|one| rests(one) && before.contains(ids[one])) {
                assert!(left.is_empty(), "{at}");
            } else {
                assert_eq!(stands(&left), before, "{at}");
                kept += 1;
            }
        }
    }
    kept
}

/// What is left after the rows listed go stands as it did, or nothing is
/// left; and none left says the words of one that went for resting on it.
#[test]
fn what_stands_after_is_what_stood_or_nothing() {
    assert!(each_way(4, 0) > 0);
    for copied in 0..4 {
        // All in the same words: whichever rests, all do.
        assert_eq!(each_way(3, copied) == 0, copied == 3, "{copied}");
    }
}

/// The partner's words are never quoted in a refusal.
#[test]
fn a_refusal_names_the_list_not_what_is_in_it() {
    let mut words = input(&[], &[]);
    words.messages = vec!["一段\n原话".into()];
    let error = resting_on(words).unwrap_err().to_string();
    assert_eq!(error, "messages holds a control character");
}
