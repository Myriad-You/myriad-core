//! Which of her intents rest on what is being forgotten
//! (`nodes.resting_on`): both hosts forget the same rows, each by a forget
//! of its own.
//!
//! An intent names its grounds in `because`. A version rests on what goes
//! when it names it, or when its `text` is word for word that of one that
//! does (an intent let go of, or kept on other grounds, carries the words
//! on): a version of the same intent, wherever it is, or of the one it was
//! carried on from as its node went into another ([`carried_id`]). One
//! whose words were changed and whose grounds were too does not. A version
//! forgotten already whose body is still given says which words those were,
//! and whose the intent is, and nothing else: so a host asks before it
//! deletes the bodies.
//!
//! Which versions of an intent stand is the fold's ([`crate::fold`]), on
//! its rows alone: whether its node folds here makes no difference.
//!
//! - One that stands (a rival too) rests on it: the intent goes whole,
//!   every version of it here. One left behind would stand again, a hand
//!   she let go of among them.
//! - Only versions written over rest on it: those go, and what stands stays.
//!   A version left out no longer says which it was written on, so one older
//!   still would stand again beside it: that goes too, until none does.
//!
//! A row whose body is not here (withheld, not arrived) says no intent and
//! no grounds. One a going version was written on goes with it (and when
//! only older versions go, unless one that stays was written on it too),
//! for a side that has its body would have it stand again. Any other is not
//! seen: the host asks again when a body comes, and with every intent row
//! that comes after the forgetting. A row the fold does not place (a body
//! it cannot check) goes on its own.
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
    upkeep::{Carried, carried_id},
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

/// One intent on one node: its audience, its object and its id.
type Place<'a> = (&'a str, &'a str, &'a str);

/// One intent's rows here.
#[derive(Default)]
struct Intent<'a> {
    /// Those not forgotten, in log order, each with its body.
    written: Vec<(&'a Row, &'a Value)>,
    /// The bodies still given of those forgotten: only for whose the intent
    /// is and what words it said.
    forgotten: Vec<&'a Value>,
    /// Every version is the partner's.
    theirs: bool,
    /// The words of those resting on what goes.
    words: BTreeSet<&'a str>,
}

impl<'a> Intent<'a> {
    /// Every body given, of those forgotten too.
    fn bodies(&self) -> impl Iterator<Item = &'a Value> {
        let written = self.written.iter().map(|(_, body)| *body);
        written.chain(self.forgotten.iter().copied())
    }
}

fn text_of(body: &Value) -> Option<&str> {
    body.get("text").and_then(Value::as_str)
}

fn base_of(body: &Value) -> impl Iterator<Item = &str> {
    body.get("base")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// The words of one intent go wherever it is again in its audience: on
/// another node under its own id (a Mac moves it so), or under what it goes
/// on as when its node goes into another. Never into what is the partner's
/// from what is not.
fn spread<'a>(intents: &mut BTreeMap<Place<'a>, Intent<'a>>) {
    let mut from: Vec<Place> = intents
        .iter()
        .filter(|(_, intent)| !intent.words.is_empty())
        .map(|(place, _)| *place)
        .collect();
    while let Some(place) = from.pop() {
        let (audience, object, id) = place;
        let node = object.strip_prefix("node:").unwrap_or(object);
        let carried = carried_id(Carried::Intent, node, id);
        let (words, theirs) = {
            let intent = &intents[&place];
            (intent.words.clone(), intent.theirs)
        };
        for (to, intent) in intents.iter_mut() {
            let again = to.0 == audience && to.1 != object && (to.2 == id || to.2 == carried);
            if !again || (intent.theirs && !theirs) {
                continue;
            }
            let had = intent.words.len();
            let said: BTreeSet<&str> = intent.bodies().filter_map(text_of).collect();
            intent.words.extend(words.intersection(&said));
            if intent.words.len() > had {
                from.push(*to);
            }
        }
    }
}

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
    let mut out: BTreeSet<&str> = BTreeSet::new();
    let mut intents: BTreeMap<Place, Intent> = BTreeMap::new();
    // The rows whose body is not here, and what each is about.
    let mut bare: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for row in full.rows.iter().filter(|row| row.kind == INTENT) {
        let Some(body) = bodies.get(&row.id) else {
            if !row.forgotten {
                bare.insert(&row.id, (&row.audience, &row.object));
            }
            continue;
        };
        match body.get("intent").and_then(Value::as_str) {
            Some(intent) => {
                let intent = intents
                    .entry((&row.audience, &row.object, intent))
                    .or_default();
                if row.forgotten {
                    intent.forgotten.push(body);
                } else {
                    intent.written.push((row, body));
                }
            }
            None if !row.forgotten && rests(body, &gone, the_partners(body)) => {
                out.insert(&row.id);
            }
            None => {}
        }
    }
    for intent in intents.values_mut() {
        let theirs = intent.bodies().all(the_partners);
        let words = intent
            .bodies()
            .filter(|body| rests(body, &gone, theirs))
            .filter_map(text_of)
            .collect();
        intent.theirs = theirs;
        intent.words = words;
    }
    spread(&mut intents);
    for (place, intent) in &intents {
        let rows: Vec<&Row> = intent.written.iter().map(|(row, _)| *row).collect();
        let mut going: BTreeSet<&str> = intent
            .written
            .iter()
            .filter(|(_, body)| {
                rests(body, &gone, intent.theirs)
                    || text_of(body).is_some_and(|text| intent.words.contains(text))
            })
            .map(|(row, _)| row.id.as_str())
            .collect();
        if going.is_empty() {
            continue;
        }
        let stands = standing(&rows, &bodies)?;
        if going.iter().any(|id| stands.contains(*id)) {
            going.extend(rows.iter().map(|row| row.id.as_str()));
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
                .filter(|id| again.contains(*id) && !stands.contains(*id))
                .collect();
            if risen.is_empty() {
                break;
            }
            going.extend(risen);
        }
        let (went, stay): (Vec<_>, Vec<_>) = intent
            .written
            .iter()
            .partition(|(row, _)| going.contains(row.id.as_str()));
        let held: BTreeSet<&str> = stay.iter().flat_map(|(_, body)| base_of(body)).collect();
        out.extend(
            went.iter()
                .flat_map(|(_, body)| base_of(body))
                .filter(|id| bare.get(id) == Some(&(place.0, place.1)) && !held.contains(id)),
        );
        out.extend(going);
    }
    Ok(RestingOn {
        intents: out.into_iter().map(str::to_string).collect(),
    })
}

#[cfg(test)]
#[path = "resting_on_tests.rs"]
mod tests;
