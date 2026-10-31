//! Two host cores sharing her nodes: a Mac and the server write to the same
//! node, swap what the other lacks through [`crate::wire`], and fold.
//!
//! What has to hold: whatever order the batches (or single rows) arrive in,
//! a side folds the same nodes; both sides fold the same shared content;
//! two changes made from one version stay two (两边都改过) and come due for
//! her upkeep until one run weighs both; upkeep marks survive the merge;
//! a forget made offline reaches the other side as a row and takes the
//! other side's later rows with it; and nothing of an audience a side may
//! not hold ever reaches it.

use std::collections::BTreeMap;

use myriad_persona_log::{
    Core, Holder, MergeInput, Row,
    forget::{self, Entry},
    merge,
    vector::holdings,
};
use myriad_persona_sync::canonical::canonical_bytes;
use serde_json::{Value, json};

use crate::{
    fold::{FoldInput, fold},
    node_id,
    queue::{QueueInput, Reason, queue},
    wire::{Batch, Carried, MAX_BODY_BYTES, SendInput, TakeInput, Why, send, take},
};

const MINE: &str = "personal:7";
const GROUP: &str = "group:g1";
const OTHER: &str = "personal:9";
const KEY: &str = "minted:release";
const T: u64 = 1_760_000_000_000;
const NOW: i64 = (T as i64 + 100_000) * 1000;

fn object(audience: &str, key: &str) -> String {
    format!("node:{}", node_id(audience, key).unwrap())
}

fn hlc(origin: &str, at: u64) -> String {
    format!("{at:015}-000000-{origin}")
}

/// A shared row of hers (or, for a forget, the partner's).
fn row(id: &str, origin: &str, dseq: u64, at: u64, audience: &str, key: &str, kind: &str) -> Row {
    let mut value = json!({
        "version": 1, "id": id, "origin": origin, "author": "her",
        "epoch": if origin == "site" { 2 } else { 1 },
        "dseq": dseq, "hlc": hlc(origin, at), "audience": audience,
        "object": object(audience, key), "kind": kind,
        "payload": { "ref": format!("doc:{id}") }, "replicate": "shared",
    });
    if kind == "forget" || kind == "retract" {
        let map = value.as_object_mut().unwrap();
        map.remove("epoch");
        map.remove("payload");
        map.insert("author".into(), json!("owner"));
    }
    serde_json::from_value(value).unwrap()
}

/// The local row a side writes beside its own retract: it keeps the node.
fn home_of(retract: &Row) -> Row {
    let mut home = retract.clone();
    home.id = format!("{}h", retract.id);
    home.kind = "retract.home".into();
    home.replicate = myriad_persona_log::Replicate::Local;
    home.dseq = None;
    home
}

fn created(key: &str, kind: &str, name: &str) -> Value {
    json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "kind": kind, "key": key, "name": name })
}

fn span(audience: &str, text: &str) -> Value {
    json!({ "version": 1, "sync": "shared", "saidBy": ["her", "partner"], "audience": audience, "text": text })
}

fn named(value: &str, base: &[&str]) -> Value {
    json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "base": base, "field": "name", "value": value })
}

fn upkept(run: &str, seen: &[(&str, u64)], wrote: &[&str]) -> Value {
    let seen: BTreeMap<&str, String> = seen
        .iter()
        .map(|(device, at)| (*device, hlc(device, *at)))
        .collect();
    json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "run": run, "seen": seen, "wrote": wrote })
}

/// One host core's log and what it keeps beside it.
#[derive(Clone)]
struct Side {
    holder: Holder,
    rows: Vec<Row>,
    bodies: BTreeMap<String, Value>,
    ledger: Vec<Entry>,
    /// The origin it writes as.
    origin: String,
    /// Person and group keys it knows.
    people: Vec<String>,
}

impl Side {
    fn new(core: Core) -> Self {
        Self::named(
            core,
            if core == Core::Server {
                "site"
            } else {
                "dev_mac"
            },
        )
    }

    fn named(core: Core, origin: &str) -> Self {
        Self {
            holder: Holder {
                core,
                owner: "7".into(),
            },
            rows: Vec::new(),
            bodies: BTreeMap::new(),
            ledger: Vec::new(),
            origin: origin.to_string(),
            people: Vec::new(),
        }
    }

    /// Writes a row, as a host does: its own retract with the local
    /// `retract.home` beside it, each row through `log.ledger`; a forget or
    /// retract stubs what it reaches here at once, before anything is sent.
    fn write(&mut self, row: Row, body: Option<Value>) {
        if let Some(body) = body {
            self.bodies.insert(row.id.clone(), body);
        }
        let mut wrote = vec![row.clone()];
        if row.kind == "retract" && row.origin == self.origin {
            wrote.push(home_of(&row));
        }
        self.ledger = forget::record(forget::RecordInput {
            rows: wrote.clone(),
            ledger: self.ledger.clone(),
        })
        .unwrap()
        .ledger;
        let forget = forget::on_ledger(&row);
        self.rows.extend(wrote);
        if forget {
            let merged = merge(MergeInput {
                ledger: Some(self.ledger.clone()),
                log: self.rows.clone(),
                incoming: Vec::new(),
            })
            .unwrap();
            self.rows = merged.log;
            for id in merged.stubbed {
                self.bodies.remove(&id);
            }
        }
    }

    fn of(&self, audience: &str) -> Vec<Row> {
        self.rows
            .iter()
            .filter(|row| row.audience == audience)
            .cloned()
            .collect()
    }

    /// What this side sends `to` of one audience.
    fn send_to(&self, to: &Side, audience: &str) -> crate::wire::Sent {
        send(SendInput {
            ledger: self.ledger.clone(),
            elsewhere: Vec::new(),
            audience: audience.into(),
            rows: self.of(audience),
            bodies: self.bodies.clone(),
            theirs: holdings(&to.of(audience)).unwrap().vector,
            ask: Vec::new(),
            holder: to.holder.clone(),
        })
        .unwrap()
    }

    /// Takes a batch in and merges it, as a host core does.
    fn take(&mut self, batch: Batch) -> crate::wire::Taken {
        self.take_held(batch, Vec::new())
    }

    /// The same, for a batch that brings rows this side asked again for.
    fn take_held(&mut self, batch: Batch, held: Vec<Row>) -> crate::wire::Taken {
        let taken = take(TakeInput {
            people: self.people.clone(),
            elsewhere: Vec::new(),
            batch,
            holder: self.holder.clone(),
            now: NOW,
            known: BTreeMap::new(),
            members: BTreeMap::new(),
            ledger: Some(self.ledger.clone()),
            epochs: BTreeMap::new(),
            held,
        })
        .unwrap();
        let merged = merge(MergeInput {
            ledger: Some(taken.admitted.ledger.clone()),
            log: self.rows.clone(),
            incoming: taken.admitted.accepted.clone(),
        })
        .unwrap();
        self.rows = merged.log;
        for id in &merged.stubbed {
            self.bodies.remove(id);
        }
        for row in &self.rows {
            if row.forgotten {
                self.bodies.remove(&row.id);
            }
        }
        self.bodies.extend(taken.bodies.clone());
        self.ledger = taken.admitted.ledger.clone();
        taken
    }

    fn sync_from(&mut self, from: &Side, audience: &str) {
        for batch in from.send_to(self, audience).batches {
            self.take(batch);
        }
    }

    /// The shared part: what every side holding the audience should agree
    /// on.
    fn shared(&self, audience: &str) -> (Vec<Row>, BTreeMap<String, Value>) {
        let mut rows: Vec<Row> = self
            .of(audience)
            .into_iter()
            .filter(|row| row.replicate == myriad_persona_log::Replicate::Shared)
            .collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        // Only bodies that may travel: the Mac keeps its own that may not.
        let bodies = rows
            .iter()
            .filter_map(|row| self.bodies.get(&row.id).map(|b| (row, b)))
            .filter(|(row, body)| crate::wire::carry(row, body).is_ok())
            .map(|(row, body)| (row.id.clone(), body.clone()))
            .collect();
        (rows, bodies)
    }

    fn folded(&self, audience: &str, shared_only: bool) -> Value {
        let (rows, bodies) = if shared_only {
            self.shared(audience)
        } else {
            (self.of(audience), self.bodies.clone())
        };
        let bodies = bodies
            .into_iter()
            .filter(|(id, _)| rows.iter().any(|row| &row.id == id))
            .collect();
        serde_json::to_value(
            fold(FoldInput {
                rows,
                bodies,
                ledger: self.ledger.clone(),
            })
            .unwrap(),
        )
        .unwrap()
    }

    fn due(&self, audience: &str) -> Vec<Vec<Reason>> {
        let bodies = self
            .bodies
            .iter()
            .filter(|(id, _)| {
                self.rows
                    .iter()
                    .any(|row| &row.id == *id && row.audience == audience)
            })
            .map(|(id, body)| (id.clone(), body.clone()))
            .collect();
        queue(QueueInput {
            rows: self.of(audience),
            bodies,
            ledger: self.ledger.clone(),
            max: 10,
        })
        .unwrap()
        .due
        .into_iter()
        .map(|due| due.why)
        .collect()
    }
}

/// A key the host filter's secret scan wipes, built at run time.
fn token() -> String {
    format!("ghp_{}", "a1B2c3D4e5".repeat(4))
}

/// The Mac and the server after the Mac's first rows reached the server,
/// then each writing on its own while the partner forced the pen over: the
/// same name changed on both sides from one version, a span each, and an
/// upkeep run each that saw only its own side.
fn apart() -> (Side, Side) {
    let mut mac = Side::new(Core::Pc);
    let mut site = Side::new(Core::Server);
    mac.write(
        row("m1", "dev_mac", 1, T, MINE, KEY, "node.created"),
        Some(created(KEY, "task", "发版")),
    );
    mac.write(
        row("m2", "dev_mac", 2, T + 1, MINE, KEY, "node.span"),
        Some(span(MINE, "伙伴说周五发。")),
    );
    // What the screen gave rise to stays on the Mac: a local row, no dseq.
    let mut local = row("m3", "dev_mac", 0, T + 2, MINE, KEY, "node.span");
    local.replicate = myriad_persona_log::Replicate::Local;
    local.dseq = None;
    mac.write(
        local,
        Some(json!({
            "version": 1, "sync": {"local": "dev_mac"}, "saidBy": ["her", "third_party"],
            "audience": MINE, "text": "测试跑了 42 个。", "refs": ["record:s#12"],
        })),
    );
    site.sync_from(&mac, MINE);

    mac.write(
        row("m4", "dev_mac", 3, T + 2_000, MINE, KEY, "node.revised"),
        Some(named("这周发版", &["m1"])),
    );
    // A key slipped into one of her texts, and a row of an older native kind.
    mac.write(
        row("m6", "dev_mac", 4, T + 2_001, MINE, KEY, "node.span"),
        Some(span(MINE, &format!("伙伴贴了 {} 让我记着。", token()))),
    );
    mac.write(
        row("m7", "dev_mac", 5, T + 2_002, MINE, KEY, "node.noted"),
        Some(json!({ "note": "旧的行" })),
    );
    mac.write(
        row("m5", "dev_mac", 6, T + 2_003, MINE, KEY, "node.upkept"),
        Some(upkept("r-mac", &[("dev_mac", T + 2_002)], &["m4"])),
    );

    site.write(
        row("s1", "site", 1, T + 2_000, MINE, KEY, "node.revised"),
        Some(named("发 1.2 版", &["m1"])),
    );
    site.write(
        row("s2", "site", 2, T + 2_010, MINE, KEY, "node.span"),
        Some(span(MINE, "网页上伙伴说先发 1.2。")),
    );
    site.write(
        row("s3", "site", 3, T + 2_011, MINE, KEY, "node.upkept"),
        Some(upkept(
            "r-site",
            &[("dev_mac", T + 1), ("site", T + 2_010)],
            &["s1"],
        )),
    );
    // What the server alone holds: a group member's node, someone else's talk.
    site.write(
        row(
            "g1",
            "site",
            1,
            T + 3_000,
            GROUP,
            "person:alice",
            "node.created",
        ),
        Some(created("person:alice", "person", "Alice")),
    );
    site.write(
        row("o1", "site", 1, T + 3_001, OTHER, "her", "node.created"),
        Some(created("her", "person", "她自己")),
    );
    (mac, site)
}

