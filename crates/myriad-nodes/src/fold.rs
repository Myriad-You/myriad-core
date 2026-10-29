//! Folding node rows into nodes and their relations, by the same rules on
//! every side.
//!
//! The rows (any order, repeated, with stubs and the forgets that reach
//! them) are checked, deduplicated by id and put in log order, as
//! `log.fold` does. Then, node by node:
//!
//! - **The key** comes from `node.created`; a created row whose audience
//!   and key make another id is passed by (`other_node`).
//! - **Head fields** (its kind, name, summary, status, who-knows, object,
//!   each person's progress, each view, open item and intent) are each
//!   folded from the rows that write them. A row's `base` names the versions
//!   it was written against; every version no row names as a base is
//!   current. One current version is the field. Two current versions that
//!   say different things mean two sides revised from the same version
//!   without seeing each other: the field is marked `bothChanged`, shows the
//!   one first in log order and lists the others as `rivals`, and no later
//!   write wins on its own. A row naming every current version settles it;
//!   `base` lists them, for the next write. The same write made on two sides
//!   (the same base, the same value) is no conflict, and a row that names
//!   one of the two has seen both. Older versions are its `history` (the
//!   newest [`HISTORY`] kept).
//! - **Spans and entries** stay, in log order, each with its three tags.
//! - **History** lists each kind change (from what, to what, why). It is
//!   not a relation.
//!
//! Relations are folded per audience: a `node.related` row stands until a
//! `node.unrelated` row on the same node names it. A relation to the node
//! itself is passed by; so is a `part_of` that would close a loop (taken in
//! log order: the one that closes it is passed by). A relation's other end
//! is a key made into an id in the row's own audience, so it never crosses
//! audiences; a relation to a forgotten node is gone with it, whether or
//! not that node's rows are here.
//!
//! A row that breaks a rule is passed by and listed in `skipped` with why:
//! a tag that does not fit its row, the partner's row not said by the
//! partner, a who-knows or progress for someone outside the audience, an
//! intent from words that were neither the partner's nor hers, a span on
//! 「关于你」, a kind change on a node whose kind is fixed.
//!
//! Forgetting a node (or its subject in its audience) folds it to nothing
//! but the mark that it was, in any order; its rows become stubs, listed in
//! `stubbed` for the caller to delete their bodies. Forgetting one span is
//! not in this batch.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::{
    Row, forget,
    kinds::Rule,
    row::{Audience, Author, object_type},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{Body, Change, IntentStatus, Object, ObjectKind, Relation, Sentence, Status, Trigger},
    key::{Key, Kind, node_id},
    limits::HISTORY,
    tags::{SaidBy, Sync, audience_fits},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FoldInput {
    /// Node rows and the forgets that reach them; other rows are passed by.
    pub rows: Vec<Row>,
    /// Each row's body, by row id: what its payload resolves to. A stub has
    /// none.
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
}

/// One version of a head field: what it says and the row that wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version<T> {
    pub value: T,
    pub row: String,
    /// The row's clock, Unix microseconds.
    pub at: i64,
    pub author: Author,
    pub said_by: Vec<SaidBy>,
    pub sync: Sync,
    /// Her rows: the lease epoch it was written under.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub epoch: Option<u64>,
}

/// A head field as its rows stand now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item<T> {
    #[serde(flatten)]
    pub current: Version<T>,
    /// Every current version's row, sorted: the `base` of the next write
    /// to this field. More than one when two sides wrote it from the same
    /// version, even when they wrote the same.
    pub base: Vec<String>,
    /// 两边都改过: two sides revised it from the same version.
    #[serde(skip_serializing_if = "is_false")]
    pub both_changed: bool,
    /// The other current versions, when it was changed on both sides.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rivals: Vec<Version<T>>,
    /// Older versions, oldest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Version<T>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindValue {
    pub kind: Kind,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub because: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewValue {
    pub text: String,
    pub because: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenValue {
    pub text: String,
    pub because: Vec<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentValue {
    pub text: String,
    pub because: Vec<String>,
    pub status: IntentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<Trigger>,
}

/// One span of experience on the node, with its three tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpanOut {
    pub row: String,
    pub at: i64,
    pub author: Author,
    pub said_by: Vec<SaidBy>,
    pub sync: Sync,
    pub audience: String,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
}

/// One entry on the node, with its three tags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryOut {
    pub row: String,
    pub at: i64,
    pub author: Author,
    pub said_by: Vec<SaidBy>,
    pub sync: Sync,
    pub audience: String,
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    #[serde(skip_serializing_if = "is_false")]
    pub quote_only: bool,
}

