//! What a refused row's place is (`log.refused`): what admit says of it,
//! how it goes on to another side, how it merges with the row itself, and
//! that it folds to nothing.

use std::collections::BTreeMap;

use myriad_persona_sync::Rev;

use super::*;
use crate::{
    admit::Reason,
    vector::{answer, holdings},
};

const AUDIENCE: &str = "personal:partner_0199d5c0b00044444444444444444444";
const PARTNER: &str = "partner_0199d5c0b00044444444444444444444";

fn holder(core: Core) -> Holder {
    Holder {
        core,
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

fn newer() -> Row {
    row("m1", 1, "node:a", "node.from_a_newer_core")
}

fn admitted(rows: Vec<Row>, holder: Holder, known: BTreeMap<String, String>) -> Admitted {
    admit(AdmitInput {
        rows,
        now: 1_760_000_100_000_000,
        known,
        members: BTreeMap::new(),
        ledger: Some(Vec::new()),
        epochs: BTreeMap::new(),
        holder,
        peer: PARTNER.into(),
    })
    .unwrap()
}

fn merged(log: Vec<Row>, incoming: Vec<Row>) -> Merged {
    merge(MergeInput {
        log,
        incoming,
        ledger: Vec::new(),
        holder: Some(holder(Core::Server)),
    })
    .unwrap()
}

fn take(log: Vec<Row>, rows: Vec<Row>, side: Holder) -> Vec<Row> {
    let admitted = admitted(rows, side, BTreeMap::new());
    merged(log, admitted.accepted).log
}

fn folded(rows: Vec<Row>) -> Folded {
    fold(FoldInput {
        rows,
        leases: Vec::new(),
        deputies: Vec::new(),
        ledger: Vec::new(),
    })
    .unwrap()
}

#[test]
fn a_row_refused_for_good_is_taken_as_its_place_and_still_listed() {
    let admitted = admitted(
        vec![newer(), row("m2", 2, "node:a", "node.noted")],
        holder(Core::Server),
        BTreeMap::new(),
    );
    assert_eq!(admitted.refused.len(), 1);
    let refusal = &admitted.refused[0];
    assert_eq!((refusal.id.as_str(), refusal.reason), ("m1", Reason::Envelope));
    assert!(refusal.placed);
    let place = &admitted.accepted[0];
    assert_eq!(Some(place.clone()), newer().place_of_refused());
    assert_eq!(
        (place.kind.as_str(), place.object.as_str(), place.dseq),
        (kinds::PLACE, "refused:m1", Some(1))
    );
    assert_eq!((place.author, place.epoch), (Author::Owner, None));
    assert!(place.payload.is_none() && place.subject.is_none());
    assert_eq!((&place.hlc, &place.origin), (&newer().hlc, &newer().origin));
    assert!(place.is_place_of(&newer()));
    place.check().unwrap();
    assert_eq!(admitted.accepted[1].id, "m2");
    let listed = serde_json::to_value(refusal).unwrap();
    assert_eq!(listed["placed"], true);

    // Its object in another audience of the same partner: placed too.
    let mut moved = newer();
    moved.kind = "node.noted".into();
    let known = BTreeMap::from([("node:a".to_string(), "own:".to_string())]);
    let admitted = admitted(vec![moved.clone()], holder(Core::Server), known);
    assert_eq!(admitted.refused[0].reason, Reason::AudienceChanged);
    assert!(admitted.refused[0].placed);
    assert_eq!(admitted.accepted, vec![moved.place_of_refused().unwrap()]);
}

#[test]
fn a_row_refused_for_now_or_with_no_place_takes_none() {
    // Epochs, members and this side's id may change: no place.
    let mut back = row("m1", 1, "node:a", "node.noted");
    back.author = Author::Her;
    back.epoch = Some(1);
    let refused = admit(AdmitInput {
        rows: vec![back],
        now: 1_760_000_100_000_000,
        known: BTreeMap::new(),
        members: BTreeMap::new(),
        ledger: Some(Vec::new()),
        epochs: BTreeMap::from([(
            AUDIENCE.to_string(),
            BTreeMap::from([("dev_mac".to_string(), 2)]),
        )]),
        holder: holder(Core::Server),
        peer: PARTNER.into(),
    })
    .unwrap();
    assert_eq!(refused.refused[0].reason, Reason::EpochBack);
    assert!(!refused.refused[0].placed);
    assert!(refused.accepted.is_empty());
    let listed = serde_json::to_value(&refused.refused[0]).unwrap();
    assert!(listed.get("placed").is_none(), "{listed}");

    // No place to keep: a local row, a row with no count, a clock of
    // another side, an audience this side may not hold.
    let mut local = newer();
    local.replicate = Replicate::Local;
    local.dseq = None;
    let mut uncounted = newer();
    uncounted.dseq = None;
    let mut clocked = newer();
    clocked.hlc = Rev::new(1_760_000_000_001, 0, "dev_win").unwrap();
    let mut group = newer();
    group.audience = "group:g1".into();
    for (one, side) in [
        (local, Core::Server),
        (uncounted, Core::Server),
        (clocked, Core::Server),
        (group, Core::Pc),
    ] {
        assert!(one.place_of_refused().is_none() || side == Core::Pc, "{one:?}");
        let admitted = admitted(vec![one.clone()], holder(side), BTreeMap::new());
        assert!(admitted.accepted.is_empty(), "{one:?}: {:?}", admitted.accepted);
        assert!(!admitted.refused[0].placed);
    }
}

#[test]
fn a_place_goes_on_and_the_side_it_reaches_keeps_up() {
    let mac = vec![
        newer(),
        row("m2", 2, "node:a", "node.noted"),
        row("m3", 3, "node:b", "node.noted"),
    ];
    let site = take(Vec::new(), answer(&mac, &Vector::new()).unwrap(), holder(Core::Server));
    // Another computer of the partner's, from the server: the place goes
    // as any shared row, and its count meets the server's.
    let theirs = Vector::new();
    let sent = crate::vector::answer_to(&site, &theirs, &holder(Core::Pc), PARTNER).unwrap();
    assert!(sent.held_back.is_empty());
    let win = take(Vec::new(), sent.rows, holder(Core::Pc));
    assert_eq!(holdings(&win).unwrap(), holdings(&site).unwrap());
    assert_eq!(win, site);
    assert!(answer(&site, &holdings(&win).unwrap().vector).unwrap().is_empty());
}

#[test]
fn every_side_ends_with_the_same_log_whatever_order_the_rows_came_in() {
    let rows = vec![
        newer(),
        row("m2", 2, "node:a", "node.noted"),
        row("m3", 3, "node:b", "node.noted"),
    ];
    let whole = take(Vec::new(), rows.clone(), holder(Core::Server));
    let orders = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    for order in orders {
        // One row a batch, in this order, the first again at the end.
        let mut log = Vec::new();
        for at in order.into_iter().chain([order[0]]) {
            log = take(log, vec![rows[at].clone()], holder(Core::Server));
        }
        assert_eq!(log, whole, "{order:?}");
        assert_eq!(holdings(&log).unwrap().vector, Vector::from([("dev_mac".into(), 3)]));
    }
}

#[test]
fn a_side_that_takes_the_row_keeps_the_row_whichever_came_first() {
    // A side whose core reads the row: as another kind it knows.
    let mut readable = newer();
    readable.kind = "node.noted".into();
    let place = readable.place_of_refused().unwrap();

    // The row first, then the place another side kept: a duplicate.
    let first = merged(vec![readable.clone()], vec![place.clone()]);
    assert_eq!(first.log, vec![readable.clone()]);
    assert_eq!(first.duplicates, 1);
    assert!(first.conflicts.is_empty() && first.added.is_empty());

    // The place first, then the row: the row takes it up.
    let then = merged(vec![place.clone()], vec![readable.clone()]);
    assert_eq!(then.log, vec![readable.clone()]);
    assert_eq!(then.filled, vec!["m1".to_string()]);
    assert!(then.conflicts.is_empty() && then.added.is_empty());

    // A forgotten place and the row whole, or the place and the row's
    // stub: still one row, the row.
    let stubbed = merged(vec![place.stub()], vec![readable.clone()]);
    assert_eq!(stubbed.log, vec![readable.clone()]);
    let both = merged(vec![place], vec![readable.stub()]);
    assert_eq!(both.log, vec![readable.stub()]);
    assert!(both.conflicts.is_empty());
}

#[test]
fn a_place_folds_to_nothing() {
    let log = take(
        Vec::new(),
        vec![newer(), row("m2", 2, "node:a", "node.noted")],
        holder(Core::Server),
    );
    assert_eq!(log.len(), 2);
    let objects = folded(log).objects;
    assert_eq!(objects.len(), 1, "{objects:?}");
    assert_eq!(objects[0].object, "node:a");
    assert_eq!(objects[0].rows, vec!["m2".to_string()]);
}

#[test]
fn only_a_place_is_about_a_refused_row_and_it_says_nothing() {
    let place = newer().place_of_refused().unwrap();
    let mut other = place.clone();
    other.object = "refused:m2".into();
    let mut said = place.clone();
    said.payload = Some(Payload {
        reference: "blob:m1".into(),
        key: None,
    });
    let mut hers = place.clone();
    hers.author = Author::Her;
    hers.epoch = Some(1);
    let mut local = place.clone();
    local.replicate = Replicate::Local;
    local.dseq = None;
    let mut about = place.clone();
    about.subject = Some(PARTNER.into());
    let mut noted = row("m9", 9, "refused:m1", "node.noted");
    noted.object = "refused:m1".into();
    let mut noting = noted.clone();
    noting.kind = "link.attach".into();
    for (name, one) in [
        ("another row's", other),
        ("with a payload", said),
        ("hers", hers),
        ("local", local),
        ("with a subject", about),
        ("a node kind", noted),
        ("a kind about anything", noting),
    ] {
        assert!(one.check().is_err(), "{name}");
    }
    place.check().unwrap();
    place.stub().check().unwrap();
}