fn together() -> (Side, Side) {
    let (mut mac, mut site) = apart();
    let mac_before = mac.clone();
    mac.sync_from(&site, MINE);
    site.sync_from(&mac_before, MINE);
    (mac, site)
}

#[test]
fn both_sides_fold_the_same_shared_node() {
    let (mac, site) = together();
    assert_eq!(mac.shared(MINE), site.shared(MINE));
    assert_eq!(mac.folded(MINE, true), site.folded(MINE, true));
    // The Mac's local span, and the span whose body held a key, stay in the
    // Mac's own fold and nowhere else.
    let spans = |folded: &Value| -> Vec<String> {
        folded["nodes"][0]["spans"]
            .as_array()
            .unwrap()
            .iter()
            .map(|span| span["row"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(spans(&mac.folded(MINE, false)), ["m2", "m3", "m6", "s2"]);
    assert_eq!(spans(&site.folded(MINE, false)), ["m2", "s2"]);
    assert!(!site.rows.iter().any(|row| row.id == "m3"));
    // 两边都改过: both names stay, neither wins by its clock.
    let name = &site.folded(MINE, true)["nodes"][0]["name"];
    assert_eq!(name["bothChanged"], true);
    assert_eq!(name["base"], json!(["m4", "s1"]));
}

#[test]
fn any_arrival_order_folds_the_same() {
    let (mac, site) = apart();
    let sent = mac.send_to(&site, MINE);
    // Every row of the batch on its own, with its payload.
    let mut singles: Vec<Batch> = Vec::new();
    for batch in &sent.batches {
        for row in &batch.rows {
            let payloads: Vec<Carried> = batch
                .payloads
                .iter()
                .filter(|one| {
                    row.payload
                        .as_ref()
                        .is_some_and(|p| p.reference == one.reference)
                })
                .cloned()
                .collect();
            singles.push(Batch {
                audience: batch.audience.clone(),
                rows: vec![row.clone()],
                payloads,
                withheld: batch
                    .withheld
                    .iter()
                    .filter(|one| one.id == row.id)
                    .cloned()
                    .collect(),
            });
        }
    }
    assert_eq!(singles.len(), 4, "m4 to m7: m1 and m2 are there already");
    let mut expected: Option<(Vec<u8>, Value)> = None;
    let mut order: Vec<usize> = (0..singles.len()).collect();
    // Every permutation of four (Heap's algorithm), each also sent twice.
    let mut count = vec![0; order.len()];
    let mut seen = 0;
    let mut check = |order: &[usize]| {
        let mut side = site.clone();
        for &at in order.iter().chain(order.iter().rev()) {
            side.take(singles[at].clone());
        }
        let log: Vec<Value> = side
            .rows
            .iter()
            .map(|row| serde_json::to_value(row).unwrap())
            .collect();
        let got = (
            canonical_bytes(&json!({ "log": log, "bodies": side.bodies })),
            side.folded(MINE, true),
        );
        match &expected {
            None => expected = Some(got),
            Some(first) => assert_eq!(first, &got, "order {order:?}"),
        }
        seen += 1;
    };
    check(&order);
    let mut at = 0;
    while at < order.len() {
        if count[at] < at {
            let swap = if at % 2 == 0 { 0 } else { count[at] };
            order.swap(swap, at);
            check(&order);
            count[at] += 1;
            at = 0;
        } else {
            count[at] = 0;
            at += 1;
        }
    }
    assert_eq!(seen, 24);
}

#[test]
fn what_cannot_travel_goes_as_a_row_without_its_body() {
    let (mac, site) = apart();
    let sent = mac.send_to(&site, MINE);
    let withheld: Vec<(&str, Why)> = sent
        .batches
        .iter()
        .flat_map(|batch| &batch.withheld)
        .map(|one| (one.id.as_str(), one.why))
        .collect();
    assert_eq!(withheld, [("m6", Why::Secret), ("m7", Why::NoShape)]);
    let text = serde_json::to_string(&sent.batches).unwrap();
    assert!(!text.contains(&token()), "the key never leaves");
    assert!(!text.contains("record:s#12"), "the local row never leaves");
    let (_, site) = together();
    // The server's count of the Mac's rows has no gap...
    assert_eq!(holdings(&site.of(MINE)).unwrap().vector["dev_mac"], 6);
    // ...and its fold passes the bodiless rows by.
    let skipped = &site.folded(MINE, true)["skipped"];
    let skipped: Vec<(&str, &str)> = skipped
        .as_array()
        .unwrap()
        .iter()
        .map(|one| (one["id"].as_str().unwrap(), one["why"].as_str().unwrap()))
        .collect();
    assert!(skipped.contains(&("m6", "no_body")), "{skipped:?}");
}

#[test]
fn a_change_made_on_both_sides_stays_due_until_one_run_weighs_both() {
    let (mut mac, mut site) = together();
    // Each side's mark covers its own change; together they cover both, yet
    // no run of hers saw the two names at once.
    assert_eq!(site.due(MINE), vec![vec![Reason::Own]]);
    assert_eq!(mac.due(MINE), site.due(MINE));
    // Both marks survived the merge, on both sides.
    for side in [&mac, &site] {
        for id in ["m5", "s3"] {
            assert!(side.bodies.contains_key(id), "{id}");
        }
    }
    // The server holds the pen now; its run weighs both and settles it.
    site.write(
        row("s4", "site", 4, T + 5_000, MINE, KEY, "node.revised"),
        Some(named("周五发 1.2 版", &["m4", "s1"])),
    );
    site.write(
        row("s5", "site", 5, T + 5_001, MINE, KEY, "node.upkept"),
        Some(upkept(
            "r-site-2",
            &[("dev_mac", T + 2_003), ("site", T + 5_000)],
            &["s4"],
        )),
    );
    mac.sync_from(&site, MINE);
    for side in [&mac, &site] {
        assert!(side.due(MINE).is_empty(), "{:?}", side.due(MINE));
        let name = &side.folded(MINE, true)["nodes"][0]["name"];
        assert_eq!(name["value"], "周五发 1.2 版");
        assert!(name.get("bothChanged").is_none());
    }
}

#[test]
fn a_forget_made_offline_reaches_the_other_side_and_takes_its_later_rows() {
    let (mut mac, mut site) = together();
    // The partner has her forget the node on the Mac, offline; meanwhile the
    // server writes another span on it.
    mac.write(
        row("f1", "dev_mac", 7, T + 6_000, MINE, KEY, "forget"),
        None,
    );
    site.write(
        row("s6", "site", 4, T + 6_500, MINE, KEY, "node.span"),
        Some(span(MINE, "又聊到发版。")),
    );
    let mac_before = mac.clone();
    let sent = mac.send_to(&site, MINE);
    assert_eq!(sent.batches[0].rows[0].kind, "forget", "forgets go first");
    site.sync_from(&mac_before, MINE);
    mac.sync_from(&site, MINE);
    for side in [&mac, &site] {
        let folded = side.folded(MINE, true);
        assert_eq!(folded["nodes"][0]["forgotten"], true);
        assert!(folded["nodes"][0].get("name").is_none());
        assert!(
            side.ledger
                .iter()
                .any(|entry| entry.target == object(MINE, KEY))
        );
        // No body of the node is kept anywhere, the later span included.
        for row in side.of(MINE) {
            if row.kind != "forget" {
                assert!(row.forgotten, "{}", row.id);
                assert!(!side.bodies.contains_key(&row.id), "{}", row.id);
            }
        }
    }
    // An old copy that comes back (a side offline since before the forget)
    // only fills the count: it arrives as a stub.
    let (stale, _) = apart();
    let mut late = mac.clone();
    late.rows.retain(|row| row.id != "m2");
    late.bodies.remove("m2");
    let taken = late.take(Batch {
        audience: MINE.into(),
        rows: vec![stale.rows[1].clone()],
        payloads: stale.send_to(&Side::new(Core::Server), MINE).batches[0]
            .payloads
            .clone(),
        withheld: Vec::new(),
    });
    assert!(taken.admitted.accepted[0].forgotten);
    assert!(taken.bodies.is_empty());
}

#[test]
fn nothing_of_an_audience_a_side_may_not_hold_reaches_it() {
    let (mut mac, site) = apart();
    for audience in [GROUP, OTHER] {
        let sent = site.send_to(&mac, audience);
        assert!(sent.not_held && sent.batches.is_empty(), "{audience}");
        // Offered anyway (a server that broke the rule), the Mac refuses it.
        let pushed = site.send_to(&Side::new(Core::Server), audience);
        for batch in pushed.batches {
            let taken = mac.take(batch);
            assert!(taken.admitted.accepted.is_empty());
            assert!(taken.bodies.is_empty());
            assert!(
                taken
                    .admitted
                    .refused
                    .iter()
                    .all(|one| one.reason == myriad_persona_log::admit::Reason::NotHeld)
            );
        }
    }
    assert!(!mac.rows.iter().any(|row| row.audience != MINE));
    // A phone keeps no log at all.
    assert!(mac.send_to(&Side::new(Core::Child), MINE).not_held);
}

#[test]
fn a_body_that_is_not_its_own_canonical_bytes_is_not_taken() {
    use base64::Engine as _;
    let (mac, site) = apart();
    let mut batch = mac.send_to(&site, MINE).batches.remove(0);
    let engine = base64::engine::general_purpose::STANDARD;
    // The same body with its keys in another order: other bytes.
    let at = batch
        .payloads
        .iter()
        .position(|one| one.reference == "doc:m4")
        .unwrap();
    let pretty = serde_json::to_vec_pretty(&named("这周发版", &["m1"])).unwrap();
    batch.payloads[at].data = engine.encode(pretty);
    let mut side = site.clone();
    let taken = side.take(batch);
    assert!(taken.admitted.accepted.iter().any(|row| row.id == "m4"));
    assert!(
        taken
            .withheld
            .iter()
            .any(|one| one.id == "m4" && one.why == Why::BadBody)
    );
    assert!(!taken.bodies.contains_key("m4"));
}

#[test]
fn a_body_lost_on_the_way_is_asked_for_again_and_then_both_fold_the_same() {
    let (mut mac, mut site) = apart();
    let mac_before = mac.clone();
    mac.sync_from(&site, MINE);
    // The Mac's batch reaches the server without one of its bodies.
    let mut asked: Vec<String> = Vec::new();
    for mut batch in mac_before.send_to(&site, MINE).batches {
        batch.payloads.retain(|one| one.reference != "doc:m4");
        asked.extend(site.take(batch).ask);
    }
    // What the server may never have (the key, the older kind) is not
    // asked for again.
    assert_eq!(asked, ["m4"]);
    let shared = |side: &Side| canonical_bytes(&side.folded(MINE, true));
    assert_ne!(shared(&site), shared(&mac));
    // Asked again, the row comes back as the server holds it: only its
    // body is taken, the log stays as it was.
    let held: Vec<Row> = site
        .rows
        .iter()
        .filter(|row| asked.contains(&row.id))
        .cloned()
        .collect();
    let sent = send(SendInput {
        ledger: Vec::new(),
        elsewhere: Vec::new(),
        audience: MINE.into(),
        rows: mac.of(MINE),
        bodies: mac.bodies.clone(),
        theirs: holdings(&site.of(MINE)).unwrap().vector,
        ask: asked.clone(),
        holder: site.holder.clone(),
    })
    .unwrap();
    let again: Vec<&str> = sent
        .batches
        .iter()
        .flat_map(|batch| &batch.rows)
        .map(|row| row.id.as_str())
        .collect();
    assert_eq!(again, ["m4"]);
    let log = site.rows.clone();
    for batch in sent.batches {
        let taken = site.take_held(batch, held.clone());
        assert!(taken.admitted.accepted.is_empty() && taken.ask.is_empty());
        assert!(taken.bodies.contains_key("m4"));
    }
    assert_eq!(site.rows, log);
    assert_eq!(shared(&site), shared(&mac));
}

#[test]
fn a_body_too_large_goes_as_a_row_alone_and_every_batch_fits_a_frame() {
    use base64::Engine as _;
    let line = |id: &str, dseq: u64| {
        let mut row = row(id, "dev_mac", dseq, T + dseq, MINE, KEY, "segment.line");
        row.object = "segment:s-1".into();
        row
    };
    let big = json!({ "at": T, "text": "好".repeat(MAX_BODY_BYTES / 3 + 1) });
    let near = json!({ "at": T, "text": "a".repeat(MAX_BODY_BYTES - 64) });
    let mut mac = Side::new(Core::Pc);
    mac.write(line("l1", 1), Some(big.clone()));
    mac.write(line("l2", 2), Some(near.clone()));
    mac.write(line("l3", 3), Some(near));
    let site = Side::new(Core::Server);
    let sent = mac.send_to(&site, MINE);
    let withheld: Vec<(&str, Why)> = sent
        .batches
        .iter()
        .flat_map(|batch| &batch.withheld)
        .map(|one| (one.id.as_str(), one.why))
        .collect();
    assert_eq!(withheld, [("l1", Why::TooLarge)]);
    // One body near the cap to a batch, each under the link's frame limit
    // (`maxFrameBytes`).
    assert_eq!(sent.batches.len(), 2);
    for batch in &sent.batches {
        assert!(serde_json::to_vec(batch).unwrap().len() < 256 << 10);
    }
    // Sent anyway, a body over the cap is not taken.
    let mut batch = sent.batches[0].clone();
    batch.withheld.clear();
    batch.payloads.push(Carried {
        reference: "doc:l1".into(),
        data: base64::engine::general_purpose::STANDARD.encode(canonical_bytes(&big)),
    });
    let taken = site.clone().take(batch);
    assert_eq!(
        taken.withheld,
        [crate::wire::Withheld {
            id: "l1".into(),
            why: Why::TooLarge
        }]
    );
    assert!(taken.bodies.contains_key("l2"));
}

/// The node every retract test is about, and a node beside it.
const OTHER_KEY: &str = "minted:other";

/// A Mac (the home) and the server holding the node and one beside it.
fn shared_node() -> (Side, Side) {
    let mut mac = Side::new(Core::Pc);
    let mut site = Side::new(Core::Server);
    mac.write(
        row("r1", "dev_mac", 1, T, MINE, KEY, "node.created"),
        Some(created(KEY, "task", "发版")),
    );
    mac.write(
        row("r2", "dev_mac", 2, T + 1, MINE, KEY, "node.span"),
        Some(span(MINE, "伙伴说周五发版。")),
    );
    mac.write(
        row("r3", "dev_mac", 3, T + 2, MINE, OTHER_KEY, "node.created"),
        Some(created(OTHER_KEY, "topic", "别的事")),
    );
    site.sync_from(&mac, MINE);
    (mac, site)
}

fn node<'a>(folded: &'a Value, key: &str) -> &'a Value {
    let id = node_id(MINE, key).unwrap();
    folded["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == id)
        .unwrap()
}

/// What a side holds of the node: its rows' ids, whether each is a stub,
/// and the bodies it keeps.
fn held_of(side: &Side, key: &str) -> (Vec<(String, bool)>, Vec<String>) {
    let object = object(MINE, key);
    let rows: Vec<(String, bool)> = side
        .rows
        .iter()
        .filter(|row| row.object == object && !forget::on_ledger(row))
        .map(|row| (row.id.clone(), row.forgotten))
        .collect();
    let bodies = rows
        .iter()
        .filter(|(id, _)| side.bodies.contains_key(id))
        .map(|(id, _)| id.clone())
        .collect();
    (rows, bodies)
}

#[test]
fn a_retract_made_offline_keeps_the_node_home_and_takes_it_from_every_other_side() {
    let (mut mac, mut site) = shared_node();
    // The Mac is offline: the partner writes on it and takes the node back
    // from sync. The server, meanwhile, writes on the same node.
    mac.write(
        row("r4", "dev_mac", 4, T + 10, MINE, KEY, "node.span"),
        Some(span(MINE, "改到下周一。")),
    );
    mac.write(row("r5", "dev_mac", 5, T + 11, MINE, KEY, "retract"), None);
    site.write(
        row("s1", "site", 1, T + 12, MINE, KEY, "node.span"),
        Some(span(MINE, "站点那边记的一段。")),
    );
    // Back online, both ways, in either order.
    let mut first = (mac.clone(), site.clone());
    first.1.sync_from(&first.0, MINE);
    first.0.sync_from(&first.1, MINE);
    let mut second = (mac.clone(), site.clone());
    second.0.sync_from(&second.1, MINE);
    second.1.sync_from(&second.0, MINE);
    for (at, (mac, site)) in [&first, &second].into_iter().enumerate() {
        // The server drops the node, its own span too, bodies and all.
        let (rows, bodies) = held_of(site, KEY);
        assert_eq!(rows.len(), 4, "every row still counts");
        assert!(rows.iter().all(|(_, stub)| *stub), "{rows:?}");
        assert!(bodies.is_empty(), "{bodies:?}");
        let folded = site.folded(MINE, false);
        let gone = node(&folded, KEY);
        assert_eq!(gone["retracted"], json!(["dev_mac"]));
        assert!(gone.get("name").is_none() && gone.get("spans").is_none());
        assert_eq!(node(&folded, OTHER_KEY)["name"]["value"], "别的事");
        // The home keeps its node and what it wrote; the server's span
        // reached it only as a stub.
        let folded = mac.folded(MINE, false);
        let kept = node(&folded, KEY);
        assert_eq!(kept["retracted"], json!(["dev_mac"]));
        assert_eq!(kept["name"]["value"], "发版");
        let spans: Vec<&str> = kept["spans"]
            .as_array()
            .unwrap()
            .iter()
            .map(|span| span["text"].as_str().unwrap())
            .collect();
        assert_eq!(spans[..2], ["伙伴说周五发版。", "改到下周一。"]);
        // The server's span (伙伴 2026-10-30: 当作忘掉). Pulled by the home
        // before the server knew of the retract, the home keeps it whole.
        // Not: the server dropped it on learning of the retract, as a
        // forget would, and no side holds a body of it any more.
        let s1 = mac.rows.iter().find(|row| row.id == "s1").unwrap();
        let reached_home = at == 1;
        assert_eq!(!s1.forgotten, reached_home);
        assert_eq!(mac.bodies.contains_key("s1"), reached_home);
        assert_eq!(spans.len(), if reached_home { 3 } else { 2 });
        assert!(!site.bodies.contains_key("s1"));
    }
    assert_eq!(first.1.rows, second.1.rows);
    assert_eq!(first.1.bodies, second.1.bodies);
    // A row of the node the server writes later is a stub at once, and
    // another computer never gets a body of it.
    let mut site = first.1.clone();
    site.take(Batch {
        audience: MINE.into(),
        rows: vec![row("s2", "site", 2, T + 20, MINE, KEY, "node.span")],
        payloads: Vec::new(),
        withheld: Vec::new(),
    });
    let mut air = Side::named(Core::Pc, "dev_air");
    air.sync_from(&site, MINE);
    let (rows, bodies) = held_of(&air, KEY);
    assert!(rows.iter().all(|(_, stub)| *stub) && bodies.is_empty());
    assert_eq!(
        node(&air.folded(MINE, false), KEY)["retracted"],
        json!(["dev_mac"])
    );
}

#[test]
fn a_retract_racing_an_edit_on_another_side_ends_the_same_in_any_order() {
    let (mut mac, mut site) = shared_node();
    let mut air = Side::named(Core::Pc, "dev_air");
    air.sync_from(&site, MINE);
    // At once: the partner takes the node back on the Mac, and the other
    // Mac, not knowing, writes on it.
    mac.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    air.write(
        row("a1", "dev_air", 1, T + 10, MINE, KEY, "node.span"),
        Some(span(MINE, "另一台记的。")),
    );
    let mut results = Vec::new();
    for mac_first in [true, false] {
        let (mut mac, mut site, mut air) = (mac.clone(), site.clone(), air.clone());
        if mac_first {
            site.sync_from(&mac, MINE);
            site.sync_from(&air, MINE);
        } else {
            site.sync_from(&air, MINE);
            site.sync_from(&mac, MINE);
        }
        air.sync_from(&site, MINE);
        mac.sync_from(&site, MINE);
        // The edit is gone from every side but the home's, where it came
        // as a stub: the home keeps what it had.
        for side in [&site, &air] {
            let (rows, bodies) = held_of(side, KEY);
            assert!(rows.iter().all(|(_, stub)| *stub), "{rows:?}");
            assert!(bodies.is_empty());
            assert_eq!(
                node(&side.folded(MINE, false), KEY)["retracted"],
                json!(["dev_mac"])
            );
        }
        assert_eq!(node(&mac.folded(MINE, false), KEY)["name"]["value"], "发版");
        results.push((site.rows.clone(), site.bodies.clone(), air.rows.clone()));
    }
    assert_eq!(results[0], results[1]);
    // The writer of the race keeps no body of it, so it cannot bring it
    // back later.
    assert!(
        !results[0]
            .2
            .iter()
            .any(|row| row.id == "a1" && !row.forgotten)
    );
    let _ = (&mut mac, &mut site);
}

#[test]
fn an_export_after_a_retract_holds_no_body_of_the_node_even_from_its_home() {
    let (mut mac, _) = shared_node();
    mac.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    // The export's log pages are what the Mac sends the partner's file
    // (`holder {pc, owner}`), with its ledger.
    let file = Side::named(Core::Pc, "dev_new");
    let pages = mac.send_to(&file, MINE).batches;
    let object = object(MINE, KEY);
    let carried: Vec<&str> = pages
        .iter()
        .flat_map(|page| &page.payloads)
        .map(|one| one.reference.as_str())
        .collect();
    assert_eq!(carried, ["doc:r3"], "only the node beside it");
    for page in &pages {
        for row in page.rows.iter().filter(|row| row.object == object) {
            assert!(row.kind == "retract" || row.forgotten, "{row:?}");
        }
    }
    // A new Mac importing the file gets the node as taken back.
    let mut new = file;
    for page in pages {
        new.take(page);
    }
    assert_eq!(
        node(&new.folded(MINE, false), KEY)["retracted"],
        json!(["dev_mac"])
    );
    // The home's own fold still has it all.
    assert_eq!(
        node(&mac.folded(MINE, false), KEY)["spans"][0]["text"],
        "伙伴说周五发版。"
    );
}

#[test]
fn a_node_retracted_on_two_sides_stays_whole_on_both() {
    let (mut mac, mut site) = shared_node();
    let mut air = Side::named(Core::Pc, "dev_air");
    let mut mini = Side::named(Core::Pc, "dev_mini");
    air.sync_from(&site, MINE);
    mini.sync_from(&site, MINE);
    // The partner takes the node back on the Mac; the other Mac, offline,
    // writes on it and takes it back there too. A third Mac that took
    // nothing back keeps a line of it to itself.
    mac.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    air.write(
        row("a1", "dev_air", 1, T + 20, MINE, KEY, "node.span"),
        Some(span(MINE, "这台离线时记的。")),
    );
    air.write(row("a2", "dev_air", 2, T + 21, MINE, KEY, "retract"), None);
    let mut local = row("i1", "dev_mini", 1, T + 22, MINE, KEY, "node.span");
    local.replicate = myriad_persona_log::Replicate::Local;
    local.dseq = None;
    let mut body = span(MINE, "只在这台记的。");
    body["sync"] = json!("local");
    mini.write(local, Some(body));
    site.sync_from(&mac, MINE);
    site.sync_from(&air, MINE);
    for side in [&mut mac, &mut air, &mut mini] {
        side.sync_from(&site, MINE);
    }
    // Both sides it was taken back on are its homes, and each keeps what
    // it wrote: the other Mac's span stays there, whole.
    let homes = json!(["dev_air", "dev_mac"]);
    for home in [&mac, &air] {
        let folded = home.folded(MINE, false);
        let kept = node(&folded, KEY);
        assert_eq!(kept["retracted"], homes);
        assert_eq!(kept["name"]["value"], "发版");
    }
    assert!(air.bodies.contains_key("a1"));
    let spans = node(&air.folded(MINE, false), KEY)["spans"].clone();
    assert!(
        spans
            .as_array()
            .unwrap()
            .iter()
            .any(|one| one["text"] == "这台离线时记的。")
    );
    // The server keeps none of it; the third Mac keeps only its local line.
    let (rows, bodies) = held_of(&site, KEY);
    assert!(rows.iter().all(|(_, stub)| *stub) && bodies.is_empty());
    assert_eq!(node(&site.folded(MINE, false), KEY)["retracted"], homes);
    let (rows, bodies) = held_of(&mini, KEY);
    assert_eq!(bodies, ["i1"]);
    assert!(
        rows.iter()
            .filter(|(id, _)| id != "i1")
            .all(|(_, stub)| *stub)
    );
    assert!(!site.rows.iter().any(|row| row.id == "i1"));
}

#[test]
fn a_home_keeps_every_body_whatever_order_stubs_and_rows_reach_it_in() {
    let (mut mac, mut site) = shared_node();
    let mut air = Side::named(Core::Pc, "dev_air");
    air.sync_from(&site, MINE);
    // The other Mac writes on the node, not knowing the partner is taking
    // it back on this one.
    air.write(
        row("a1", "dev_air", 1, T + 9, MINE, KEY, "node.span"),
        Some(span(MINE, "另一台记的。")),
    );
    mac.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    site.sync_from(&air, MINE);
    site.sync_from(&mac, MINE);
    // The server, not home, sends the span as a stub; the other Mac, not
    // knowing, sends it whole. The home takes both, in either order.
    let stub = site.send_to(&mac, MINE).batches;
    let whole = air.send_to(&mac, MINE).batches;
    let mut ends = Vec::new();
    for stub_first in [true, false] {
        let mut home = mac.clone();
        let (one, two) = if stub_first {
            (&stub, &whole)
        } else {
            (&whole, &stub)
        };
        for batch in one.iter().chain(two) {
            home.take(batch.clone());
        }
        let a1 = home.rows.iter().find(|row| row.id == "a1").unwrap();
        assert!(!a1.forgotten && home.bodies.contains_key("a1"));
        ends.push((home.rows, home.bodies));
    }
    assert_eq!(ends[0], ends[1]);
    // Taking its own export back loses nothing either.
    let file = Side::named(Core::Pc, "dev_new");
    let pages = mac.send_to(&file, MINE).batches;
    let before = (mac.rows.clone(), mac.bodies.clone());
    for page in pages {
        mac.take(page);
    }
    assert_eq!((mac.rows.clone(), mac.bodies.clone()), before);
}

#[test]
fn a_retract_from_a_side_not_seen_before_is_taken() {
    let (_, mut site) = shared_node();
    // A Mac linked just now: its first row in the audience is the retract,
    // and the server has never seen a row of it.
    let mut new = Side::named(Core::Pc, "dev_new");
    new.sync_from(&site, MINE);
    new.write(row("n1", "dev_new", 1, T + 10, MINE, KEY, "retract"), None);
    site.sync_from(&new, MINE);
    let (rows, bodies) = held_of(&site, KEY);
    assert!(rows.iter().all(|(_, stub)| *stub) && bodies.is_empty());
    assert_eq!(
        node(&site.folded(MINE, false), KEY)["retracted"],
        json!(["dev_new"])
    );
    assert_eq!(node(&new.folded(MINE, false), KEY)["name"]["value"], "发版");
}

#[test]
fn a_retract_whose_home_no_one_is_keeps_the_node_nowhere() {
    let (mut mac, mut site) = shared_node();
    mac.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    let mut ghost = mac.send_to(&site, MINE).batches.remove(0);
    ghost.rows[0].origin = "dev_ghost".into();
    ghost.rows[0].hlc = hlc("dev_ghost", T + 10).parse().unwrap();
    ghost.rows[0].dseq = Some(1);
    // Taken, not refused: a home no side is keeps nothing, so it is a
    // forget everywhere (the safe way), on the Mac too.
    let mut other = Side::named(Core::Pc, "dev_other");
    other.sync_from(&site, MINE);
    site.take(ghost.clone());
    other.take(ghost);
    for side in [&site, &other] {
        let (rows, bodies) = held_of(side, KEY);
        assert!(rows.iter().all(|(_, stub)| *stub) && bodies.is_empty());
        assert_eq!(
            node(&side.folded(MINE, false), KEY)["retracted"],
            json!(["dev_ghost"])
        );
    }
}

#[test]
fn a_side_that_took_back_a_node_merged_into_another_keeps_it() {
    let (mut mac, mut site) = shared_node();
    let mut air = Side::named(Core::Pc, "dev_air");
    // The Mac merges 「别的事」 into 「发版」, and the other Mac learns of it.
    mac.write(
        row("x1", "dev_mac", 4, T + 5, MINE, OTHER_KEY, "node.span"),
        Some(span(MINE, "别的事其实就是发版。")),
    );
    let mut absorbed = row("x2", "dev_mac", 5, T + 6, MINE, OTHER_KEY, "node.absorbed");
    absorbed.to = Some(object(MINE, KEY));
    mac.write(
        absorbed,
        Some(
            json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "how": "merged",
                     "into": KEY, "reason": "同一件事", "because": ["span:x1"] }),
        ),
    );
    site.sync_from(&mac, MINE);
    air.sync_from(&site, MINE);
    // Then, each offline: the partner takes 「发版」 back on the Mac, and
    // 「别的事」 on the other Mac.
    mac.write(row("x3", "dev_mac", 6, T + 10, MINE, KEY, "retract"), None);
    air.write(
        row("a1", "dev_air", 1, T + 11, MINE, OTHER_KEY, "retract"),
        None,
    );
    site.sync_from(&mac, MINE);
    site.sync_from(&air, MINE);
    air.sync_from(&site, MINE);
    mac.sync_from(&site, MINE);
    // The other Mac is a home of 「别的事」: its rows stay whole there,
    // though 「发版」, which it went into, is the Mac's alone.
    for id in ["r3", "x1", "x2"] {
        let row = air.rows.iter().find(|row| row.id == id).unwrap();
        assert!(!row.forgotten && air.bodies.contains_key(id), "{id}");
    }
    let (rows, bodies) = held_of(&air, KEY);
    assert!(rows.iter().all(|(_, stub)| *stub) && bodies.is_empty());
    // The Mac is a home of 「发版」, so of what went into it too.
    for id in ["r1", "r2", "r3", "x1", "x2"] {
        assert!(mac.bodies.contains_key(id), "{id}");
    }
    // The server keeps none of either.
    for id in ["r1", "r2", "r3", "x1", "x2"] {
        assert!(!site.bodies.contains_key(id), "{id}");
    }
}

