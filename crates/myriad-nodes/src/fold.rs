//! Folding node rows into nodes and their relations, by the same rules on
//! every side.
//!
//! The rows (any order, repeated, with stubs and the forgets that reach
//! them) are checked, deduplicated by id and put in log order, as
//! `log.fold` does. Then, node by node:
//!
//! - **The key** comes from `node.created`; a created row whose audience
//!   and key make another id is passed by (`other_node`). A node whose
//!   created row puts a person or a group in her own audience reads as
//!   nothing, every row of it passed by (`person_elsewhere`): they live in
//!   their own audiences.
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
//! - **The partner's**: `createdBy` says who wrote the created row; each
//!   head field's versions say who wrote them, so upkeep knows what the
//!   partner set.
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
//!
//! What upkeep leaves:
//!
//! - **A split** (`node.split_off`, on the node split): the spans it names
//!   that are on that node at that point in the log (written there, or
//!   brought there by a merge or an earlier split) are on the new part,
//!   marked `movedFrom`. Both nodes get a lineage line.
//! - **A merge or folding back** (`node.absorbed`, on the node that goes):
//!   it reads as gone (`absorbedInto`), its spans and entries are on the
//!   node it went into (the last one, along a chain), and its relations go
//!   with it. Only a minted node goes into another, once (`absorbed_twice`),
//!   never into itself or round a loop (`cycle`), never into 「关于你」
//!   (`no_spans_here`).
//! - **Leaving a whole** (a `part_of` taken back): a lineage line, 原来属于.
//! - **What "done" is** (`node.done_when`): a head field like the others.
//! - **Parts rolled up**: a whole counts its standing parts, done, dropped.
//! - **Recall use** (`node.used`): how often, and when last, on the node
//!   and on each relation recall came along. A `related` relation fades by
//!   this, never by its age alone; the weights are the caller's.
//!
//! Forgetting reaches what upkeep moved: a node that went into a forgotten
//! node is forgotten with it (its forget is listed in `forgets` for the
//! caller to write, so it holds once the bodies are gone), and the spans a
//! forgotten part took in a split go with it, and so does the split row.
//!
//! **Taken back from sync** (a `retract` row, `myriad_persona_log::forget`):
//! on the node's home (the retract's origin, one of `here`) the node folds
//! as ever and says so (`retracted`: its home); everywhere else it folds
//! to that mark alone, its rows become stubs (`stubbed`), and relations to
//! it go, as for a forgotten node. A node that went into it goes with it;
//! on the home those are listed in `retracts`, with the node's old ids, for
//! the home to write, so every side takes them back whatever bodies it
//! holds. A fold given a retract must be told `here`.
//!
//! **Moved nodes** (`crate::migrate`): a created row whose body was born
//! under another audience or key, and whose id was made from those, folds
//! as the node its audience and key make now; its old id is in `aliases`.
//! Two nodes of one key made under two of the partner's ids fold into one.
//! A forget or retract of any of its ids holds for all of them; the ids it
//! did not name are listed in `forgets` (`retracts`) for the caller to
//! write, so the sides that cannot see the old id is the same node take it
//! too.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::{
    Row, forget,
    kinds::Rule,
    row::{Audience, Author, object_type},
};
use myriad_persona_sync::Rev;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{
        Absorbed, Body, Change, How, IntentStatus, Object, ObjectKind, Relation, Sentence, Status,
        Trigger,
    },
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
    /// Every origin this side writes or wrote as: a node it is the home of
    /// stays whole. Needed once a retract is in the rows.
    #[serde(default)]
    pub here: Option<Vec<String>>,
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
    /// The node it was written on, when a split or a merge moved it here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<String>,
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
    /// The node it was written on, when a merge moved it here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<String>,
}

/// Where an absorbed node went: it reads as gone, its spans and entries
/// are on the node it went into (the last one, when that one went on).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Absorption {
    pub into: String,
    pub how: How,
    pub row: String,
    pub at: i64,
}

/// What a lineage line says happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LineEvent {
    /// A part was split off this node into `other`.
    SplitOff,
    /// This node was split off `other`.
    SplitFrom,
    /// `other` went into this node.
    Absorbed,
    /// This node went into `other`.
    AbsorbedInto,
    /// This node stopped being part of `other` (原来属于).
    Left,
}

/// One line of where a node came from and went: splits, merges, folding
/// back, leaving a whole. History, not a relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub row: String,
    pub at: i64,
    pub event: LineEvent,
    pub other: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub how: Option<How>,
    pub reason: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub because: Vec<String>,
    pub author: Author,
    pub said_by: Vec<SaidBy>,
}

/// How a whole's parts stand (属于 rolled up): every standing part, and
/// those done or dropped. A part's own status, absent, reads as active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Parts {
    pub total: usize,
    pub done: usize,
    pub dropped: usize,
}