/// One kind change, in the node's history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindChange {
    pub row: String,
    pub at: i64,
    /// The kind of the version it was written against, when that is known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<Kind>,
    pub to: Kind,
    pub reason: String,
    pub because: Vec<String>,
    pub author: Author,
    pub said_by: Vec<SaidBy>,
}

/// One node as its rows stand now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub audience: String,
    /// Forgotten: nothing else about it reads back.
    #[serde(skip_serializing_if = "is_false")]
    pub forgotten: bool,
    /// Absent until a created row is folded (it may not have arrived yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Item<KindValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Item<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<Item<Vec<Sentence>>>,
    /// Absent reads as active.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Item<Status>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub who_knows: Option<Item<Vec<String>>>,
    /// A null value: the object was taken off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<Item<Option<Object>>>,
    /// By person. A null value: cleared.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub progress: BTreeMap<String, Item<Option<String>>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub views: BTreeMap<String, Item<ViewValue>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub open: BTreeMap<String, Item<OpenValue>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub intents: BTreeMap<String, Item<IntentValue>>,
    /// In log order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<SpanOut>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<EntryOut>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<KindChange>,
    /// Unix microseconds: its first created row (or first row), last row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

impl Node {
    fn empty(id: &str, audience: &str) -> Self {
        Self {
            id: id.to_string(),
            audience: audience.to_string(),
            forgotten: false,
            key: None,
            kind: None,
            name: None,
            summary: None,
            status: None,
            who_knows: None,
            object: None,
            progress: BTreeMap::new(),
            views: BTreeMap::new(),
            open: BTreeMap::new(),
            intents: BTreeMap::new(),
            spans: Vec::new(),
            entries: Vec::new(),
            history: Vec::new(),
            created_at: None,
            updated_at: None,
        }
    }
}

/// One relation between two nodes of one audience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationOut {
    pub audience: String,
    pub from: String,
    pub to: String,
    pub to_key: String,
    pub relation: Relation,
    /// From its first row: her reason and its evidence, who and when.
    pub reason: String,
    pub because: Vec<String>,
    pub author: Author,
    pub said_by: Vec<SaidBy>,
    pub sync: Sync,
    pub at: i64,
    /// Every standing row that says it, in log order; a `node.unrelated`
    /// naming all of them (on their own nodes) takes it back.
    pub rows: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Why a row was passed by. A row's place still counts as a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// Not a stub, and no body was given for it.
    NoBody,
    /// Its body does not fit its kind.
    BadBody,
    /// A created row whose audience and key make another node's id.
    OtherNode,
    /// Its sync tag does not fit the row (shared on a local row, local on a
    /// shared row or another device's).
    SyncMismatch,
    /// The partner's row, not said by the partner.
    AuthorMismatch,
    /// A span or entry said in another audience, not the partner's own
    /// words from a group kept in their private audience.
    AudienceTag,
    /// Who-knows or progress for someone not in this audience.
    NotInAudience,
    /// An intent from words that were neither the partner's nor hers.
    UntrustedIntent,
    /// A span on 「关于你」: only entries go there.
    NoSpansHere,
    /// A kind change on a node whose kind is fixed by its key.
    KindFixed,
    /// A status the node's kind never has (a person is never done).
    StatusForKind,
    /// A repo object off a repo key, or a repo node's object taken away.
    ObjectForKey,
    /// A relation from a node to itself.
    SelfRelation,
    /// A `part_of` that would close a loop.
    Cycle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub id: String,
    pub why: Why,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folded {
    /// By audience, then id.
    pub nodes: Vec<Node>,
    /// By audience, from, relation, to.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationOut>,
    /// Rows that had a payload and are stubs now, sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stubbed: Vec<String>,
    /// Node by node, each in log order; then relations, in log order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<Skipped>,
}