#[test]
fn a_retract_sent_in_this_sides_name_is_a_forget_here() {
    let (mut mac, mut site) = shared_node();
    // Another side writes a retract that names the Mac as its writer (a
    // broken or hostile side, or the Mac's own row after it lost its log):
    // the Mac never pressed it, so it keeps nothing.
    site.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    mac.sync_from(&site, MINE);
    let (rows, bodies) = held_of(&mac, KEY);
    assert!(rows.iter().all(|(_, stub)| *stub) && bodies.is_empty());
    assert!(node(&mac.folded(MINE, false), KEY).get("name").is_none());
    // A side's home never travels: a batch carrying one is refused it.
    let (mut home, mut other) = shared_node();
    home.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    let mut batch = home.send_to(&other, MINE).batches.remove(0);
    batch.rows.push(home_of(&batch.rows[0]));
    let taken = other.take(batch);
    assert!(taken.admitted.refused.iter().any(|one| one.id == "r4h"));
    // Only the partner writes a retract, and it travels.
    let mut hers = row("r9", "dev_mac", 9, T, MINE, KEY, "retract");
    hers.author = myriad_persona_log::Author::Her;
    hers.epoch = Some(1);
    assert!(hers.check().is_err());
    let mut kept = home_of(&row("r9", "dev_mac", 9, T, MINE, KEY, "retract"));
    kept.replicate = myriad_persona_log::Replicate::Shared;
    kept.dseq = Some(9);
    assert!(kept.check().is_err());
}

