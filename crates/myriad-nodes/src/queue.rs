//! Her upkeep queue: which nodes are due for upkeep, and in what order.
//!
//! A node is due only when something it watches changed since her upkeep
//! last looked at it (its `node.upkept` rows): never by the calendar, never
//! by a count. What a node watches:
//!
//! - **own**: its own rows;
//! - **relation**: a relation another node made to it, or took back;
//! - **part**: the rows of its parts (属于: one end changed, the other is
//!   upkept);
//! - **whole**: the rows of what it is part of;
//! - **blocker**: the rows of what blocks it (挡着: the blocker changed, so
//!   the blocked one looks again: the wake its structure brings);
//! - **mutual_block**: it and another block each other, and that is new to
//!   it (a hand to raise);
//! - **clash**: a clash with it is new to it (撞上: a hand to raise).
//!
//! Its own rows are also unseen while a head item is 两边都改过 and no one
//! run of hers saw every current version of it (why `own`). Two of her ran
//! at once only when the partner forced the pen while the old holder kept
//! going, or a side was used alone; each side's upkeep then marks its own
//! change seen, and the marks together would cover both. She settles it by
//! writing a version that names both, as the fold asks, or one run sees
//! both and leaves it.
//!
//! A related node (相关) never makes it due. Recall use (`node.used`) and
//! upkeep marks (`node.upkept`) are not changes; neither is a row the fold
//! passed by. A forgotten node, one that went into another, and one whose
//! created row has not arrived are never due.
//!
//! Her upkeep saw a row when its device's clock in `seen` is at least the
//! row's (or, for a device past the ones `seen` holds, its wall time is not
//! after `floor`), or the run wrote it (`wrote`): what she wrote does not
//! make the node due again, and what anything else wrote meanwhile, even on
//! the same device, does. A device's rows arrive in its order, so one clock
//! per device is enough.
//!
//! Order: the node whose oldest unseen change is oldest goes first (the
//! backlog is worked from old to new, so nothing waits forever); ties by
//! that change's clock (wall, counter, device), then audience, then id.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::{Row, kinds::Rule};
use myriad_persona_sync::Rev;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{READ, Relation},
    fold::{FoldInput, Full, fold_full},
    limits::QUEUE_MAX,
};

