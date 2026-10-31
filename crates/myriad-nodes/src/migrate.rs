//! Moving her nodes to the partner's one id (`myriad_persona_log::partner`).
//!
//! A side still on `local-owner` that links for the first time moves once,
//! before its first sync on the link:
//! its rows (`partner::row`), its ledger (`partner::entry`: audiences, and
//! forgets of all about the partner, so nothing forgotten comes back), and
//! the bodies that name the partner: the audience tag of a span or an
//! entry, who knows and whose progress, and, in the partner's private talk
//! only, a `person:<from>` key (made, related to, gone into). Every side
//! moves a body to the same bytes.
//!
//! **A node keeps its id.** The id of a node in the partner's private talk
//! is not made from the partner's id (`crate::key`), so moving relabels the
//! audience and nothing else: every row keeps its object, and the names in
//! envelopes (`to`, `takes`) and on the ledger stay true. A key moves only
//! in the partner's private talk, where `person:<from>` and `person:<to>`
//! make the same node id; in her own audience a person key's id is made
//! from the key itself, so it stays as written. A move is only from
//! `local-owner`: rows under a partner id are that partner's, and rows
//! under a site's account id cannot be moved (their node ids are made from
//! it), so a site holding any has to say so rather than move them. A
//! private audience of several people (`personal:a,b`) does not move.
//!
//! A move is the same whether it runs once or again after it was cut off:
//! rows already under the new id are left as they are. A body the core
//! cannot read is never changed; its row is listed (`unread`) so the host
//! can see what may still name `local-owner`. A body the core keeps opaque
//! is not read either: a `case.*` payload (a judged case, 判过的事) goes as
//! it is, and the host moves any id of the partner's inside it itself.
//!
//! The host writes a move in one transaction where it can. Where it
//! cannot, it sets `here.moving` to the id it moves to before the first
//! write and clears it after
//! the last; the id, `linked`, and clearing `filed` are saved last, so a
//! move cut off anywhere is taken up again by the next link
//! (`myriad_persona_log::partner::link`).

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
    /// The id it went by: `local-owner` (`partner::link` moves from nothing
    /// else).
    pub from: String,
    /// The id the link gave.
    pub to: String,
}

/// Two rows that land on one place: the same side's same count in one
/// audience. A side wrote under two of the partner's ids with one count,
/// which moving cannot mend. The row already under `to` stays (or, with
/// none, the first in log order), and the other (`id`) is left out of what
/// is returned: rows, bodies and `moved`. The host deletes it, or sets it
/// aside out of the log: left there under `local-owner`, it keeps
/// `partner.link` moving again forever (`Here::audiences`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Clash {
    pub id: String,
    pub with: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Migrated {
    /// Every row, moved, in log order, but the later of each clash.
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
    /// Rows of a node kind in the moved audiences whose body the core does
    /// not read (or no longer reads once moved): left as they are, sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unread: Vec<String>,
}

/// A key moved: `person:<from>` is `person:<to>`.
fn key(key: &str, by: &Move) -> String {
    match key.strip_prefix("person:") {
        Some(person) if person == by.from => format!("person:{}", by.to),
        _ => key.to_string(),
    }
}

/// The body of `row` (as it was before moving) moved: `Ok(Some)` when
/// anything in it changes, `Ok(None)` when nothing does or it is no body
/// that moves, `Err(())` when it is a node body the core does not read (it
/// goes unchanged, and is listed).
fn body(row: &Row, value: &Value, by: &Move) -> Result<Option<Value>, ()> {
    let moves = by.is_ours(&row.audience) || row.audience == "own:";
    if !moves || !row.kind.starts_with("node.") {
        return Ok(None);
    }
    let mut body = Body::read(&row.kind, value).map_err(|_| ())?;
    let before = body.clone();
    let keys = by.is_ours(&row.audience);
    match &mut body {
        Body::Created(created) if keys => created.key = key(&created.key, by),
        Body::Related(related) if keys => related.to = key(&related.to, by),
        Body::Absorbed(absorbed) if keys => absorbed.into = key(&absorbed.into, by),
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
        return Ok(None);
    }
    // Read again: the one byte form, and the same checks as any body.
    Body::read(&row.kind, &body.to_value())
        .map(|body| Some(body.to_value()))
        .map_err(|_| ())
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
    // A row about another partner id is that partner's: no move makes it
    // this one's.
    let another = |id: &str| id != by.to && partner::check(id).is_ok();
    if rows.iter().any(|row| {
        row.subject.as_deref().is_some_and(another)
            || row
                .object
                .split_once(':')
                .is_some_and(|(kind, id)| partner::PERSON_OBJECTS.contains(&kind) && another(id))
    }) {
        return Err(NodeError::new(
            "a row is about another partner id: it is not this partner's to move",
        ));
    }
    // Rows already under `to`, before the move: on a clash, they stay.
    let under_to: std::collections::BTreeSet<String> = rows
        .iter()
        .filter(|row| row.audience.strip_prefix("personal:") == Some(by.to.as_str()))
        .map(|row| row.id.clone())
        .collect();
    let mut moved = Vec::new();
    let mut unread = Vec::new();
    let mut out = Vec::new();
    for row in rows {
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
        let mut changed = false;
        match bodies.get(&row.id).map(|value| body(&row, value, &by)) {
            Some(Ok(Some(new))) => {
                bodies.insert(row.id.clone(), new);
                changed = true;
            }
            Some(Err(())) => unread.push(row.id.clone()),
            Some(Ok(None)) | None => {}
        }
        let new = partner::row(&row, &by);
        changed |= new != row;
        if changed {
            moved.push(new.id.clone());
        }
        out.push(new);
    }
    sort(&mut out);
    // Each place's rows in log order; the one already under `to` stays,
    // else the first.
    let mut places: BTreeMap<(&str, &str, u64), Vec<&str>> = BTreeMap::new();
    for row in &out {
        let Some(dseq) = row.dseq else { continue };
        let ids = places
            .entry((row.audience.as_str(), row.origin.as_str(), dseq))
            .or_default();
        if !ids.contains(&row.id.as_str()) {
            ids.push(&row.id);
        }
    }
    let mut clashes = Vec::new();
    for ids in places.values().filter(|ids| ids.len() > 1) {
        let stays = ids
            .iter()
            .find(|id| under_to.contains(**id))
            .unwrap_or(&ids[0]);
        for id in ids.iter().filter(|id| *id != stays) {
            clashes.push(Clash {
                id: id.to_string(),
                with: stays.to_string(),
            });
        }
    }
    clashes.sort_by(|a, b| a.id.cmp(&b.id));
    let dropped: std::collections::BTreeSet<String> =
        clashes.iter().map(|clash| clash.id.clone()).collect();
    out.retain(|row| !dropped.contains(&row.id));
    bodies.retain(|id, _| !dropped.contains(id));
    moved.retain(|id| !dropped.contains(id));
    unread.retain(|id| !dropped.contains(id));
    moved.sort();
    moved.dedup();
    unread.sort();
    unread.dedup();
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
        unread,
    })
}