#[test]
fn a_retract_reaches_spans_split_off_to_it_by_their_envelopes_alone() {
    let (mut mac, mut site) = shared_node();
    let mut air = Side::named(Core::Pc, "dev_air");
    air.sync_from(&site, MINE);
    // Her upkeep on the Mac splits the span r2 off 「发版」 into a part.
    const PART: &str = "minted:friday";
    let mut split = row("p1", "dev_mac", 4, T + 5, MINE, KEY, "node.split_off");
    split.to = Some(object(MINE, PART));
    split.takes = vec!["r2".into()];
    mac.write(
        split,
        Some(
            json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "to": PART,
                     "spans": ["r2"], "reason": "周五的事单独记", "because": ["span:r2"] }),
        ),
    );
    mac.write(
        row("p2", "dev_mac", 5, T + 6, MINE, PART, "node.created"),
        Some(created(PART, "task", "周五")),
    );
    // The partner takes the part back on the Mac.
    mac.write(row("p3", "dev_mac", 6, T + 7, MINE, PART, "retract"), None);
    site.sync_from(&mac, MINE);
    // The server keeps 「发版」 but not the span the part took, nor the
    // split that says so: their bodies go, their envelopes stay.
    let r2 = site.rows.iter().find(|row| row.id == "r2").unwrap();
    assert!(r2.forgotten && !site.bodies.contains_key("r2"));
    assert!(!site.bodies.contains_key("p1"));
    assert_eq!(
        node(&site.folded(MINE, false), KEY)["name"]["value"],
        "发版"
    );
    // A server that never held the split's body: the other Mac's whole
    // span reaches it first, then the server's rows, the split among them
    // only as a stub. Its envelope alone takes the span away.
    let mut fresh = Side::new(Core::Server);
    fresh.sync_from(&air, MINE);
    assert!(fresh.bodies.contains_key("r2"));
    fresh.sync_from(&site, MINE);
    let r2 = fresh.rows.iter().find(|row| row.id == "r2").unwrap();
    assert!(r2.forgotten && !fresh.bodies.contains_key("r2"));
    assert!(
        fresh
            .rows
            .iter()
            .any(|row| row.id == "p1" && row.forgotten && row.takes == ["r2"])
    );
    // And once the other Mac takes the server's rows, it drops it too.
    air.sync_from(&site, MINE);
    assert!(!air.bodies.contains_key("r2"));
    assert!(air.rows.iter().any(|row| row.id == "r2" && row.forgotten));
    // The home keeps all of it.
    for id in ["r2", "p1", "p2"] {
        assert!(mac.bodies.contains_key(id), "{id}");
    }
}

