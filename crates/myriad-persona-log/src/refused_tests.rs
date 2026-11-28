//! A row refused for good keeps its place: its side's count goes on past
//! it, nothing asks for it again, and every side ends with the same log
//! whatever order the rows came in. A row refused for now holds the count.

use std::collections::BTreeMap;

use myriad_persona_sync::Rev;

use super::*;
use crate::vector::{answer, holdings};

const AUDIENCE: &str = "personal:partner_0199d5c0b00044444444444444444444";
const PARTNER: &str = "partner_0199d5c0b00044444444444444444444";

fn server() -> Holder {
    Holder {
        core: Core::Server,
        owner: PARTNER.into(),
    }
}

fn pc() -> Holder {
    Holder {
        core: Core::Pc,
        owner: PARTNER.into(),
    }
}

fn row(id: &str, dseq: u64, object: &str, kind: &str) -> Row {
    Row {
        version: LOG_VERSION,
        id: id.into(),
        origin: "dev_mac".into(),
        author: Author::Owner,
        epoch: None,
        dseq: Some(dseq),
        hlc: Rev::new(1_760_000_000_000 + dseq, 0, "dev_mac").unwrap(),
        audience: AUDIENCE.into(),
        subject: None,
        object: object.into(),
        kind: kind.into(),
        payload: Some(Payload {
            reference: format!("blob:{id}"),
            key: None,
        }),
        replicate: Replicate::Shared,
        forgotten: false,
        to: None,
        takes: Vec::new(),
    }
}

/// The Mac's rows: the first of a kind only a newer core reads.
fn mac() -> Vec<Row> {
    vec![
        row("m1", 1, "node:a", "node.from_a_newer_core"),
        row("m2", 2, "node:a", "node.noted"),
        row("m3", 3, "node:b", "node.noted"),
    ]
}

/// What `rows` are, admitted by `holder` holding `log` (`known` as given)
/// and merged into it.
fn take(
    log: Vec<Row>,
    rows: Vec<Row>,
    holder: &Holder,
    known: &BTreeMap<String, String>,
) -> Vec<Row> {
    let admitted = admit(AdmitInput {
        rows,
        now: 1_760_000_100_000_000,
        known: known.clone(),
        members: BTreeMap::new(),
        ledger: Some(Vec::new()),
        epochs: BTreeMap::new(),
        holder: holder.clone(),
        peer: PARTNER.into(),
    })
    .unwrap();
    merge(MergeInput {
        log,
        incoming: admitted.accepted,
        ledger: admitted.ledger,
        holder: Some(holder.clone()),
    })
    .unwrap()
    .log
}

/// Rounds of the server's vector and the Mac's answer to it, as every link
/// does: how many rows each round sent, and the server's log after them.
fn rounds(mac: &[Row], known: &BTreeMap<String, String>) -> (Vec<usize>, Vec<Row>) {
    let mut site = Vec::new();
    let mut sent = Vec::new();
    for _ in 0..4 {
        let theirs = holdings(&site).unwrap().vector;
        let rows = answer(mac, &theirs).unwrap();
        sent.push(rows.len());
        site = take(site, rows, &server(), known);
    }
    (sent, site)
}

#[test]
fn a_row_refused_for_good_keeps_its_sides_count_whole() {
    // A kind this core does not read: refused however often it comes.
    let (sent, site) = rounds(&mac(), &BTreeMap::new());
    let held = holdings(&site).unwrap();
    assert_eq!(held.vector, Vector::from([("dev_mac".into(), 3)]), "{site:?}");
    assert!(held.beyond.is_empty(), "{:?}", held.beyond);
    assert_eq!(sent, vec![3, 0, 0, 0], "nothing goes again");
    assert!(site.iter().all(|row| row.payload.is_none() || row.id != "m1"));

    // Its object already in another audience of the same partner.
    let mut moved = mac();
    moved[0].kind = "node.noted".into();
    let known = BTreeMap::from([("node:a".to_string(), "own:".to_string())]);
    let (sent, site) = rounds(&moved[..1], &known);
    assert_eq!(
        holdings(&site).unwrap().vector,
        Vector::from([("dev_mac".into(), 1)]),
        "{site:?}"
    );
    assert_eq!(sent, vec![1, 0, 0, 0]);
}

#[test]
fn a_row_refused_for_now_still_holds_the_count() {
    // Her row with an epoch lower than one its side wrote before it here:
    // what this side was told of epochs may be wrong, so it takes no place.
    let mut back = row("m1", 1, "node:a", "node.noted");
    back.author = Author::Her;
    back.epoch = Some(1);
    let admitted = admit(AdmitInput {
        rows: vec![back],
        now: 1_760_000_100_000_000,
        known: BTreeMap::new(),
        members: BTreeMap::new(),
        ledger: Some(Vec::new()),
        epochs: BTreeMap::from([(
            AUDIENCE.to_string(),
            BTreeMap::from([("dev_mac".to_string(), 2)]),
        )]),
        holder: server(),
        peer: PARTNER.into(),
    })
    .unwrap();
    assert!(admitted.accepted.is_empty(), "{:?}", admitted.accepted);
    assert_eq!(admitted.refused.len(), 1);
    // A group's row on a PC: not held, and nothing of it is kept.
    let mut group = row("g1", 1, "node:g", "node.from_a_newer_core");
    group.audience = "group:g1".into();
    let admitted = admit(AdmitInput {
        rows: vec![group],
        now: 1_760_000_100_000_000,
        known: BTreeMap::new(),
        members: BTreeMap::new(),
        ledger: Some(Vec::new()),
        epochs: BTreeMap::new(),
        holder: pc(),
        peer: PARTNER.into(),
    })
    .unwrap();
    assert!(admitted.accepted.is_empty(), "{:?}", admitted.accepted);
}
