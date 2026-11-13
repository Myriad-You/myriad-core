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
//!   order, then its newest to fill, at most [`Budget::entries`]; with two
//!   places or more its newest entry always among them (it takes the last
//!   place when the ranked fill them all), since what the words found may
//!   all be from before and the newest says how things are now.
//! - **How things stand** (`node.standing`): each picked node's, and
//!   their own node's, come before its entries and take none of their
//!   places: every one that has not ended, newest first. Ranking never
//!   finds them; they are how things are now, which entries only tell of.
//! - **How their life stands, every time** (`resident`): what stands in
//!   someone's life on any of her nodes not gone, shown whatever the words
//!   found, since an old way of things on a node the words did find says
//!   the opposite of what is now. Each item once, not ended, with words,
//!   not a rule's or a repository's (only an item whose id says whose life
//!   it is), not on her own node (`her`, whose head the host shows every
//!   time), and not shown above already. Items on active and ongoing nodes
//!   first, then the rest; within each, the newest written first. At most
//!   [`Budget::resident`] items and [`Budget::resident_chars`] chars, each
//!   counted as the host shows it (`node name：text`): the first that does
//!   not fit ends the list.
//! - **About them**: the entries of their own node, ranked first, then the
//!   newest, at most [`Budget::about_them`]; its newest always among them
//!   too.
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
    /// Entries of their own node.
    pub about_them: usize,
    /// Entries added in all when going through all of a kind.
    pub together: usize,
    /// Items of how their life stands shown every time, at most.
    pub resident: usize,
    /// The chars those items take, at most.
    pub resident_chars: usize,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            picked: 3,
            entries: 3,
            about_them: 5,
            together: 24,
            resident: 24,
            resident_chars: 600,
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

/// How one thing on a node stands now: its item, when its current
/// version was written (Unix microseconds), and whether it ended.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickStanding {
    pub item: String,
    pub at: i64,
    #[serde(default)]
    pub ended: bool,
    /// Its words, as the host shows them. Without them it is never among
    /// how their life stands every time.
    #[serde(default)]
    pub text: String,
}

/// A node of the audience as recall sees it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickNode {
    pub id: String,
    /// Its name, as the host shows it before an item of how things stand.
    #[serde(default)]
    pub name: Option<String>,
    /// Absent reads as active.
    #[serde(default)]
    pub status: Option<Status>,
    /// Forgotten, or gone into another node.
    #[serde(default)]
    pub gone: bool,
    #[serde(default)]
    pub entries: Vec<PickEntry>,
    #[serde(default)]
    pub standing: Vec<PickStanding>,
}

impl PickNode {
    /// A folded node as recall sees it: each entry by the memory row it is
    /// the place of (`memory:<id>`), else by its own row.
    pub fn of(node: &Node) -> Self {
        Self {
            id: node.id.clone(),
            name: node.name.as_ref().map(|name| name.current.value.clone()),
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
            standing: node
                .standing
                .iter()
                .map(|(item, standing)| PickStanding {
                    item: item.clone(),
                    at: standing.current.at,
                    ended: standing.current.value.ended,
                    text: standing.current.value.text.clone(),
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
    /// How things on it stand, by item, newest first: shown before its
    /// entries.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub standing: Vec<String>,
    pub entries: Vec<String>,
}

/// One item of how their life stands, shown every time: on which node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resident {
    pub node: String,
    pub item: String,
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
    /// How things stand on their own node, by item, newest first: shown
    /// before its entries.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub about_them_standing: Vec<String>,
    /// How their life stands, shown every time, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resident: Vec<Resident>,
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

/// How things on `node` stand: every one not ended, newest first (the same
/// time: by item).
fn standing(node: &PickNode) -> Vec<String> {
    let mut now: Vec<&PickStanding> = node.standing.iter().filter(|one| !one.ended).collect();
    now.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.item.cmp(&b.item)));
    let mut seen = BTreeSet::new();
    now.into_iter()
        .filter(|one| seen.insert(one.item.as_str()))
        .map(|one| one.item.clone())
        .collect()
}

/// How their life stands across `nodes` (not `her` own node), shown every
/// time, leaving out what `shown` (node, item) already has; within `budget`.
fn resident(
    nodes: &[PickNode],
    her: Option<&str>,
    shown: &BTreeSet<(&str, &str)>,
    budget: &Budget,
) -> Vec<Resident> {
    let mut candidates: Vec<(u8, i64, &PickNode, &PickStanding)> = Vec::new();
    // Her own node's head is the host's to show every time already.
    for node in nodes
        .iter()
        .filter(|node| !node.gone && Some(node.id.as_str()) != her)
    {
        let tier = u8::from(tier(node.status) > 0);
        let mut seen = BTreeSet::new();
        for one in &node.standing {
            if one.ended
                || one.text.trim().is_empty()
                || crate::stands::whose_of(&one.item).is_none()
                || shown.contains(&(node.id.as_str(), one.item.as_str()))
                || !seen.insert(one.item.as_str())
            {
                continue;
            }
            candidates.push((tier, one.at, node, one));
        }
    }
    candidates.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.id.cmp(&b.2.id))
            .then_with(|| a.3.item.cmp(&b.3.item))
    });
    let mut out = Vec::new();
    let mut chars = 0;
    for (_, _, node, one) in candidates {
        let name = node.name.as_deref().unwrap_or_default().trim();
        let line = name.chars().count() + 1 + one.text.trim().chars().count();
        if out.len() >= budget.resident || chars + line > budget.resident_chars {
            break;
        }
        chars += line;
        out.push(Resident {
            node: node.id.clone(),
            item: one.item.clone(),
        });
    }
    out
}

