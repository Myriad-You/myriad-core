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

use myriad_persona_log::{Core, Holder, MergeInput, Row, forget::Entry, merge, vector::holdings};
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
    /// The origins it writes as.
    here: Vec<String>,
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
            here: vec![origin.to_string()],
        }
    }

    /// Writes a row; a forget stubs what it reaches here at once, as the
    /// writing side does before anything is sent.
    fn write(&mut self, row: Row, body: Option<Value>) {
        if let Some(body) = body {
            self.bodies.insert(row.id.clone(), body);
        }
        let forget = row.kind == "forget" || row.kind == "retract";
        self.rows.push(row);
        if forget {
            let merged = merge(MergeInput {
                here: Some(self.here.clone()),
                log: self.rows.clone(),
                incoming: Vec::new(),
            });
            self.rows = merged.log;
            for id in merged.stubbed {
                self.bodies.remove(&id);
            }
            self.ledger = myriad_persona_log::ledger(&self.rows);
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
            here: Some(self.here.clone()),
            partner: None,
            batch,
            holder: self.holder.clone(),
            now: NOW,
            known: BTreeMap::new(),
            members: BTreeMap::new(),
            ledger: self.ledger.clone(),
            epochs: BTreeMap::new(),
            held,
        })
        .unwrap();
        let merged = merge(MergeInput {
            here: Some(self.here.clone()),
            log: self.rows.clone(),
            incoming: taken.admitted.accepted.clone(),
        });
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
                here: Some(self.here.clone()),
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
            here: Some(self.here.clone()),
            rows: self.of(audience),
            bodies,
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
        .filter(|row| row.object == object && row.kind != "retract")
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
        assert_eq!(gone["retracted"], "dev_mac");
        assert!(gone.get("name").is_none() && gone.get("spans").is_none());
        assert_eq!(node(&folded, OTHER_KEY)["name"]["value"], "别的事");
        // The home keeps its node and what it wrote; the server's span
        // reached it only as a stub.
        let folded = mac.folded(MINE, false);
        let kept = node(&folded, KEY);
        assert_eq!(kept["retracted"], "dev_mac");
        assert_eq!(kept["name"]["value"], "发版");
        let spans: Vec<&str> = kept["spans"]
            .as_array()
            .unwrap()
            .iter()
            .map(|span| span["text"].as_str().unwrap())
            .collect();
        assert_eq!(spans[..2], ["伙伴说周五发版。", "改到下周一。"]);
        // The server's span: a stub once the server knew of the retract;
        // pulled before that, the home keeps it as it came (it is home).
        let s1 = mac.rows.iter().find(|row| row.id == "s1").unwrap();
        assert_eq!(s1.forgotten, at == 0);
        assert_eq!(spans.len(), if at == 0 { 2 } else { 3 });
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
    assert_eq!(node(&air.folded(MINE, false), KEY)["retracted"], "dev_mac");
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
            assert_eq!(node(&side.folded(MINE, false), KEY)["retracted"], "dev_mac");
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
    assert_eq!(node(&new.folded(MINE, false), KEY)["retracted"], "dev_mac");
    // The home's own fold still has it all.
    assert_eq!(
        node(&mac.folded(MINE, false), KEY)["spans"][0]["text"],
        "伙伴说周五发版。"
    );
}

#[test]
fn a_retract_needs_to_know_which_side_folds_it() {
    let (mut mac, _) = shared_node();
    mac.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    let error = fold(FoldInput {
        rows: mac.of(MINE),
        bodies: mac.bodies.clone(),
        here: None,
    })
    .unwrap_err();
    assert!(error.to_string().contains("here"), "{error}");
    // Only the partner writes one, and it travels.
    let mut hers = row("r9", "dev_mac", 9, T, MINE, KEY, "retract");
    hers.author = myriad_persona_log::Author::Her;
    hers.epoch = Some(1);
    assert!(hers.check().is_err());
}

mod partner_id {
    use myriad_persona_log::{Partner, partner};