/// Why a node is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Own,
    Relation,
    Part,
    Whole,
    Blocker,
    MutualBlock,
    Clash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Due {
    pub audience: String,
    pub node: String,
    /// Unix microseconds: its oldest unseen change.
    pub since: i64,
    pub why: Vec<Reason>,
    #[serde(skip)]
    pub(crate) first: Option<Rev>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueueInput {
    pub rows: Vec<Row>,
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    /// How many to hand out, 1 to [`QUEUE_MAX`].
    pub max: usize,
    /// This side's origins, as for the fold: needed once a retract is in
    /// the rows.
    #[serde(default)]
    pub here: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub due: Vec<Due>,
    /// How many more are due after these.
    #[serde(skip_serializing_if = "is_zero")]
    pub more: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

pub fn queue(input: QueueInput) -> Result<Queue, NodeError> {
    if input.max == 0 || input.max > QUEUE_MAX {
        return Err(NodeError::new(format!("max is 1 to {QUEUE_MAX}")));
    }
    let full = fold_full(FoldInput {
        rows: input.rows,
        bodies: input.bodies,
        here: input.here,
    })?;
    let mut due: Vec<Due> = Watch::new(&full).all();
    let more = due.len().saturating_sub(input.max);
    due.truncate(input.max);
    Ok(Queue { due, more })
}

/// What every node watches, from one fold.
pub(crate) struct Watch<'a> {
    full: &'a Full,
    /// (audience, node) → its rows that are changes.
    changes: BTreeMap<(String, String), Vec<&'a Row>>,
    by_id: BTreeMap<&'a str, &'a Row>,
}

impl<'a> Watch<'a> {
    pub(crate) fn new(full: &'a Full) -> Self {
        let skipped: BTreeSet<&str> = full
            .folded
            .skipped
            .iter()
            .map(|skipped| skipped.id.as_str())
            .collect();
        let mut changes: BTreeMap<(String, String), Vec<&Row>> = BTreeMap::new();
        let mut by_id = BTreeMap::new();
        for row in &full.rows {
            by_id.insert(row.id.as_str(), row);
            let change = row.rule() != Rule::Forget
                && !row.forgotten
                && READ.contains(&row.kind.as_str())
                && !matches!(row.kind.as_str(), "node.used" | "node.upkept")
                && !skipped.contains(row.id.as_str());
            if change {
                let id = row.object.strip_prefix("node:").unwrap_or(&row.object);
                changes
                    .entry((row.audience.clone(), id.to_string()))
                    .or_default()
                    .push(row);
            }
        }
        Self {
            full,
            changes,
            by_id,
        }
    }

    fn rows_of(&self, audience: &str, id: &str) -> &[&'a Row] {
        self.changes
            .get(&(audience.to_string(), id.to_string()))
            .map_or(&[], Vec::as_slice)
    }

    /// Every node due, in queue order.
    pub(crate) fn all(&self) -> Vec<Due> {
        let mut due: Vec<Due> = self
            .full
            .folded
            .nodes
            .iter()
            .filter_map(|node| self.due(&node.audience, &node.id))
            .collect();
        due.sort_by(|a, b| {
            (a.since, &a.first, &a.audience, &a.node).cmp(&(
                b.since,
                &b.first,
                &b.audience,
                &b.node,
            ))
        });
        due
    }

    /// The watched rows of one node, each with why it watches it.
    fn watched(&self, audience: &str, id: &str) -> Vec<(Reason, &'a Row)> {
        let mut out: Vec<(Reason, &Row)> = self
            .rows_of(audience, id)
            .iter()
            .map(|row| (Reason::Own, *row))
            .collect();
        for (row, (to_audience, to)) in &self.full.points_at {
            if to_audience == audience
                && to == id
                && let Some(row) = self.by_id.get(row.as_str())
                && row.object.strip_prefix("node:") != Some(id)
                && !row.forgotten
            {
                out.push((Reason::Relation, *row));
            }
        }
        for relation in &self.full.folded.relations {
            if relation.audience != audience {
                continue;
            }
            let (reason, other) = match relation.relation {
                Relation::PartOf if relation.to == id => (Reason::Part, &relation.from),
                Relation::PartOf if relation.from == id => (Reason::Whole, &relation.to),
                Relation::Blocks if relation.to == id => (Reason::Blocker, &relation.from),
                _ => continue,
            };
            out.extend(
                self.rows_of(audience, other)
                    .iter()
                    .map(|row| (reason, *row)),
            );
        }
        out
    }

    /// The newest clock per device of what one node watches, and of the
    /// other nodes she was shown with it (`shown`): what her upkeep of it
    /// has seen once it looks. A node she was shown and then made it watch
    /// (a blocker, a whole) has nothing new for it.
    pub(crate) fn newest(&self, audience: &str, id: &str, shown: &[&str]) -> BTreeMap<String, Rev> {
        let mut newest: BTreeMap<String, Rev> = BTreeMap::new();
        let watched = self.watched(audience, id).into_iter().map(|(_, row)| row);
        let shown = shown
            .iter()
            .flat_map(|other| self.rows_of(audience, other).iter().copied());
        for row in watched.chain(shown) {
            let known = newest
                .entry(row.origin.clone())
                .or_insert_with(|| row.hlc.clone());
            if row.hlc > *known {
                *known = row.hlc.clone();
            }
        }
        newest
    }

    /// Whether one node is due, and why.
    pub(crate) fn due(&self, audience: &str, id: &str) -> Option<Due> {
        let node = self
            .full
            .folded
            .nodes
            .iter()
            .find(|node| node.audience == audience && node.id == id)?;
        if node.forgotten
            || node.key.is_none()
            || self
                .full
                .away
                .contains(&(audience.to_string(), id.to_string()))
        {
            return None;
        }
        let seen = self.full.seen.get(&(audience.to_string(), id.to_string()));
        let unseen = |row: &Row| !seen.is_some_and(|seen| seen.saw(row));
        let mut why: BTreeSet<Reason> = BTreeSet::new();
        let mut first: Option<&Rev> = None;
        let watched = self.watched(audience, id);
        for (reason, row) in &watched {
            if unseen(row) {
                why.insert(*reason);
                if first.is_none_or(|known| row.hlc < *known) {
                    first = Some(&row.hlc);
                }
            }
        }
        // Hands: a mutual block or a clash that is new to it.
        let rows_unseen = |rows: &[String]| {
            rows.iter()
                .filter_map(|row| self.by_id.get(row.as_str()))
                .any(|row| unseen(row))
        };
        let relations: Vec<_> = self
            .full
            .folded
            .relations
            .iter()
            .filter(|relation| relation.audience == audience)
            .collect();
        for relation in &relations {
            match relation.relation {
                Relation::Clashes
                    if (relation.from == id || relation.to == id)
                        && rows_unseen(&relation.rows) =>
                {
                    why.insert(Reason::Clash);
                }
                Relation::Blocks if relation.from == id => {
                    let back = relations.iter().find(|other| {
                        other.relation == Relation::Blocks
                            && other.from == relation.to
                            && other.to == id
                    });
                    if let Some(back) = back
                        && (rows_unseen(&relation.rows) || rows_unseen(&back.rows))
                    {
                        why.insert(Reason::MutualBlock);
                    }
                }
                _ => {}
            }
        }
        for base in node.both_changed() {
            let rows: Vec<&Row> = base
                .iter()
                .filter_map(|row| self.by_id.get(row.as_str()).copied())
                .collect();
            let weighed = seen.is_some_and(|seen| {
                seen.each
                    .iter()
                    .any(|mark| rows.iter().all(|row| mark.saw(row)))
            });
            if weighed {
                continue;
            }
            why.insert(Reason::Own);
            // It became two when the later of them was written.
            if let Some(newest) = rows.iter().map(|row| &row.hlc).max()
                && first.is_none_or(|known| newest < known)
            {
                first = Some(newest);
            }
        }
        let first = first?.clone();
        Some(Due {
            audience: audience.to_string(),
            node: id.to_string(),
            since: i64::try_from(first.wall_ms).unwrap_or(i64::MAX / 1000) * 1000,
            why: why.into_iter().collect(),
            first: Some(first),
        })
    }
}
