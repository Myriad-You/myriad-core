//! Forgetting absorbs: once an object (or everything about a subject in an
//! audience) is forgotten, no row of it reads back, whether its rows came
//! before the forget, after it, or from a side that was offline when it
//! happened.
//!
//! The ledger holds only what was forgotten, where and when: never a word
//! of what it said. A row on the ledger becomes a stub: its payload goes,
//! its place in the log (id, side, count) stays so vectors still agree.
//!
//! **Taking a node back from sync** (撤回同步) works as a forget does,
//! except on its homes. The partner presses it on a side that is to keep
//! the node, and that side writes a shared `retract`, which every side
//! reads, and a local `retract.home` beside it. Every side without a
//! `retract.home` of the node reads the retract as a forget of what it
//! reaches: those rows become stubs there, their bodies go, and rows that
//! come later (an edit raced from another side, a side that was offline)
//! come in as stubs. A local row never comes from another side
//! ([`crate::admit`] refuses one), so only the side that pressed it is ever
//! a home: a retract another side sends that names this side as its
//! writer, and that this side never wrote, is a forget here like any other.
//! A node retracted on two sides (each pressed while the other was
//! offline) has two homes, and each keeps what it holds: no clock decides
//! between them (the partner's decision, 2026-10-30: 两台都留).
//!
//! **What a retract reaches is named outright.** Every row whose own
//! object is the node, whatever its clock or the side it came from, and
//! the rows its `takes` lists: those of other nodes the writing home's fold
//! placed in the node when it was pressed (what went into it, spans split
//! onto it), as `nodes.retract` lists them, in several retract rows when
//! there are more than [`crate::row::MAX_RETRACT_TAKES`]. A row's object
//! never changes, so no side reads it otherwise. A retract never reaches
//! anything through a merge or a split, in either direction: what goes
//! into the node later, or what it went into, is not taken back by it.
//!
//! A home never loses a body to a stub: a stub of a row it holds whole does
//! not replace it ([`crate::merge`]), and a whole row of a stub it holds
//! fills it, from any side or file that still has it whole, so a home ends
//! the same whatever order the same rows reach it in (the partner's
//! decision, 2026-10-30: 补回来). A body that was written elsewhere and had
//! not reached any home when its side learnt of the retract is dropped
//! there, as a forget would drop it, and is kept nowhere (the partner's
//! decision, 2026-10-30: 当作忘掉). A local row stays whole on the side
//! that wrote it: it was never anywhere else. It is one way, as a forget
//! is: sharing the node again would be a new row of the partner's, which
//! the core does not have yet.
//!
//! **Other names of a node, for forgets.** A forget names one node, and
//! reaches every node that went into it (`node.absorbed`), and the spans a
//! split moved to it (`node.split_off`). Which those are is in the rows'
//! envelopes (`to`, `takes`, which every merge and split row carries), so
//! a stub keeps it and every side reads the same from its own log: the
//! ledger holds them too ([`Kind::Into`], [`Kind::Split`]), so a call given
//! only part of the log still knows them. Every merge and split counts,
//! whenever it was written and whatever its body: what a forget reaches
//! only grows, and the same on every side in any order. A node that went
//! into two is reached through both, and a span through every place a
//! split ever moved it.
//!
//! **A home keeps what its retracts reach.** On a home, a row a retract of
//! its own reaches is taken away only by a forget that names it directly:
//! of its own node, of that retract's node, or of its subject, whatever
//! its clock. No forget that reaches it through a merge, a split or
//! another node takes it. Kept rows fold as any rows do there, wherever
//! the fold puts them.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_sync::Rev;
use serde::{Deserialize, Serialize};

use crate::{LogError, Row, kinds::Rule, kinds::SUBJECT, row::Replicate};

/// What a ledger entry is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The target is forgotten.
    #[default]
    Forget,
    /// The target node is taken back from sync; `homes` wrote it, and it
    /// lists the rows of other nodes it takes too (`takes`).
    Retract,
    /// This side keeps the target node: it wrote its retract.
    Home,
    /// The target node went into `to`.
    Into,
    /// A part was split off the target node into `to`, with the rows in
    /// `takes`.
    Split,
}

