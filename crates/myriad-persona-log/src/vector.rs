//! What a side holds, as a version vector, and what the other lacks.
//!
//! Each side numbers its shared rows in each audience 1, 2, 3… without
//! gaps (`dseq`), so "every row of `origin` up to n" says all a side holds
//! of it in that audience. A vector is one audience's: two sides swap
//! vectors audience by audience and send each other what is past the
//! other's number; no content is compared.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    Holder, LogError, Row,
    row::{Replicate, sort},
};

/// For each side, the last `dseq` held in one audience with every one
/// before it.
pub type Vector = BTreeMap<String, u64>;

/// What a log holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holdings {
    pub vector: Vector,
    /// Rows held past a gap, by side: they arrived before a row ahead of
    /// them did. The vector does not count them until the gap fills.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub beyond: BTreeMap<String, Vec<u64>>,
}

/// Refuses rows of more than one audience: their counts are separate, so
/// one vector cannot say what they hold.
fn one_audience(rows: &[Row]) -> Result<(), LogError> {
    match rows.split_first() {
        Some((first, rest)) if rest.iter().any(|row| row.audience != first.audience) => Err(
            LogError::new("a vector is one audience's; these rows are of more than one"),
        ),
        _ => Ok(()),
    }
}

/// What `rows`, one audience's, hold. Local rows hold no place in it.
pub fn holdings(rows: &[Row]) -> Result<Holdings, LogError> {
    one_audience(rows)?;
    let mut held: BTreeMap<&str, BTreeSet<u64>> = BTreeMap::new();
    for row in rows {
        if let (Replicate::Shared, Some(dseq)) = (row.replicate, row.dseq) {
            held.entry(&row.origin).or_default().insert(dseq);
        }
    }
    let mut vector = Vector::new();
    let mut beyond = BTreeMap::new();
    for (origin, seqs) in held {
        let mut through = 0;
        while seqs.contains(&(through + 1)) {
            through += 1;
        }
        if through > 0 {
            vector.insert(origin.to_string(), through);
        }
        let past: Vec<u64> = seqs.range(through + 1..).copied().collect();
        if !past.is_empty() {
            beyond.insert(origin.to_string(), past);
        }
    }
    Ok(Holdings { vector, beyond })
}

/// A run of one side's rows: `from` through `through`, both included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Span {
    pub origin: String,
    pub from: u64,
    pub through: u64,
}

/// What `theirs` holds that `mine` does not, by side.
pub fn missing(mine: &Vector, theirs: &Vector) -> Vec<Span> {
    theirs
        .iter()
        .filter_map(|(origin, &through)| {
            let have = mine.get(origin).copied().unwrap_or(0);
            (through > have).then(|| Span {
                origin: origin.clone(),
                from: have + 1,
                through,
            })
        })
        .collect()
}

/// The rows of `rows`, one audience's, a side holding `theirs` of that
/// audience lacks, in the order to send
/// them: every forget first, so the other side knows what is forgotten
/// before anything it might have to forget arrives; then by side and count.
/// Local rows never go.
pub fn answer(rows: &[Row], theirs: &Vector) -> Result<Vec<Row>, LogError> {
    one_audience(rows)?;
    let lacking = |row: &&Row| {
        row.replicate == Replicate::Shared
            && row.dseq.unwrap_or(0) > theirs.get(&row.origin).copied().unwrap_or(0)
    };
    let mut forgets: Vec<Row> = rows
        .iter()
        .filter(lacking)
        .filter(|row| crate::forget::on_ledger(row))
        .cloned()
        .collect();
    sort(&mut forgets);
    let mut rest: Vec<Row> = rows
        .iter()
        .filter(lacking)
        .filter(|row| !crate::forget::on_ledger(row))
        .cloned()
        .collect();
    rest.sort_by(|a, b| (&a.origin, a.dseq, &a.id).cmp(&(&b.origin, b.dseq, &b.id)));
    let mut seen = BTreeSet::new();
    Ok(forgets
        .into_iter()
        .chain(rest)
        .filter(|row| seen.insert(row.id.clone()))
        .collect())
}

/// What [`answer_to`] sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Answered {
    pub rows: Vec<Row>,
    /// The side asking may not hold this audience: nothing goes.
    #[serde(skip_serializing_if = "is_false")]
    pub not_held: bool,
    /// Rows the other side may not hold whole, sorted: each goes as a stub
    /// (a forget is left out), as `nodes.wire_send`'s `heldBack`. Only a
    /// host's fault leaves such a row in a log.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub held_back: Vec<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// [`answer`], for the side that is `holder`: a side that may not hold the
/// audience gets nothing ([`crate::holder`]).
///
/// Only for a log of rows from before ABI 2: a row that names another node
/// (`to`, `takes`) or a retract is refused, for what such a log sends
/// depends on this side's ledger (stubs of what it forgot or took back)
/// and on the other side's ABI, which `nodes.wire_send` weighs and this
/// does not.
///
/// `here` is the id this side goes by: a side under another id gets
/// nothing ([`crate::partner::exchange`]).
pub fn answer_to(
    rows: &[Row],
    theirs: &Vector,
    holder: &Holder,
    here: &str,
) -> Result<Answered, LogError> {
    if rows
        .iter()
        .any(|row| row.to.is_some() || !row.takes.is_empty() || row.rule() == crate::Rule::Retract)
    {
        return Err(LogError::new(
            "a row names another node or is a retract: it goes by nodes.wire_send",
        ));
    }
    holder.check()?;
    // A child core holds no log: it gets nothing, whatever id it names.
    if holder.core != crate::Core::Child {
        crate::partner::exchange(here, &holder.owner)?;
    }
    one_audience(rows)?;
    if rows.first().is_some_and(|row| !holder.holds(&row.audience)) {
        return Ok(Answered {
            rows: Vec::new(),
            not_held: true,
            held_back: Vec::new(),
        });
    }
    // As `nodes.wire_send` does: a row the other side may not hold whole
    // goes as a stub, so its count has no gap.
    let mut held_back = Vec::new();
    let rows = answer(rows, theirs)?
        .into_iter()
        .filter_map(|row| {
            if holder.holds_row(&row) {
                return Some(row);
            }
            held_back.push(row.id.clone());
            // A forget has no stub: it is left out.
            (!crate::forget::on_ledger(&row)).then(|| row.stub())
        })
        .collect();
    held_back.sort();
    Ok(Answered {
        rows,
        not_held: false,
        held_back,
    })
}
