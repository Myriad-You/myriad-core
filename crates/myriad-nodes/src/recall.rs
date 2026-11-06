//! Picking nodes for recall: which of her nodes come to mind before she
//! answers, and which entries of each.
//!
//! The host ranks its entries (memories) for the words, as it always has
//! (most relevant first; each marked `named` when the words found it, not
//! when it only fills a try); retired ones (replaced, faded, forgotten) are
//! the host's to leave out before calling. [`pick`] turns that ranking
//! into nodes, by these rules:
//!
//! - **Picked nodes**: only a named entry picks its node (one that only
//!   fills was not about these words). Active and ongoing nodes first, then
//!   resting ones, then done or dropped ones; within each, by where their
//!   first named entry ranks. A node gone (forgotten, or gone into another:
//!   its entries are on that one now) is never picked; a done or dropped
//!   one only when named, which is the only way any node is picked, so it
//!   comes when they ask after it and after everything still going. Their
//!   own node (「关于你」) and hers (`her`) are never picked: the first has
//!   its own place below, the second's head is the host's to show every
//!   time. At most [`Budget::picked`].
//! - **Each picked node's entries**: its ranked entries first, in rank
//!   order, then its newest to fill, at most [`Budget::entries`].
//! - **About them**: the ranked entries of their own node, in rank order,
//!   at most [`Budget::about_them`]; never its newest to fill. What she
//!   makes of them comes every turn with its head (its summary, views and
//!   open items, the host's to show); filling with whatever they said last
//!   only brought what these words were not about.
//! - **Loose**: the ranked entries shown in none of those, in rank order:
//!   on no node, on a node not picked, past a node's room. The host shows
//!   them as it showed memories before, so nothing it ranked is lost.
//! - **Thorough** (they ask for all of a kind: "which ones", "how many"):
//!   the nodes of the named entries, in the order their first named entry
//!   ranks (the same nodes as above may come again), each in turn giving
//!   its entries newest first that are not shown yet, until
//!   [`Budget::together`] are added in all; each node gone through says how
//!   many of its entries are shown and of how many, so she knows when not
//!   all of it is there.
//!
//! The defaults are the native app's (`NodeRecall.Budget.chat`: three
//! nodes, three entries each, five about them) and the site's (24 more
//! when going through all of a kind).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    NodeError,
    body::{MEMORY_REF, Status},
    check_token,
    fold::Node,
};

/// How much recall brings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Budget {
    /// Nodes picked.
    pub picked: usize,
    /// Entries of each picked node.
    pub entries: usize,
    /// Ranked entries of their own node.
    pub about_them: usize,
    /// Entries added in all when going through all of a kind.
    pub together: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            picked: 3,
            entries: 3,
            about_them: 5,
            together: 24,
        }
    }
}

/// One ranked entry: its id as the host knows it (the memory row's), and
/// whether the words found it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ranked {
    pub id: String,
    #[serde(default)]
    pub named: bool,
}

/// One entry of a node: its id (the memory row's, else the entry row's)
/// and when it was written (Unix microseconds).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickEntry {
    pub id: String,
    pub at: i64,
}

/// A node of the audience as recall sees it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickNode {
    pub id: String,
    /// Absent reads as active.
    #[serde(default)]
    pub status: Option<Status>,
    /// Forgotten, or gone into another node.
    #[serde(default)]
    pub gone: bool,
    #[serde(default)]
    pub entries: Vec<PickEntry>,
}