impl Kind {
    fn is_forget(&self) -> bool {
        *self == Self::Forget
    }
}

/// One line of the ledger, in one audience: an object forgotten, or
/// `subject:<id>` for everything about that subject; a node taken back
/// from sync, or kept here; or one of a node's other names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub audience: String,
    pub target: String,
    /// Its row's clock (the earliest, when there were two).
    pub at: Rev,
    #[serde(default, skip_serializing_if = "Kind::is_forget")]
    pub kind: Kind,
    /// A retract's: every side that wrote one, sorted. A forget that lists
    /// rows (`takes`): the side that wrote it, one entry per side.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub homes: Vec<String>,
    /// A name's: the other node (`node:<id>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// A split's: the rows it moved, sorted. A retract's: the rows of other
    /// nodes it lists (every retract row of the node, together), sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub takes: Vec<String>,
}

impl Entry {
    pub fn is_retract(&self) -> bool {
        self.kind == Kind::Retract
    }
}

/// Which side a row is taken away on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// This side, as its log says: a home keeps its node.
    Here,
    /// Any other side (what this side sends, or a file): no node is kept.
    Away,
}

/// Whether a row is one the ledger is made of: a forget, a retract or a
/// retract's home. None of them is ever stubbed.
pub fn on_ledger(row: &Row) -> bool {
    matches!(row.rule(), Rule::Forget | Rule::Retract | Rule::Home)
}

/// The ledger `rows` make: their forgets, retracts, homes and names, sorted,
/// earliest kept. A stub's envelope makes the same as its row. Their names
/// are taken as their envelopes give them: for rows this side wrote.
pub fn ledger(rows: &[Row]) -> Vec<Entry> {
    let entries: Vec<Entry> = rows.iter().filter_map(entry_of).collect();
    joined(&[], &entries)
}

/// Whether an entry is one of a node's other names.
pub fn is_name(entry: &Entry) -> bool {
    matches!(entry.kind, Kind::Into | Kind::Split)
}

fn entry_of(row: &Row) -> Option<Entry> {
    let entry = |kind| Entry {
        audience: row.audience.clone(),
        target: row.object.clone(),
        at: row.hlc.clone(),
        kind,
        homes: Vec::new(),
        to: None,
        takes: Vec::new(),
    };
    match row.rule() {
        // A forget that lists rows keeps who wrote it: its list counts
        // against a kept row only from a home of a retract that reaches it.
        Rule::Forget => Some(Entry {
            homes: if row.takes.is_empty() {
                Vec::new()
            } else {
                vec![row.origin.clone()]
            },
            takes: row.takes.clone(),
            ..entry(Kind::Forget)
        }),
        Rule::Retract => Some(Entry {
            homes: vec![row.origin.clone()],
            takes: row.takes.clone(),
            ..entry(Kind::Retract)
        }),
        // Only the side that wrote it holds a local row.
        Rule::Home if row.replicate == Replicate::Local => Some(entry(Kind::Home)),
        _ => {
            let to = row.to.clone()?;
            let kind = match crate::kinds::names(&row.kind) {
                crate::kinds::Names::None => return None,
                crate::kinds::Names::Into => Kind::Into,
                crate::kinds::Names::Split => Kind::Split,
            };
            Some(Entry {
                to: Some(to),
                takes: row.takes.clone(),
                ..entry(kind)
            })
        }
    }
}

/// The ledger with `more` added, sorted: per target one forget (and one
/// more for each side that wrote a forget listing rows), one retract with
/// every home, one home, one entry per node it went into and one split per
/// part, each at its earliest. Nothing on it is ever dropped.
pub fn joined(ledger: &[Entry], more: &[Entry]) -> Vec<Entry> {
    type Key<'a> = (&'a str, &'a str, Kind, Option<&'a str>);
    let mut entries: BTreeMap<Key, Entry> = BTreeMap::new();
    for entry in ledger.iter().chain(more) {
        let part = if is_name(entry) {
            entry.to.as_deref()
        } else if entry.kind == Kind::Forget {
            entry.homes.first().map(String::as_str)
        } else {
            None
        };
        let key = (
            entry.audience.as_str(),
            entry.target.as_str(),
            entry.kind,
            part,
        );
        let Some(kept) = entries.get_mut(&key) else {
            entries.insert(key, entry.clone());
            continue;
        };
        if (&entry.at, &entry.to) < (&kept.at, &kept.to) {
            kept.at = entry.at.clone();
            kept.to = entry.to.clone();
        }
        kept.homes = union(&kept.homes, &entry.homes);
        kept.takes = union(&kept.takes, &entry.takes);
    }
    entries.into_values().collect()
}