#[test]
fn a_node_merged_into_a_retracted_one_is_stubbed_from_its_envelope_alone() {
    let (mut mac, mut site) = shared_node();
    let mut air = Side::named(Core::Pc, "dev_air");
    air.sync_from(&site, MINE);
    // The other Mac writes on 「别的事」 while offline.
    air.write(
        row("a1", "dev_air", 1, T + 4, MINE, OTHER_KEY, "node.span"),
        Some(span(MINE, "别的事的一段。")),
    );
    // The Mac merges 「别的事」 into 「发版」 and takes 「发版」 back.
    let mut absorbed = row("x1", "dev_mac", 4, T + 5, MINE, OTHER_KEY, "node.absorbed");
    absorbed.to = Some(object(MINE, KEY));
    mac.write(
        absorbed,
        Some(
            json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "how": "merged",
                     "into": KEY, "reason": "同一件事", "because": ["node:r3"] }),
        ),
    );
    mac.write(row("x2", "dev_mac", 5, T + 6, MINE, KEY, "retract"), None);
    site.sync_from(&mac, MINE);
    // The server holds the merge only as a stub now; its envelope still
    // names where 「别的事」 went, so the other Mac's span comes in a stub.
    let x1 = site.rows.iter().find(|row| row.id == "x1").unwrap();
    assert!(x1.forgotten && x1.to.is_some());
    site.sync_from(&air, MINE);
    let a1 = site.rows.iter().find(|row| row.id == "a1").unwrap();
    assert!(a1.forgotten && !site.bodies.contains_key("a1"));
    let (rows, _) = held_of(&site, OTHER_KEY);
    assert!(rows.iter().all(|(_, stub)| *stub), "{rows:?}");
    // The home keeps its own rows whole; the other Mac's span reached it
    // only as a stub (当作忘掉).
    mac.sync_from(&site, MINE);
    for id in ["r1", "r2", "r3", "x1"] {
        assert!(mac.bodies.contains_key(id), "{id}");
    }
}

const THIRD_KEY: &str = "minted:third";

/// A merge of `from` into `into`, with its name in the envelope.
fn merge_of(id: &str, origin: &str, dseq: u64, at: u64, from: &str, into: &str) -> (Row, Value) {
    let mut merge = row(id, origin, dseq, at, MINE, from, "node.absorbed");
    merge.to = Some(object(MINE, into));
    let body = json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "how": "merged",
                       "into": into, "reason": "同一件事", "because": ["node:r3"] });
    (merge, body)
}

/// A split of `spans` off `from` into the part `part`.
fn split_of(id: &str, origin: &str, dseq: u64, at: u64, from: &str, part: &str) -> (Row, Value) {
    let mut split = row(id, origin, dseq, at, MINE, from, "node.split_off");
    split.to = Some(object(MINE, part));
    split.takes = vec!["r2".into()];
    let body = json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "to": part,
                       "spans": ["r2"], "reason": "单独记", "because": ["span:r2"] });
    (split, body)
}

/// The Mac, the server and the other Mac, each holding 「发版」, 「别的事」
/// and a third node.
fn three_sides() -> (Side, Side, Side) {
    let (mut mac, mut site) = shared_node();
    mac.write(
        row("r5", "dev_mac", 4, T + 3, MINE, THIRD_KEY, "node.created"),
        Some(created(THIRD_KEY, "topic", "第三件事")),
    );
    site.sync_from(&mac, MINE);
    let mut air = Side::named(Core::Pc, "dev_air");
    air.sync_from(&site, MINE);
    (mac, site, air)
}

/// Whether a side holds a row whole, its body with it.
fn whole(side: &Side, id: &str) -> bool {
    side.rows.iter().any(|row| row.id == id && !row.forgotten) && side.bodies.contains_key(id)
}

/// The server takes both Macs' rows, in one order or the other; then each
/// Mac takes the server's.
fn meet(mac: &mut Side, site: &mut Side, air: &mut Side, mac_first: bool) {
    if mac_first {
        site.sync_from(mac, MINE);
        site.sync_from(air, MINE);
    } else {
        site.sync_from(air, MINE);
        site.sync_from(mac, MINE);
    }
    mac.sync_from(site, MINE);
    air.sync_from(site, MINE);
}

#[test]
fn a_node_merged_into_two_is_reached_through_both_in_any_order() {
    for both_kept in [false, true] {
        let mut servers = Vec::new();
        for mac_first in [true, false] {
            let (mut mac, mut site, mut air) = three_sides();
            // Offline, each Mac merges 「别的事」 into another node: the Mac
            // into 「发版」 first, the other Mac into the third later, and
            // the partner takes the third back there.
            let (merge, body) = merge_of("m1", "dev_mac", 5, T + 5, OTHER_KEY, KEY);
            mac.write(merge, Some(body));
            let (merge, body) = merge_of("a1", "dev_air", 1, T + 6, OTHER_KEY, THIRD_KEY);
            air.write(merge, Some(body));
            air.write(
                row("a2", "dev_air", 2, T + 7, MINE, THIRD_KEY, "retract"),
                None,
            );
            if both_kept {
                // And 「发版」 on the Mac.
                mac.write(row("m2", "dev_mac", 6, T + 8, MINE, KEY, "retract"), None);
            }
            meet(&mut mac, &mut site, &mut air, mac_first);
            // The third's retract reaches 「别的事」 through the later merge
            // as well as the earlier one stands: only a home keeps it.
            for id in ["r3", "m1", "a1"] {
                assert!(!whole(&site, id), "{id} {mac_first}");
                assert_eq!(whole(&mac, id), both_kept && id != "a1", "{id} {mac_first}");
            }
            for id in ["r3", "a1"] {
                assert!(whole(&air, id), "{id} {mac_first}");
            }
            servers.push(site.shared(MINE));
        }
        assert_eq!(servers[0], servers[1]);
    }
}

#[test]
fn a_span_split_on_and_on_is_reached_from_every_part_in_any_order() {
    const PART: &str = "minted:friday";
    const PIECE: &str = "minted:friday-noon";
    let mut servers = Vec::new();
    for mac_first in [true, false] {
        let (mut mac, mut site, mut air) = three_sides();
        // The Mac splits the span r2 off 「发版」 into a part, and the other
        // Mac learns of it.
        let (split, body) = split_of("p1", "dev_mac", 5, T + 5, KEY, PART);
        mac.write(split, Some(body));
        mac.write(
            row("p2", "dev_mac", 6, T + 6, MINE, PART, "node.created"),
            Some(created(PART, "task", "周五")),
        );
        site.sync_from(&mac, MINE);
        air.sync_from(&site, MINE);
        // Then, each offline: the other Mac splits it on into a piece, and
        // the partner takes the part back on the Mac.
        let (split, body) = split_of("a1", "dev_air", 1, T + 7, PART, PIECE);
        air.write(split, Some(body));
        air.write(
            row("a2", "dev_air", 2, T + 8, MINE, PIECE, "node.created"),
            Some(created(PIECE, "task", "周五中午")),
        );
        mac.write(row("p3", "dev_mac", 7, T + 9, MINE, PART, "retract"), None);
        meet(&mut mac, &mut site, &mut air, mac_first);
        // The part's retract reaches the span wherever a split moved it on.
        assert!(!whole(&site, "r2") && !whole(&air, "r2"), "{mac_first}");
        assert!(whole(&mac, "r2"), "{mac_first}");
        servers.push(site.shared(MINE));
    }
    assert_eq!(servers[0], servers[1]);
}

