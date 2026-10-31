//! Merging is a union: the log only grows, and no order of arrival loses a
//! row. A row seen twice counts once (by id); a stub beats the row it was,
//! except on a home of the node it is about, where the whole row stays (and
//! fills a stub held): a retract keeps everything there; after the union
//! every row the ledger takes away here becomes a stub.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    LogError, Row, forget,
    row::{Replicate, sort},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergeInput {
    /// This side's log, or the part of it that matters: the rows sharing an
    /// id or a place (`audience`, `origin`, `dseq`) with `incoming`, and the
    /// rows any forget, retract or name in `incoming` reaches.
    pub log: Vec<Row>,
    /// Rows already through [`crate::admit`].
    pub incoming: Vec<Row>,
    /// This side's ledger ([`crate::forget`]): the forgets, retracts, homes
    /// and names held here that the rows given may not show. Always given
    /// (empty when there is none): a later batch without the retract never
    /// stubs a row a home keeps.
    pub ledger: Vec<forget::Entry>,
}

/// Why an incoming row was not taken. Either means a side broke the format
/// (an id or a count used twice); the row already held stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Clash {
    /// The same id with another envelope.
    SameIdDiffers,
    /// Another row already holds this side's count in this audience.
    SamePlace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub id: String,
    pub clash: Clash,
    /// The row already held, for [`Clash::SamePlace`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Merged {
    /// The union, in log order.
    pub log: Vec<Row>,
    /// Incoming rows that were new here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    /// Rows of this side's log that are stubs now: delete their payloads.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stubbed: Vec<String>,
    /// Stubs held that an incoming whole row filled, on a home of their
    /// node: store their payloads.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filled: Vec<String>,
    /// Incoming rows already held.
    pub duplicates: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<Conflict>,
}

pub fn merge(input: MergeInput) -> Result<Merged, LogError> {
    let MergeInput {
        log,
        mut incoming,
        ledger,
    } = input;
    // A local row stays on the side that wrote it: one in `incoming` came
    // from elsewhere (it would make another side's home this one's).
    if incoming.iter().any(|row| row.replicate == Replicate::Local) {
        return Err(LogError::new(
            "a local row never comes in from another side",
        ));
    }
    let ledger = forget::given(
        Some(&ledger),
        &[log.as_slice(), incoming.as_slice()].concat(),
    )?;
    let view = forget::View::new(&ledger);
    let mut by_id: BTreeMap<String, Row> = BTreeMap::new();
    let mut places: BTreeMap<(String, String, u64), String> = BTreeMap::new();
    let mut held_payload = Vec::new();
    for row in log {
        if let (Replicate::Shared, Some(dseq)) = (row.replicate, row.dseq) {
            places.insert(
                (row.audience.clone(), row.origin.clone(), dseq),
                row.id.clone(),
            );
        }
        if !row.forgotten {
            held_payload.push(row.id.clone());
        }
        by_id.insert(row.id.clone(), row);
    }
    sort(&mut incoming);
    let mut added = Vec::new();
    let mut filled = Vec::new();
    let mut duplicates = 0;
    let mut conflicts = Vec::new();
    for row in incoming {
        if let Some(held) = by_id.get(&row.id) {
            let home = view.kept_home(&row);
            if *held == row {
                duplicates += 1;
            } else if held.is_stub_of(&row) {
                duplicates += 1;
                if home {
                    filled.push(row.id.clone());
                    by_id.insert(row.id.clone(), row);
                }
            } else if row.is_stub_of(held) {
                duplicates += 1;
                if !home {
                    by_id.insert(row.id.clone(), row);
                }
            } else {
                conflicts.push(Conflict {
                    id: row.id.clone(),
                    clash: Clash::SameIdDiffers,
                    with: None,
                });
            }
            continue;
        }
        if let (Replicate::Shared, Some(dseq)) = (row.replicate, row.dseq) {
            let place = (row.audience.clone(), row.origin.clone(), dseq);
            if let Some(other) = places.get(&place) {
                conflicts.push(Conflict {
                    id: row.id.clone(),
                    clash: Clash::SamePlace,
                    with: Some(other.clone()),
                });
                continue;
            }
            places.insert(place, row.id.clone());
        }
        added.push(row.id.clone());
        by_id.insert(row.id.clone(), row);
    }
    let rows: Vec<Row> = by_id.into_values().collect();
    let (mut log, _) = forget::absorb(rows, &ledger, forget::Side::Here);
    sort(&mut log);
    let forgotten: BTreeMap<&str, bool> = log
        .iter()
        .map(|row| (row.id.as_str(), row.forgotten))
        .collect();
    let mut stubbed: Vec<String> = held_payload
        .into_iter()
        .filter(|id| forgotten.get(id.as_str()) == Some(&true))
        .collect();
    stubbed.sort();
    added.sort();
    filled.sort();
    Ok(Merged {
        log,
        added,
        stubbed,
        filled,
        duplicates,
        conflicts,
    })
}