/// What is wrong with a row, by field or rule: the log's own message may
/// quote the row's values, and a refusal never does.
fn row_refusal(message: &str) -> &'static str {
    const BY_START: &[(&str, &str)] = &[
        (
            "row version",
            "a row: its version is not one this core reads",
        ),
        ("id ", "a row: its id is not a token"),
        ("node ", "a row: its origin is not a clock node name"),
        ("hlc node", "a row: its clock's node is not its origin"),
        ("her row", "a row: her row carries no lease epoch"),
        (
            "the owner's row",
            "a row: the owner's row carries a lease epoch",
        ),
        ("a shared row", "a row: a shared row needs a dseq from 1"),
        ("a local row", "a row: a local row takes no dseq"),
        (
            "audience",
            "a row: its audience is not personal:, own: or group:",
        ),
        (
            "subject is not",
            "a row: its subject is not of its audience",
        ),
        ("subject", "a row: its subject is not a token"),
        ("object", "a row: its object is not <type>:<id>"),
        ("kind", "a row: its kind is not one the log holds"),
        (
            "only a forget",
            "a row: only a forget is about a whole subject",
        ),
        ("payload", "a row: its payload ref or key is not a token"),
        ("a forget holds", "a row: a forget holds no payload"),
        ("a forget is never", "a row: a forget is never forgotten"),
        ("a stub", "a row: a stub keeps no payload"),
    ];
    if message.contains(" row is about a ") {
        return "a row: its kind is about another type of object";
    }
    BY_START
        .iter()
        .find(|(start, _)| message.starts_with(start))
        .map_or("a row is not a whole log row", |(_, refusal)| refusal)
}

/// A row's clock as Unix microseconds.
fn micros(row: &Row) -> i64 {
    // The clock's wall is at most 10^15 ms, well inside i64 microseconds.
    i64::try_from(row.hlc.wall_ms).unwrap_or(i64::MAX / 1000) * 1000
}

/// What every version written by one row carries.
struct Meta<'a> {
    row: &'a Row,
    time: i64,
    said_by: &'a [SaidBy],
    sync: &'a Sync,
}

impl Meta<'_> {
    fn version<T>(&self, value: T) -> Version<T> {
        Version {
            value,
            row: self.row.id.clone(),
            at: self.time,
            author: self.row.author,
            said_by: self.said_by.to_vec(),
            sync: self.sync.clone(),
            epoch: self.row.epoch,
        }
    }
}

/// The writes to one head field, in log order.
struct Register<T> {
    writes: Vec<(Vec<String>, Version<T>)>,
}

impl<T> Default for Register<T> {
    fn default() -> Self {
        Self { writes: Vec::new() }
    }
}

impl<T: Clone + PartialEq> Register<T> {
    fn push(&mut self, base: &[String], version: Version<T>) {
        self.writes.push((base.to_vec(), version));
    }

