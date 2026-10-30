//! Moving her nodes to the partner's one id (`myriad_persona_log::partner`).
//!
//! A side whose partner id changed rewrites its rows once
//! (`partner::row`), and the bodies that name the partner: the audience tag
//! of a span or an entry, a `person:<alias>` key (made, related to, gone
//! into), who knows and whose progress. Every side that holds a body moves
//! it to the same bytes, so a body is still the same bytes wherever it is;
//! bodies that arrive later from a side that has not moved are moved as
//! they arrive (`wire::take` with `partner`).
//!
//! **A node keeps its id.** The id of a node in the partner's private talk
//! is not made from the partner's id (`crate::key::node_id`), so moving
//! relabels the audience and nothing else: every row keeps its object, and
//! the names in envelopes (`to`, `takes`) and on the ledger stay true. A
//! private audience of several people (`personal:a,b`) does not move.

use std::collections::BTreeMap;

use myriad_persona_log::{Partner, Row, partner, row::sort};
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
    /// audience under any of their ids, and of her own.
    pub rows: Vec<Row>,
    /// Each row's body, by row id.
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    pub partner: Partner,
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
    /// The rows whose envelope or body changed: what to write back.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub clashes: Vec<Clash>,
}

/// A key as it reads under `partner`: `person:<alias>` is `person:<id>`.
pub fn key(key: &str, partner: &Partner) -> String {
    match key.strip_prefix("person:") {
        Some(person) if partner.is(person) => format!("person:{}", partner.id),
        _ => key.to_string(),
    }
}

/// Whether the rows of `audience` move with the partner: theirs, under any
/// id, and her own.
fn moves(audience: &str, partner: &Partner) -> bool {
    audience == "own:"
        || audience
            .strip_prefix("personal:")
            .is_some_and(|member| partner.is(member))
}

/// The body of `row` (as it was before moving) moved under `partner`, when
/// anything in it changes; `None` when nothing does, or the body is not one
/// the core reads (it goes unchanged).
pub fn body(row: &Row, value: &Value, partner: &Partner) -> Option<Value> {
    if !moves(&row.audience, partner) || !row.kind.starts_with("node.") {
        return None;
    }
    let mut body = Body::read(&row.kind, value).ok()?;
    let before = body.clone();
    match &mut body {
        Body::Created(created) => created.key = key(&created.key, partner),
        Body::Related(related) => related.to = key(&related.to, partner),
        Body::Absorbed(absorbed) => absorbed.into = key(&absorbed.into, partner),
        Body::Span(span) => span.audience = partner.audience(&span.audience),
        Body::Entry(entry) => entry.audience = partner.audience(&entry.audience),
        Body::Revised(revised) => {
            if let Some(person) = &revised.person {
                revised.person = Some(partner.person(person).to_string());
            }
            if let Ok(Change::WhoKnows(ids)) = revised.change() {
                let mut ids: Vec<String> = ids
                    .iter()
                    .map(|id| partner.person(id).to_string())
                    .collect();
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

/// `rows` and their bodies moved to the partner's id in use.
pub fn migrate(input: MigrateInput) -> Result<Migrated, NodeError> {
    let MigrateInput {
        rows,
        mut bodies,
        partner,
    } = input;
    partner
        .check()
        .map_err(|error| NodeError::new(error.to_string()))?;
    let mut moved = Vec::new();
    let mut out = Vec::new();
    for row in rows {
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
        let mut changed = false;
        let body = bodies
            .get(&row.id)
            .and_then(|value| body(&row, value, &partner));
        if let Some(new) = body {
            bodies.insert(row.id.clone(), new);
            changed = true;
        }
        let new = partner::row(&row, &partner);
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
    Ok(Migrated {
        rows: out,
        bodies,
        moved,
        clashes,
    })
}
