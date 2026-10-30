//! Forgetting absorbs: once an object (or everything about a subject in an
//! audience) is forgotten, no row of it reads back, whether its rows came
//! before the forget, after it, or from a side that was offline when it
//! happened.
//!
//! The ledger holds only what was forgotten, where and when: never a word
//! of what it said. A row on the ledger becomes a stub: its payload goes,
//! its place in the log (id, side, count) stays so vectors still agree.
//!
//! **Taking a node back from sync** (撤回同步, a `retract` row) works as a
//! forget does, except on its homes. The partner writes it on a side that is
//! to keep the node: the row's `origin` is a home. A node retracted on two
//! sides (each pressed while the other was offline) has two homes, and
//! each keeps what it holds: no clock decides between them (the partner's
//! decision, 2026-10-30: 两台都留). Every other
//! side reads it as a forget of that node: its rows become stubs there,
//! their bodies go, and rows of it that come later (an edit raced from
//! another side, a side that was offline) come in as stubs. A home never
//! loses a body to a stub: a stub of a row it holds whole does not replace
//! it ([`crate::merge`]), and a whole row of a stub it holds fills it, so
//! a home ends the same whatever order the same rows reach it in. A body
//! that was written elsewhere and had not reached any home when its side
//! learnt of the retract is dropped there, as a forget would drop it, and
//! is kept nowhere (the partner's decision, 2026-10-30: 当作忘掉). A local row stays whole on
//! the side that wrote it: it was never anywhere else, so there is nothing
//! to take it from. So a retract's ledger entry names its homes, and a side
//! asks the ledger as itself: `here` is every origin it writes or wrote as
//! (`loc_…` before it linked, `dev_…` after, `site`). It is one way, as a
//! forget is: sharing the node again would be a new row of the partner's,
//! which the core does not have yet.
//!
//! **Other names of an object** ([`Names`]): a node keeps its id when the
//! partner's id moves, so after a move its old rows name the old id; and a
//! node that went into another is shown there. A forget or a retract of a
//! node reaches every id it has and every node that went into it. Which ids
//! those are only the node layer can tell (from bodies a side may delete),
//! so the caller keeps them, as `myriad_nodes` folds them, and gives them to
//! every call that stubs.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_sync::{Rev, hlc};
use serde::{Deserialize, Serialize};

use crate::{LogError, Row, kinds::Rule, kinds::SUBJECT, row::Replicate};

/// One thing forgotten: an object, or `subject:<id>` for everything about
/// that subject, in one audience. Or one node taken back from sync, with
/// the sides that keep it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub audience: String,
    pub target: String,
    /// The forget's clock (the earliest, when it was forgotten twice).
    pub at: Rev,
    /// A retract's: every side that keeps the node (each retract's origin),
    /// sorted, where this entry forgets nothing. A forget has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub homes: Vec<String>,
}

impl Entry {
    pub fn is_retract(&self) -> bool {
        !self.homes.is_empty()
    }

    /// Whether this side (`here`) is one of the retract's homes.
    pub fn home_is(&self, here: &[String]) -> bool {
        self.homes.iter().any(|home| here.contains(home))
    }
}

/// The other names of objects, as the caller keeps them: an old id and the
/// id it reads as now (`same`), and a node and the node it went into
/// (`into`). Objects are `<type>:<id>`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Names {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub same: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub into: BTreeMap<String, String>,
}

impl Names {
    pub fn is_empty(&self) -> bool {
        self.same.is_empty() && self.into.is_empty()
    }

    /// `object` as it reads now.
    pub fn now<'a>(&'a self, object: &'a str) -> &'a str {
        let mut at = object;
        for _ in 0..=self.same.len() {
            match self.same.get(at) {
                Some(next) if next != at => at = next,
                _ => break,
            }
        }
        at
    }

    /// Every object a forget or a retract of which reaches `object`, each
    /// as it reads now: itself, then each node it went into.
    pub fn reached_by<'a>(&'a self, object: &'a str) -> Vec<&'a str> {
        let mut out = vec![self.now(object)];
        while let Some(next) = self.into.get(*out.last().expect("one at least")) {
            let next = self.now(next);
            if out.contains(&next) {
                break;
            }
            out.push(next);
        }
        out
    }

    /// These names with `more`: a name already known stays.
    pub fn joined(&self, more: &Names) -> Names {
        let mut out = self.clone();
        for (from, to) in &more.same {
            out.same.entry(from.clone()).or_insert_with(|| to.clone());
        }
        for (from, to) in &more.into {
            out.into.entry(from.clone()).or_insert_with(|| to.clone());
        }
        out
    }

    pub fn check(&self) -> Result<(), LogError> {
        let well = |object: &str| {
            object
                .split_once(':')
                .is_some_and(|(kind, id)| !kind.is_empty() && !id.is_empty())
        };
        if self
            .same
            .iter()
            .chain(&self.into)
            .any(|(from, to)| !well(from) || !well(to) || from == to)
        {
            return Err(LogError::new(
                "names map one <type>:<id> to another, never to itself",
            ));
        }
        Ok(())
    }
}

/// Whether a row is one the ledger is made of: a forget or a retract.
pub fn on_ledger(row: &Row) -> bool {
    matches!(row.rule(), Rule::Forget | Rule::Retract)
}

