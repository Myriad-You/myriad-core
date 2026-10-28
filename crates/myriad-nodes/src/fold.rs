//! Folding node rows into nodes, by the same rules on every side.
//!
//! The rows (any order, repeated, with stubs and the forgets that reach
//! them) are checked, deduplicated by id and put in log order, as
//! `log.fold` does; then each node is folded from its rows and their bodies:
//!
//! - its owner, key and form from `node.created`, its title from the last
//!   created row that names one (a together node may have none);
//! - a reference to each event she decided about, first one first, the 200
//!   newest kept;
//! - a moment for each line she raised or noted: the last row about it says
//!   what it is, `node.opened` and `node.said` say it was said, and the 64
//!   most recently touched are kept (an opened one moves up, as on the Mac);
//! - when it was made (its first created row) and last changed (its last
//!   row, stubs too), from the rows' clocks. A forget is not a row of the
//!   node: forgetting a piece of it does not move when it last changed, so
//!   the node reads the same before and after its rows become stubs.
//!
//! Forgetting absorbs, in any order:
//!
//! - a `forget` of the node (or of its subject in its audience) folds it to
//!   nothing but the mark that it was, and its rows become stubs;
//! - a `forget` of one piece of its evidence, an `event:<id>`, takes the
//!   reference, the moment and every row naming that event, which become
//!   stubs; the rest of the node stays. A stub alone does not forget a node.
//!
//! `stubbed` lists the rows that had a payload and are stubs now: the
//! caller deletes their bodies.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::{Row, forget, kinds::Rule, row::object_type};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{Body, Form},
    id::node_id,
};

/// The most moments and references a node keeps (as on the Mac).
pub const MOMENT_LIMIT: usize = 64;
pub const REF_LIMIT: usize = 200;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FoldInput {
    /// Node rows and the forgets that reach them; other rows are passed by.
    pub rows: Vec<Row>,
    /// Each row's body, by row id: what its payload resolves to. A stub has
    /// none.
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
}

/// One line she had or a note she kept, as the node keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Moment {
    pub event: String,
    #[serde(default)]
    pub happened: String,
    pub action: String,
    pub line: String,
    /// Said (or noted, which counts as said); false while it waits or once
    /// it went unsaid.
    pub said: bool,
    /// The clock of the row that raised or noted it, Unix microseconds.
    pub at: i64,
}

/// Where something came from: `event:<id>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ref {
    pub id: String,
    pub at: i64,
}

/// One node as its rows stand now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub audience: String,
    /// Forgotten: nothing else about it reads back.
    #[serde(default, skip_serializing_if = "is_false")]
    pub forgotten: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Absent until a created row is folded (it may not have arrived yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<Form>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moments: Vec<Moment>,
    /// Oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<Ref>,
    /// Unix microseconds, from the rows' clocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Node {
    fn empty(id: &str, audience: &str) -> Self {
        Self {
            id: id.to_string(),
            audience: audience.to_string(),
            forgotten: false,
            owner: None,
            key: None,
            form: None,
            title: String::new(),
            moments: Vec::new(),
            refs: Vec::new(),
            created_at: None,
            updated_at: None,
        }
    }

    /// Whether it already holds `event` (decided, raised or noted).
    pub fn holds(&self, event: &str) -> bool {
        let reference = format!("event:{event}");
        self.refs.iter().any(|known| known.id == reference)
            || self.moments.iter().any(|moment| moment.event == event)
    }
}