    fn resolve(self) -> Option<Item<T>> {
        let mut named: BTreeSet<&str> = self
            .writes
            .iter()
            .flat_map(|(base, _)| base.iter().map(String::as_str))
            .collect();
        // The same write made on two sides (the same base, the same value)
        // is one version: a write that saw one of them saw both.
        let twins: Vec<&str> = self
            .writes
            .iter()
            .filter(|(base, version)| {
                !named.contains(version.row.as_str())
                    && self.writes.iter().any(|(seen_base, seen)| {
                        named.contains(seen.row.as_str())
                            && seen_base == base
                            && seen.value == version.value
                    })
            })
            .map(|(_, version)| version.row.as_str())
            .collect();
        named.extend(twins);
        let (mut heads, mut older): (Vec<&Version<T>>, Vec<&Version<T>>) = self
            .writes
            .iter()
            .map(|(_, version)| version)
            .partition(|version| !named.contains(version.row.as_str()));
        if heads.is_empty() {
            // Bases that name each other round: the last in log order stands.
            heads.push(older.pop()?);
        }
        let current = heads[0].clone();
        let mut base: Vec<String> = heads.iter().map(|head| head.row.clone()).collect();
        base.sort();
        let mut rivals: Vec<Version<T>> = Vec::new();
        for head in &heads[1..] {
            if head.value != current.value && rivals.iter().all(|seen| seen.value != head.value) {
                rivals.push((*head).clone());
            }
        }
        let over = older.len().saturating_sub(HISTORY);
        Some(Item {
            current,
            base,
            both_changed: !rivals.is_empty(),
            rivals,
            history: older.into_iter().skip(over).cloned().collect(),
        })
    }
}

fn resolve_all<T: Clone + PartialEq>(
    registers: BTreeMap<String, Register<T>>,
) -> BTreeMap<String, Item<T>> {
    registers
        .into_iter()
        .filter_map(|(id, register)| Some((id, register.resolve()?)))
        .collect()
}

/// A `node.related` row while folding.
struct Edge {
    at: usize,
    audience: String,
    from: String,
    to: String,
    to_key: String,
    row: String,
    time: i64,
    author: Author,
    said_by: Vec<SaidBy>,
    sync: Sync,
    relation: Relation,
    reason: String,
    because: Vec<String>,
}