fn union(a: &[String], b: &[String]) -> Vec<String> {
    let all: BTreeSet<&String> = a.iter().chain(b).collect();
    all.into_iter().cloned().collect()
}

/// Whether a retract is on `ledger`.
pub fn has_retract(ledger: &[Entry]) -> bool {
    ledger.iter().any(Entry::is_retract)
}

/// The ledger read for one call: what reaches which row.
pub struct View<'a> {
    /// (audience, target) → its forgets, retracts and homes.
    by_target: BTreeMap<(&'a str, &'a str), Vec<&'a Entry>>,
    /// (audience, node) → every node it went into.
    into: BTreeMap<(&'a str, &'a str), BTreeSet<&'a str>>,
    /// (audience, row id) → every split that took it: (from, to).
    splits: BTreeMap<(&'a str, &'a str), BTreeSet<(&'a str, &'a str)>>,
    /// (audience, row id) → every retract that lists it.
    listed: BTreeMap<(&'a str, &'a str), Vec<&'a Entry>>,
    /// (audience, row id) → every forget that lists it: one pressed on a
    /// home names the kept rows its fold showed in the node.
    forget_listed: BTreeMap<(&'a str, &'a str), Vec<&'a Entry>>,
}

impl<'a> View<'a> {
    pub fn new(ledger: &'a [Entry]) -> Self {
        let mut view = Self {
            by_target: BTreeMap::new(),
            into: BTreeMap::new(),
            splits: BTreeMap::new(),
            listed: BTreeMap::new(),
            forget_listed: BTreeMap::new(),
        };
        for entry in ledger {
            let place = (entry.audience.as_str(), entry.target.as_str());
            match (entry.kind, entry.to.as_deref()) {
                (Kind::Home, _) => {
                    view.by_target.entry(place).or_default().push(entry);
                }
                (Kind::Forget, _) => {
                    view.by_target.entry(place).or_default().push(entry);
                    for row in &entry.takes {
                        view.forget_listed
                            .entry((entry.audience.as_str(), row.as_str()))
                            .or_default()
                            .push(entry);
                    }
                }
                (Kind::Retract, _) => {
                    view.by_target.entry(place).or_default().push(entry);
                    for row in &entry.takes {
                        view.listed
                            .entry((entry.audience.as_str(), row.as_str()))
                            .or_default()
                            .push(entry);
                    }
                }
                (Kind::Into, Some(to)) => {
                    view.into.entry(place).or_default().insert(to);
                }
                (Kind::Split, Some(to)) => {
                    for row in &entry.takes {
                        view.splits
                            .entry((entry.audience.as_str(), row.as_str()))
                            .or_default()
                            .insert((entry.target.as_str(), to));
                    }
                }
                (Kind::Into | Kind::Split, None) => {}
            }
        }
        view
    }

    /// `nodes`, with every node any of them went into, and on: all of
    /// them, however many merges each saw.
    fn onward(&self, audience: &str, nodes: &mut BTreeSet<&'a str>) {
        let mut todo: Vec<&str> = nodes.iter().copied().collect();
        while let Some(node) = todo.pop() {
            for &next in self.into.get(&(audience, node)).into_iter().flatten() {
                if nodes.insert(next) {
                    todo.push(next);
                }
            }
        }
    }