/// Why a row was passed by. A row's place still counts as a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// Not a stub, and no body was given for it.
    NoBody,
    /// Its body does not fit its kind.
    BadBody,
    /// A created row whose owner, audience and key make another node's id.
    OtherNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skipped {
    pub id: String,
    pub why: Why,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folded {
    /// By audience, then id.
    pub nodes: Vec<Node>,
    /// Rows that had a payload and are stubs now, sorted.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stubbed: Vec<String>,
    /// Node by node, each in log order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<Skipped>,
}

/// A row's clock as Unix microseconds.
fn micros(row: &Row) -> i64 {
    // The clock's wall is at most 10^15 ms, well inside i64 microseconds.
    i64::try_from(row.hlc.wall_ms).unwrap_or(i64::MAX / 1000) * 1000
}

/// A moment while folding, with the log position that last touched it.
struct Held {
    moment: Moment,
    touched: usize,
}

pub fn fold(input: FoldInput) -> Result<Folded, NodeError> {
    for row in &input.rows {
        row.check()
            .map_err(|error| NodeError::new(format!("a row: {error}")))?;
    }
    let ids: BTreeSet<&str> = input.rows.iter().map(|row| row.id.as_str()).collect();
    if input.bodies.keys().any(|id| !ids.contains(id.as_str())) {
        return Err(NodeError::new("a body is for no row given"));
    }
    let rows: Vec<Row> = input
        .rows
        .into_iter()
        .filter(|row| row.rule() == Rule::Forget || object_type(&row.object) == Some("node"))
        .collect();
    let rows = myriad_persona_log::fold::dedupe(rows)
        .map_err(|_| NodeError::new("two different rows share an id"))?;
    let ledger = forget::ledger(&rows);
    // Whole nodes (or subjects) forgotten, before their rows turn to stubs.
    let gone: BTreeSet<(String, String)> = rows
        .iter()
        .filter(|row| row.rule() != Rule::Forget && forget::forgets(&ledger, row))
        .map(|row| (row.audience.clone(), row.object.clone()))
        .collect();
    let (rows, mut stubbed) = forget::absorb(rows, &ledger);
    // One piece of evidence forgotten: its event, by audience.
    let evidence: BTreeSet<(&str, &str)> = ledger
        .iter()
        .filter_map(|entry| {
            let event = entry.target.strip_prefix("event:")?;
            Some((entry.audience.as_str(), event))
        })
        .collect();

    let mut by_node: BTreeMap<(&str, &str), Vec<(usize, &Row)>> = BTreeMap::new();
    for (at, row) in rows.iter().enumerate() {
        if row.rule() == Rule::Forget {
            continue;
        }
        by_node
            .entry((row.audience.as_str(), row.object.as_str()))
            .or_default()
            .push((at, row));
    }

    let mut skipped = Vec::new();
    let mut nodes = Vec::new();
    for ((audience, object), rows) in by_node {
        let id = object.strip_prefix("node:").unwrap_or(object);
        if gone.contains(&(audience.to_string(), object.to_string())) {
            nodes.push(Node {
                forgotten: true,
                ..Node::empty(id, audience)
            });
            continue;
        }
        let mut node = Node::empty(id, audience);
        let mut moments: BTreeMap<String, Held> = BTreeMap::new();
        let mut refs: Vec<Ref> = Vec::new();
        let mut created = None::<i64>;
        let mut first = None::<i64>;
        let mut last = None::<i64>;
        for (position, row) in rows {
            let time = micros(row);
            first = Some(first.map_or(time, |known| known.min(time)));
            last = Some(last.map_or(time, |known| known.max(time)));
            if row.forgotten {
                continue;
            }
            let Some(raw) = input.bodies.get(&row.id) else {
                if crate::body::READ.contains(&row.kind.as_str()) {
                    skipped.push(Skipped {
                        id: row.id.clone(),
                        why: Why::NoBody,
                    });
                }
                continue;
            };
            let Ok(body) = Body::read(&row.kind, raw) else {
                skipped.push(Skipped {
                    id: row.id.clone(),
                    why: Why::BadBody,
                });
                continue;
            };
            if body
                .event()
                .is_some_and(|event| evidence.contains(&(audience, event)))
            {
                stubbed.push(row.id.clone());
                continue;
            }
            match body {
                Body::Created(body) => {
                    if node_id(&body.owner, audience, &body.key).ok().as_deref() != Some(id) {
                        skipped.push(Skipped {
                            id: row.id.clone(),
                            why: Why::OtherNode,
                        });
                        continue;
                    }
                    created = Some(created.map_or(time, |known| known.min(time)));
                    node.owner = Some(body.owner);
                    node.key = Some(body.key);
                    node.form = Some(body.form);
                    if !body.title.is_empty() {
                        node.title = body.title;
                    }
                }
                Body::Decided(body) => {
                    let reference = format!("event:{}", body.event);
                    if !refs.iter().any(|known| known.id == reference) {
                        refs.push(Ref {
                            id: reference,
                            at: time,
                        });
                    }
                }
                Body::Raised(body) => {
                    let moment = Moment {
                        event: body.event.clone(),
                        happened: body.happened,
                        action: body.action,
                        line: body.line,
                        said: false,
                        at: time,
                    };
                    moments.insert(
                        body.event,
                        Held {
                            moment,
                            touched: position,
                        },
                    );
                }
                Body::Noted(body) => {
                    let moment = Moment {
                        event: body.event.clone(),
                        happened: body.happened,
                        action: "remember".to_string(),
                        line: body.line,
                        said: true,
                        at: time,
                    };
                    moments.insert(
                        body.event,
                        Held {
                            moment,
                            touched: position,
                        },
                    );
                }
                Body::Opened(body) => {
                    if let Some(held) = moments.get_mut(&body.event) {
                        held.moment.said = true;
                        held.touched = position;
                    }
                }
                Body::Said(body) => {
                    // A note counts as said already.
                    if let Some(held) = moments
                        .get_mut(&body.event)
                        .filter(|held| held.moment.action != "remember")
                    {
                        held.moment.said = true;
                        if let Some(line) = body.line {
                            held.moment.line = line;
                        }
                        held.touched = position;
                    }
                }
                Body::Notified(_)
                | Body::Shown(_)
                | Body::Faded(_)
                | Body::Observed(_)
                | Body::Later(_) => {}
            }
        }
        let mut held: Vec<Held> = moments.into_values().collect();
        held.sort_by_key(|held| held.touched);
        let over = held.len().saturating_sub(MOMENT_LIMIT);
        node.moments = held
            .into_iter()
            .skip(over)
            .map(|held| held.moment)
            .collect();
        let over = refs.len().saturating_sub(REF_LIMIT);
        node.refs = refs.split_off(over);
        node.created_at = created.or(first);
        node.updated_at = last;
        nodes.push(node);
    }
    stubbed.sort();
    stubbed.dedup();
    Ok(Folded {
        nodes,
        stubbed,
        skipped,
    })
}
