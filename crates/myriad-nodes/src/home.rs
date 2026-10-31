//! A home's own backup: what a side keeps of nodes taken back from sync
//! (撤回同步), whole, for the export file's home part
//! (`myriad_persona_sync::archive::Part::Home`).
//!
//! The log pages of a file carry a retracted node's rows only as stubs
//! ([`crate::wire`]), so a home's backup would otherwise lose what it is the
//! home of. A home page carries, for one audience, every row of a node this
//! side keeps ([`myriad_persona_log::forget::View::kept_home`]): its rows
//! whole, its local rows, the `retract` and the local `retract.home` marks,
//! and each row's body as its canonical bytes, base64, in the batch frame
//! (`{audience, rows, payloads, withheld?}`).
//!
//! Only a computer of the partner's writes or takes one ([`send`],
//! [`take`]): a site or a server never holds what a home keeps. Taking a
//! page onto another computer makes that one the home, as the side that
//! pressed it was: its marks go on that side's ledger, and the rows are
//! stored as that side's own, before the file's log pages are taken in
//! (whose stubs never replace a row a home holds whole). A row the page
//! holds of a node this side has forgotten since comes in as a stub.

use std::collections::BTreeMap;

use base64::Engine as _;
use myriad_persona_log::{
    Core, Holder, Row, Rule,
    forget::{self, Entry, Side},
};
use myriad_persona_sync::canonical::canonical_bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{Body, READ},
    wire::{Batch, Carried, Why, Withheld},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendInput {
    pub audience: String,
    /// This side's rows of the audience.
    pub rows: Vec<Row>,
    /// Each row's body, by row id.
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    /// This side's ledger.
    pub ledger: Vec<Entry>,
    /// This side: a computer of the partner's.
    pub holder: Holder,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TakeInput {
    /// A home page, as [`send`] made it.
    pub page: Batch,
    /// This side: a computer of the partner's.
    pub holder: Holder,
    /// This side's ledger.
    pub ledger: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Taken {
    /// The rows to store as this side's own, in log order.
    pub rows: Vec<Row>,
    /// Their bodies, by row id, as read.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bodies: BTreeMap<String, Value>,
    /// This side's ledger with the page's marks: this side is now a home of
    /// the nodes it took.
    pub ledger: Vec<Entry>,
}

/// Only a computer of the partner's holds what a home keeps, and only of an
/// audience it may hold.
fn check_holder(holder: &Holder, audience: &str) -> Result<(), NodeError> {
    holder
        .check()
        .map_err(|error| NodeError::new(error.to_string()))?;
    if holder.core != Core::Pc {
        return Err(NodeError::new(
            "only a computer of the partner's holds a home's pages",
        ));
    }
    if !holder.holds(audience) {
        return Err(NodeError::new(
            "a home page of an audience this side may not hold",
        ));
    }
    Ok(())
}

/// Whether a row is a retract or a home's mark of one.
fn mark(row: &Row) -> bool {
    matches!(row.rule(), Rule::Retract | Rule::Home)
}

/// A body's canonical bytes, read as its kind reads: a node kind the core
/// reads must read as one.
fn canonical(row: &Row, body: &Value) -> Result<Vec<u8>, NodeError> {
    if row.kind.starts_with("node.") && READ.contains(&row.kind.as_str()) {
        return Body::read(&row.kind, body)
            .map(|body| body.canonical_bytes())
            .map_err(|_| NodeError::new("a home body does not read as its kind"));
    }
    Ok(canonical_bytes(body))
}

/// The home page of one audience: every row of a node this side keeps, with
/// its body. A row whose body this side lacks goes alone (`no_body`).
pub fn send(input: SendInput) -> Result<Batch, NodeError> {
    let SendInput {
        audience,
        rows,
        bodies,
        ledger,
        holder,
    } = input;
    check_holder(&holder, &audience)?;
    if rows.iter().any(|row| row.audience != audience) {
        return Err(NodeError::new("a row is not of the audience"));
    }
    let ledger = forget::joined(&ledger, &forget::ledger(&rows));
    let view = forget::View::new(&ledger);
    let mut kept: Vec<Row> = rows
        .iter()
        .filter(|row| {
            mark(row) && view.home_of(&row.audience, &row.object)
                || !row.forgotten && view.kept_home(row)
        })
        .cloned()
        .collect();
    myriad_persona_log::row::sort(&mut kept);
    let engine = base64::engine::general_purpose::STANDARD;
    let mut payloads = Vec::new();
    let mut withheld = Vec::new();
    for row in &kept {
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
        let Some(payload) = &row.payload else {
            continue;
        };
        match bodies.get(&row.id) {
            Some(body) => payloads.push(Carried {
                reference: payload.reference.clone(),
                data: engine.encode(canonical(row, body)?),
            }),
            None => withheld.push(Withheld {
                id: row.id.clone(),
                why: Why::NoBody,
            }),
        }
    }
    Ok(Batch {
        audience,
        rows: kept,
        payloads,
        withheld,
    })
}

/// A home page taken onto this side: every row must be of a node the page's
/// marks (or this side's ledger) keep here, and every body its canonical
/// bytes.
pub fn take(input: TakeInput) -> Result<Taken, NodeError> {
    let TakeInput {
        page,
        holder,
        ledger,
    } = input;
    check_holder(&holder, &page.audience)?;
    let mut carried: BTreeMap<&str, &str> = BTreeMap::new();
    for one in &page.payloads {
        if carried
            .insert(one.reference.as_str(), one.data.as_str())
            .is_some()
        {
            return Err(NodeError::new("a payload is in the page twice"));
        }
    }
    for row in &page.rows {
        if row.audience != page.audience {
            return Err(NodeError::new("a row is not of the page's audience"));
        }
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
    }
    let ledger = forget::joined(&ledger, &forget::ledger(&page.rows));
    let view = forget::View::new(&ledger);
    if page
        .rows
        .iter()
        .any(|row| !view.home_of(&row.audience, &row.object) && !view.kept_home(row))
    {
        return Err(NodeError::new(
            "a row of a home page is of no node kept here",
        ));
    }
    let (mut rows, _) = forget::absorb(page.rows, &ledger, Side::Here);
    myriad_persona_log::row::sort(&mut rows);
    let engine = base64::engine::general_purpose::STANDARD;
    let mut bodies = BTreeMap::new();
    for row in &rows {
        let Some(payload) = row.payload.as_ref().filter(|_| !row.forgotten) else {
            continue;
        };
        let Some(data) = carried.get(payload.reference.as_str()) else {
            continue;
        };
        let bad = || NodeError::new("a home body is not its canonical bytes");
        let bytes = engine.decode(data).map_err(|_| bad())?;
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| bad())?;
        if canonical(row, &body)? != bytes {
            return Err(bad());
        }
        bodies.insert(row.id.clone(), body);
    }
    Ok(Taken {
        rows,
        bodies,
        ledger,
    })
}
