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
    wire::{Batch, Carried, MAX_BODY_BYTES, SendInput, TakeInput, Why, Withheld, send, take},
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
    // The mark names the node alone: what the retract lists stays on it.
    home.takes.clear();
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
    /// Its core's ABI, as its hello says (`log2`: 2).
    abi: u32,
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
            abi: 2,
        }
    }

    /// Writes a row, as a host does: its own retract listing what its fold
    /// places in the node (`nodes.retract`), with the local `retract.home`
    /// beside it, each row through `log.ledger`; a forget or retract stubs
    /// what it reaches here at once, before anything is sent.
    fn write(&mut self, mut row: Row, body: Option<Value>) {
        if let Some(body) = body {
            self.bodies.insert(row.id.clone(), body);
        }
        if row.kind == "retract" && row.origin == self.origin && row.takes.is_empty() {
            row.takes = self.retract_list(&row, false);
        }
        // A forget pressed here lists the kept rows this side's fold shows
        // in the node (none off a home).
        if row.kind == "forget"
            && row.origin == self.origin
            && row.takes.is_empty()
            && row.object.starts_with("node:")
        {
            row.takes = self.retract_list(&row, true);
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

    /// What a retract (or, `forget`, a forget) of this row's node lists, as
    /// this side folds it now (one row's worth: no test node holds more).
    fn retract_list(&self, retract: &Row, forget: bool) -> Vec<String> {
        let rows = self.of(&retract.audience);
        let bodies = self
            .bodies
            .iter()
            .filter(|(id, _)| rows.iter().any(|row| &row.id == *id))
            .map(|(id, body)| (id.clone(), body.clone()))
            .collect();
        let mut lists = crate::fold::retract_lists(crate::fold::RetractInput {
            audience: retract.audience.clone(),
            node: retract.object.strip_prefix("node:").unwrap().into(),
            rows,
            bodies,
            ledger: self.ledger.clone(),
            forget,
        })
        .map(|lists| lists.takes)
        .unwrap_or_else(|_| vec![Vec::new()]);
        assert_eq!(lists.len(), 1);
        lists.remove(0)
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
            peer_abi: to.abi,
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
        peer_abi: 2,
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
fn a_retract_reaches_what_its_list_names_wherever_a_split_moved_it() {
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
    // The partner takes the part back on the Mac: its retract names the
    // span the part holds and the split that made it, as the Mac folds it.
    mac.write(row("p3", "dev_mac", 6, T + 7, MINE, PART, "retract"), None);
    let retract = mac.rows.iter().find(|row| row.id == "p3").unwrap();
    assert_eq!(retract.takes, ["p1", "r2"]);
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
    // A server that never held the split: the other Mac's whole span
    // reaches it first, then the server's rows. The retract's list alone
    // takes the span away.
    let mut fresh = Side::new(Core::Server);
    fresh.sync_from(&air, MINE);
    assert!(fresh.bodies.contains_key("r2"));
    fresh.sync_from(&site, MINE);
    let r2 = fresh.rows.iter().find(|row| row.id == "r2").unwrap();
    assert!(r2.forgotten && !fresh.bodies.contains_key("r2"));
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
fn a_row_of_a_merged_node_the_retract_did_not_list_is_not_reached() {
    let (mut mac, site) = shared_node();
    let mut air = Side::named(Core::Pc, "dev_air");
    air.sync_from(&site, MINE);
    // The other Mac writes on 「别的事」 while offline.
    air.write(
        row("a1", "dev_air", 1, T + 4, MINE, OTHER_KEY, "node.span"),
        Some(span(MINE, "别的事的一段。")),
    );
    // The Mac merges 「别的事」 into 「发版」 and takes 「发版」 back: its
    // retract lists what it holds of 「别的事」, not the span it never saw.
    let (merge, body) = merge_of("x1", "dev_mac", 4, T + 5, OTHER_KEY, KEY);
    mac.write(merge, Some(body));
    mac.write(row("x2", "dev_mac", 5, T + 6, MINE, KEY, "retract"), None);
    let retract = mac.rows.iter().find(|row| row.id == "x2").unwrap();
    assert_eq!(retract.takes, ["r3", "x1"]);
    for mac_first in [true, false] {
        let (mut mac, mut site, mut air) = (mac.clone(), site.clone(), air.clone());
        meet(&mut mac, &mut site, &mut air, mac_first);
        for side in [&site, &air] {
            assert!(!whole(side, "r3") && !whole(side, "x1"), "{mac_first}");
            // The span is not reached through the merge: its body stays,
            // and it folds into 「发版」, which shows nothing but its mark
            // on a side that does not keep it.
            assert!(whole(side, "a1"), "{mac_first}");
            let folded = side.folded(MINE, false);
            let release = node(&folded, KEY);
            assert_eq!(release["retracted"], json!(["dev_mac"]));
            assert!(release.get("spans").is_none(), "{release}");
            // The merge is a stub there, and still places 「别的事」 in
            // 「发版」 by its envelope: the span shows nowhere.
            assert!(!whole(side, "x1"));
            assert!(!shows(&folded, "别的事的一段。"), "{mac_first} {folded}");
        }
        // On the home it is one more line of 「发版」.
        let folded = mac.folded(MINE, false);
        let spans = node(&folded, KEY)["spans"].as_array().unwrap().clone();
        assert!(spans.iter().any(|one| one["text"] == "别的事的一段。"));
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

/// Whether any node of a fold shows a span of this text.
fn shows(folded: &Value, text: &str) -> bool {
    folded["nodes"].as_array().unwrap().iter().any(|node| {
        node["spans"]
            .as_array()
            .is_some_and(|spans| spans.iter().any(|span| span["text"] == text))
    })
}

/// The ids a fold stubs, for the host to delete their bodies.
fn stubbed(folded: &Value) -> Vec<String> {
    folded["stubbed"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .map(|id| id.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The nodes a fold says are reached, by id.
fn reached(folded: &Value) -> Vec<String> {
    folded["reached"]
        .as_array()
        .map(|all| {
            all.iter()
                .map(|one| one["target"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
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
fn a_retract_reaches_what_its_side_merged_into_it_never_what_another_did() {
    for both_kept in [false, true] {
        let mut servers = Vec::new();
        for mac_first in [true, false] {
            let (mut mac, mut site, mut air) = three_sides();
            // Offline, each Mac merges 「别的事」 into another node: the Mac
            // into 「发版」, the other Mac into the third, and the partner
            // takes the third back there.
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
            // The third's retract lists 「别的事」 and the other Mac's
            // merge; the Mac's merge, which it never saw, only 「发版」's
            // retract reaches.
            for id in ["r3", "a1"] {
                assert!(!whole(&site, id), "{id} {mac_first}");
                assert!(whole(&air, id), "{id} {mac_first}");
            }
            assert_eq!(whole(&site, "m1"), !both_kept, "{mac_first}");
            assert_eq!(whole(&air, "m1"), !both_kept, "{mac_first}");
            assert!(whole(&mac, "m1"), "{mac_first}");
            // The Mac keeps 「别的事」's own row only as a home of a retract
            // that lists it.
            assert_eq!(whole(&mac, "r3"), both_kept, "{mac_first}");
            assert!(!whole(&mac, "a1"), "{mac_first}");
            servers.push(site.shared(MINE));
        }
        assert_eq!(servers[0], servers[1]);
    }
}

#[test]
fn a_span_a_retract_lists_is_reached_wherever_a_split_moves_it_on() {
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
        // The part's retract names the span, wherever a split moved it on;
        // the split on, a row of the part, it reaches by its node.
        assert!(!whole(&site, "r2") && !whole(&air, "r2"), "{mac_first}");
        assert!(!whole(&site, "a1") && !whole(&air, "a1"), "{mac_first}");
        assert!(whole(&mac, "r2"), "{mac_first}");
        // The piece itself is not the part: it stays.
        assert!(whole(&site, "a2"), "{mac_first}");
        servers.push(site.shared(MINE));
    }
    assert_eq!(servers[0], servers[1]);
}

#[test]
fn a_merge_from_elsewhere_never_keeps_a_retract_from_what_it_lists() {
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
        // The other merge is not listed, and reached by nothing.
        assert!(whole(&site, "z1") && whole(&air, "z1"), "{mac_first}");
        servers.push(site.shared(MINE));
    }
    assert_eq!(servers[0], servers[1]);
}

#[test]
fn a_merge_is_named_by_its_envelope_for_a_forget_and_never_for_a_retract() {
    let (mut mac, mut site, mut air) = three_sides();
    // The other Mac sends a merge of 「别的事」 whose envelope says 「发版」
    // and whose body says the third: the fold passes it by (bad_body), but
    // its envelope names where 「别的事」 went, for what a forget reaches.
    let (mut merge, body) = merge_of("a1", "dev_air", 1, T + 5, OTHER_KEY, THIRD_KEY);
    merge.to = Some(object(MINE, KEY));
    air.write(merge, Some(body));
    meet(&mut mac, &mut site, &mut air, false);
    let folded = site.folded(MINE, false);
    assert!(node(&folded, OTHER_KEY).get("absorbedInto").is_none());
    assert!(
        folded["skipped"]
            .as_array()
            .unwrap()
            .contains(&json!({ "id": "a1", "why": "bad_body" })),
        "{folded}"
    );
    for side in [&site, &mac] {
        assert!(side.ledger.iter().any(forget::is_name));
    }
    // A retract of 「发版」 lists only what the Mac's fold puts in it, and
    // its fold put 「别的事」 nowhere: 「别的事」 stays on every side.
    let mut retracted = (mac.clone(), site.clone(), air.clone());
    retracted
        .0
        .write(row("m1", "dev_mac", 5, T + 6, MINE, KEY, "retract"), None);
    let (mut one, mut two, mut three) = retracted;
    meet(&mut one, &mut two, &mut three, true);
    assert!(whole(&two, "r3") && whole(&three, "r3"));
    assert!(!whole(&two, "r1"));
    // A forget of 「发版」 reaches 「别的事」 through the envelope on every
    // side, though the forget names 「发版」 alone.
    site.write(row("f1", "site", 10, T + 7, MINE, KEY, "forget"), None);
    meet(&mut mac, &mut site, &mut air, true);
    for side in [&site, &mac, &air] {
        assert!(!whole(side, "r3") && !whole(side, "a1"));
    }
}

#[test]
fn a_home_keeps_what_its_retract_reaches_wherever_the_fold_puts_it() {
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
    // The merge is a row of 「别的事」, so the retract reaches it; the home
    // keeps it, and folds it as it stands: 「别的事」 went into 「发版」.
    assert!(whole(&air, "m1") && whole(&air, "r3"));
    let folded = air.folded(MINE, false);
    let other = node(&folded, OTHER_KEY);
    assert_eq!(
        other["absorbedInto"]["into"],
        node_id(MINE, KEY).unwrap(),
        "{other}"
    );
    assert!(folded.get("skipped").is_none(), "{folded}");
    // Every other side keeps nothing of 「别的事」.
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
        // into it after. A retract never reaches through a merge.
        air.write(
            row("a1", "dev_air", 1, T + 5, MINE, OTHER_KEY, "retract"),
            None,
        );
        let (merge, body) = merge_of("m1", "dev_mac", 5, T + 6, KEY, OTHER_KEY);
        mac.write(merge, Some(body));
        meet(&mut mac, &mut site, &mut air, mac_first);
        // 「发版」 stays whole everywhere, the merge too, and it went into
        // 「别的事」: a side that does not keep that shows only its mark.
        for side in [&site, &mac, &air] {
            for id in ["r1", "r2", "m1"] {
                assert!(whole(side, id), "{id} {mac_first}");
            }
            let folded = side.folded(MINE, false);
            let release = node(&folded, KEY);
            assert_eq!(
                release["absorbedInto"]["into"],
                node_id(MINE, OTHER_KEY).unwrap(),
                "{release}"
            );
        }
        let folded = site.folded(MINE, false);
        assert!(node(&folded, OTHER_KEY).get("spans").is_none());
        servers.push(site.shared(MINE));
    }
    assert_eq!(servers[0], servers[1]);
}

#[test]
fn a_home_keeps_what_its_retract_reaches_through_a_merged_forget_and_loses_it_only_to_a_direct_one()
{
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
    // node, which took its span: each reaches 「别的事」's rows through a
    // merge or a split, and neither names the node the Mac took back, so
    // neither takes them off its home.
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
    // Every other side forgot them twice over.
    for side in [&site, &air] {
        assert!(!whole(side, "r3") && !whole(side, "s1"));
    }
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
fn a_merge_whose_body_is_withheld_names_a_forgets_reach_and_lists_nothing() {
    let (mut mac, mut site, mut air) = three_sides();
    // The other Mac merges 「别的事」 into 「发版」 with a key in its reason:
    // the body never leaves it, the row does, with its envelope.
    let (merge, mut body) = merge_of("a1", "dev_air", 1, T + 5, OTHER_KEY, KEY);
    body["reason"] = json!(format!("同一件事 {}", token()));
    air.write(merge, Some(body));
    meet(&mut mac, &mut site, &mut air, true);
    assert!(!site.bodies.contains_key("a1") && !mac.bodies.contains_key("a1"));
    // The Mac folds no merge it has no body of, so 「别的事」 stands apart
    // there, and taking 「发版」 back lists none of it.
    mac.write(row("m1", "dev_mac", 5, T + 6, MINE, KEY, "retract"), None);
    let retract = mac.rows.iter().find(|row| row.id == "m1").unwrap();
    assert!(retract.takes.is_empty());
    meet(&mut mac, &mut site, &mut air, true);
    assert!(whole(&site, "r3") && whole(&air, "r3") && whole(&mac, "r3"));
    assert!(!whole(&site, "r1"));
    // A forget of 「发版」 reaches it all the same, by the envelope.
    site.write(row("f1", "site", 10, T + 7, MINE, KEY, "forget"), None);
    meet(&mut mac, &mut site, &mut air, true);
    for side in [&site, &air, &mac] {
        assert!(!whole(side, "r3"));
    }
}

#[test]
fn a_home_restored_onto_a_new_computer_is_its_home() {
    use crate::home::{self, SendInput as HomeSend, TakeInput as HomeTake};
    let (mut mac, mut site, mut air) = three_sides();
    // A key slipped into a span of 「别的事」 before the partner took it
    // back: the home keeps it, but it never goes in a page.
    mac.write(
        row("k1", "dev_mac", 5, T + 4, MINE, OTHER_KEY, "node.span"),
        Some(span(MINE, &format!("伙伴贴了 {} 让我记着。", token()))),
    );
    // A local upkeep row on it: kept on its device, but in the page.
    let mut local = row("l1", "dev_mac", 0, T + 4, MINE, OTHER_KEY, "node.span");
    local.replicate = myriad_persona_log::Replicate::Local;
    local.dseq = None;
    mac.write(
        local,
        Some(json!({
            "version": 1, "sync": {"local": "dev_mac"}, "saidBy": ["her"],
            "audience": MINE, "text": "只在这台电脑上的一段。",
        })),
    );
    mac.write(
        row("m1", "dev_mac", 6, T + 5, MINE, OTHER_KEY, "retract"),
        None,
    );
    meet(&mut mac, &mut site, &mut air, true);
    // The Mac's export carries what it keeps in a home page.
    let send_as = |holder: Holder| {
        home::send(HomeSend {
            audience: MINE.into(),
            rows: mac.of(MINE),
            bodies: mac.bodies.clone(),
            ledger: mac.ledger.clone(),
            holder,
            elsewhere: Vec::new(),
        })
    };
    let page = send_as(mac.holder.clone()).unwrap();
    let ids: Vec<&str> = page.rows.iter().map(|row| row.id.as_str()).collect();
    assert!(ids.contains(&"r3") && ids.contains(&"m1"), "{ids:?}");
    assert!(!ids.contains(&"r1"), "{ids:?}");
    assert!(page.rows.iter().any(|row| row.kind == "retract.home"));
    assert!(ids.contains(&"k1"));
    assert_eq!(
        page.withheld,
        [Withheld {
            id: "k1".into(),
            why: Why::Secret
        }]
    );
    assert!(!page.payloads.iter().any(|one| one.reference == "doc:k1"));
    assert!(page.payloads.iter().any(|one| one.reference == "doc:l1"));
    // A site or a server never writes or takes one.
    let server = Holder {
        core: Core::Server,
        owner: "7".into(),
    };
    assert!(send_as(server.clone()).is_err());
    let take_as = |page: Batch, holder: Holder, held: Vec<Row>| {
        home::take(HomeTake {
            page,
            holder,
            ledger: Vec::new(),
            held,
        })
    };
    assert!(take_as(page.clone(), server, Vec::new()).is_err());
    // A new Mac takes the home page, then the log from the server: it is
    // the home now, and keeps 「别的事」 whole.
    let restore = |fresh: &mut Side| {
        let taken = take_as(page.clone(), fresh.holder.clone(), fresh.rows.clone()).unwrap();
        // The host stores each row as its own, replacing the one it holds.
        for row in &taken.rows {
            match fresh.rows.iter_mut().find(|held| held.id == row.id) {
                Some(held) => *held = row.clone(),
                None => fresh.rows.push(row.clone()),
            }
        }
        fresh.bodies.extend(taken.bodies.clone());
        fresh.ledger = taken.ledger.clone();
        taken
    };
    let mut fresh = Side::named(Core::Pc, "dev_new");
    let taken = restore(&mut fresh);
    assert!(taken.filled.is_empty());
    assert_eq!(
        taken.withheld,
        [Withheld {
            id: "k1".into(),
            why: Why::Secret
        }]
    );
    // The key's span is stored with its body missing, not as a stub.
    assert!(
        taken
            .rows
            .iter()
            .any(|row| row.id == "k1" && !row.forgotten)
    );
    fresh.sync_from(&site, MINE);
    assert!(whole(&fresh, "r3"));
    assert!(whole(&fresh, "r1"));
    assert!(whole(&fresh, "l1"));
    let folded = fresh.folded(MINE, false);
    let other = node(&folded, OTHER_KEY);
    assert_eq!(other["name"]["value"], "别的事", "{other}");
    // A new Mac that synced first holds 「别的事」 as stubs: the page fills
    // them, and it is the home as the other.
    let mut late = Side::named(Core::Pc, "dev_late");
    late.sync_from(&site, MINE);
    assert!(!whole(&late, "r3"));
    let taken = restore(&mut late);
    // The retract has no body, and the span with the key comes without
    // one: only 「别的事」's own row is filled.
    assert_eq!(taken.filled, ["r3"]);
    late.sync_from(&site, MINE);
    for id in ["r1", "r3"] {
        assert!(whole(&late, id), "{id}");
    }
    assert_eq!(late.folded(MINE, false), {
        let mut at = fresh.clone();
        at.origin = late.origin.clone();
        at.folded(MINE, false)
    });
    // A page with a row of a node no mark keeps is refused.
    let mut stray = page.clone();
    stray.rows.retain(|row| !row.kind.starts_with("retract"));
    assert!(take_as(stray, fresh.holder.clone(), Vec::new()).is_err());
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
    // it: a preference, not a rule of the log.
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
}

#[test]
fn a_forget_after_a_merge_into_a_retracted_node_ends_the_same_in_any_order() {
    let mut ends = Vec::new();
    for mac_first in [true, false] {
        let (mut mac, mut site, mut air) = three_sides();
        // The Mac takes 「发版」 back at t3; the other Mac, offline, merges
        // 「别的事」 into it at t5 and the partner forgets 「发版」 there at
        // t10. The merge always counts for the forget, whatever came first.
        mac.write(row("h1", "dev_mac", 5, T + 3, MINE, KEY, "retract"), None);
        let (merge, body) = merge_of("a1", "dev_air", 1, T + 5, OTHER_KEY, KEY);
        air.write(merge, Some(body));
        air.write(row("a2", "dev_air", 2, T + 10, MINE, KEY, "forget"), None);
        meet(&mut mac, &mut site, &mut air, mac_first);
        // And once more, so every side has every row.
        meet(&mut mac, &mut site, &mut air, mac_first);
        // Nothing of either is whole anywhere: the forget names 「发版」,
        // the home's own node, and reaches 「别的事」 through the merge.
        for side in [&mac, &site, &air] {
            for id in ["r1", "r2", "r3", "a1"] {
                assert!(!whole(side, id), "{id} {mac_first}");
            }
            assert!(whole(side, "r5"), "{mac_first}");
        }
        ends.push((site.shared(MINE), mac.shared(MINE), air.shared(MINE)));
    }
    assert_eq!(ends[0], ends[1]);
}

#[test]
fn a_forget_reached_through_a_merge_is_never_written_as_a_row() {
    let (mut mac, mut site, mut air) = three_sides();
    let (merge, body) = merge_of("m1", "dev_mac", 5, T + 5, OTHER_KEY, KEY);
    mac.write(merge, Some(body));
    meet(&mut mac, &mut site, &mut air, true);
    // The partner forgets 「发版」 on the Mac: the fold says 「别的事」 went
    // with it, for the Mac's own cleanup; the Mac writes the one forget.
    let before = mac.rows.len();
    mac.write(row("f1", "dev_mac", 6, T + 6, MINE, KEY, "forget"), None);
    assert_eq!(mac.rows.len(), before + 1);
    let folded = mac.folded(MINE, false);
    assert_eq!(
        folded["reached"],
        json!([{ "audience": MINE, "target": object(MINE, OTHER_KEY) }])
    );
    assert!(folded.get("forgets").is_none());
    // Every other side reaches 「别的事」 from that forget alone.
    meet(&mut mac, &mut site, &mut air, true);
    for side in [&site, &air] {
        assert!(!whole(side, "r3") && !whole(side, "m1"));
        let forgets = side.rows.iter().filter(|row| row.kind == "forget").count();
        assert_eq!(forgets, 1);
    }
}

#[test]
fn a_side_whose_core_is_older_gets_no_row_it_cannot_read() {
    let (mut mac, mut site) = shared_node();
    mac.write(row("r4", "dev_mac", 4, T + 10, MINE, KEY, "retract"), None);
    // A side whose hello does not list log2 is of ABI 1: it would take the
    // retract as nothing and keep every body, so nothing goes to it.
    site.abi = 1;
    let sent = mac.send_to(&site, MINE);
    assert!(sent.too_old && sent.batches.is_empty());
    // Before the retract, it got what it could read.
    let (before, mut old) = shared_node();
    old.abi = 1;
    assert!(!before.send_to(&old, MINE).too_old);
    // ABI 0 is no ABI.
    let refused = send(SendInput {
        ledger: mac.ledger.clone(),
        elsewhere: Vec::new(),
        audience: MINE.into(),
        rows: mac.of(MINE),
        bodies: mac.bodies.clone(),
        theirs: BTreeMap::new(),
        ask: Vec::new(),
        holder: site.holder.clone(),
        peer_abi: 0,
    });
    assert!(refused.is_err());
}

#[test]
fn a_retract_of_a_node_that_took_many_rows_is_written_in_lists_that_fit() {
    let (mut mac, _) = shared_node();
    // 「别的事」 holds more spans than one retract row lists, then goes into
    // 「发版」.
    let many = myriad_persona_log::row::MAX_RETRACT_TAKES + 10;
    for at in 0..many {
        mac.write(
            row(
                &format!("x{at:04}"),
                "dev_mac",
                4 + at as u64,
                T + 3 + at as u64,
                MINE,
                OTHER_KEY,
                "node.span",
            ),
            Some(span(MINE, "别的事的一段。")),
        );
    }
    let (merge, body) = merge_of(
        "y1",
        "dev_mac",
        4 + many as u64,
        T + 3 + many as u64,
        OTHER_KEY,
        KEY,
    );
    mac.write(merge, Some(body));
    let lists = crate::fold::retract_lists(crate::fold::RetractInput {
        audience: MINE.into(),
        node: node_id(MINE, KEY).unwrap(),
        rows: mac.of(MINE),
        bodies: mac.bodies.clone(),
        ledger: mac.ledger.clone(),
        forget: false,
    })
    .unwrap()
    .takes;
    assert_eq!(lists.len(), 2);
    assert_eq!(lists[0].len(), myriad_persona_log::row::MAX_RETRACT_TAKES);
    let all: Vec<&String> = lists.iter().flatten().collect();
    assert_eq!(all.len(), many + 2, "every span, r3 and the merge");
    assert!(all.windows(2).all(|pair| pair[0] < pair[1]));
    // Written as two retract rows, they reach it all.
    let mut site = Side::new(Core::Server);
    site.sync_from(&mac, MINE);
    for (at, takes) in lists.into_iter().enumerate() {
        let mut retract = row(
            &format!("z{at}"),
            "dev_mac",
            100 + many as u64 + at as u64,
            T + 100 + many as u64,
            MINE,
            KEY,
            "retract",
        );
        retract.takes = takes;
        mac.write(retract, None);
    }
    site.sync_from(&mac, MINE);
    assert!(!whole(&site, "r3") && !whole(&site, "x0000") && !whole(&site, "y1"));
    assert!(whole(&mac, "x0000"));
    // A node nothing went into lists nothing, in one row; one this side
    // does not hold is refused.
    let (fresh, _) = shared_node();
    let lists = |key: &str| {
        crate::fold::retract_lists(crate::fold::RetractInput {
            audience: MINE.into(),
            node: node_id(MINE, key).unwrap(),
            rows: fresh.of(MINE),
            bodies: fresh.bodies.clone(),
            ledger: fresh.ledger.clone(),
            forget: false,
        })
    };
    assert_eq!(lists(OTHER_KEY).unwrap().takes, [Vec::<String>::new()]);
    assert!(lists(THIRD_KEY).is_err());
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
        peer_abi: 2,
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
        peer_abi: 2,
    })
    .unwrap();
    assert!(sent.batches.iter().all(|batch| batch.payloads.is_empty()));
}

/// The Mac splits the span r2 off 「发版」 into a part, the server learns
/// of it, and the partner takes 「发版」 back on the Mac: the span is a row
/// of 「发版」, so the Mac keeps it, though it folds in the part.
fn split_then_retracted() -> (Side, Side) {
    const PART: &str = "minted:friday";
    let (mut mac, mut site) = shared_node();
    let (split, body) = split_of("p1", "dev_mac", 4, T + 5, KEY, PART);
    mac.write(split, Some(body));
    mac.write(
        row("p2", "dev_mac", 5, T + 6, MINE, PART, "node.created"),
        Some(created(PART, "task", "周五")),
    );
    site.sync_from(&mac, MINE);
    mac.write(row("p3", "dev_mac", 6, T + 7, MINE, KEY, "retract"), None);
    site.sync_from(&mac, MINE);
    (mac, site)
}

#[test]
fn a_home_deletes_nothing_it_keeps_when_a_part_it_split_off_is_forgotten() {
    const PART: &str = "minted:friday";
    let (mut mac, mut site) = split_then_retracted();
    // The partner forgets the part on the site. Its forget reaches the
    // span only through the split, which takes nothing the Mac keeps.
    site.write(row("f1", "site", 1, T + 8, MINE, PART, "forget"), None);
    mac.sync_from(&site, MINE);
    let folded = mac.folded(MINE, false);
    for id in ["r2", "p1"] {
        assert!(whole(&mac, id), "{id}");
        assert!(!stubbed(&folded).contains(&id.to_string()), "{id} {folded}");
    }
    // Nor does it show anywhere: it ends on a forgotten part.
    assert!(!shows(&folded, "伙伴说周五发版。"), "{folded}");
}

#[test]
fn a_shared_run_is_never_shown_what_a_retract_reaches() {
    use crate::upkeep::{UpkeepInput, prepare};
    const PART: &str = "minted:friday";
    let shown = |side: &Side, key: &str| {
        prepare(UpkeepInput {
            rows: side.of(MINE),
            bodies: side.bodies.clone(),
            audience: MINE.into(),
            node: Some(node_id(MINE, key).unwrap()),
            candidates: Vec::new(),
            inbox: Vec::new(),
            ledger: side.ledger.clone(),
        })
        .unwrap()
    };
    // A span split off into a part, then taken back with 「发版」: the
    // part is no node taken back, so a run on it is shared and is not
    // shown the span, though the Mac's fold shows it there.
    let (mac, _) = split_then_retracted();
    assert!(shows(&mac.folded(MINE, false), "伙伴说周五发版。"));
    let part = shown(&mac, PART);
    assert!(!part.local);
    assert!(part.spans.is_empty(), "{:?}", part.spans);
    // Before the retract it was.
    let (mut before, _) = shared_node();
    let (split, body) = split_of("p1", "dev_mac", 4, T + 5, KEY, PART);
    before.write(split, Some(body));
    before.write(
        row("p2", "dev_mac", 5, T + 6, MINE, PART, "node.created"),
        Some(created(PART, "task", "周五")),
    );
    assert_eq!(shown(&before, PART).spans.len(), 1);
    // 「发版」 merged into 「别的事」 after its retract: a shared run on
    // 「别的事」 is not shown 「发版」's span either; a local run on a node
    // taken back still sees its own.
    let (mut mac, _) = shared_node();
    mac.write(row("m1", "dev_mac", 4, T + 5, MINE, KEY, "retract"), None);
    assert!(shown(&mac, KEY).local);
    assert_eq!(shown(&mac, KEY).spans.len(), 1);
    let (merge, body) = merge_of("m2", "dev_mac", 5, T + 6, KEY, OTHER_KEY);
    mac.write(merge, Some(body));
    assert!(shows(&mac.folded(MINE, false), "伙伴说周五发版。"));
    let other = shown(&mac, OTHER_KEY);
    assert!(!other.local);
    assert!(other.spans.is_empty(), "{:?}", other.spans);
}

#[test]
fn a_forget_pressed_on_a_home_takes_the_kept_rows_it_showed() {
    const PART: &str = "minted:friday";
    // The split: the part shows the span the Mac keeps; the partner
    // forgets the part on the Mac, and the forget lists the span and the
    // split that moved it.
    let (mut mac, mut site) = split_then_retracted();
    mac.write(row("f1", "dev_mac", 7, T + 8, MINE, PART, "forget"), None);
    let forget = mac.rows.iter().find(|row| row.id == "f1").unwrap();
    assert_eq!(forget.takes, ["p1", "r2"]);
    for id in ["r2", "p1"] {
        assert!(!whole(&mac, id), "{id}");
    }
    // 「发版」's own created row stays kept.
    assert!(whole(&mac, "r1"));
    site.sync_from(&mac, MINE);
    assert!(!whole(&site, "r2"));
    // The merge: 「发版」, taken back, went into 「别的事」 after; the
    // partner forgets 「别的事」 on the Mac, which showed 「发版」 in it.
    let (mut mac, _) = shared_node();
    mac.write(row("m1", "dev_mac", 4, T + 5, MINE, KEY, "retract"), None);
    let (merge, body) = merge_of("m2", "dev_mac", 5, T + 6, KEY, OTHER_KEY);
    mac.write(merge, Some(body));
    // A forget from elsewhere, which lists nothing, takes none of it: the
    // Mac keeps 「发版」, and its fold does not tell it to clean it up.
    let mut elsewhere = mac.clone();
    let mut site = Side::new(Core::Server);
    site.write(row("f2", "site", 1, T + 7, MINE, OTHER_KEY, "forget"), None);
    elsewhere.sync_from(&site, MINE);
    for id in ["r1", "r2", "m2"] {
        assert!(whole(&elsewhere, id), "{id}");
    }
    let id = node_id(MINE, KEY).unwrap();
    assert!(!reached(&elsewhere.folded(MINE, false)).contains(&format!("node:{id}")));
    // Pressed on the Mac, it lists all of 「发版」 that 「别的事」 showed.
    mac.write(
        row("f3", "dev_mac", 6, T + 7, MINE, OTHER_KEY, "forget"),
        None,
    );
    let forget = mac.rows.iter().find(|row| row.id == "f3").unwrap();
    assert_eq!(forget.takes, ["m2", "r1", "r2"]);
    for id in ["r1", "r2", "m2"] {
        assert!(!whole(&mac, id), "{id}");
    }
    assert!(reached(&mac.folded(MINE, false)).contains(&format!("node:{id}")));
}