/// A forget the caller writes: a node gone with the node it went into.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToForget {
    pub audience: String,
    pub target: String,
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
    /// Taken back from sync: the side that keeps it. On any other side
    /// nothing else about it reads back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retracted: Option<String>,
    /// Its ids from before the partner's id changed, that fold into it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Absent until a created row is folded (it may not have arrived yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Who wrote the created row its key comes from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<Author>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Item<KindValue>>,
    /// What "done" is for it; a null value: none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_when: Option<Item<Option<String>>>,
    /// Gone into another node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absorbed_into: Option<Absorption>,
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
    /// Splits, merges, folding back and leaving, in log order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lineage: Vec<Line>,
    /// Its parts, when it is a whole.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Parts>,
    /// How often recall used it, and when last (its `node.used` rows).
    #[serde(skip_serializing_if = "is_zero")]
    pub uses: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<i64>,
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
            retracted: None,
            aliases: Vec::new(),
            key: None,
            created_by: None,
            kind: None,
            done_when: None,
            absorbed_into: None,
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
            lineage: Vec::new(),
            parts: None,
            uses: 0,
            last_used: None,
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
    /// How often recall came along it, and when last: a `related` one
    /// fades by these (unused), never by its age alone.
    #[serde(skip_serializing_if = "is_zero")]
    pub uses: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<i64>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &usize) -> bool {
    *value == 0
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
    /// A `part_of`, or a node going into another, that would close a loop.
    Cycle,
    /// A split-off part, or a node going into another, whose key is not
    /// minted (a repository, a person: one node each, always).
    NotMinted,
    /// A node going into another a second time; the first stands.
    AbsorbedTwice,
    /// Something she watched or read with the partner going into another:
    /// it only ever takes others in.
    Together,
    /// A person's or a group's node in her own audience: it lives in that
    /// person's or group's own audience, never in hers.
    PersonElsewhere,
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
    /// Nodes gone with a forgotten node they went into, not on the ledger
    /// yet: the caller writes their forgets, so the forget holds once the
    /// bodies are deleted. And the ids of a forgotten node that no forget
    /// named (its old ids, or the one it reads as).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub forgets: Vec<ToForget>,
    /// On a home: the nodes taken back with one it keeps (gone into it, or
    /// its ids no retract named), for the home to write retracts of.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub retracts: Vec<ToForget>,
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

/// A `node.split_off` row while folding.
struct Splitting {
    at: usize,
    audience: String,
    from: String,
    to: String,
    spans: Vec<String>,
    line: Line,
}

/// A `node.unrelated` row while folding: what lineage it leaves.
#[derive(Clone)]
struct TakenBack {
    row: String,
    time: i64,
    reason: String,
    author: Author,
    said_by: Vec<SaidBy>,
}

/// The id of the node a node row is about.
fn node_of(row: &Row) -> &str {
    row.object.strip_prefix("node:").unwrap_or(&row.object)
}

/// Why a row's tags do not fit it: a sync tag for another row, or the
/// partner's row not said by the partner.
fn misfit(row: &Row, body: &Body) -> Option<Why> {
    let (sync, said_by) = body.tags();
    if !sync.fits(row.replicate, &row.origin) {
        return Some(Why::SyncMismatch);
    }
    if row.author == Author::Owner && said_by != [SaidBy::Partner] {
        return Some(Why::AuthorMismatch);
    }
    None
}

/// The nodes that went into another, read before forgets turn rows into
/// stubs: (audience, node) → (the node it went into, the row's place in
/// the log); and why each `node.absorbed` row passed by was.
struct Absorbing {
    into: BTreeMap<(String, String), (String, usize)>,
    why: BTreeMap<String, Why>,
}

impl Absorbing {
    /// Whether going on from `start` reaches `target`.
    fn reaches(&self, audience: &str, start: &str, target: &str) -> bool {
        let mut at = start.to_string();
        let mut seen = BTreeSet::new();
        loop {
            if at == target {
                return true;
            }
            if !seen.insert(at.clone()) {
                return false;
            }
            match self.into.get(&(audience.to_string(), at.clone())) {
                Some((next, _)) => at = next.clone(),
                None => return false,
            }
        }
    }
}

/// (audience, node it reads as) → the ids that fold into it.
type Members = BTreeMap<(String, String), BTreeSet<String>>;

/// The rows with each moved node's rows (and the forgets and retracts that
/// name it) under the node its audience and key make now: a created row
/// whose body says where its id was born, and whose id is the one made
/// there.
fn moved(rows: Vec<Row>, bodies: &BTreeMap<String, Value>) -> (Vec<Row>, Members) {
    let mut now: BTreeMap<(String, String), String> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.kind == "node.created") {
        let Some(Body::Created(created)) = (!row.forgotten)
            .then(|| bodies.get(&row.id))
            .flatten()
            .and_then(|raw| Body::read(&row.kind, raw).ok())
        else {
            continue;
        };
        let Some(born) = &created.born else { continue };
        let was = node_id(&born.audience, &born.key).ok();
        let is = node_id(&row.audience, &created.key).ok();
        if let (Some(was), Some(is)) = (was, is)
            && was == node_of(row)
            && was != is
        {
            now.entry((row.audience.clone(), row.object.clone()))
                .or_insert(format!("node:{is}"));
        }
    }
    let mut members: Members = BTreeMap::new();
    let rows = rows
        .into_iter()
        .map(|mut row| {
            if let Some(is) = now.get(&(row.audience.clone(), row.object.clone())) {
                members
                    .entry((row.audience.clone(), is.clone()))
                    .or_default()
                    .insert(node_of(&row).to_string());
                row.object = is.clone();
            }
            row
        })
        .collect();
    (rows, members)
}