pub fn fold(input: FoldInput) -> Result<Folded, NodeError> {
    for row in &input.rows {
        row.check()
            .map_err(|error| NodeError::new(row_refusal(&error.to_string())))?;
    }
    let ids: BTreeSet<&str> = input.rows.iter().map(|row| row.id.as_str()).collect();
    if input.bodies.keys().any(|id| !ids.contains(id.as_str())) {
        return Err(NodeError::new("a body is for no row given"));
    }
    let rows: Vec<Row> = input
        .rows
        .into_iter()
        .filter(|row| row.rule() == Rule::Forget || object_type(&row.object) == Some("node"))
        .collect();
    let rows = myriad_persona_log::fold::dedupe(rows)
        .map_err(|_| NodeError::new("two different rows share an id"))?;
    let ledger = forget::ledger(&rows);
    // Whole nodes forgotten: every node the ledger names, whether or not
    // its rows are here, and every node whose rows a subject forget reaches
    // (found before they turn to stubs).
    let gone: BTreeSet<(String, String)> = ledger
        .iter()
        .filter(|entry| object_type(&entry.target) == Some("node"))
        .map(|entry| (entry.audience.clone(), entry.target.clone()))
        .chain(
            rows.iter()
                .filter(|row| row.rule() != Rule::Forget && forget::forgets(&ledger, row))
                .map(|row| (row.audience.clone(), row.object.clone())),
        )
        .collect();
    let (rows, stubbed) = forget::absorb(rows, &ledger);

    let mut by_node: BTreeMap<(&str, &str), Vec<(usize, &Row)>> = BTreeMap::new();
    for (at, row) in rows.iter().enumerate() {
        if row.rule() == Rule::Forget {
            continue;
        }
        by_node
            .entry((row.audience.as_str(), row.object.as_str()))
            .or_default()
            .push((at, row));
    }

    let mut skipped = Vec::new();
    let mut nodes = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    // (audience, node, related row) taken back by a `node.unrelated`.
    let mut taken_back: BTreeSet<(String, String, String)> = BTreeSet::new();
    for ((audience, object), rows) in by_node {
        let id = object.strip_prefix("node:").unwrap_or(object);
        if gone.contains(&(audience.to_string(), object.to_string())) {
            nodes.push(Node {
                forgotten: true,
                ..Node::empty(id, audience)
            });
            continue;
        }
        let mut skip = |row: &Row, why: Why| {
            skipped.push(Skipped {
                id: row.id.clone(),
                why,
            });
        };
        // Read every body once; the key comes from the created rows.
        let mut read: Vec<(usize, &Row, Body)> = Vec::new();
        let mut first = None::<i64>;
        let mut last = None::<i64>;
        for (at, row) in rows {
            let time = micros(row);
            first = Some(first.map_or(time, |known| known.min(time)));
            last = Some(last.map_or(time, |known| known.max(time)));
            if row.forgotten || !crate::body::READ.contains(&row.kind.as_str()) {
                continue;
            }
            let Some(raw) = input.bodies.get(&row.id) else {
                skip(row, Why::NoBody);
                continue;
            };
            match Body::read(&row.kind, raw) {
                Ok(body) => read.push((at, row, body)),
                Err(_) => skip(row, Why::BadBody),
            }
        }
        let key: Option<String> = read.iter().find_map(|(_, _, body)| match body {
            Body::Created(created)
                if node_id(audience, &created.key).ok().as_deref() == Some(id) =>
            {
                Some(created.key.clone())
            }
            _ => None,
        });
        let parsed = key.as_deref().and_then(|key| Key::parse(key).ok());
        let place = Audience::parse(audience).ok();
        let admits = |person: &str| place.is_some_and(|place| place.admits(Some(person)));

        let mut node = Node::empty(id, audience);
        node.key = key.clone();
        let mut kind = Register::<KindValue>::default();
        let mut name = Register::<String>::default();
        let mut summary = Register::<Vec<Sentence>>::default();
        let mut status = Register::<Status>::default();
        let mut who_knows = Register::<Vec<String>>::default();
        let mut object_of = Register::<Option<Object>>::default();
        let mut progress: BTreeMap<String, Register<Option<String>>> = BTreeMap::new();
        let mut views: BTreeMap<String, Register<ViewValue>> = BTreeMap::new();
        let mut open: BTreeMap<String, Register<OpenValue>> = BTreeMap::new();
        let mut intents: BTreeMap<String, Register<IntentValue>> = BTreeMap::new();
        let mut kinds_by_row: BTreeMap<String, Kind> = BTreeMap::new();
        let mut created = None::<i64>;
        for (at, row, body) in read {
            let time = micros(row);
            let (sync, said_by) = {
                let (sync, said_by) = body.tags();
                (sync.clone(), said_by.to_vec())
            };
            if !sync.fits(row.replicate, &row.origin) {
                skip(row, Why::SyncMismatch);
                continue;
            }
            if row.author == Author::Owner && said_by != [SaidBy::Partner] {
                skip(row, Why::AuthorMismatch);
                continue;
            }
            let meta = Meta {
                row,
                time,
                said_by: &said_by,
                sync: &sync,
            };
            let base = body.base().to_vec();
            match body {
                Body::Created(body) => {
                    if key.as_deref() != Some(body.key.as_str()) {
                        skip(row, Why::OtherNode);
                        continue;
                    }
                    created = Some(created.map_or(time, |known| known.min(time)));
                    kinds_by_row.insert(row.id.clone(), body.kind);
                    kind.push(
                        &[],
                        meta.version(KindValue {
                            kind: body.kind,
                            reason: String::new(),
                            because: Vec::new(),
                        }),
                    );
                    name.push(&[], meta.version(body.name));
                    if let Some(object) = body.object {
                        object_of.push(&[], meta.version(Some(object)));
                    }
                }
                Body::KindChanged(body) => {
                    if parsed.is_some_and(|parsed| !parsed.kinds().contains(&body.kind)) {
                        skip(row, Why::KindFixed);
                        continue;
                    }
                    node.history.push(KindChange {
                        row: row.id.clone(),
                        at: time,
                        from: base.iter().find_map(|base| kinds_by_row.get(base).copied()),
                        to: body.kind,
                        reason: body.reason.clone(),
                        because: body.because.clone(),
                        author: row.author,
                        said_by: said_by.clone(),
                    });
                    kinds_by_row.insert(row.id.clone(), body.kind);
                    kind.push(
                        &base,
                        meta.version(KindValue {
                            kind: body.kind,
                            reason: body.reason,
                            because: body.because,
                        }),
                    );
                }
                Body::Revised(body) => {
                    // Read and checked already.
                    let Ok(change) = body.change() else {
                        skip(row, Why::BadBody);
                        continue;
                    };
                    match change {
                        Change::Name(value) => name.push(&base, meta.version(value)),
                        Change::Summary(value) => summary.push(&base, meta.version(value)),
                        Change::Status(value) => {
                            let fixed = parsed.map(|parsed| parsed.kinds());
                            if fixed == Some(&[Kind::Person][..]) && !value.fits(Kind::Person) {
                                skip(row, Why::StatusForKind);
                                continue;
                            }
                            status.push(&base, meta.version(value));
                        }
                        Change::WhoKnows(value) => {
                            if !value.iter().all(|person| admits(person)) {
                                skip(row, Why::NotInAudience);
                                continue;
                            }
                            who_knows.push(&base, meta.version(value));
                        }
                        Change::Object(value) => {
                            let repo_object = value
                                .as_ref()
                                .is_some_and(|object| object.kind == ObjectKind::Repo);
                            let repo_key = parsed.map(|parsed| matches!(parsed, Key::Repo(_)));
                            if repo_key.is_some_and(|repo_key| repo_key != repo_object) {
                                skip(row, Why::ObjectForKey);
                                continue;
                            }
                            object_of.push(&base, meta.version(value));
                        }
                        Change::Progress(person, value) => {
                            if !admits(&person) {
                                skip(row, Why::NotInAudience);
                                continue;
                            }
                            progress
                                .entry(person)
                                .or_default()
                                .push(&base, meta.version(value));
                        }
                    }
                }
                Body::ViewChanged(body) => views.entry(body.view).or_default().push(
                    &base,
                    meta.version(ViewValue {
                        text: body.text,
                        because: body.because,
                    }),
                ),
                Body::OpenItem(body) => open.entry(body.item).or_default().push(
                    &base,
                    meta.version(OpenValue {
                        text: body.text,
                        because: body.because,
                        closed: body.closed,
                    }),
                ),
                Body::Intent(body) => {
                    if !body
                        .said_by
                        .iter()
                        .all(|said_by| matches!(said_by, SaidBy::Partner | SaidBy::Her))
                    {
                        skip(row, Why::UntrustedIntent);
                        continue;
                    }
                    intents.entry(body.intent).or_default().push(
                        &base,
                        meta.version(IntentValue {
                            text: body.text,
                            because: body.because,
                            status: body.status,
                            trigger: body.trigger,
                        }),
                    );
                }
                Body::Span(body) => {
                    if !audience_fits(&body.audience, audience, &body.said_by) {
                        skip(row, Why::AudienceTag);
                        continue;
                    }
                    if parsed.is_some_and(|parsed| parsed.is_about_you(audience)) {
                        skip(row, Why::NoSpansHere);
                        continue;
                    }
                    node.spans.push(SpanOut {
                        row: row.id.clone(),
                        at: time,
                        author: row.author,
                        said_by: body.said_by,
                        sync: body.sync,
                        audience: body.audience,
                        text: body.text,
                        refs: body.refs,
                        from: body.from,
                        to: body.to,
                    });
                }
                Body::Entry(body) => {
                    if !audience_fits(&body.audience, audience, &body.said_by) {
                        skip(row, Why::AudienceTag);
                        continue;
                    }
                    node.entries.push(EntryOut {
                        row: row.id.clone(),
                        at: time,
                        author: row.author,
                        said_by: body.said_by,
                        sync: body.sync,
                        audience: body.audience,
                        text: body.text,
                        refs: body.refs,
                        quote_only: body.quote_only,
                    });
                }
                Body::Related(body) => {
                    // The key was checked as the body was read.
                    let Ok(to) = node_id(audience, &body.to) else {
                        skip(row, Why::BadBody);
                        continue;
                    };
                    if to == id {
                        skip(row, Why::SelfRelation);
                        continue;
                    }
                    edges.push(Edge {
                        at,
                        audience: audience.to_string(),
                        from: id.to_string(),
                        to,
                        to_key: body.to,
                        row: row.id.clone(),
                        time,
                        author: row.author,
                        said_by: body.said_by,
                        sync: body.sync,
                        relation: body.relation,
                        reason: body.reason,
                        because: body.because,
                    });
                }
                Body::Unrelated(body) => {
                    for named in body.base {
                        taken_back.insert((audience.to_string(), id.to_string(), named));
                    }
                }
            }
        }
        node.kind = kind.resolve();
        node.name = name.resolve();
        node.summary = summary.resolve();
        node.status = status.resolve();
        node.who_knows = who_knows.resolve();
        node.object = object_of.resolve();
        node.progress = resolve_all(progress);
        node.views = resolve_all(views);
        node.open = resolve_all(open);
        node.intents = resolve_all(intents);
        node.created_at = created.or(first);
        node.updated_at = last;
        nodes.push(node);
    }

    let relations = relate(edges, &taken_back, &gone, &mut skipped);
    Ok(Folded {
        nodes,
        relations,
        stubbed,
        skipped,
    })
}