/// The ledger the forgets and retracts in `rows` make, sorted by audience
/// and target (a forget before a retract), earliest kept.
pub fn ledger(rows: &[Row]) -> Vec<Entry> {
    let entries: Vec<Entry> = rows
        .iter()
        .filter(|row| on_ledger(row))
        .map(|row| Entry {
            audience: row.audience.clone(),
            target: row.object.clone(),
            at: row.hlc.clone(),
            homes: if row.rule() == Rule::Retract {
                vec![row.origin.clone()]
            } else {
                Vec::new()
            },
        })
        .collect();
    joined(&[], &entries)
}

/// The ledger with `more` added, sorted: one forget and one retract per
/// target at most, each at its earliest, a retract with every home.
pub fn joined(ledger: &[Entry], more: &[Entry]) -> Vec<Entry> {
    let mut entries: BTreeMap<(&str, &str, bool), (&Rev, BTreeSet<&str>)> = BTreeMap::new();
    for entry in ledger.iter().chain(more) {
        let key = (
            entry.audience.as_str(),
            entry.target.as_str(),
            entry.is_retract(),
        );
        let kept = entries
            .entry(key)
            .or_insert_with(|| (&entry.at, BTreeSet::new()));
        if entry.at < *kept.0 {
            kept.0 = &entry.at;
        }
        kept.1.extend(entry.homes.iter().map(String::as_str));
    }
    entries
        .into_iter()
        .map(|((audience, target, _), (at, homes))| Entry {
            audience: audience.to_string(),
            target: target.to_string(),
            at: at.clone(),
            homes: homes.into_iter().map(str::to_string).collect(),
        })
        .collect()
}

/// The entries of `ledger` that reach `row` (a forget or a retract itself
/// is reached by none).
fn reaching<'a>(ledger: &'a [Entry], row: &Row, names: &Names) -> Vec<&'a Entry> {
    if on_ledger(row) {
        return Vec::new();
    }
    let subject = row
        .subject
        .as_deref()
        .map(|subject| format!("{SUBJECT}:{subject}"));
    let reached = names.reached_by(&row.object);
    ledger
        .iter()
        .filter(|entry| {
            entry.audience == row.audience
                && (reached.contains(&names.now(&entry.target))
                    || Some(&entry.target) == subject.as_ref())
        })
        .collect()
}

/// Whether `ledger` forgets `row` (retracts aside).
pub fn forgets(ledger: &[Entry], row: &Row, names: &Names) -> bool {
    reaching(ledger, row, names)
        .iter()
        .any(|entry| !entry.is_retract())
}

/// Whether `ledger` takes `row` away on the side that writes as `here`:
/// a forget of it, or a retract none of whose homes is `here` (unless the
/// row is a local one `here` wrote). An empty `here` is no home at all:
/// every retract takes its node away (what a side sends another, or a
/// file).
pub fn absorbs(ledger: &[Entry], row: &Row, here: &[String], names: &Names) -> bool {
    // Its own local row, which exists nowhere else.
    let own_local = row.replicate == Replicate::Local && here.contains(&row.origin);
    reaching(ledger, row, names)
        .iter()
        .any(|entry| !entry.is_retract() || (!entry.home_is(here) && !own_local))
}

/// Whether `row` is of a node this side (`here`) is a home of, and nothing
/// forgets it: its body stays here, whatever arrives.
pub fn kept_home(ledger: &[Entry], row: &Row, here: &[String], names: &Names) -> bool {
    let reaching = reaching(ledger, row, names);
    reaching
        .iter()
        .any(|entry| entry.is_retract() && entry.home_is(here))
        && !reaching.iter().any(|entry| !entry.is_retract())
}

/// The homes of every retract reaching `audience`'s `object`, sorted.
pub fn homes(ledger: &[Entry], audience: &str, object: &str, names: &Names) -> Vec<String> {
    let reached = names.reached_by(object);
    let homes: BTreeSet<&str> = ledger
        .iter()
        .filter(|entry| {
            entry.is_retract()
                && entry.audience == audience
                && reached.contains(&names.now(&entry.target))
        })
        .flat_map(|entry| entry.homes.iter().map(String::as_str))
        .collect();
    homes.into_iter().map(str::to_string).collect()
}

/// `rows` with every row `ledger` takes away on the side `here` turned into
/// a stub, and the ids of the rows that had a payload until now (whose
/// payload the caller deletes).
pub fn absorb(
    rows: Vec<Row>,
    ledger: &[Entry],
    here: &[String],
    names: &Names,
) -> (Vec<Row>, Vec<String>) {
    let mut stubbed = Vec::new();
    let rows = rows
        .into_iter()
        .map(|row| {
            if row.forgotten || !absorbs(ledger, &row, here, names) {
                return row;
            }
            stubbed.push(row.id.clone());
            row.stub()
        })
        .collect();
    stubbed.sort();
    (rows, stubbed)
}

/// The side's names, checked: a ledger with a retract on it needs them, so
/// a home never loses its node to a call that did not say where it runs.
/// Without a retract, no names is none.
pub fn here<'a>(ledger: &[Entry], here: Option<&'a [String]>) -> Result<&'a [String], LogError> {
    match here {
        Some(names) => {
            if names.is_empty() {
                return Err(LogError::new(
                    "here names this side: every origin it writes or wrote as",
                ));
            }
            for name in names {
                hlc::check_node(name)
                    .map_err(|_| LogError::new("here holds a name that is not a clock's node"))?;
            }
            Ok(names)
        }
        None if ledger.iter().any(Entry::is_retract) => Err(LogError::new(
            "a retract is on the ledger: say which side this is (here)",
        )),
        None => Ok(&[]),
    }
}