fn absorbing(rows: &[Row], bodies: &BTreeMap<String, Value>) -> Absorbing {
    let read = |row: &Row| {
        (!row.forgotten)
            .then(|| bodies.get(&row.id))
            .flatten()
            .and_then(|raw| Body::read(&row.kind, raw).ok())
    };
    let mut keys: BTreeMap<(String, String), String> = BTreeMap::new();
    for row in rows.iter().filter(|row| row.kind == "node.created") {
        if let Some(Body::Created(created)) = read(row)
            && node_id(&row.audience, &created.key).ok().as_deref() == Some(node_of(row))
        {
            keys.entry((row.audience.clone(), node_of(row).to_string()))
                .or_insert(created.key);
        }
    }
    let mut out = Absorbing {
        into: BTreeMap::new(),
        why: BTreeMap::new(),
    };
    for (at, row) in rows.iter().enumerate() {
        if row.kind != "node.absorbed" {
            continue;
        }
        let Some(body) = read(row) else { continue };
        if misfit(row, &body).is_some() {
            continue;
        }
        let Body::Absorbed(Absorbed { into, .. }) = body else {
            continue;
        };
        let (audience, from) = (row.audience.clone(), node_of(row).to_string());
        let about_you = Key::parse(&into).is_ok_and(|key| key.is_about_you(&audience));
        // The key was checked as the body was read.
        let Ok(into) = node_id(&audience, &into) else {
            continue;
        };
        let place = (audience.clone(), from.clone());
        let why = if into == from {
            Some(Why::SelfRelation)
        } else if about_you {
            Some(Why::NoSpansHere)
        } else if keys
            .get(&place)
            .is_some_and(|key| !key.starts_with("minted:"))
        {
            Some(Why::NotMinted)
        } else if keys
            .get(&place)
            .is_some_and(|key| Key::parse(key).is_ok_and(|key| key.is_together()))
        {
            Some(Why::Together)
        } else if out.into.contains_key(&place) {
            Some(Why::AbsorbedTwice)
        } else if out.reaches(&audience, &into, &from) {
            Some(Why::Cycle)
        } else {
            None
        };
        match why {
            Some(why) => {
                out.why.insert(row.id.clone(), why);
            }
            None => {
                out.into.insert(place, (into, at));
            }
        }
    }
    out
}

impl Node {
    /// The `base` of every head item two sides changed (两边都改过).
    pub(crate) fn both_changed(&self) -> Vec<&[String]> {
        fn one<T>(item: &Item<T>) -> Option<&[String]> {
            item.both_changed.then_some(item.base.as_slice())
        }
        let mut out = Vec::new();
        out.extend(self.kind.as_ref().and_then(one));
        out.extend(self.done_when.as_ref().and_then(one));
        out.extend(self.name.as_ref().and_then(one));
        out.extend(self.summary.as_ref().and_then(one));
        out.extend(self.status.as_ref().and_then(one));
        out.extend(self.who_knows.as_ref().and_then(one));
        out.extend(self.object.as_ref().and_then(one));
        out.extend(self.progress.values().filter_map(one));
        out.extend(self.views.values().filter_map(one));
        out.extend(self.open.values().filter_map(one));
        out.extend(self.intents.values().filter_map(one));
        out
    }
}

/// What her upkeep marks say she saw of one node.
#[derive(Debug, Clone, Default)]
pub(crate) struct Seen {
    /// Per device, the newest clock.
    pub clocks: BTreeMap<String, Rev>,
    /// For a device not in `clocks`: seen up to this wall time (Unix ms).
    pub floor: Option<u64>,
    /// Rows her runs wrote.
    pub wrote: BTreeSet<String>,
    /// Each mark on its own: what one run saw. Together they say whether
    /// some run saw a row; only one run having seen two rows at once says
    /// she weighed them together.
    pub each: Vec<Seen>,
}