/// Of `node`, the ranked entries on it in rank order, then its newest, at
/// most `count`; with two places or more its newest entry always among
/// them. What the words found may all be from before, and the newest says
/// how things are now: where the ranked fill every place, the newest takes
/// the last of them. A single place stays the best found.
fn best(
    node: &PickNode,
    ranked: &[&str],
    on: &BTreeMap<&str, usize>,
    at: usize,
    count: usize,
) -> Vec<String> {
    let newest = newest(node);
    let mut out: Vec<String> = Vec::new();
    for id in ranked
        .iter()
        .copied()
        .filter(|id| on.get(id) == Some(&at))
        .chain(newest.iter().copied())
    {
        if out.len() >= count {
            break;
        }
        if !out.iter().any(|seen| seen == id) {
            out.push(id.to_string());
        }
    }
    if let Some(&latest) = newest.first()
        && out.len() >= 2
        && !out.iter().any(|seen| seen == latest)
    {
        out.pop();
        out.push(latest.to_string());
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
        for one in &node.standing {
            crate::tags::check_item_id("standing item", &one.item)?;
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
            standing: standing(&input.nodes[*at]),
            entries: best(&input.nodes[*at], &ranked, &on, *at, input.budget.entries),
        })
        .collect();
    let about_them_at = input.about_them.as_ref().and_then(|id| {
        input
            .nodes
            .iter()
            .position(|node| &node.id == id && !node.gone)
    });
    let about_them = about_them_at
        .map(|at| best(&input.nodes[at], &ranked, &on, at, input.budget.about_them))
        .unwrap_or_default();
    let about_them_standing = about_them_at
        .map(|at| standing(&input.nodes[at]))
        .unwrap_or_default();
    let nodes = &input.nodes;
    let shown_standing: BTreeSet<(&str, &str)> = picked
        .iter()
        .flat_map(|picked| {
            picked
                .standing
                .iter()
                .map(|item| (picked.node.as_str(), item.as_str()))
        })
        .chain(about_them_at.into_iter().flat_map(|at| {
            about_them_standing
                .iter()
                .map(move |item| (nodes[at].id.as_str(), item.as_str()))
        }))
        .collect();
    let resident = resident(nodes, input.her.as_deref(), &shown_standing, &input.budget);
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
        about_them_standing,
        resident,
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
        assert_eq!(out.about_them, ["a3", "a6", "a5", "a4", "a2"]);
        // Shown nowhere above: on her node, on a gone node, on no node.
        assert_eq!(out.loose, ["x", "y1", "h1"]);
        assert!(out.together.is_empty());
    }

    #[test]
    fn a_picked_node_always_shows_its_newest_entry() {
        // The words found three old entries: they would fill all three
        // places, and the newest (how things are now) would not come.
        let out = run(json!({
            "ranked": [
                {"id": "r2", "named": true},
                {"id": "r1", "named": true},
                {"id": "r3", "named": true}
            ],
            "nodes": [{"id": "running", "entries": entries("r", 6)}]
        }));
        assert_eq!(out.picked[0].entries, ["r2", "r1", "r6"]);
        // What it pushed out is still shown, as loose.
        assert_eq!(out.loose, ["r3"]);
        // Already among what the words found: nothing changes.
        let out = run(json!({
            "ranked": [
                {"id": "r2", "named": true},
                {"id": "r6", "named": true},
                {"id": "r3", "named": true}
            ],
            "nodes": [{"id": "running", "entries": entries("r", 6)}]
        }));
        assert_eq!(out.picked[0].entries, ["r2", "r6", "r3"]);
        // Their own node too.
        let out = run(json!({
            "ranked": [
                {"id": "a1"}, {"id": "a2"}, {"id": "a3"}, {"id": "a4"}, {"id": "a5"}
            ],
            "nodes": [{"id": "you", "entries": entries("a", 9)}],
            "aboutThem": "you"
        }));
        assert_eq!(out.about_them, ["a1", "a2", "a3", "a4", "a9"]);
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

    /// How their life stands comes every time, from every node not gone:
    /// not what is shown above already, not ended, not a rule's, active
    /// nodes first and the newest first within them, until it is full.
    #[test]
    fn how_their_life_stands_comes_every_time_within_its_room() {
        let item = |node: &str, n: usize| crate::stands::item_id("them", node, "memory:m", n);
        let (run1, run2) = (item("run", 1), item("run", 2));
        let (job, old_job, mom) = (item("job", 1), item("job", 2), item("mom", 1));
        let (pause, gone, you) = (item("pause", 1), item("gone", 1), item("you", 1));
        let mine = item("me", 1);
        let out = run(json!({
            "ranked": [{"id": "r1", "named": true}],
            "nodes": [
                {"id": "run", "name": "跑步", "entries": entries("r", 1), "standing": [
                    {"item": run1, "at": 9, "text": "膝盖伤了，三个月不跑"},
                    {"item": run2, "at": 1, "text": "每周三夜跑", "ended": true}
                ]},
                {"id": "job", "name": "工作", "standing": [
                    {"item": job, "at": 5, "text": "在宠物医疗公司做品牌"},
                    {"item": old_job, "at": 2, "text": "在趣养做运营", "ended": true},
                    {"item": "now", "at": 8, "text": "规则写的"}
                ]},
                {"id": "mom", "name": "妈妈", "standing": [
                    {"item": mom, "at": 3, "text": "每周六中午给妈妈打电话"},
                    {"item": item("mom", 2), "at": 4, "text": "  "}
                ]},
                {"id": "pause", "name": "吉他", "status": "resting", "standing": [
                    {"item": pause, "at": 7, "text": "在家对着教程练"}
                ]},
                {"id": "gone", "gone": true, "name": "旧", "standing": [
                    {"item": gone, "at": 6, "text": "已并走的"}
                ]},
                {"id": "you", "name": "你", "standing": [
                    {"item": you, "at": 6, "text": "住在双楠"}
                ]},
                {"id": "me", "name": "她自己", "standing": [
                    {"item": mine, "at": 9, "text": "她自己节点上的"}
                ]}
            ],
            "aboutThem": "you",
            "her": "me"
        }));
        let resident: Vec<(&str, &str)> = out
            .resident
            .iter()
            .map(|one| (one.node.as_str(), one.item.as_str()))
            .collect();
        // Shown above already: the picked node's, and their own node's.
        assert_eq!(out.picked[0].standing, std::slice::from_ref(&run1));
        assert_eq!(out.about_them_standing, std::slice::from_ref(&you));
        assert_eq!(
            resident,
            [
                ("job", job.as_str()),
                ("mom", mom.as_str()),
                ("pause", pause.as_str())
            ]
        );
        // The room ends it: the first that does not fit, and all after.
        let tight = |budget: serde_json::Value| {
            run(json!({
                "ranked": [],
                "nodes": [
                    {"id": "job", "name": "工作", "standing": [
                        {"item": job, "at": 5, "text": "在宠物医疗公司做品牌"}
                    ]},
                    {"id": "mom", "name": "妈妈", "standing": [
                        {"item": mom, "at": 3, "text": "每周六中午给妈妈打电话"}
                    ]}
                ],
                "budget": budget
            }))
            .resident
            .len()
        };
        assert_eq!(tight(json!({})), 2);
        assert_eq!(tight(json!({"resident": 1})), 1);
        // 「工作：在宠物医疗公司做品牌」 is 13 chars; the next one 14 more.
        assert_eq!(tight(json!({"residentChars": 13})), 1);
        assert_eq!(tight(json!({"residentChars": 26})), 1);
        assert_eq!(tight(json!({"residentChars": 27})), 2);
        assert_eq!(tight(json!({"residentChars": 12})), 0);
    }
}
