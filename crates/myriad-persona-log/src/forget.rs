//! Forgetting absorbs: once an object (or everything about a subject in an
//! audience) is forgotten, no row of it reads back, whether its rows came
//! before the forget, after it, or from a side that was offline when it
//! happened.
//!
//! The ledger holds only what was forgotten, where and when: never a word
//! of what it said. A row on the ledger becomes a stub: its payload goes,
//! its place in the log (id, side, count) stays so vectors still agree.
//!
//! **Taking a node back from sync** (撤回同步, a `retract` row) is the same
//! on every side but one. The partner writes it on the side that is to keep
//! the node, its home: the row's `origin`. Every other side reads it as a
//! forget of that node: its rows become stubs there, their bodies go, and
//! rows of it that come later (an edit raced from another side, a side that
//! was offline) come in as stubs. The home keeps the node and every row of
//! it. So a retract's ledger entry names its home, and a side asks the
//! ledger as itself: `here` is every origin it writes or wrote as (`loc_…`
//! before it linked, `dev_…` after, `site`). The first retract of a node, in
//! clock order, is the one that holds. It is one way, as a forget is:
//! sharing the node again would be a new row of the partner's, which the
//! core does not have yet.

use std::collections::BTreeMap;

use myriad_persona_sync::{Rev, hlc};
use serde::{Deserialize, Serialize};

use crate::{LogError, Row, kinds::Rule, kinds::SUBJECT};

/// One thing forgotten: an object, or `subject:<id>` for everything about
/// that subject, in one audience. Or one node taken back from sync, with
/// the side that keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub audience: String,
    pub target: String,
    /// The forget's clock (the earliest, when it was forgotten twice).
    pub at: Rev,
    /// A retract's: the side that keeps the node (the retract's origin),
    /// where this entry forgets nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home: Option<String>,
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
            home: (row.rule() == Rule::Retract).then(|| row.origin.clone()),
        })
        .collect();
    joined(&[], &entries)
}

/// The ledger with `more` added, sorted, earliest forget kept, and the
/// earliest retract with its home.
pub fn joined(ledger: &[Entry], more: &[Entry]) -> Vec<Entry> {
    let mut entries: BTreeMap<(&str, &str, bool), (&Rev, Option<&str>)> = BTreeMap::new();
    for entry in ledger.iter().chain(more) {
        let key = (
            entry.audience.as_str(),
            entry.target.as_str(),
            entry.home.is_some(),
        );
        let home = entry.home.as_deref();
        let kept = entries.entry(key).or_insert((&entry.at, home));
        if entry.at < *kept.0 {
            *kept = (&entry.at, home);
        }
    }
    entries
        .into_iter()
        .map(|((audience, target, _), (at, home))| Entry {
            audience: audience.to_string(),
            target: target.to_string(),
            at: at.clone(),
            home: home.map(str::to_string),
        })
        .collect()
}

/// Whether `ledger` forgets `row` (retracts aside). A forget or a retract
/// itself is never forgotten.
pub fn forgets(ledger: &[Entry], row: &Row) -> bool {
    absorbs(
        &ledger
            .iter()
            .filter(|entry| entry.home.is_none())
            .cloned()
            .collect::<Vec<_>>(),
        row,
        &[],
    )
}

/// Whether `ledger` takes `row` away on the side that writes as `here`:
/// a forget of it, or a retract whose home is not `here`. An empty `here`
/// is no home at all: every retract takes its node away (what a side sends
/// another, or a file).
pub fn absorbs(ledger: &[Entry], row: &Row, here: &[String]) -> bool {
    if on_ledger(row) {
        return false;
    }
    let subject = row
        .subject
        .as_deref()
        .map(|subject| format!("{SUBJECT}:{subject}"));
    ledger.iter().any(|entry| {
        entry.audience == row.audience
            && (entry.target == row.object || Some(&entry.target) == subject.as_ref())
            && entry.home.as_ref().is_none_or(|home| !here.contains(home))
    })
}

/// `rows` with every row `ledger` takes away on the side `here` turned into
/// a stub, and the ids of the rows that had a payload until now (whose
/// payload the caller deletes).
pub fn absorb(rows: Vec<Row>, ledger: &[Entry], here: &[String]) -> (Vec<Row>, Vec<String>) {
    let mut stubbed = Vec::new();
    let rows = rows
        .into_iter()
        .map(|row| {
            if row.forgotten || !absorbs(ledger, &row, here) {
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
        None if ledger.iter().any(|entry| entry.home.is_some()) => Err(LogError::new(
            "a retract is on the ledger: say which side this is (here)",
        )),
        None => Ok(&[]),
    }
}