    use super::*;
    use crate::migrate::{MigrateInput, migrate};

    const OLD: &str = "personal:local-owner";
    const SITE_OLD: &str = "personal:o41";

    fn id() -> Partner {
        partner::minted(0x0199_d5c0_a000, &[0x5a; 10], Some("local-owner")).unwrap()
    }

    fn new() -> String {
        format!("personal:{}", id().id)
    }

    fn about_you(audience: &str) -> String {
        format!("person:{}", audience.strip_prefix("personal:").unwrap())
    }

    /// A Mac's log under its legacy id: 「关于你」, a task with a span and a
    /// relation to 「关于你」, and who knows it.
    fn legacy(audience: &str, origin: &str, prefix: &str) -> (Vec<Row>, BTreeMap<String, Value>) {
        let you = about_you(audience);
        let who = audience.strip_prefix("personal:").unwrap();
        let mut rows = Vec::new();
        let mut bodies = BTreeMap::new();
        let mut put = |n: u64, key: &str, kind: &str, body: Value| {
            let id = format!("{prefix}{n}");
            rows.push(row(&id, origin, n, T + n, audience, key, kind));
            bodies.insert(id, body);
        };
        put(1, &you, "node.created", created(&you, "person", "关于你"));
        put(2, KEY, "node.created", created(KEY, "task", "发版"));
        put(3, KEY, "node.span", span(audience, "伙伴说周五发版。"));
        put(
            4,
            KEY,
            "node.related",
            json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "relation": "involves",
                    "to": you, "reason": "伙伴的事", "because": [format!("span:{prefix}3")] }),
        );
        put(
            5,
            KEY,
            "node.revised",
            json!({ "version": 1, "sync": "shared", "saidBy": ["her"], "base": [],
                    "field": "who_knows", "value": [who] }),
        );
        (rows, bodies)
    }

    fn moved(
        rows: Vec<Row>,
        bodies: BTreeMap<String, Value>,
    ) -> (Vec<Row>, BTreeMap<String, Value>) {
        let out = migrate(MigrateInput {
            rows,
            bodies,
            partner: id(),
        })
        .unwrap();
        assert!(out.clashes.is_empty());
        (out.rows, out.bodies)
    }

    fn fold_of(rows: Vec<Row>, bodies: BTreeMap<String, Value>) -> Value {
        serde_json::to_value(
            fold(FoldInput {
                rows,
                bodies,
                here: None,
            })
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn moving_keeps_every_node_its_rows_and_its_old_id() {
        let (rows, bodies) = legacy(OLD, "dev_mac", "m");
        let before = fold_of(rows.clone(), bodies.clone());
        let (moved_rows, moved_bodies) = moved(rows.clone(), bodies);
        // Ids, sides, counts and clocks stay; the audience is the new one.
        for (was, is) in rows.iter().zip(&moved_rows) {
            assert_eq!(
                (&was.id, &was.origin, was.dseq, &was.hlc, &was.object),
                (&is.id, &is.origin, is.dseq, &is.hlc, &is.object)
            );
            assert_eq!(is.audience, new());
        }
        // Moving again changes nothing.
        let again = migrate(MigrateInput {
            rows: moved_rows.clone(),
            bodies: moved_bodies.clone(),
            partner: id(),
        })
        .unwrap();
        assert!(again.moved.is_empty());
        assert_eq!(again.bodies, moved_bodies);
        let after = fold_of(moved_rows, moved_bodies);
        let nodes = after["nodes"].as_array().unwrap();
        let you = id().id;
        let mine = format!("personal:{you}");
        let about = nodes
            .iter()
            .find(|node| node["id"] == node_id(&mine, &format!("person:{you}")).unwrap())
            .unwrap();
        assert_eq!(about["key"], format!("person:{you}"));
        assert_eq!(
            about["aliases"],
            json!([node_id(OLD, "person:local-owner").unwrap()])
        );
        let task = nodes
            .iter()
            .find(|node| node["id"] == node_id(&mine, KEY).unwrap())
            .unwrap();
        assert_eq!(task["name"]["value"], "发版");
        assert_eq!(task["spans"][0]["audience"], mine);
        assert_eq!(task["whoKnows"]["value"], json!([you]));
        // The relation still finds 「关于你」 by its key.
        assert_eq!(after["relations"][0]["to"], about["id"]);
        assert_eq!(
            before["nodes"].as_array().unwrap().len(),
            nodes.len(),
            "no node lost or split"
        );
    }

    #[test]
    fn two_ids_minted_apart_fold_into_one_node_on_either_side() {
        let (mac_rows, mac_bodies) = legacy(OLD, "dev_mac", "m");
        let (site_rows, site_bodies) = legacy(SITE_OLD, "site", "s");
        let mut partner = id();
        partner.aliases = vec!["local-owner".into(), "o41".into()];
        let one = |rows: Vec<Row>, bodies| {
            let out = migrate(MigrateInput {
                rows,
                bodies,
                partner: partner.clone(),
            })
            .unwrap();
            (out.rows, out.bodies)
        };
        let (mut rows, mut bodies) = one(mac_rows, mac_bodies);
        let (site_rows, site_bodies) = one(site_rows, site_bodies);
        rows.extend(site_rows);
        bodies.extend(site_bodies);
        let folded = fold_of(rows.clone(), bodies.clone());
        let nodes = folded["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 2, "one 「关于你」 and one task");
        let task = nodes.iter().find(|node| node["key"] == KEY).unwrap();
        assert_eq!(task["aliases"].as_array().unwrap().len(), 2);
        assert_eq!(task["spans"].as_array().unwrap().len(), 2);
        // Forgetting it by the id it reads as takes every old id along:
        // they are listed for the caller to write.
        let mut forget = row("f1", "dev_mac", 9, T + 50, &new(), KEY, "forget");
        forget.object = task["id"].as_str().map(|id| format!("node:{id}")).unwrap();
        rows.push(forget);
        let gone = fold(FoldInput {
            rows,
            bodies,
            here: None,
        })
        .unwrap();
        let forgets: Vec<String> = gone.forgets.iter().map(|one| one.target.clone()).collect();
        let mut olds: Vec<String> = task["aliases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| format!("node:{}", id.as_str().unwrap()))
            .collect();
        olds.sort();
        assert_eq!(forgets, olds);
    }

    #[test]
    fn a_batch_from_a_side_not_moved_yet_is_moved_as_it_arrives() {
        let (rows, bodies) = legacy(OLD, "dev_mac", "m");
        let (moved_rows, moved_bodies) = moved(rows.clone(), bodies.clone());
        // The old Mac sends the server its batch under the old id; the
        // server has moved.
        let mut old_mac = Side::new(Core::Pc);
        old_mac.holder.owner = "local-owner".into();
        for row in &rows {
            old_mac.write(row.clone(), bodies.get(&row.id).cloned());
        }
        let mut site = Side::new(Core::Server);
        site.holder.owner = id().id;
        let batches = old_mac.send_to(&site, OLD).batches;
        for batch in batches {
            let taken = take(TakeInput {
                batch,
                holder: site.holder.clone(),
                now: NOW,
                known: BTreeMap::new(),
                members: BTreeMap::new(),
                ledger: Vec::new(),
                epochs: BTreeMap::new(),
                held: Vec::new(),
                here: Some(site.here.clone()),
                partner: Some(id()),
            })
            .unwrap();
            assert!(taken.withheld.is_empty(), "{:?}", taken.withheld);
            site.rows.extend(taken.admitted.accepted);
            site.bodies.extend(taken.bodies);
        }
        // The same bytes the Mac's own move gives.
        let mut got = site.rows.clone();
        got.sort_by(|a, b| a.id.cmp(&b.id));
        let mut want = moved_rows;
        want.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(got, want);
        assert_eq!(site.bodies, moved_bodies);
    }
}