impl PickNode {
    /// A folded node as recall sees it: each entry by the memory row it is
    /// the place of (`memory:<id>`), else by its own row.
    pub fn of(node: &Node) -> Self {
        Self {
            id: node.id.clone(),
            status: node.status.as_ref().map(|status| status.current.value),
            gone: node.forgotten || node.absorbed_into.is_some(),
            entries: node
                .entries
                .iter()
                .map(|entry| PickEntry {
                    id: entry
                        .refs
                        .iter()
                        .find_map(|reference| reference.strip_prefix(MEMORY_REF))
                        .unwrap_or(&entry.row)
                        .to_string(),
                    at: entry.at,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickInput {
    /// Most relevant first; a repeat is passed by.
    pub ranked: Vec<Ranked>,
    /// The nodes of the audience.
    pub nodes: Vec<PickNode>,
    /// The id of their own node (「关于你」, `person:<id>`), if any.
    #[serde(default)]
    pub about_them: Option<String>,
    /// The id of hers (`her`) in this audience, if any.
    #[serde(default)]
    pub her: Option<String>,
    /// They ask for all of a kind.
    #[serde(default)]
    pub thorough: bool,
    #[serde(default)]
    pub budget: Budget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Picked {
    pub node: String,
    pub entries: Vec<String>,
}

/// One node gone through when they ask for all of a kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Together {
    pub node: String,
    /// Its entries added now, newest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    /// Its entries shown in all, of how many it holds.
    pub shown: usize,
    pub of: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pick {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub picked: Vec<Picked>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub about_them: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub loose: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub together: Vec<Together>,
}

fn tier(status: Option<Status>) -> u8 {
    match status.unwrap_or(Status::Active) {
        Status::Active | Status::Ongoing => 0,
        Status::Resting => 1,
        Status::Done | Status::Dropped => 2,
    }
}

/// A node's entries, newest first (the same time: by id).
fn newest(node: &PickNode) -> Vec<&str> {
    let mut entries: Vec<&PickEntry> = node.entries.iter().collect();
    entries.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.id.cmp(&b.id)));
    let mut seen = BTreeSet::new();
    entries
        .into_iter()
        .map(|entry| entry.id.as_str())
        .filter(|id| seen.insert(*id))
        .collect()
}

/// Of `node`, the ranked entries on it in rank order, then its newest, at
/// most `count`.
fn best(
    node: &PickNode,
    ranked: &[&str],
    on: &BTreeMap<&str, usize>,
    at: usize,
    count: usize,
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ranked
        .iter()
        .copied()
        .filter(|id| on.get(id) == Some(&at))
        .chain(newest(node))
    {
        if out.len() >= count {
            break;
        }
        if !out.iter().any(|seen| seen == id) {
            out.push(id.to_string());
        }
    }
    out
}

pub fn pick(input: PickInput) -> Result<Pick, NodeError> {
    let mut ids = BTreeSet::new();
    for node in &input.nodes {
        check_token("node id", &node.id)?;
        if !ids.insert(node.id.as_str()) {
            return Err(NodeError::new("node ids repeat"));
        }
        for entry in &node.entries {
            check_token("entry id", &entry.id)?;
        }
    }
    for own in [&input.about_them, &input.her].into_iter().flatten() {
        check_token("node id", own)?;
    }
    let mut ranked: Vec<&str> = Vec::new();
    let mut named: BTreeSet<&str> = BTreeSet::new();
    for one in &input.ranked {
        check_token("ranked id", &one.id)?;
        if ranked.contains(&one.id.as_str()) {
            continue;
        }
        ranked.push(&one.id);
        if one.named {
            named.insert(&one.id);
        }
    }
    // Which node each entry is on: the first live node that lists it.
    let mut on: BTreeMap<&str, usize> = BTreeMap::new();
    for (at, node) in input.nodes.iter().enumerate() {
        if node.gone {
            continue;
        }
        for entry in &node.entries {
            on.entry(entry.id.as_str()).or_insert(at);
        }
    }
    let own = |at: usize| {
        let id = Some(&input.nodes[at].id);
        id == input.about_them.as_ref() || id == input.her.as_ref()
    };
    // The nodes of the named entries, in the order the first of each ranks.
    let mut by_rank: Vec<usize> = Vec::new();
    for id in &ranked {
        if !named.contains(id) {
            continue;
        }
        if let Some(&at) = on.get(id)
            && !own(at)
            && !by_rank.contains(&at)
        {
            by_rank.push(at);
        }
    }
    let mut order = by_rank.clone();
    // Stable: within a tier, the rank order stands.
    order.sort_by_key(|at| tier(input.nodes[*at].status));
    order.truncate(input.budget.picked);
    let picked: Vec<Picked> = order
        .iter()
        .map(|at| Picked {
            node: input.nodes[*at].id.clone(),
            entries: best(&input.nodes[*at], &ranked, &on, *at, input.budget.entries),
        })
        .collect();
    let about_them = input
        .about_them
        .as_ref()
        .and_then(|id| {
            input
                .nodes
                .iter()
                .position(|node| &node.id == id && !node.gone)
        })
        .map(|at| {
            ranked
                .iter()
                .filter(|id| on.get(*id) == Some(&at))
                .take(input.budget.about_them)
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut shown: BTreeSet<String> = picked
        .iter()
        .flat_map(|picked| picked.entries.iter().cloned())
        .chain(about_them.iter().cloned())
        .collect();
    let loose: Vec<String> = ranked
        .iter()
        .filter(|id| !shown.contains(**id))
        .map(|id| id.to_string())
        .collect();
    shown.extend(loose.iter().cloned());
    let mut together = Vec::new();
    if input.thorough {
        let mut added = 0;
        for at in by_rank {
            let node = &input.nodes[at];
            let all = newest(node);
            let mut entries = Vec::new();
            for id in &all {
                if added < input.budget.together && !shown.contains(*id) {
                    shown.insert(id.to_string());
                    entries.push(id.to_string());
                    added += 1;
                }
            }
            together.push(Together {
                node: node.id.clone(),
                shown: all.iter().filter(|id| shown.contains(**id)).count(),
                of: all.len(),
                entries,
            });
        }
    }
    Ok(Pick {
        picked,
        about_them,
        loose,
        together,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn run(input: serde_json::Value) -> Pick {
        pick(serde_json::from_value(input).unwrap()).unwrap()
    }

    fn entries(prefix: &str, count: i64) -> serde_json::Value {
        json!(
            (1..=count)
                .map(|n| json!({"id": format!("{prefix}{n}"), "at": n}))
                .collect::<Vec<_>>()
        )
    }

    #[test]
    fn named_entries_pick_their_nodes_active_first() {
        let out = run(json!({
            "ranked": [
                {"id": "r1", "named": true},
                {"id": "p2", "named": true},
                {"id": "d1", "named": true},
                {"id": "a3"},
                {"id": "x"},
                {"id": "y1", "named": true},
                {"id": "h1", "named": true}
            ],
            "nodes": [
                {"id": "plants", "entries": entries("p", 5)},
                {"id": "resting", "status": "resting", "entries": entries("r", 2)},
                {"id": "done", "status": "done", "entries": entries("d", 1)},
                {"id": "you", "entries": entries("a", 6)},
                {"id": "her", "entries": entries("h", 1)},
                {"id": "unnamed", "entries": entries("u", 1)},
                {"id": "gone", "gone": true, "entries": entries("y", 1)}
            ],
            "aboutThem": "you",
            "her": "her"
        }));
        let nodes: Vec<&str> = out
            .picked
            .iter()
            .map(|picked| picked.node.as_str())
            .collect();
        assert_eq!(nodes, ["plants", "resting", "done"]);
        // Ranked first, then the newest.
        assert_eq!(out.picked[0].entries, ["p2", "p5", "p4"]);
        // Only what was ranked of them: nothing newest to fill.
        assert_eq!(out.about_them, ["a3"]);
        // Shown nowhere above: on her node, on a gone node, on no node.
        assert_eq!(out.loose, ["x", "y1", "h1"]);
        assert!(out.together.is_empty());
    }

    #[test]
    fn nothing_about_them_found_brings_none_of_them() {
        let out = run(json!({
            "ranked": [{"id": "x", "named": true}],
            "nodes": [{"id": "you", "entries": entries("a", 6)}],
            "aboutThem": "you"
        }));
        assert!(out.about_them.is_empty());
        assert_eq!(out.loose, ["x"]);
    }

    #[test]
    fn going_through_all_of_a_kind_adds_the_rest_up_to_the_budget() {
        let out = run(json!({
            "ranked": [{"id": "p1", "named": true}, {"id": "s1", "named": true}, {"id": "f1"}],
            "nodes": [
                {"id": "plants", "entries": entries("p", 4)},
                {"id": "sports", "entries": entries("s", 30)},
                {"id": "food", "entries": entries("f", 3)}
            ],
            "thorough": true,
            "budget": {"together": 10}
        }));
        assert_eq!(out.together.len(), 2, "only the named entries' nodes");
        assert_eq!(out.together[0].node, "plants");
        // Three of plants were picked; the fourth comes now.
        assert_eq!(out.together[0].entries, ["p2"]);
        assert_eq!((out.together[0].shown, out.together[0].of), (4, 4));
        // Three of sports were picked; nine more fill the ten.
        assert_eq!(out.together[1].entries.len(), 9);
        assert_eq!(out.together[1].entries[0], "s28");
        assert_eq!((out.together[1].shown, out.together[1].of), (12, 30));
        assert_eq!(out.loose, ["f1"]);
    }
}