/// The relations that stand, from the related rows in log order.
fn relate(
    mut edges: Vec<Edge>,
    taken_back: &BTreeSet<(String, String, String)>,
    gone: &BTreeSet<(String, String)>,
    skipped: &mut Vec<Skipped>,
) -> Vec<RelationOut> {
    edges.sort_by_key(|edge| edge.at);
    // (audience, from) → tos, of the part_of relations taken so far.
    let mut part_of: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut standing: BTreeMap<(String, String, Relation, String), RelationOut> = BTreeMap::new();
    for edge in edges {
        let back = (edge.audience.clone(), edge.from.clone(), edge.row.clone());
        if taken_back.contains(&back)
            || gone.contains(&(edge.audience.clone(), format!("node:{}", edge.to)))
        {
            continue;
        }
        if edge.relation == Relation::PartOf {
            if reaches(&part_of, &edge.audience, &edge.to, &edge.from) {
                skipped.push(Skipped {
                    id: edge.row,
                    why: Why::Cycle,
                });
                continue;
            }
            part_of
                .entry((edge.audience.clone(), edge.from.clone()))
                .or_default()
                .insert(edge.to.clone());
        }
        let (one, other) = if edge.relation.symmetric() && edge.to < edge.from {
            (edge.to.clone(), edge.from.clone())
        } else {
            (edge.from.clone(), edge.to.clone())
        };
        standing
            .entry((edge.audience.clone(), one, edge.relation, other))
            .and_modify(|known| known.rows.push(edge.row.clone()))
            .or_insert(RelationOut {
                audience: edge.audience,
                from: edge.from,
                to: edge.to,
                to_key: edge.to_key,
                relation: edge.relation,
                reason: edge.reason,
                because: edge.because,
                author: edge.author,
                said_by: edge.said_by,
                sync: edge.sync,
                at: edge.time,
                rows: vec![edge.row],
            });
    }
    standing.into_values().collect()
}

/// Whether `from` reaches `to` along `part_of` in `audience`.
fn reaches(
    part_of: &BTreeMap<(String, String), BTreeSet<String>>,
    audience: &str,
    from: &str,
    to: &str,
) -> bool {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut next = vec![from];
    while let Some(at) = next.pop() {
        if at == to {
            return true;
        }
        if !seen.insert(at) {
            continue;
        }
        if let Some(tos) = part_of.get(&(audience.to_string(), at.to_string())) {
            next.extend(tos.iter().map(String::as_str));
        }
    }
    false
}