#[test]
fn an_early_merge_from_elsewhere_never_keeps_a_retract_from_its_node() {
    let mut servers = Vec::new();
    for mac_first in [true, false] {
        let (mut mac, mut site, mut air) = three_sides();
        // The other Mac relays a merge of 「别的事」 into the third that a
        // side wrote with an early clock.
        let (merge, body) = merge_of("z1", "dev_odd", 1, T + 4, OTHER_KEY, THIRD_KEY);
        air.write(merge, Some(body));
        // The Mac merges 「别的事」 into 「发版」 later, and the partner
        // takes 「发版」 back there.
        let (merge, body) = merge_of("m1", "dev_mac", 5, T + 5, OTHER_KEY, KEY);
        mac.write(merge, Some(body));
        mac.write(row("m2", "dev_mac", 6, T + 6, MINE, KEY, "retract"), None);
        meet(&mut mac, &mut site, &mut air, mac_first);
        for id in ["r3", "m1"] {
            assert!(!whole(&site, id) && !whole(&air, id), "{id} {mac_first}");
            assert!(whole(&mac, id), "{id} {mac_first}");
        }
        servers.push(site.shared(MINE));
    }
    assert_eq!(servers[0], servers[1]);
}

#[test]
fn a_merge_is_named_by_its_envelope_whatever_its_body_says() {
    let (mut mac, mut site, mut air) = three_sides();
    // The other Mac sends a merge of 「别的事」 whose envelope says 「发版」
    // and whose body says the third: the fold passes it by (bad_body), but
    // its envelope names where 「别的事」 went, for what is reached.
    let (mut merge, body) = merge_of("a1", "dev_air", 1, T + 5, OTHER_KEY, THIRD_KEY);
    merge.to = Some(object(MINE, KEY));
    air.write(merge, Some(body));
    mac.write(row("m1", "dev_mac", 5, T + 6, MINE, KEY, "retract"), None);
    meet(&mut mac, &mut site, &mut air, false);
    // The retract of 「发版」 reaches 「别的事」 through the envelope: the
    // server keeps none of it, the home keeps it whole.
    assert!(!whole(&site, "r3") && !whole(&site, "a1"));
    assert!(whole(&mac, "r3"));
    for side in [&site, &mac] {
        assert!(side.ledger.iter().any(forget::is_name));
    }
    // Before the retract, the fold says why it passed the merge by.
    let (_, mut site, mut air) = three_sides();
    let (mut merge, body) = merge_of("a1", "dev_air", 1, T + 5, OTHER_KEY, THIRD_KEY);
    merge.to = Some(object(MINE, KEY));
    air.write(merge, Some(body));
    site.sync_from(&air, MINE);
    let folded = site.folded(MINE, false);
    assert!(node(&folded, OTHER_KEY).get("absorbedInto").is_none());
    assert!(
        folded["skipped"]
            .as_array()
            .unwrap()
            .contains(&json!({ "id": "a1", "why": "bad_body" })),
        "{folded}"
    );
}

#[test]
fn a_merge_written_after_a_retract_is_void_on_every_side() {
    let (mut mac, mut site, mut air) = three_sides();
    // The partner takes 「别的事」 back on the other Mac; the Mac, offline,
    // merges it into 「发版」 after, and its merge reaches the server first,
    // whole, and from there the other Mac.
    air.write(
        row("a1", "dev_air", 1, T + 5, MINE, OTHER_KEY, "retract"),
        None,
    );
    let (merge, body) = merge_of("m1", "dev_mac", 5, T + 6, OTHER_KEY, KEY);
    mac.write(merge, Some(body));
    site.sync_from(&mac, MINE);
    air.sync_from(&site, MINE);
    site.sync_from(&air, MINE);
    mac.sync_from(&site, MINE);
    // The home kept the merge's body, but shows 「别的事」 as its own node:
    // the merge came after the retract, so it is void.
    assert!(whole(&air, "m1") && whole(&air, "r3"));
    let folded = air.folded(MINE, false);
    let other = node(&folded, OTHER_KEY);
    assert!(other.get("absorbedInto").is_none(), "{other}");
    assert_eq!(other["name"]["value"], "别的事");
    assert!(
        folded["skipped"]
            .as_array()
            .unwrap()
            .contains(&json!({ "id": "m1", "why": "void" })),
        "{folded}"
    );
    // Every other side keeps nothing of it.
    for side in [&site, &mac] {
        assert!(!whole(side, "m1") && !whole(side, "r3"));
    }
}

#[test]
fn a_merge_into_a_retracted_node_after_its_retract_reaches_nothing_in_any_order() {
    let mut servers = Vec::new();
    for mac_first in [true, false] {
        let (mut mac, mut site, mut air) = three_sides();
        // The other Mac takes 「别的事」 back; the Mac, offline, merges 「发版」
        // into it after. Were the merge to count, the retract would reach
        // 「发版」 through it.
        air.write(
            row("a1", "dev_air", 1, T + 5, MINE, OTHER_KEY, "retract"),
            None,
        );
        let (merge, body) = merge_of("m1", "dev_mac", 5, T + 6, KEY, OTHER_KEY);
        mac.write(merge, Some(body));
        meet(&mut mac, &mut site, &mut air, mac_first);
        // 「发版」 stays whole everywhere, and the merge names nothing.
        for side in [&site, &mac, &air] {
            assert!(whole(side, "r1") && whole(side, "r2"), "{mac_first}");
            let folded = side.folded(MINE, false);
            let release = node(&folded, KEY);
            assert!(release.get("absorbedInto").is_none(), "{release}");
            assert!(
                folded["skipped"]
                    .as_array()
                    .unwrap()
                    .contains(&json!({ "id": "m1", "why": "void" })),
                "{folded}"
            );
        }
        servers.push(site.shared(MINE));
    }
    assert_eq!(servers[0], servers[1]);
}

#[test]
fn a_home_loses_its_node_only_to_a_forget_of_a_node_it_keeps() {
    let (mut mac, mut site, mut air) = three_sides();
    // 「别的事」 gets a span, which a split moves to the third node; then it
    // goes into 「发版」; then the Mac takes 「别的事」 back.
    mac.write(
        row("s1", "dev_mac", 5, T + 4, MINE, OTHER_KEY, "node.span"),
        Some(span(MINE, "别的事的一段。")),
    );
    let mut split = row("s2", "dev_mac", 6, T + 5, MINE, OTHER_KEY, "node.split_off");
    split.to = Some(object(MINE, THIRD_KEY));
    split.takes = vec!["s1".into()];
    mac.write(
        split,
        Some(
            json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "to": THIRD_KEY,
                     "spans": ["s1"], "reason": "单独记", "because": ["span:s1"] }),
        ),
    );
    let (merge, body) = merge_of("s3", "dev_mac", 7, T + 6, OTHER_KEY, KEY);
    mac.write(merge, Some(body));
    mac.write(
        row("s4", "dev_mac", 8, T + 7, MINE, OTHER_KEY, "retract"),
        None,
    );
    meet(&mut mac, &mut site, &mut air, true);
    // The partner forgets 「发版」, which 「别的事」 went into, and the third
    // node, which took its span: neither is kept on the Mac, so neither
    // takes 「别的事」's rows off its home.
    site.write(row("f1", "site", 10, T + 8, MINE, KEY, "forget"), None);
    site.write(
        row("f2", "site", 11, T + 9, MINE, THIRD_KEY, "forget"),
        None,
    );
    mac.sync_from(&site, MINE);
    for id in ["r3", "s1", "s2", "s3"] {
        assert!(whole(&mac, id), "{id}");
    }
    assert!(!whole(&mac, "r1") && !whole(&mac, "r5"));
    // A forget of 「别的事」 itself does.
    site.write(
        row("f3", "site", 12, T + 10, MINE, OTHER_KEY, "forget"),
        None,
    );
    mac.sync_from(&site, MINE);
    for id in ["r3", "s1", "s2", "s3"] {
        assert!(!whole(&mac, id), "{id}");
    }
}

#[test]
fn a_merge_whose_body_is_withheld_still_carries_its_name() {
    let (mut mac, mut site, mut air) = three_sides();
    // The other Mac merges 「别的事」 into 「发版」 with a key in its reason:
    // the body never leaves it, the row does, with its envelope.
    let (merge, mut body) = merge_of("a1", "dev_air", 1, T + 5, OTHER_KEY, KEY);
    body["reason"] = json!(format!("同一件事 {}", token()));
    air.write(merge, Some(body));
    meet(&mut mac, &mut site, &mut air, true);
    assert!(!site.bodies.contains_key("a1") && !mac.bodies.contains_key("a1"));
    // The Mac takes 「发版」 back: the server keeps nothing of 「别的事」.
    mac.write(row("m1", "dev_mac", 5, T + 6, MINE, KEY, "retract"), None);
    meet(&mut mac, &mut site, &mut air, true);
    assert!(!whole(&site, "r3") && !whole(&air, "r3"));
    assert!(whole(&mac, "r3"));
}

#[test]
fn a_home_restored_onto_a_new_computer_is_its_home() {
    use crate::home::{self, SendInput as HomeSend, TakeInput as HomeTake};
    let (mut mac, mut site, mut air) = three_sides();
    mac.write(
        row("m1", "dev_mac", 5, T + 5, MINE, OTHER_KEY, "retract"),
        None,
    );
    meet(&mut mac, &mut site, &mut air, true);
    // The Mac's export carries what it keeps in a home page.
    let page = home::send(HomeSend {
        audience: MINE.into(),
        rows: mac.of(MINE),
        bodies: mac.bodies.clone(),
        ledger: mac.ledger.clone(),
        holder: mac.holder.clone(),
    })
    .unwrap();
    let ids: Vec<&str> = page.rows.iter().map(|row| row.id.as_str()).collect();
    assert!(ids.contains(&"r3") && ids.contains(&"m1"), "{ids:?}");
    assert!(!ids.contains(&"r1"), "{ids:?}");
    assert!(page.rows.iter().any(|row| row.kind == "retract.home"));
    // A site or a server never writes or takes one.
    let server = Holder {
        core: Core::Server,
        owner: "7".into(),
    };
    assert!(
        home::send(HomeSend {
            audience: MINE.into(),
            rows: mac.of(MINE),
            bodies: mac.bodies.clone(),
            ledger: mac.ledger.clone(),
            holder: server.clone(),
        })
        .is_err()
    );
    assert!(
        home::take(HomeTake {
            page: page.clone(),
            holder: server,
            ledger: Vec::new(),
        })
        .is_err()
    );
    // A new Mac takes the home page, then the log from the server: it is
    // the home now, and keeps 「别的事」 whole.
    let mut fresh = Side::named(Core::Pc, "dev_new");
    let taken = home::take(HomeTake {
        page: page.clone(),
        holder: fresh.holder.clone(),
        ledger: Vec::new(),
    })
    .unwrap();
    fresh.rows = taken.rows;
    fresh.bodies = taken.bodies;
    fresh.ledger = taken.ledger;
    fresh.sync_from(&site, MINE);
    assert!(whole(&fresh, "r3"));
    assert!(whole(&fresh, "r1"));
    let folded = fresh.folded(MINE, false);
    let other = node(&folded, OTHER_KEY);
    assert_eq!(other["name"]["value"], "别的事", "{other}");
    // A page with a row of a node no mark keeps is refused.
    let mut stray = page;
    stray.rows.retain(|row| !row.kind.starts_with("retract"));
    assert!(
        home::take(HomeTake {
            page: stray,
            holder: fresh.holder.clone(),
            ledger: Vec::new(),
        })
        .is_err()
    );
}

