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
//! the node, and that side writes two rows: a shared `retract`, which every
//! side reads, and a local `retract.home` beside it. Every side without a
//! `retract.home` of the node reads the retract as a forget of it: its rows
//! become stubs there, their bodies go, and rows of it that come later (an
//! edit raced from another side, a side that was offline) come in as
//! stubs. A local row never comes from another side ([`crate::admit`]
//! refuses one), so only the side that pressed it is ever a home: a
//! retract another side sends that names this side as its writer, and that
//! this side never wrote, is a forget here like any other. A node retracted
//! on two sides (each pressed while the other was offline) has two homes,
//! and each keeps what it holds: no clock decides between them (the
//! partner's decision, 2026-10-30: 两台都留).
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
//! **Other names of a node.** A forget or a retract names one node, and
//! reaches every node that went into it (`node.absorbed`), and the spans a
//! split moved to it (`node.split_off`). Which those are is in the rows'
//! envelopes (`to`, `takes`), so a stub keeps it and every side reads the
//! same from its own log: the ledger holds them too ([`Kind::Into`],
//! [`Kind::Split`]), so a call given only part of the log still knows them.
//! A node that went into two (two sides each merged it, offline) went into
//! the one written first, by clock and then side. A name counts whatever
//! its body says: a forget or a retract errs toward taking more.

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
    /// The target node is taken back from sync; `homes` wrote it.
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
    /// A retract's: every side that wrote one, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub homes: Vec<String>,
    /// A name's: the other node (`node:<id>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// A split's: the rows it moved, sorted.
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
/// earliest kept. A stub's envelope makes the same as its row.
pub fn ledger(rows: &[Row]) -> Vec<Entry> {
    let entries: Vec<Entry> = rows.iter().filter_map(entry_of).collect();
    joined(&[], &entries)
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
        Rule::Forget => Some(entry(Kind::Forget)),
        Rule::Retract => Some(Entry {
            homes: vec![row.origin.clone()],
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

/// The ledger with `more` added, sorted: per target one forget, one
/// retract with every home, one home, one node it went into (the earliest
/// written) and one split per part, each at its earliest.
pub fn joined(ledger: &[Entry], more: &[Entry]) -> Vec<Entry> {
    type Key<'a> = (&'a str, &'a str, Kind, Option<&'a str>);
    let mut entries: BTreeMap<Key, Entry> = BTreeMap::new();
    for entry in ledger.iter().chain(more) {
        let part = (entry.kind == Kind::Split)
            .then_some(entry.to.as_deref())
            .flatten();
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

/// A split taking a row: its clock, the node split and the part.
type Taking<'a> = (&'a Rev, &'a str, &'a str);

/// The ledger read for one call: what reaches which row.
pub struct View<'a> {
    /// (audience, target) → its forgets, retracts and homes.
    by_target: BTreeMap<(&'a str, &'a str), Vec<&'a Entry>>,
    /// (audience, node) → the node it went into.
    into: BTreeMap<(&'a str, &'a str), &'a str>,
    /// (audience, row id) → the splits that take it: (at, from, to).
    splits: BTreeMap<(&'a str, &'a str), Vec<Taking<'a>>>,
}

impl<'a> View<'a> {
    pub fn new(ledger: &'a [Entry]) -> Self {
        let mut view = Self {
            by_target: BTreeMap::new(),
            into: BTreeMap::new(),
            splits: BTreeMap::new(),
        };
        // `joined` leaves one into per node, but a caller's ledger may not
        // have been through it: the earliest still wins.
        let mut into: BTreeMap<(&str, &str), (&Rev, &str)> = BTreeMap::new();
        for entry in ledger {
            let place = (entry.audience.as_str(), entry.target.as_str());
            match (entry.kind, entry.to.as_deref()) {
                (Kind::Forget | Kind::Retract | Kind::Home, _) => {
                    view.by_target.entry(place).or_default().push(entry);
                }
                (Kind::Into, Some(to)) => {
                    let first = into.entry(place).or_insert((&entry.at, to));
                    if (&entry.at, to) < *first {
                        *first = (&entry.at, to);
                    }
                }
                (Kind::Split, Some(to)) => {
                    for row in &entry.takes {
                        view.splits
                            .entry((entry.audience.as_str(), row.as_str()))
                            .or_default()
                            .push((&entry.at, entry.target.as_str(), to));
                    }
                }
                (Kind::Into | Kind::Split, None) => {}
            }
        }
        view.into = into
            .into_iter()
            .map(|(place, (_, to))| (place, to))
            .collect();
        for splits in view.splits.values_mut() {
            splits.sort();
        }
        view
    }

    /// `object`, then each node it went into, in order.
    pub fn chain(&self, audience: &str, object: &'a str) -> Vec<&'a str> {
        let mut out = vec![object];
        while let Some(&next) = self
            .into
            .get(&(audience, *out.last().expect("one at least")))
        {
            if out.contains(&next) {
                break;
            }
            out.push(next);
        }
        out
    }

    /// Every object whose forget or retract reaches `row`: its object and
    /// each node that went on from it; where the splits that took it put
    /// it, and on from there; and for a row naming another node, that one.
    fn targets(&self, row: &'a Row) -> BTreeSet<&'a str> {
        let audience = row.audience.as_str();
        let mut out: BTreeSet<&str> = self.chain(audience, &row.object).into_iter().collect();
        let mut at: &str = &row.object;
        for &(_, from, to) in self
            .splits
            .get(&(audience, row.id.as_str()))
            .into_iter()
            .flatten()
        {
            if self.chain(audience, at).contains(&from) {
                at = to;
            }
        }
        out.extend(self.chain(audience, at));
        if let Some(to) = &row.to {
            out.extend(self.chain(audience, to));
        }
        out
    }

    /// The forgets, retracts and homes that reach `row` (none reaches a
    /// row of the ledger itself).
    pub fn reaching(&self, row: &'a Row) -> Vec<&'a Entry> {
        if on_ledger(row) {
            return Vec::new();
        }
        let audience = row.audience.as_str();
        let subject = row
            .subject
            .as_deref()
            .map(|subject| format!("{SUBJECT}:{subject}"));
        let mut out: Vec<&Entry> = self
            .targets(row)
            .into_iter()
            .filter_map(|target| self.by_target.get(&(audience, target)))
            .flatten()
            .copied()
            .collect();
        if let Some(subject) = &subject
            && let Some(entries) = self.by_target.get(&(audience, subject.as_str()))
        {
            out.extend(entries.iter().filter(|entry| entry.kind == Kind::Forget));
        }
        out
    }

    /// Whether a forget reaches `row` through its object (or a node it
    /// went into) or its subject: the whole object is gone, not only this
    /// row.
    pub fn forgets_object(&self, row: &'a Row) -> bool {
        let subject = row
            .subject
            .as_deref()
            .map(|subject| format!("{SUBJECT}:{subject}"));
        self.forgotten(&row.audience, &row.object).is_some()
            || subject.is_some_and(|subject| {
                self.by_target
                    .get(&(row.audience.as_str(), subject.as_str()))
                    .is_some_and(|entries| entries.iter().any(|entry| entry.kind == Kind::Forget))
            })
    }

    /// Whether `row` is taken away on `side`: a forget reaches it, or
    /// retracts do and no home of this side does (a local row stays on the
    /// side that wrote it).
    pub fn absorbs(&self, row: &'a Row, side: Side) -> bool {
        let reaching = self.reaching(row);
        if reaching.iter().any(|entry| entry.kind == Kind::Forget) {
            return true;
        }
        if !reaching.iter().any(|entry| entry.is_retract()) {
            return false;
        }
        match side {
            Side::Away => true,
            Side::Here => {
                row.replicate != Replicate::Local
                    && !reaching.iter().any(|entry| entry.kind == Kind::Home)
            }
        }
    }

    /// Whether `row` is of a node this side keeps, and nothing forgets it:
    /// its body stays here, whatever arrives.
    pub fn kept_home(&self, row: &'a Row) -> bool {
        let reaching = self.reaching(row);
        !reaching.iter().any(|entry| entry.kind == Kind::Forget)
            && reaching.iter().any(|entry| entry.kind == Kind::Home)
    }

    /// The entries that name `audience`'s `object` or a node it went on
    /// into, of `kind`.
    fn about(&self, audience: &str, object: &'a str, kind: Kind) -> Vec<&'a Entry> {
        self.chain(audience, object)
            .into_iter()
            .filter_map(|target| self.by_target.get(&(audience, target)))
            .flatten()
            .filter(|entry| entry.kind == kind)
            .copied()
            .collect()
    }

    /// The sides that wrote a retract reaching `audience`'s `object`,
    /// sorted.
    pub fn homes(&self, audience: &str, object: &'a str) -> Vec<String> {
        let homes: BTreeSet<&String> = self
            .about(audience, object, Kind::Retract)
            .into_iter()
            .flat_map(|entry| &entry.homes)
            .collect();
        homes.into_iter().cloned().collect()
    }

    /// Whether this side keeps `audience`'s `object` (a home of it reaches
    /// it).
    pub fn kept(&self, audience: &str, object: &'a str) -> bool {
        !self.about(audience, object, Kind::Home).is_empty()
    }

    /// The earliest forget of `audience`'s `object` or a node it went on
    /// into.
    pub fn forgotten(&self, audience: &str, object: &'a str) -> Option<&'a Rev> {
        self.about(audience, object, Kind::Forget)
            .into_iter()
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
    for row in &input.rows {
        row.check()
            .map_err(|_| LogError::new("a row's envelope is not whole"))?;
    }
    Ok(Recorded {
        ledger: joined(&input.ledger, &ledger(&input.rows)),
    })
}
