//! Moving her nodes to the partner's one id (`myriad_persona_log::partner`).
//!
//! A side that links for the first time (or that took an id from a file
//! while empty, then links) moves once, before its first sync on the link:
//! its rows (`partner::row`), its ledger (`partner::entry`: audiences, and
//! forgets of all about the partner, so nothing forgotten comes back), and
//! the bodies that name the partner: the audience tag of a span or an
//! entry, a `person:<from>` key (made, related to, gone into), who knows
//! and whose progress. Every side moves a body to the same bytes.
//!
//! **A node keeps its id.** The id of a node in the partner's private talk
//! is not made from the partner's id (`crate::key`), so moving relabels the
//! audience and nothing else: every row keeps its object, and the names in
//! envelopes (`to`, `takes`) and on the ledger stay true. A move is only
//! from `local-owner` or a partner id: rows under a site's account id
//! cannot be moved (their node ids are made from it), so a site holding any
//! has to say so rather than move them. A private audience of several
//! people (`personal:a,b`) does not move.

use std::collections::BTreeMap;

use myriad_persona_log::{
    Row,
    forget::{self, Entry},
    partner::{self, Move},
    row::sort,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{Body, Change},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrateInput {
    /// The rows to move: a side's whole log of the partner's private
    /// audience, and of her own.
    pub rows: Vec<Row>,
    /// Each row's body, by row id.
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    /// This side's ledger.
    pub ledger: Vec<Entry>,
    /// The id it went by: `local-owner`, or one it took from a file.
    pub from: String,
    /// The id the link gave.
    pub to: String,
}

/// Two rows that land on one place: the same side's same count in one
/// audience. A side wrote under two of the partner's ids with one count,
/// which moving cannot mend; the first in log order stays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clash {
    pub id: String,
    pub with: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Migrated {
    /// Every row, moved, in log order.
    pub rows: Vec<Row>,
    /// Every body given, moved (the same when nothing in it named the
    /// partner).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bodies: BTreeMap<String, Value>,
    /// The ledger, moved: what to keep from now on.
    pub ledger: Vec<Entry>,
    /// The rows whose envelope or body changed: what to write back.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clashes: Vec<Clash>,
}

/// A key moved: `person:<from>` is `person:<to>`.
fn key(key: &str, by: &Move) -> String {
    match key.strip_prefix("person:") {
        Some(person) if person == by.from => format!("person:{}", by.to),
        _ => key.to_string(),
    }
}

/// The body of `row` (as it was before moving) moved, when anything in it
/// changes; `None` when nothing does, or the body is not one the core reads
/// (it goes unchanged).
fn body(row: &Row, value: &Value, by: &Move) -> Option<Value> {
    let moves = by.is_ours(&row.audience) || row.audience == "own:";
    if !moves || !row.kind.starts_with("node.") {
        return None;
    }
    let mut body = Body::read(&row.kind, value).ok()?;
    let before = body.clone();
    match &mut body {
        Body::Created(created) => created.key = key(&created.key, by),
        Body::Related(related) => related.to = key(&related.to, by),
        Body::Absorbed(absorbed) => absorbed.into = key(&absorbed.into, by),
        Body::Span(span) => span.audience = by.audience(&span.audience),
        Body::Entry(entry) => entry.audience = by.audience(&entry.audience),
        Body::Revised(revised) => {
            if let Some(person) = &revised.person {
                revised.person = Some(by.person(person).to_string());
            }
            if let Ok(Change::WhoKnows(ids)) = revised.change() {
                let mut ids: Vec<String> = ids.iter().map(|id| by.person(id).to_string()).collect();
                ids.sort();
                ids.dedup();
                revised.value = Value::from(ids);
            }
        }
        _ => {}
    }
    if body == before {
        return None;
    }
    // Read again: the one byte form, and the same checks as any body.
    Body::read(&row.kind, &body.to_value())
        .ok()
        .map(|body| body.to_value())
}

/// `rows`, their bodies and the ledger moved from one id to the other.
pub fn migrate(input: MigrateInput) -> Result<Migrated, NodeError> {
    let MigrateInput {
        rows,
        mut bodies,
        ledger,
        from,
        to,
    } = input;
    let by = Move { from, to };
    by.check()
        .map_err(|error| NodeError::new(error.to_string()))?;
    let mut moved = Vec::new();
    let mut out = Vec::new();
    for row in rows {
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
        let mut changed = false;
        let body = bodies.get(&row.id).and_then(|value| body(&row, value, &by));
        if let Some(new) = body {
            bodies.insert(row.id.clone(), new);
            changed = true;
        }
        let new = partner::row(&row, &by);
        changed |= new != row;
        if changed {
            moved.push(new.id.clone());
        }
        out.push(new);
    }
    sort(&mut out);
    let mut places: BTreeMap<(&str, &str, u64), &str> = BTreeMap::new();
    let mut clashes = Vec::new();
    for row in &out {
        let Some(dseq) = row.dseq else { continue };
        let place = (row.audience.as_str(), row.origin.as_str(), dseq);
        match places.get(&place) {
            Some(other) if *other != row.id => clashes.push(Clash {
                id: row.id.clone(),
                with: other.to_string(),
            }),
            _ => {
                places.insert(place, &row.id);
            }
        }
    }
    moved.sort();
    moved.dedup();
    let ledger: Vec<Entry> = ledger
        .iter()
        .map(|entry| partner::entry(entry, &by))
        .collect();
    Ok(Migrated {
        rows: out,
        bodies,
        ledger: forget::joined(&[], &ledger),
        moved,
        clashes,
    })
}