    /// `object` and every node it went into, and on.
    pub fn reach(&self, audience: &str, object: &'a str) -> BTreeSet<&'a str> {
        let mut out = BTreeSet::from([object]);
        self.onward(audience, &mut out);
        out
    }

    /// Every object whose forget reaches `row`: its object and each node
    /// it went on into; every place a split that took it moved it from one
    /// of those, and on from there; and for a merge or a split, the other
    /// node it names and on.
    fn targets(&self, row: &'a Row) -> BTreeSet<&'a str> {
        let audience = row.audience.as_str();
        let mut out = BTreeSet::from([row.object.as_str()]);
        if let Some(to) = &row.to {
            out.insert(to.as_str());
        }
        self.onward(audience, &mut out);
        let splits = self.splits.get(&(audience, row.id.as_str()));
        loop {
            let more: Vec<&str> = splits
                .into_iter()
                .flatten()
                .filter(|(from, to)| out.contains(from) && !out.contains(to))
                .map(|&(_, to)| to)
                .collect();
            if more.is_empty() {
                return out;
            }
            out.extend(more);
            self.onward(audience, &mut out);
        }
    }

    /// The retracts that reach `row`: one of its own node, and one that
    /// lists it. Never through a merge or a split.
    fn retracts(&self, row: &'a Row) -> Vec<&'a Entry> {
        let audience = row.audience.as_str();
        let mut out: Vec<&Entry> = self
            .by_target
            .get(&(audience, row.object.as_str()))
            .into_iter()
            .flatten()
            .filter(|entry| entry.is_retract())
            .copied()
            .collect();
        out.extend(
            self.listed
                .get(&(audience, row.id.as_str()))
                .into_iter()
                .flatten()
                .copied(),
        );
        out
    }

    /// The forgets, retracts and homes that reach `row` (none reaches a
    /// row of the ledger itself): forgets through every name and the
    /// subject; retracts, and this side's homes of them, only directly.
    pub fn reaching(&self, row: &'a Row) -> Vec<&'a Entry> {
        if on_ledger(row) {
            return Vec::new();
        }
        let audience = row.audience.as_str();
        let mut out: Vec<&Entry> = self
            .targets(row)
            .into_iter()
            .filter_map(|target| self.by_target.get(&(audience, target)))
            .flatten()
            .filter(|entry| entry.kind == Kind::Forget)
            .copied()
            .collect();
        out.extend(self.subject_of(row));
        out.extend(self.listing_forgets(row));
        let retracts = self.retracts(row);
        let nodes: BTreeSet<&str> = retracts.iter().map(|entry| entry.target.as_str()).collect();
        out.extend(retracts);
        out.extend(
            nodes
                .into_iter()
                .filter_map(|node| self.by_target.get(&(audience, node)))
                .flatten()
                .filter(|entry| entry.kind == Kind::Home)
                .copied(),
        );
        out
    }

    /// The forgets that list `row`.
    fn listing_forgets(&self, row: &'a Row) -> Vec<&'a Entry> {
        self.forget_listed
            .get(&(row.audience.as_str(), row.id.as_str()))
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }

    /// The forgets of everything about `row`'s subject.
    fn subject_of(&self, row: &'a Row) -> Vec<&'a Entry> {
        let Some(subject) = row.subject.as_deref() else {
            return Vec::new();
        };
        let target = format!("{SUBJECT}:{subject}");
        self.by_target
            .get(&(row.audience.as_str(), target.as_str()))
            .into_iter()
            .flatten()
            .filter(|entry| entry.kind == Kind::Forget)
            .copied()
            .collect()
    }

    /// Whether a forget reaches `row` through its object (or a node it
    /// went into) or its subject: the whole object is gone, not only this
    /// row.
    pub fn forgets_object(&self, row: &'a Row) -> bool {
        self.forgotten(&row.audience, &row.object).is_some() || !self.subject_of(row).is_empty()
    }

    /// Whether this side keeps `row`: a retract of this side's reaches it.
    fn kept_by(&self, reaching: &[&'a Entry]) -> bool {
        reaching.iter().any(|entry| entry.kind == Kind::Home)
    }

    /// Whether a forget takes a row this side keeps: only one that names
    /// it directly: its own node, the node of a retract of this side's that
    /// reaches it, its subject, or a forget that lists it (pressed on a
    /// home, it names the kept rows its fold showed in the node: what the
    /// partner saw is what they forget; it counts only when its writer is
    /// a home of a retract that reaches the row, since no other side's fold
    /// shows it). None through a merge, a split or another node, whatever
    /// its clock.
    fn forgets_kept(&self, row: &'a Row, reaching: &[&'a Entry]) -> bool {
        let audience = row.audience.as_str();
        let named = |target: &str| {
            self.by_target
                .get(&(audience, target))
                .is_some_and(|entries| entries.iter().any(|entry| entry.kind == Kind::Forget))
        };
        named(&row.object)
            || reaching
                .iter()
                .filter(|entry| entry.kind == Kind::Home)
                .any(|entry| named(&entry.target))
            || !self.subject_of(row).is_empty()
            || self.listing_forgets(row).iter().any(|forget| {
                forget.homes.iter().any(|origin| {
                    reaching
                        .iter()
                        .any(|entry| entry.is_retract() && entry.homes.contains(origin))
                })
            })
    }

    /// Whether `origin` wrote a retract in `audience`: a home, whose fold
    /// may show rows a retract keeps.
    pub fn retracted_in(&self, audience: &str, origin: &str) -> bool {
        self.by_target
            .iter()
            .filter(|((at, _), _)| *at == audience)
            .flat_map(|(_, entries)| entries)
            .any(|entry| entry.is_retract() && entry.homes.iter().any(|home| home == origin))
    }

    /// Whether `row` is taken away on `side`. On a home, a row a retract of
    /// its own reaches is taken only by a forget that names it directly.
    /// Otherwise a forget that reaches it takes it, and so do retracts (a
    /// local row stays on the side that wrote it).
    pub fn absorbs(&self, row: &'a Row, side: Side) -> bool {
        let reaching = self.reaching(row);
        if side == Side::Here && self.kept_by(&reaching) {
            return self.forgets_kept(row, &reaching);
        }
        if reaching.iter().any(|entry| entry.kind == Kind::Forget) {
            return true;
        }
        if !reaching.iter().any(|entry| entry.is_retract()) {
            return false;
        }
        match side {
            Side::Away => true,
            Side::Here => row.replicate != Replicate::Local,
        }
    }

    /// Whether a retract of this side's reaches `row`, forgotten since or
    /// not.
    pub fn keeps(&self, row: &'a Row) -> bool {
        self.kept_by(&self.reaching(row))
    }

    /// Whether `row` is one this side keeps, and nothing forgets it: its
    /// body stays here, whatever arrives.
    pub fn kept_home(&self, row: &'a Row) -> bool {
        let reaching = self.reaching(row);
        self.kept_by(&reaching) && !self.forgets_kept(row, &reaching)
    }

    /// The sides that wrote a retract of `audience`'s `object`, sorted.
    pub fn homes(&self, audience: &str, object: &str) -> Vec<String> {
        let homes: BTreeSet<&String> = self
            .by_target
            .get(&(audience, object))
            .into_iter()
            .flatten()
            .filter(|entry| entry.is_retract())
            .flat_map(|entry| &entry.homes)
            .collect();
        homes.into_iter().cloned().collect()
    }

    /// Whether this side keeps `audience`'s `object`: it wrote its retract.
    pub fn kept(&self, audience: &str, object: &str) -> bool {
        self.home_of(audience, object)
    }

    /// Whether this side wrote the retract of `audience`'s `object`.
    pub fn home_of(&self, audience: &str, object: &str) -> bool {
        self.by_target
            .get(&(audience, object))
            .is_some_and(|entries| entries.iter().any(|entry| entry.kind == Kind::Home))
    }

    /// The earliest forget of `audience`'s `object` or a node it went on
    /// into.
    pub fn forgotten(&self, audience: &str, object: &'a str) -> Option<&'a Rev> {
        self.reach(audience, object)
            .into_iter()
            .filter_map(|target| self.by_target.get(&(audience, target)))
            .flatten()
            .filter(|entry| entry.kind == Kind::Forget)
            .map(|entry| &entry.at)
            .min()
    }
}

/// Whether `ledger` forgets `row` (retracts aside).
pub fn forgets(ledger: &[Entry], row: &Row) -> bool {
    View::new(ledger)
        .reaching(row)
        .iter()
        .any(|entry| entry.kind == Kind::Forget)
}

/// `rows` with every row `ledger` takes away on `side` turned into a stub,
/// and the ids of the rows that had a payload until now (whose payload the
/// caller deletes).
pub fn absorb(rows: Vec<Row>, ledger: &[Entry], side: Side) -> (Vec<Row>, Vec<String>) {
    let view = View::new(ledger);
    let taken: Vec<bool> = rows
        .iter()
        .map(|row| !row.forgotten && view.absorbs(row, side))
        .collect();
    let mut stubbed = Vec::new();
    let rows = rows
        .into_iter()
        .zip(taken)
        .map(|(row, taken)| {
            if !taken {
                return row;
            }
            stubbed.push(row.id.clone());
            row.stub()
        })
        .collect();
    stubbed.sort();
    (rows, stubbed)
}

/// The ledger a call holds: the one given, with what `rows` show. Once a
/// retract is in play the caller's own ledger must be given (a home may be
/// only on it, and a home never loses its node to a call that did not say
/// what it keeps).
pub fn given(ledger: Option<&[Entry]>, rows: &[Row]) -> Result<Vec<Entry>, LogError> {
    let shown = ledger_of_checked(rows);
    match ledger {
        Some(ledger) => Ok(joined(ledger, &shown)),
        None if has_retract(&shown) => Err(LogError::new(
            "a retract is in the rows: give this side's ledger",
        )),
        None => Ok(shown),
    }
}

/// The ledger of the rows whose envelope is whole.
fn ledger_of_checked(rows: &[Row]) -> Vec<Entry> {
    let checked: Vec<Row> = rows
        .iter()
        .filter(|row| row.check().is_ok())
        .cloned()
        .collect();
    ledger(&checked)
}

/// `log.ledger`: rows this side wrote (or its whole log), and its ledger.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordInput {
    pub rows: Vec<Row>,
    #[serde(default)]
    pub ledger: Vec<Entry>,
    /// This side: given, a row it may not hold ([`crate::Holder::holds_row`])
    /// is refused, so none is ever written.
    #[serde(default)]
    pub holder: Option<crate::Holder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recorded {
    pub ledger: Vec<Entry>,
}

/// The ledger with what `rows` make: a side runs every row it writes
/// through this (its forgets, retracts and their homes, and rows naming
/// another node), as `log.admit` does for the rows it takes. Its homes come
/// only from here: no side sends a local row.
pub fn record(input: RecordInput) -> Result<Recorded, LogError> {
    if let Some(holder) = &input.holder {
        holder.check()?;
    }
    for row in &input.rows {
        row.check()
            .map_err(|_| LogError::new("a row's envelope is not whole"))?;
        // A side never writes what it may not hold: about the partner
        // under an id it does not go by, or of an audience not its own.
        if input
            .holder
            .as_ref()
            .is_some_and(|holder| !holder.holds_row(row))
        {
            return Err(LogError::new("a row this side may not hold"));
        }
    }
    Ok(Recorded {
        ledger: joined(&input.ledger, &ledger(&input.rows)),
    })
}

/// `log.reach`: rows of this side's log (any of them, as stubs or whole)
/// and its ledger.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReachInput {
    pub rows: Vec<Row>,
    pub ledger: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reached {
    /// The rows given whole that the ledger takes away here, sorted: their
    /// payloads go.
    pub stubbed: Vec<String>,
}

/// Which of this side's rows its ledger takes away now: a host that merged
/// only part of its log (or learnt a name or a retract) asks this over the
/// rest, rather than working out what a forget reaches itself.
pub fn reach(input: ReachInput) -> Result<Reached, LogError> {
    for row in &input.rows {
        row.check()
            .map_err(|_| LogError::new("a row's envelope is not whole"))?;
    }
    let (_, stubbed) = absorb(input.rows, &input.ledger, Side::Here);
    Ok(Reached { stubbed })
}
