//! Which of her intents rest on what is being forgotten
//! (`nodes.resting_on`): both hosts forget the same rows, each by a forget
//! of its own.
//!
//! An intent names its grounds in `because`. Which versions of it stand is
//! the fold's ([`crate::fold`]), on the rows, bodies and ledger given:
//!
//! - One that stands (a rival too) names what goes: the intent goes whole,
//!   every version of it here. One left behind would stand again, a hand
//!   she let go of among them.
//! - Only versions written over name it: those go, and what stands stays. A
//!   version left out no longer says which it was written on, so one older
//!   still would stand again beside it: that goes too, until none does.
//!
//! A row the fold does not place (a body it cannot check) goes on its own.
//!
//! The names are:
//!
//! - `message:<id>`, `memory:<id>`: an original or a memory forgotten.
//! - `span:<name>`, `entry:<name>`: a span or an entry forgotten, wherever
//!   the fold shows it now. It is named by its row's id; by the slot the
//!   Mac that wrote it keeps it in (the row id is `<slot>@<writer>`, or
//!   `<slot>@<writer>~<n>` for a later version), which an intent still
//!   names when no row stood in the slot as it was written; or, held from
//!   elsewhere, by that row's id there too. All three are read, on either
//!   side of the comparison. A row id over the Mac's length is a digest of
//!   the whole, from which no slot is read: the Mac gives the slot too.
//! - `loose:<id>`: a loose end that holds words forgotten.
//!
//! An intent is the partner's when every version of it here is theirs
//! (`origin` partner, or `saidBy` the partner alone: a row may carry no
//! origin). It rests on their own messages only, so no span, entry or loose
//! end it names takes it; a message or a memory does, as before. A hand of
//! hers the partner let go of is still hers.
//!
//! What is only replaced by a forgetting's mark (`forgotten:<mark>`) is not
//! named, and a span only moved to another node is not forgotten: the
//! caller lists neither. Grounds are read from a body as it is, not as
//! [`crate::Body`] checks it.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::{Row, forget};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{ENTRY_REF, LOOSE_REF, MEMORY_REF, SPAN_REF},
    check_token,
    fold::{FoldInput, fold, fold_full},
};

const MESSAGE_REF: &str = "message:";
const INTENT: &str = "node.intent";

/// What the fold reads ([`FoldInput`]), and what is being forgotten: each
/// by its id or by the name grounds use (`message:…`, `span:…`), all of one
/// forgetting. The forgets of the spans and entries may be among the rows
/// already or not.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestingOnInput {
    pub rows: Vec<Row>,
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    pub ledger: Vec<forget::Entry>,
    /// The `node.span` and `node.entry` rows (stubs too), or their slots.
    #[serde(default)]
    pub gone: Vec<String>,
    #[serde(default)]
    pub messages: Vec<String>,
    #[serde(default)]
    pub memories: Vec<String>,
    #[serde(default)]
    pub loose: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestingOn {
    /// The `node.intent` rows to forget, sorted.
    pub intents: Vec<String>,
}