impl Seen {
    pub fn saw(&self, row: &Row) -> bool {
        if self.wrote.contains(&row.id) {
            return true;
        }
        match self.clocks.get(&row.origin) {
            Some(rev) => row.hlc <= *rev,
            None => self.floor.is_some_and(|floor| row.hlc.wall_ms <= floor),
        }
    }
}

/// The fold, with what the upkeep queue reads beside it.
pub(crate) struct Full {
    pub folded: Folded,
    /// The rows in log order, forgotten ones as stubs.
    pub rows: Vec<Row>,
    /// (audience, node) → what her upkeep saw of it.
    pub seen: BTreeMap<(String, String), Seen>,
    /// (audience, from, to, row) of every `part_of` row the fold weighs, in
    /// log order: those taken back, to a forgotten node, or of a node gone
    /// into another left out; those it passes by as a loop kept in.
    pub part_of: Vec<(String, String, String, String)>,
    /// (audience, node) of the nodes that went into another.
    pub away: BTreeSet<(String, String)>,
    /// A related or unrelated row → the (audience, node) it points at.
    pub points_at: BTreeMap<String, (String, String)>,
}

pub fn fold(input: FoldInput) -> Result<Folded, NodeError> {
    Ok(fold_full(input)?.folded)
}

pub(crate) fn fold_full(input: FoldInput) -> Result<Full, NodeError> {
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
    // What forgets and retracts named before moved ids fold into one.
    let named: BTreeSet<(String, String, bool)> = rows
        .iter()
        .filter(|row| forget::on_ledger(row))
        .map(|row| {
            (
                row.audience.clone(),
                row.object.clone(),
                row.rule() == Rule::Retract,
            )
        })
        .collect();
    let (rows, members) = moved(rows, &input.bodies);
    let ledger = forget::ledger(&rows);
    let names = forget::here(&ledger, input.here.as_deref())
        .map_err(|error| NodeError::new(error.to_string()))?;
    let kept = |entry: &forget::Entry| entry.home.as_ref().is_some_and(|home| names.contains(home));
    let absorbing = absorbing(&rows, &input.bodies);
    // Whole nodes forgotten, with the forget that did it: every node the
    // ledger names, whether or not its rows are here, and every node whose
    // rows a subject forget reaches (found before they turn to stubs).
    let mut gone: BTreeMap<(String, String), Rev> = BTreeMap::new();
    for entry in ledger.iter().filter(|entry| entry.home.is_none()) {
        if object_type(&entry.target) == Some("node") {
            gone.insert(
                (entry.audience.clone(), entry.target.clone()),
                entry.at.clone(),
            );
        }
    }
    for row in rows.iter().filter(|row| row.rule() != Rule::Forget) {
        let subject = row
            .subject
            .as_deref()
            .map(|subject| format!("subject:{subject}"));
        if let Some(entry) = ledger.iter().find(|entry| {
            entry.home.is_none()
                && entry.audience == row.audience
                && Some(&entry.target) == subject.as_ref()
        }) {
            gone.entry((row.audience.clone(), row.object.clone()))
                .or_insert_with(|| entry.at.clone());
        }
    }
    // A node that went into a forgotten one is gone with it: what it held
    // was shown there.
    let mut extra: Vec<forget::Entry> = Vec::new();
    for (audience, from) in absorbing.into.keys() {
        let mut at = from.clone();
        let mut seen = BTreeSet::new();
        while let Some((next, _)) = absorbing.into.get(&(audience.clone(), at.clone())) {
            if !seen.insert(at.clone()) {
                break;
            }
            if let Some(when) = gone.get(&(audience.clone(), format!("node:{next}"))) {
                extra.push(forget::Entry {
                    audience: audience.clone(),
                    target: format!("node:{from}"),
                    at: when.clone(),
                    home: None,
                });
                break;
            }
            at = next.clone();
        }
    }
    let extra: Vec<forget::Entry> = extra
        .into_iter()
        .filter(|entry| !gone.contains_key(&(entry.audience.clone(), entry.target.clone())))
        .collect();
    for entry in &extra {
        gone.insert(
            (entry.audience.clone(), entry.target.clone()),
            entry.at.clone(),
        );
    }
    // Every id of a forgotten node a forget did not name, to write.
    let unnamed = |place: &(String, String), retract: bool| -> Vec<ToForget> {
        let mut ids = vec![place.1.clone()];
        ids.extend(
            members
                .get(place)
                .into_iter()
                .flatten()
                .map(|id| format!("node:{id}")),
        );
        ids.into_iter()
            .filter(|id| !named.contains(&(place.0.clone(), id.clone(), retract)))
            .map(|target| ToForget {
                audience: place.0.clone(),
                target,
            })
            .collect()
    };
    let mut forgets: BTreeSet<ToForget> = BTreeSet::new();
    for place in gone.keys() {
        forgets.extend(unnamed(place, false));
    }
    let forgets: Vec<ToForget> = forgets.into_iter().collect();
    // Taken back from sync, and its home: what a retract names, and the
    // nodes that went into such a node (the home writes their retracts).
    let mut retracted: BTreeMap<(String, String), (Rev, String)> = ledger
        .iter()
        .filter_map(|entry| {
            let home = entry.home.clone()?;
            let place = (entry.audience.clone(), entry.target.clone());
            (!gone.contains_key(&place)).then(|| (place, (entry.at.clone(), home)))
        })
        .collect();
    for (audience, from) in absorbing.into.keys() {
        let mut at = from.clone();
        let mut seen = BTreeSet::new();
        while let Some((next, _)) = absorbing.into.get(&(audience.clone(), at.clone())) {
            if !seen.insert(at.clone()) {
                break;
            }
            let place = (audience.clone(), format!("node:{next}"));
            if let Some(taken) = retracted.get(&place).cloned() {
                let from = (audience.clone(), format!("node:{from}"));
                if !gone.contains_key(&from) {
                    retracted.entry(from).or_insert(taken);
                }
                break;
            }
            at = next.clone();
        }
    }
    let mut retracts: BTreeSet<ToForget> = BTreeSet::new();
    let mut retract_entries: Vec<forget::Entry> = Vec::new();
    for (place, (at, home)) in &retracted {
        let entry = forget::Entry {
            audience: place.0.clone(),
            target: place.1.clone(),
            at: at.clone(),
            home: Some(home.clone()),
        };
        if kept(&entry) {
            retracts.extend(unnamed(place, true));
        }
        retract_entries.push(entry);
    }
    let retracts: Vec<ToForget> = retracts.into_iter().collect();
    // Away: taken back, and this side is not its home.
    let away_here: BTreeMap<(String, String), String> = retracted
        .into_iter()
        .filter(|(_, (_, at_home))| !names.contains(at_home))
        .map(|(place, (_, at_home))| (place, at_home))
        .collect();
    let mut gone: BTreeSet<(String, String)> = gone.into_keys().collect();
    gone.extend(away_here.keys().cloned());
    let (rows, mut stubbed) = forget::absorb(
        rows,
        &forget::joined(&forget::joined(&ledger, &extra), &retract_entries),
        names,
    );
    let position: BTreeMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(at, row)| (row.id.clone(), at))
        .collect();

    let mut by_node: BTreeMap<(&str, &str), Vec<(usize, &Row)>> = BTreeMap::new();
    for (at, row) in rows.iter().enumerate() {
        if forget::on_ledger(row) {
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
    let mut taken_back: BTreeMap<(String, String, String), TakenBack> = BTreeMap::new();
    let mut splits: Vec<Splitting> = Vec::new();
    // (audience, node) a lineage line goes on, written from another node.
    let mut lines_for: Vec<((String, String), Line)> = Vec::new();
    // (audience, related row) → how often recall came along it, and when last.
    let mut used_via: BTreeMap<(String, String), (usize, i64)> = BTreeMap::new();
    let mut seen_by: BTreeMap<(String, String), Seen> = BTreeMap::new();
    let mut unrelating: Vec<(String, String, Vec<String>)> = Vec::new();
    for ((audience, object), rows) in by_node {
        let id = object.strip_prefix("node:").unwrap_or(object);
        let place = (audience.to_string(), object.to_string());
        let aliases: Vec<String> = members
            .get(&place)
            .map(|ids| ids.iter().cloned().collect())
            .unwrap_or_default();
        if let Some(home) = away_here.get(&place) {
            nodes.push(Node {
                retracted: Some(home.clone()),
                aliases,
                ..Node::empty(id, audience)
            });
            continue;
        }
        if gone.contains(&place) {
            nodes.push(Node {
                forgotten: true,
                aliases,
                ..Node::empty(id, audience)
            });
            continue;
        }
        let kept_home = ledger
            .iter()
            .chain(&retract_entries)
            .find(|entry| entry.audience == audience && entry.target == object && kept(entry))
            .and_then(|entry| entry.home.clone());
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
        // A person or a group made in her own audience: it lives in its
        // own, so nothing of it reads back here.
        let misplaced = read.iter().any(|(_, _, body)| {
            matches!(body, Body::Created(created)
                if Key::parse(&created.key).is_ok_and(|parsed| !parsed.fits(audience)))
        });
        if misplaced {
            for (_, row, _) in &read {
                skip(row, Why::PersonElsewhere);
            }
            nodes.push(Node::empty(id, audience));
            continue;
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
        node.retracted = kept_home;
        node.aliases = aliases;
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
        let mut done_when = Register::<Option<String>>::default();
        let mut kinds_by_row: BTreeMap<String, Kind> = BTreeMap::new();
        let mut created = None::<i64>;
        for (at, row, body) in read {
            let time = micros(row);
            if let Some(why) = misfit(row, &body) {
                skip(row, why);
                continue;
            }
            let (sync, said_by) = {
                let (sync, said_by) = body.tags();
                (sync.clone(), said_by.to_vec())
            };
            let line = |event: LineEvent,
                        other: &str,
                        how: Option<How>,
                        reason: &str,
                        because: &[String]| Line {
                row: row.id.clone(),
                at: time,
                event,
                other: other.to_string(),
                how,
                reason: reason.to_string(),
                because: because.to_vec(),
                author: row.author,
                said_by: said_by.clone(),
            };
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
                    node.created_by.get_or_insert(row.author);
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
                        moved_from: None,
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
                        moved_from: None,
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
                    let back = TakenBack {
                        row: row.id.clone(),
                        time,
                        reason: body.reason,
                        author: row.author,
                        said_by: said_by.clone(),
                    };
                    for named in &body.base {
                        taken_back
                            .entry((audience.to_string(), id.to_string(), named.clone()))
                            .or_insert_with(|| back.clone());
                    }
                    unrelating.push((row.id.clone(), audience.to_string(), body.base));
                }
                Body::DoneWhen(body) => done_when.push(&base, meta.version(body.text)),
                Body::SplitOff(body) => {
                    if parsed.is_some_and(|parsed| parsed.kinds() == [Kind::Person]) {
                        skip(row, Why::KindFixed);
                        continue;
                    }
                    // The key was checked as the body was read.
                    let Ok(to) = node_id(audience, &body.to) else {
                        skip(row, Why::BadBody);
                        continue;
                    };
                    if to == id {
                        skip(row, Why::SelfRelation);
                        continue;
                    }
                    splits.push(Splitting {
                        at,
                        audience: audience.to_string(),
                        from: id.to_string(),
                        to: to.clone(),
                        spans: body.spans,
                        line: line(LineEvent::SplitOff, &to, None, &body.reason, &body.because),
                    });
                }
                Body::Absorbed(body) => {
                    if let Some(why) = absorbing.why.get(&row.id) {
                        skip(row, *why);
                        continue;
                    }
                    let place = (audience.to_string(), id.to_string());
                    let Some((into, _)) =
                        absorbing.into.get(&place).filter(|(_, first)| *first == at)
                    else {
                        continue;
                    };
                    node.absorbed_into = Some(Absorption {
                        into: into.clone(),
                        how: body.how,
                        row: row.id.clone(),
                        at: time,
                    });
                    node.lineage.push(line(
                        LineEvent::AbsorbedInto,
                        into,
                        Some(body.how),
                        &body.reason,
                        &body.because,
                    ));
                    lines_for.push((
                        (audience.to_string(), into.clone()),
                        line(
                            LineEvent::Absorbed,
                            id,
                            Some(body.how),
                            &body.reason,
                            &body.because,
                        ),
                    ));
                }
                Body::Used(body) => {
                    node.uses += 1;
                    node.last_used = Some(node.last_used.map_or(time, |last| last.max(time)));
                    for via in body.via {
                        let used = used_via
                            .entry((audience.to_string(), via))
                            .or_insert((0, time));
                        used.0 += 1;
                        used.1 = used.1.max(time);
                    }
                }
                Body::Upkept(body) => {
                    let seen = seen_by
                        .entry((audience.to_string(), id.to_string()))
                        .or_default();
                    seen.each.push(Seen {
                        clocks: body.seen.clone(),
                        floor: body.floor,
                        wrote: body.wrote.iter().cloned().collect(),
                        each: Vec::new(),
                    });
                    for (device, rev) in body.seen {
                        let newest = seen.clocks.entry(device).or_insert_with(|| rev.clone());
                        if rev > *newest {
                            *newest = rev;
                        }
                    }
                    if let Some(floor) = body.floor {
                        seen.floor = Some(seen.floor.map_or(floor, |known| known.max(floor)));
                    }
                    seen.wrote.extend(body.wrote);
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
        node.done_when = done_when.resolve();
        node.created_at = created.or(first);
        node.updated_at = last;
        nodes.push(node);
    }

    let here: BTreeMap<(String, String), usize> = nodes
        .iter()
        .enumerate()
        .map(|(at, node)| ((node.audience.clone(), node.id.clone()), at))
        .collect();
    let is_gone =
        |audience: &str, id: &str| gone.contains(&(audience.to_string(), format!("node:{id}")));

    // Splits and merges, in log order, move each span from where it is at
    // that point: a split takes the spans it names that are on the node
    // split (whether written there or brought there), a merge everything
    // on the node that goes. A span whose place ends on a forgotten node is
    // forgotten with it.
    enum Move {
        Split(Splitting),
        Absorb(String, String, String),
    }
    let mut moves: Vec<(usize, Move)> = splits
        .into_iter()
        .map(|split| (split.at, Move::Split(split)))
        .collect();
    for ((audience, from), (into, at)) in &absorbing.into {
        moves.push((
            *at,
            Move::Absorb(audience.clone(), from.clone(), into.clone()),
        ));
    }
    moves.sort_by_key(|(at, _)| *at);
    let mut location: BTreeMap<(String, String), String> = BTreeMap::new();
    for node in nodes.iter().filter(|node| !node.forgotten) {
        for span in &node.spans {
            location.insert((node.audience.clone(), span.row.clone()), node.id.clone());
        }
    }
    // Merges so far, into nodes folded here.
    let mut went: BTreeMap<(String, String), String> = BTreeMap::new();
    let end = |went: &BTreeMap<(String, String), String>, audience: &str, id: &str| {
        let mut at = id.to_string();
        let mut seen = BTreeSet::new();
        while let Some(next) = went.get(&(audience.to_string(), at.clone())) {
            if !seen.insert(at.clone()) {
                break;
            }
            at = next.clone();
        }
        at
    };
    for (_, step) in moves {
        match step {
            Move::Absorb(audience, from, into) => {
                if here.contains_key(&(audience.clone(), into.clone())) {
                    went.insert((audience, from), into);
                }
            }
            Move::Split(split) => {
                let Some(&from) = here.get(&(split.audience.clone(), split.from.clone())) else {
                    continue;
                };
                for span in &split.spans {
                    let place = (split.audience.clone(), span.clone());
                    if let Some(now) = location.get(&place)
                        && end(&went, &split.audience, now) == split.from
                    {
                        location.insert(place, split.to.clone());
                    }
                }
                if is_gone(&split.audience, &split.to) {
                    // The part was forgotten: what she said of it goes too.
                    stubbed.push(split.line.row);
                    continue;
                }
                let mut line = split.line.clone();
                line.event = LineEvent::SplitFrom;
                line.other = split.from.clone();
                lines_for.push(((split.audience.clone(), split.to.clone()), line));
                nodes[from].lineage.push(split.line);
            }
        }
    }
    let mut moving: Vec<(usize, Option<SpanOut>, Option<EntryOut>)> = Vec::new();
    for (at, node) in nodes.iter_mut().enumerate() {
        if node.forgotten {
            continue;
        }
        let (audience, id) = (node.audience.clone(), node.id.clone());
        for mut span in std::mem::take(&mut node.spans) {
            let now = location
                .get(&(audience.clone(), span.row.clone()))
                .cloned()
                .unwrap_or_else(|| id.clone());
            let last = end(&went, &audience, &now);
            if is_gone(&audience, &now) || is_gone(&audience, &last) {
                stubbed.push(span.row);
                continue;
            }
            match here.get(&(audience.clone(), last)) {
                Some(&to) if to != at => {
                    span.moved_from = Some(id.clone());
                    moving.push((to, Some(span), None));
                }
                _ => node.spans.push(span),
            }
        }
        let last = end(&went, &audience, &id);
        if let Some(&to) = here.get(&(audience.clone(), last))
            && to != at
        {
            for mut entry in std::mem::take(&mut node.entries) {
                entry.moved_from = Some(id.clone());
                moving.push((to, None, Some(entry)));
            }
        }
    }
    for (to, span, entry) in moving {
        nodes[to].spans.extend(span);
        nodes[to].entries.extend(entry);
    }
    for (place, line) in lines_for {
        if let Some(&at) = here.get(&place)
            && !nodes[at].forgotten
        {
            nodes[at].lineage.push(line);
        }
    }
    let order = |row: &str| position.get(row).copied().unwrap_or(usize::MAX);
    for node in &mut nodes {
        node.spans.sort_by_key(|span| order(&span.row));
        node.entries.sort_by_key(|entry| order(&entry.row));
        node.lineage
            .sort_by_key(|line| (order(&line.row), line.event as u8, line.other.clone()));
    }
    stubbed.sort();
    stubbed.dedup();

    let away: BTreeSet<(String, String)> = absorbing.into.keys().cloned().collect();
    let mut points_at: BTreeMap<String, (String, String)> = BTreeMap::new();
    for edge in &edges {
        points_at.insert(edge.row.clone(), (edge.audience.clone(), edge.to.clone()));
    }
    for (row, audience, names) in unrelating {
        if let Some(target) = names.iter().find_map(|named| points_at.get(named).cloned()) {
            points_at.insert(row, (audience, target.1));
        }
    }
    let mut weighed: Vec<&Edge> = edges
        .iter()
        .filter(|edge| {
            edge.relation == Relation::PartOf
                && !gone.contains(&(edge.audience.clone(), format!("node:{}", edge.to)))
                && !away.contains(&(edge.audience.clone(), edge.from.clone()))
                && !away.contains(&(edge.audience.clone(), edge.to.clone()))
                && !taken_back.contains_key(&(
                    edge.audience.clone(),
                    edge.from.clone(),
                    edge.row.clone(),
                ))
        })
        .collect();
    weighed.sort_by_key(|edge| edge.at);
    let part_of: Vec<(String, String, String, String)> = weighed
        .into_iter()
        .map(|edge| {
            (
                edge.audience.clone(),
                edge.from.clone(),
                edge.to.clone(),
                edge.row.clone(),
            )
        })
        .collect();
    let (mut relations, left) = relate(edges, &taken_back, &gone, &away, &mut skipped);
    for (place, line) in left {
        if let Some(&at) = here.get(&place) {
            nodes[at].lineage.push(line);
        }
    }
    for relation in &mut relations {
        for row in &relation.rows {
            if let Some((uses, last)) = used_via.get(&(relation.audience.clone(), row.clone())) {
                relation.uses += uses;
                relation.last_used =
                    Some(relation.last_used.map_or(*last, |known| known.max(*last)));
            }
        }
        if relation.relation != Relation::PartOf {
            continue;
        }
        let status = here
            .get(&(relation.audience.clone(), relation.from.clone()))
            .and_then(|&at| nodes[at].status.as_ref())
            .map_or(Status::Active, |status| status.current.value);
        if let Some(&whole) = here.get(&(relation.audience.clone(), relation.to.clone())) {
            let parts = nodes[whole].parts.get_or_insert(Parts {
                total: 0,
                done: 0,
                dropped: 0,
            });
            parts.total += 1;
            parts.done += usize::from(status == Status::Done);
            parts.dropped += usize::from(status == Status::Dropped);
        }
    }
    for node in &mut nodes {
        node.lineage
            .sort_by_key(|line| (order(&line.row), line.event as u8, line.other.clone()));
    }
    Ok(Full {
        folded: Folded {
            nodes,
            relations,
            stubbed,
            skipped,
            forgets,
            retracts,
        },
        rows,
        seen: seen_by,
        part_of,
        away,
        points_at,
    })
}

/// The relations that stand, from the related rows in log order.
/// And the lineage a part leaving its whole leaves (原来属于), by the
/// (audience, node) it goes on. A relation from or to a node that went into
/// another goes with it.
#[allow(clippy::type_complexity)]
fn relate(
    mut edges: Vec<Edge>,
    taken_back: &BTreeMap<(String, String, String), TakenBack>,
    gone: &BTreeSet<(String, String)>,
    away: &BTreeSet<(String, String)>,
    skipped: &mut Vec<Skipped>,
) -> (Vec<RelationOut>, Vec<((String, String), Line)>) {
    edges.sort_by_key(|edge| edge.at);
    // (audience, from) → tos, of the part_of relations taken so far.
    let mut part_of: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    let mut standing: BTreeMap<(String, String, Relation, String), RelationOut> = BTreeMap::new();
    let mut left: BTreeMap<(String, String, String, String), Line> = BTreeMap::new();
    for edge in edges {
        if gone.contains(&(edge.audience.clone(), format!("node:{}", edge.to)))
            || away.contains(&(edge.audience.clone(), edge.from.clone()))
            || away.contains(&(edge.audience.clone(), edge.to.clone()))
        {
            continue;
        }
        let back = (edge.audience.clone(), edge.from.clone(), edge.row.clone());
        if let Some(back) = taken_back.get(&back) {
            if edge.relation == Relation::PartOf {
                left.entry((
                    edge.audience.clone(),
                    edge.from.clone(),
                    back.row.clone(),
                    edge.to.clone(),
                ))
                .or_insert_with(|| Line {
                    row: back.row.clone(),
                    at: back.time,
                    event: LineEvent::Left,
                    other: edge.to.clone(),
                    how: None,
                    reason: back.reason.clone(),
                    because: Vec::new(),
                    author: back.author,
                    said_by: back.said_by.clone(),
                });
            }
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
                uses: 0,
                last_used: None,
            });
    }
    (
        standing.into_values().collect(),
        left.into_iter()
            .map(|((audience, from, _, _), line)| ((audience, from), line))
            .collect(),
    )
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
