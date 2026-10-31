//! A home's own backup: what a side keeps of nodes taken back from sync
//! (撤回同步), whole, for the export file's home part
//! (`myriad_persona_sync::archive::Part::Home`).
//!
//! The log pages of a file carry a retracted node's rows only as stubs
//! ([`crate::wire`]), so a home's backup would otherwise lose what it is the
//! home of. A home page carries, for one audience, every row this side
//! keeps ([`myriad_persona_log::forget::View::kept_home`]: the rows its own
//! retracts reach), its local rows of them too, the `retract` and the
//! local `retract.home` marks, and each row's body as its canonical bytes,
//! base64, in the batch frame (`{audience, rows, payloads, withheld?}`).
//!
//! A body goes into a page only as it would go on the wire, but for its
//! sync tag ([`crate::wire::carry_home`]): a body kept on its device goes
//! too, for upkeep on a kept node writes only such bodies and the file's
//! home part is opened only on a computer of the partner's
//! (`myriad_persona_sync::archive`). Still no local path, no secret, not
//! too large, of its row's audience, not a person's or a group's node in
//! hers (by the created bodies in the page, and `elsewhere`). One that
//! fails goes as its row alone, and the page says why (`withheld`), on
//! writing and again on taking: the host tells the partner what the file
//! leaves out. Taken onto another computer, such a row is stored whole but
//! with its body missing, not as a stub: it is no row forgotten, and a body
//! of it that comes later fills it.
//!
//! Only a computer of the partner's writes or takes one ([`send`],
//! [`take`]): a site or a server never holds what a home keeps. Taking a
//! page onto another computer makes that one the home, as the side that
//! pressed it was: its marks go on that side's ledger, and the rows are
//! stored as that side's own, replacing any stub of them it holds already
//! (`filled`), before the file's log pages are taken in (whose stubs never
//! replace a row a home holds whole). A row the page holds that this side
//! has forgotten since comes in as a stub.

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine as _;
use myriad_persona_log::{
    Core, Holder, Row, Rule,
    forget::{self, Entry, Side},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    wire::{Batch, Carried, Decoded, Why, Withheld, carry_home, elsewhere},
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
    /// Nodes known to put a person or a group in her own audience
    /// ([`crate::wire::Taken::elsewhere`], kept): no body of theirs goes.
    #[serde(default)]
    pub elsewhere: Vec<String>,
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
    /// The rows of the page's audience this side holds already (it synced
    /// before the file came): a stub of them the page holds whole is
    /// `filled`.
    #[serde(default)]
    pub held: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Taken {
    /// The rows to store as this side's own, in log order, each replacing
    /// the row of its id this side holds. A row held whole whose body the
    /// page does not bring is left out (and out of `withheld`): it stays as
    /// held.
    pub rows: Vec<Row>,
    /// Their bodies, by row id, as read.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bodies: BTreeMap<String, Value>,
    /// The rows of `held` that were stubs and are whole now, sorted: store
    /// their bodies.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filled: Vec<String>,
    /// The page's rows whose body is not taken, and why.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub withheld: Vec<Withheld>,
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

/// The home page of one audience: every row this side keeps, with its body
/// when it may go. A row whose body this side lacks, or whose body may not
/// go, goes alone (`withheld`).
pub fn send(input: SendInput) -> Result<Batch, NodeError> {
    let SendInput {
        audience,
        rows,
        bodies,
        ledger,
        holder,
        elsewhere: known_away,
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
        // Nothing a computer of the partner's would refuse to take back.
        .filter(|row| holder.holds_row(row))
        .cloned()
        .collect();
    myriad_persona_log::row::sort(&mut kept);
    let mut away = elsewhere(
        kept.iter()
            .filter_map(|row| bodies.get(&row.id).map(|body| (row, body))),
    );
    away.extend(known_away);
    let engine = base64::engine::general_purpose::STANDARD;
    let mut payloads = Vec::new();
    let mut withheld = Vec::new();
    for row in &kept {
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
        let Some(payload) = &row.payload else {
            continue;
        };
        let carried = match bodies.get(&row.id) {
            None => Err(Why::NoBody),
            Some(_) if away.contains(&row.object) => Err(Why::PersonElsewhere),
            Some(body) => carry_home(row, body).map(|(bytes, _)| bytes),
        };
        match carried {
            Ok(bytes) => payloads.push(Carried {
                reference: payload.reference.clone(),
                data: engine.encode(bytes),
            }),
            Err(why) => withheld.push(Withheld {
                id: row.id.clone(),
                why,
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

/// A home page taken onto this side: every row must be a mark of the page,
/// or one the page's marks (or this side's ledger) keep here; each body is
/// checked as one taken from the wire, and must be its canonical bytes.
pub fn take(input: TakeInput) -> Result<Taken, NodeError> {
    let TakeInput {
        page,
        holder,
        ledger,
        held,
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
    let said: BTreeMap<&str, Why> = page
        .withheld
        .iter()
        .map(|one| (one.id.as_str(), one.why))
        .collect();
    for row in &page.rows {
        if row.audience != page.audience {
            return Err(NodeError::new("a row is not of the page's audience"));
        }
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
        // As from the wire: nothing about the partner under an id this side
        // does not go by (`Holder::holds_row`).
        if !holder.holds_row(row) {
            return Err(NodeError::new(
                "a row of a home page is one this side may not hold",
            ));
        }
    }
    let ledger = forget::joined(&ledger, &forget::ledger(&page.rows));
    let view = forget::View::new(&ledger);
    // A row forgotten here since is still one the marks keep: it comes in
    // as a stub.
    let stray = page
        .rows
        .iter()
        .any(|row| !(mark(row) && view.home_of(&row.audience, &row.object)) && !view.keeps(row));
    if stray {
        return Err(NodeError::new(
            "a row of a home page is of no node kept here",
        ));
    }
    let stubs: BTreeSet<&str> = held
        .iter()
        .filter(|row| row.forgotten && row.audience == page.audience)
        .map(|row| row.id.as_str())
        .collect();
    let whole: BTreeSet<String> = held
        .iter()
        .filter(|row| !row.forgotten && row.audience == page.audience)
        .map(|row| row.id.clone())
        .collect();
    let (mut rows, _) = forget::absorb(page.rows.clone(), &ledger, Side::Here);
    myriad_persona_log::row::sort(&mut rows);
    let engine = base64::engine::general_purpose::STANDARD;
    let decoded: Vec<(&Row, Decoded)> = rows
        .iter()
        .filter_map(|row| {
            let payload = row.payload.as_ref().filter(|_| !row.forgotten)?;
            let read = match carried.get(payload.reference.as_str()) {
                None => Err(said.get(row.id.as_str()).copied().unwrap_or(Why::NoBody)),
                Some(data) => engine
                    .decode(data)
                    .map_err(|_| Why::BadBody)
                    .and_then(|bytes| {
                        let body = serde_json::from_slice(&bytes).map_err(|_| Why::BadBody)?;
                        Ok((bytes, body))
                    }),
            };
            Some((row, read))
        })
        .collect();
    // A person's or a group's node in her own audience, by the created
    // bodies in the page: no body of it comes in.
    let away = elsewhere(
        decoded
            .iter()
            .filter_map(|(row, read)| read.as_ref().ok().map(|(_, body)| (*row, body))),
    );
    let mut bodies = BTreeMap::new();
    let mut withheld = Vec::new();
    for (row, read) in decoded {
        let read = read.and_then(|(bytes, body)| {
            if away.contains(&row.object) {
                return Err(Why::PersonElsewhere);
            }
            let (canonical, value) = carry_home(row, &body)?;
            if canonical == bytes {
                Ok(value)
            } else {
                Err(Why::BadBody)
            }
        });
        match read {
            Ok(body) => {
                bodies.insert(row.id.clone(), body);
            }
            Err(why) => withheld.push(Withheld {
                id: row.id.clone(),
                why,
            }),
        }
    }
    // A row held here whole that the page brings without its body stays as
    // it is held: a copy without the body would replace it (two homes, or a
    // home taking its own file back with a body withheld from it).
    let kept_whole = |row: &Row| {
        whole.contains(&row.id)
            && !row.forgotten
            && row.payload.is_some()
            && !bodies.contains_key(&row.id)
    };
    let held_whole: BTreeSet<String> = rows
        .iter()
        .filter(|row| kept_whole(row))
        .map(|row| row.id.clone())
        .collect();
    rows.retain(|row| !held_whole.contains(&row.id));
    withheld.retain(|one| !held_whole.contains(&one.id));
    let filled: Vec<String> = rows
        .iter()
        .filter(|row| stubs.contains(row.id.as_str()) && bodies.contains_key(&row.id))
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(Taken {
        rows,
        bodies,
        filled,
        withheld,
        ledger,
    })
}