/// The slot a row of a Mac's was written in: its id without its writer. An
/// event's own id may end in `@` and digits (it was raised again), which is
/// no writer.
fn slot(row: &str) -> Option<&str> {
    let (slot, written) = row.rsplit_once('@')?;
    let writer = match written.rsplit_once('~') {
        Some((writer, n)) if digits(n) => writer,
        _ => written,
    };
    (!slot.is_empty() && !writer.is_empty() && !digits(writer)).then_some(slot)
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn named(prefix: &str, id: &str) -> String {
    match id.strip_prefix(prefix) {
        Some(_) => id.to_string(),
        None => format!("{prefix}{id}"),
    }
}

struct Gone {
    /// Whole names: a message, a memory, a loose end.
    said: BTreeSet<String>,
    loose: BTreeSet<String>,
    /// `(prefix, row or slot)` as given.
    pieces: BTreeSet<(&'static str, String)>,
    /// The slots of those given by their row.
    slots: BTreeSet<(&'static str, String)>,
}

impl Gone {
    fn read(input: &RestingOnInput) -> Result<Self, NodeError> {
        for (what, ids) in [
            ("gone", &input.gone),
            ("messages", &input.messages),
            ("memories", &input.memories),
            ("loose", &input.loose),
        ] {
            for id in ids {
                check_token(what, id)?;
            }
        }
        let mut pieces = BTreeSet::new();
        let mut slots = BTreeSet::new();
        for id in &input.gone {
            let as_given = [SPAN_REF, ENTRY_REF]
                .into_iter()
                .find_map(|prefix| Some((prefix, id.strip_prefix(prefix)?)));
            // A row's id alone says no kind; no other row has it.
            let both = [(SPAN_REF, id.as_str()), (ENTRY_REF, id.as_str())];
            for (prefix, id) in as_given.as_ref().map_or(&both[..], std::slice::from_ref) {
                pieces.insert((*prefix, id.to_string()));
                if let Some(slot) = slot(id) {
                    slots.insert((*prefix, slot.to_string()));
                }
            }
        }
        let each = |prefix: &str, ids: &[String]| -> Vec<String> {
            ids.iter().map(|id| named(prefix, id)).collect()
        };
        Ok(Self {
            said: each(MESSAGE_REF, &input.messages)
                .into_iter()
                .chain(each(MEMORY_REF, &input.memories))
                .collect(),
            loose: each(LOOSE_REF, &input.loose).into_iter().collect(),
            pieces,
            slots,
        })
    }

    fn takes(&self, ground: &str, the_partners: bool) -> bool {
        if self.said.contains(ground) {
            return true;
        }
        if the_partners {
            return false;
        }
        if self.loose.contains(ground) {
            return true;
        }
        [SPAN_REF, ENTRY_REF].into_iter().any(|prefix| {
            let Some(name) = ground.strip_prefix(prefix) else {
                return false;
            };
            let holds = |set: &BTreeSet<(&'static str, String)>, name: &str| {
                set.contains(&(prefix, name.to_string()))
            };
            holds(&self.pieces, name)
                || holds(&self.slots, name)
                || slot(name).is_some_and(|slot| holds(&self.pieces, slot))
        })
    }
}

fn the_partners(body: &Value) -> bool {
    match body.get("origin") {
        Some(origin) if !origin.is_null() => origin == "partner",
        _ => body
            .get("saidBy")
            .and_then(Value::as_array)
            .is_some_and(|said| said.len() == 1 && said[0] == "partner"),
    }
}

/// Every row one intent was written in, on one node: those not forgotten,
/// in log order, each with its body.
type Versions<'a> = BTreeMap<(&'a str, &'a str, &'a str), Vec<(&'a Row, &'a Value)>>;

fn rests(body: &Value, gone: &Gone, the_partners: bool) -> bool {
    body.get("because")
        .and_then(Value::as_array)
        .is_some_and(|because| {
            because
                .iter()
                .filter_map(Value::as_str)
                .any(|ground| gone.takes(ground, the_partners))
        })
}

/// The rows an intent stands at, with only `rows` of it here. Forgets are
/// not given again: a row one reaches is a stub in `rows` already left out.
fn standing(
    rows: &[&Row],
    bodies: &BTreeMap<String, Value>,
) -> Result<BTreeSet<String>, NodeError> {
    let folded = fold(FoldInput {
        rows: rows.iter().map(|row| (*row).clone()).collect(),
        bodies: rows
            .iter()
            .filter_map(|row| Some((row.id.clone(), bodies.get(&row.id)?.clone())))
            .collect(),
        ledger: Vec::new(),
    })?;
    Ok(folded
        .nodes
        .iter()
        .flat_map(|node| node.intents.values())
        .flat_map(|intent| intent.base.iter().cloned())
        .collect())
}

pub fn resting_on(input: RestingOnInput) -> Result<RestingOn, NodeError> {
    let gone = Gone::read(&input)?;
    let bodies = input.bodies;
    let full = fold_full(FoldInput {
        rows: input.rows,
        bodies: bodies.clone(),
        ledger: input.ledger,
    })?;
    let stands: BTreeSet<&str> = full
        .folded
        .nodes
        .iter()
        .flat_map(|node| node.intents.values())
        .flat_map(|intent| intent.base.iter().map(String::as_str))
        .collect();
    let mut out: BTreeSet<&str> = BTreeSet::new();
    let mut versions: Versions = BTreeMap::new();
    for row in &full.rows {
        if row.kind != INTENT || row.forgotten {
            continue;
        }
        let Some(body) = bodies.get(&row.id) else {
            continue;
        };
        match body.get("intent").and_then(Value::as_str) {
            Some(intent) => versions
                .entry((&row.audience, &row.object, intent))
                .or_default()
                .push((row, body)),
            None if rests(body, &gone, the_partners(body)) => {
                out.insert(&row.id);
            }
            None => {}
        }
    }
    for written in versions.values() {
        let theirs = written.iter().all(|(_, body)| the_partners(body));
        let rows: Vec<&Row> = written.iter().map(|(row, _)| *row).collect();
        let mut going: BTreeSet<&str> = written
            .iter()
            .filter(|(_, body)| rests(body, &gone, theirs))
            .map(|(row, _)| row.id.as_str())
            .collect();
        if going.is_empty() {
            continue;
        }
        if going.iter().any(|id| stands.contains(id)) {
            out.extend(rows.iter().map(|row| row.id.as_str()));
            continue;
        }
        loop {
            let left: Vec<&Row> = rows
                .iter()
                .copied()
                .filter(|row| !going.contains(row.id.as_str()))
                .collect();
            let again = standing(&left, &bodies)?;
            let risen: Vec<&str> = left
                .iter()
                .map(|row| row.id.as_str())
                .filter(|id| again.contains(*id) && !stands.contains(id))
                .collect();
            if risen.is_empty() {
                break;
            }
            going.extend(risen);
        }
        out.extend(going);
    }
    Ok(RestingOn {
        intents: out.into_iter().map(str::to_string).collect(),
    })
}

#[cfg(test)]
#[path = "resting_on_tests.rs"]
mod tests;