#[test]
fn upkeep_leaves_a_retracted_node_to_a_local_run_on_its_home() {
    use crate::{
        Sync,
        upkeep::{ReadInput, UpkeepInput, prepare, read},
    };
    let (mut mac, _site, _air) = three_sides();
    mac.write(row("m1", "dev_mac", 5, T + 5, MINE, KEY, "retract"), None);
    let id = |key: &str| node_id(MINE, key).unwrap();
    let input = |node: &str, candidates: Vec<String>| UpkeepInput {
        rows: mac.of(MINE),
        bodies: mac.bodies.clone(),
        audience: MINE.into(),
        node: Some(id(node)),
        candidates,
        inbox: Vec::new(),
        ledger: mac.ledger.clone(),
    };
    // A shared run on another node is never shown it, so never merges into
    // it.
    let prepared = prepare(input(OTHER_KEY, vec![id(KEY), id(THIRD_KEY)])).unwrap();
    assert!(!prepared.local);
    assert_eq!(prepared.candidates.len(), 1);
    // A run on it writes local rows only.
    assert!(prepare(input(KEY, Vec::new())).unwrap().local);
    let run = |sync: Sync| {
        read(ReadInput {
            input: input(KEY, Vec::new()),
            run: "run1".into(),
            sync,
            output: r#"{"actions": []}"#.into(),
        })
    };
    assert!(run(Sync::Shared).is_err());
    let read = run(Sync::Local("dev_mac".into())).unwrap();
    assert!(!read.writes.is_empty());
    assert!(
        read.writes
            .iter()
            .all(|write| write.body["sync"] == json!({ "local": "dev_mac" }))
    );
    // Its structure is frozen: a local run splits nothing off it and merges
    // it into nothing, and names no node this side does not keep.
    for output in [
        r#"{"actions": [{"do": "split", "as": "new1", "name": "单独", "spans": ["s1"], "reason": "单独记", "because": ["s1"]}]}"#,
        r#"{"actions": [{"do": "absorb", "into": "n2", "how": "merged", "reason": "同一件事", "because": ["s1"]}]}"#,
    ] {
        let read = crate::upkeep::read(ReadInput {
            input: input(KEY, vec![id(OTHER_KEY)]),
            run: "run2".into(),
            sync: Sync::Local("dev_mac".into()),
            output: output.into(),
        })
        .unwrap();
        assert!(read.fallback.is_some(), "{output}");
        assert!(
            read.writes
                .iter()
                .all(|write| !matches!(write.kind.as_str(), "node.split_off" | "node.absorbed")),
            "{output}"
        );
    }
}

/// A person's node written in her own audience anyway (a writer's bug): its
/// created row's body never travels, so the node must stay known as one
/// across batches, or a later body of it would reach every computer.
#[test]
fn a_person_in_her_own_audience_stays_held_back_across_batches() {
    use myriad_persona_sync::canonical::sha256_hex;
    const OWN: &str = "own:";
    // `nodes.id` refuses this id; a writer with a bug made it anyway.
    let node = format!(
        "node:{}",
        &sha256_hex("node\u{1f}own:\u{1f}person:9".as_bytes())[..32]
    );
    let mut writer = Side::new(Core::Pc);
    let mut made = row("p1", "dev_mac", 1, T, OWN, KEY, "node.created");
    made.object = node.clone();
    writer.write(made, Some(created("person:9", "person", "小九")));
    let mut site = Side::new(Core::Server);
    // The created row comes alone, its body kept back by the writer.
    let first = writer.send_to(&site, OWN).batches;
    assert_eq!(first[0].withheld[0].why, Why::PersonElsewhere);
    // A side that knows no such person does not take the sender's word:
    // no sender takes an ordinary node's bodies away.
    let mut stranger = site.clone();
    for batch in first.clone() {
        assert!(stranger.take(batch).elsewhere.is_empty());
    }
    // The server knows 小九 (her private talk is there).
    site.people = vec!["person:9".into()];
    let mut known: Vec<String> = Vec::new();
    for batch in first {
        known.extend(site.take(batch).elsewhere);
    }
    assert_eq!(
        known,
        std::slice::from_ref(&node),
        "the sender's word marks the node"
    );
    // Later, in a batch of its own, a span of the same node.
    let mut said = row("p2", "dev_mac", 2, T + 1, OWN, KEY, "node.span");
    said.object = node.clone();
    writer.write(said, Some(span(OWN, "小九说的话。")));
    let second = send(SendInput {
        audience: OWN.into(),
        rows: writer.of(OWN),
        bodies: writer.bodies.clone(),
        theirs: holdings(&site.of(OWN)).unwrap().vector,
        ask: Vec::new(),
        holder: site.holder.clone(),
        ledger: Vec::new(),
        elsewhere: Vec::new(),
    })
    .unwrap()
    .batches;
    let take_with = |known: Vec<String>| {
        take(TakeInput {
            people: Vec::new(),
            batch: second[0].clone(),
            holder: site.holder.clone(),
            now: NOW,
            known: BTreeMap::new(),
            members: BTreeMap::new(),
            ledger: None,
            epochs: BTreeMap::new(),
            held: Vec::new(),
            elsewhere: known,
        })
        .unwrap()
    };
    // The writer keeps it back too (it holds the created body); a batch
    // that carries it anyway is held back on the known node.
    assert_eq!(second[0].withheld[0].why, Why::PersonElsewhere);
    let mut carried = second[0].clone();
    carried.withheld.clear();
    carried.payloads.push(Carried {
        reference: "doc:p2".into(),
        data: {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .encode(canonical_bytes(&span(OWN, "小九说的话。")))
        },
    });
    let lone = |known: Vec<String>| {
        take(TakeInput {
            people: Vec::new(),
            batch: carried.clone(),
            holder: site.holder.clone(),
            now: NOW,
            known: BTreeMap::new(),
            members: BTreeMap::new(),
            ledger: None,
            epochs: BTreeMap::new(),
            held: Vec::new(),
            elsewhere: known,
        })
        .unwrap()
    };
    assert!(
        lone(Vec::new()).bodies.contains_key("p2"),
        "the gap without it"
    );
    let taken = lone(known.clone());
    assert!(taken.bodies.is_empty());
    assert_eq!(taken.withheld[0].why, Why::PersonElsewhere);
    assert!(take_with(known.clone()).bodies.is_empty());
    // And a side that did store a body of it sends none to a computer.
    site.rows.extend(taken.admitted.accepted.clone());
    site.bodies.insert("p2".into(), span(OWN, "小九说的话。"));
    let air = Side::named(Core::Pc, "dev_air");
    let sent = send(SendInput {
        audience: OWN.into(),
        rows: site.of(OWN),
        bodies: site.bodies.clone(),
        theirs: BTreeMap::new(),
        ask: Vec::new(),
        holder: air.holder.clone(),
        ledger: Vec::new(),
        elsewhere: known,
    })
    .unwrap();
    assert!(sent.batches.iter().all(|batch| batch.payloads.is_empty()));
}

/// The partner's one id: the site issues it, a Mac takes it over the link
/// and moves to it once (rows, bodies, ledger), files never change an id,
/// and no side holds the talk of an id that is not its partner's.
mod partner_id {
    use myriad_persona_log::partner::{self, Here};

    use super::*;
    use crate::migrate::{MigrateInput, migrate};

    const PART_KEY: &str = "minted:friday";
    const P0: &str = "partner_0199d5c0900033333333333333333333";
    const PF: &str = "partner_0199d5c0a00022222222222222222222";

    fn private(owner: &str) -> String {
        format!("personal:{owner}")
    }

    impl Side {
        fn owned(core: Core, origin: &str, owner: &str) -> Self {
            let mut side = Self::named(core, origin);
            side.holder.owner = owner.into();
            side
        }

        /// What it tells `partner.link` and `partner.file`.
        fn here(&self, linked: bool) -> Here {
            let mut audiences: Vec<String> =
                self.rows.iter().map(|row| row.audience.clone()).collect();
            audiences.sort();
            audiences.dedup();
            Here {
                id: (self.holder.owner != partner::LEGACY).then(|| self.holder.owner.clone()),
                linked,
                audiences,
            }
        }

        /// Links to the site's id, as a host does: `partner.link`, then
        /// moving its rows, bodies and ledger once before it syncs.
        fn link_to(&mut self, linked: bool, id: &str) {
            let link = partner::link(&self.here(linked), id).unwrap();
            if let Some(from) = link.from {
                let out = migrate(MigrateInput {
                    rows: self.rows.clone(),
                    bodies: self.bodies.clone(),
                    ledger: self.ledger.clone(),
                    from,
                    to: id.into(),
                })
                .unwrap();
                assert!(out.clashes.is_empty());
                self.rows = out.rows;
                self.bodies.extend(out.bodies);
                self.ledger = out.ledger;
            }
            self.holder.owner = link.id;
        }

        /// The nodes it folds in `audience`, by key.
        fn by_key(&self, audience: &str) -> BTreeMap<String, Value> {
            let folded = self.folded(audience, false);
            folded["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|node| Some((node["key"].as_str()?.to_string(), node.clone())))
                .collect()
        }
    }

    /// A Mac with 「发版」 under `owner` (`local-owner` unless it took a
    /// file's id); `dev_a` also merged 「别的事」 into it and split a part
    /// off it.
    fn mac(origin: &str, at: u64, owner: &str) -> Side {
        let old = private(owner);
        let mut mac = Side::owned(Core::Pc, origin, owner);
        let id = |n: u64| format!("{origin}-{n}");
        mac.write(
            row(&id(1), origin, 1, at, &old, KEY, "node.created"),
            Some(created(KEY, "task", "发版")),
        );
        mac.write(
            row(&id(2), origin, 2, at + 1, &old, KEY, "node.span"),
            Some(span(&old, &format!("{origin} 记的：周五发版。"))),
        );
        if origin != "dev_a" {
            return mac;
        }
        mac.write(
            row(&id(3), origin, 3, at + 2, &old, OTHER_KEY, "node.created"),
            Some(created(OTHER_KEY, "topic", "别的事")),
        );
        let mut absorbed = row(&id(4), origin, 4, at + 3, &old, OTHER_KEY, "node.absorbed");
        absorbed.to = Some(object(&old, KEY));
        mac.write(
            absorbed,
            Some(json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "how": "merged",
                         "into": KEY, "reason": "同一件事", "because": [format!("node:{}", id(3))] })),
        );
        let mut split = row(&id(5), origin, 5, at + 4, &old, KEY, "node.split_off");
        split.to = Some(object(&old, PART_KEY));
        split.takes = vec![id(2)];
        mac.write(
            split,
            Some(json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "to": PART_KEY,
                         "spans": [id(2)], "reason": "周五的事单独记", "because": [format!("span:{}", id(2))] })),
        );
        mac.write(
            row(&id(6), origin, 6, at + 5, &old, PART_KEY, "node.created"),
            Some(created(PART_KEY, "task", "周五")),
        );
        mac
    }

    #[test]
    fn two_macs_linked_end_on_one_node_and_one_id_and_old_upkeep_still_folds() {
        let audience = private(P0);
        let mut ends = Vec::new();
        for a_first in [true, false] {
            let mut a = mac("dev_a", T, partner::LEGACY);
            let mut b = mac("dev_b", T + 100, partner::LEGACY);
            let mut site = Side::owned(Core::Server, "site", P0);
            a.link_to(false, P0);
            b.link_to(false, P0);
            let order: [&mut Side; 2] = if a_first {
                [&mut a, &mut b]
            } else {
                [&mut b, &mut a]
            };
            for mac in order {
                site.sync_from(mac, &audience);
            }
            a.sync_from(&site, &audience);
            b.sync_from(&site, &audience);
            for side in [&a, &b, &site] {
                assert!(side.rows.iter().all(|row| row.audience == audience));
                let nodes = side.by_key(&audience);
                // One node per key, under the id it always had.
                let release = &nodes[KEY];
                assert_eq!(
                    release["id"],
                    node_id(&private(partner::LEGACY), KEY).unwrap()
                );
                assert_eq!(release["name"]["value"], "发版");
                // The merge and the split written before the move fold.
                assert_eq!(
                    nodes[OTHER_KEY]["absorbedInto"]["into"],
                    node_id(&audience, KEY).unwrap()
                );
                assert_eq!(
                    nodes[PART_KEY]["spans"][0]["text"],
                    "dev_a 记的：周五发版。"
                );
                assert!(
                    side.folded(&audience, false)["skipped"]
                        .as_array()
                        .is_none(),
                    "nothing passed by on {}",
                    side.origin
                );
            }
            assert_eq!(
                canonical_bytes(&a.folded(&audience, true)),
                canonical_bytes(&site.folded(&audience, true))
            );
            ends.push(canonical_bytes(&site.folded(&audience, true)));
        }
        assert_eq!(ends[0], ends[1]);
    }

    /// A row of her own audience about the partner (her mood towards
    /// them), or a forget of it.
    fn about_partner(id: &str, origin: &str, dseq: u64, at: u64, partner: &str, kind: &str) -> Row {
        let mut about = row(id, origin, dseq, at, "own:", KEY, kind);
        about.object = format!("affect:{partner}");
        about
    }

    #[test]
    fn a_forget_of_all_about_the_partner_holds_across_the_move() {
        // The Mac forgot her mood towards the partner while still
        // `local-owner`; the site holds such a row under its id.
        let mut mac = Side::owned(Core::Pc, "dev_mac", partner::LEGACY);
        mac.write(
            about_partner("f1", "dev_mac", 1, T, partner::LEGACY, "forget"),
            None,
        );
        let mut site = Side::owned(Core::Server, "site", P0);
        site.write(
            about_partner("s1", "site", 1, T + 5, P0, "affect.change"),
            None,
        );
        // The move takes the Mac's ledger with it, so the forget now names
        // the partner's one id, and the site's row comes in a stub.
        mac.link_to(false, P0);
        assert!(
            mac.ledger
                .iter()
                .any(|entry| entry.target == format!("affect:{P0}"))
        );
        mac.sync_from(&site, "own:");
        let s1 = mac.rows.iter().find(|row| row.id == "s1").unwrap();
        assert!(s1.forgotten);
        // And the site, taking the Mac's forget, stubs its own.
        site.sync_from(&mac, "own:");
        assert!(site.rows.iter().any(|row| row.id == "s1" && row.forgotten));
    }

    #[test]
    fn a_forget_of_everything_about_the_partner_holds_across_the_move() {
        // The Mac forgot all about the partner in their talk while still
        // `local-owner`; the site holds a memory of them under its id.
        let mut mac = Side::owned(Core::Pc, "dev_mac", partner::LEGACY);
        let mut forget = row(
            "f1",
            "dev_mac",
            1,
            T,
            &private(partner::LEGACY),
            KEY,
            "forget",
        );
        forget.object = format!("subject:{}", partner::LEGACY);
        mac.write(forget, None);
        let mut site = Side::owned(Core::Server, "site", P0);
        let mut memory = row("s1", "site", 1, T + 5, &private(P0), KEY, "memory.write");
        memory.object = "memory:m1".into();
        memory.subject = Some(P0.into());
        site.write(memory, None);
        // The move takes the forget to the one id: the site's memory comes
        // in a stub, and the site, taking the forget, stubs its own.
        mac.link_to(false, P0);
        assert!(
            mac.ledger
                .iter()
                .any(|entry| entry.target == format!("subject:{P0}"))
        );
        mac.sync_from(&site, &private(P0));
        assert!(mac.rows.iter().any(|row| row.id == "s1" && row.forgotten));
        site.sync_from(&mac, &private(P0));
        assert!(site.rows.iter().any(|row| row.id == "s1" && row.forgotten));
    }

    #[test]
    fn a_row_of_hers_about_the_partner_under_another_id_is_refused() {
        // A Mac that has not moved still writes her mood towards
        // `local-owner`; another writes it towards someone else's partner.
        let mut unmoved = Side::owned(Core::Pc, "dev_old", partner::LEGACY);
        unmoved.write(
            about_partner("u1", "dev_old", 1, T, partner::LEGACY, "affect.change"),
            None,
        );
        let mut stranger = Side::owned(Core::Pc, "dev_x", PF);
        stranger.write(
            about_partner("x1", "dev_x", 1, T, PF, "affect.change"),
            None,
        );
        let mut moved = Side::owned(Core::Pc, "dev_mac", partner::LEGACY);
        moved.link_to(false, P0);
        let site = Side::owned(Core::Server, "site", P0);
        for side in [moved, site] {
            for from in [&unmoved, &stranger] {
                let mut side = side.clone();
                for batch in from.send_to(&side, "own:").batches {
                    let taken = side.take(batch);
                    assert!(taken.admitted.accepted.is_empty(), "{}", side.origin);
                    assert!(
                        taken
                            .admitted
                            .refused
                            .iter()
                            .all(|one| one.reason == myriad_persona_log::admit::Reason::NotHeld)
                    );
                }
                assert!(!side.rows.iter().any(|row| row.origin == from.origin));
            }
        }
        // Her mood towards the partner under the one id comes in.
        let mut mine = Side::owned(Core::Pc, "dev_b", P0);
        mine.write(
            about_partner("b1", "dev_b", 1, T, P0, "affect.change"),
            None,
        );
        let mut site = Side::owned(Core::Server, "site", P0);
        site.sync_from(&mine, "own:");
        assert!(site.rows.iter().any(|row| row.id == "b1"));
    }

    #[test]
    fn no_side_takes_talk_under_an_id_that_is_not_its_partners() {
        // A stranger's pages: from before (local-owner, no header partner),
        // and under another partner id.
        let stranger = mac("dev_x", T + 50, partner::LEGACY);
        let other = mac("dev_y", T + 60, PF);
        let page = |side: &Side, owner: &str| {
            let reader = Side::owned(Core::Pc, "dev_file", owner);
            side.send_to(&reader, &private(owner)).batches.remove(0)
        };
        let legacy = page(&stranger, partner::LEGACY);
        let foreign = page(&other, PF);
        // A Mac that holds anything, and the site.
        let mut mine = mac("dev_b", T, partner::LEGACY);
        mine.link_to(false, P0);
        let site = Side::owned(Core::Server, "site", P0);
        for side in [&mine, &site] {
            assert!(partner::file(&side.here(true), None).is_err());
            assert!(partner::file(&side.here(true), Some(PF)).is_err());
            assert!(partner::file(&side.here(true), Some(P0)).is_ok());
            for batch in [&legacy, &foreign] {
                let mut side = side.clone();
                let taken = side.take(batch.clone());
                assert!(taken.admitted.accepted.is_empty(), "{}", side.origin);
                assert!(taken.bodies.is_empty());
            }
        }
        // A Mac that holds anything and never linked takes no other Mac's
        // file either: link both, or import onto an empty Mac.
        let unlinked = mac("dev_c", T, partner::LEGACY);
        assert!(partner::file(&unlinked.here(false), None).is_err());
        assert!(partner::file(&unlinked.here(false), Some(PF)).is_err());
    }

    #[test]
    fn an_empty_mac_that_took_a_files_id_never_moves_it_to_another_account() {
        // Bob's file, under his id, goes onto Alice's empty Mac, which takes
        // its id.
        let writer = mac("dev_a", T, PF);
        let mut empty = Side::owned(Core::Pc, "dev_new", partner::LEGACY);
        let filed = partner::file(&empty.here(false), Some(PF)).unwrap();
        assert!(filed.adopt);
        empty.holder.owner = filed.id;
        for batch in writer.send_to(&empty, &private(PF)).batches {
            let taken = empty.take(batch);
            assert!(taken.admitted.refused.is_empty());
        }
        // Alice links it to her account: refused, so Bob's talk is never
        // relabelled into hers. Nor does another file's id replace his.
        assert!(partner::link(&empty.here(false), P0).is_err());
        assert!(partner::file(&empty.here(false), Some(P0)).is_err());
        assert!(empty.rows.iter().all(|row| row.audience == private(PF)));
        // Started empty, the Mac links to her account with nothing to move.
        let mut fresh = Side::owned(Core::Pc, "dev_new", partner::LEGACY);
        fresh.link_to(false, P0);
        assert!(fresh.rows.is_empty() && fresh.holder.owner == P0);
        // Bob's own account links the Mac that holds his file: the same id,
        // nothing to move, and his nodes keep their ids.
        let mut bobs = empty.clone();
        bobs.link_to(false, PF);
        let mut site = Side::owned(Core::Server, "site", PF);
        site.sync_from(&bobs, &private(PF));
        let nodes = site.by_key(&private(PF));
        assert_eq!(nodes[KEY]["id"], node_id(&private(PF), KEY).unwrap());
        // Linked now, it takes no other id.
        assert!(partner::link(&bobs.here(true), P0).is_err());
        assert!(partner::file(&bobs.here(true), Some(P0)).is_err());
    }
}
