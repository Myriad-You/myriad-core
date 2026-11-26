//! Picking nodes for recall: which of her nodes come to mind before she
//! answers, and which entries of each.
//!
//! The host ranks its entries (memories) for the words, as it always has
//! (most relevant first; each marked `named` when the words found it, not
//! when it only fills a try); retired ones (replaced, faded, forgotten) are
//! the host's to leave out before calling. [`pick`] turns that ranking
//! into nodes, by these rules:
//!
//! - **Picked nodes**: a node is picked when a `named` entry picks it, OR
//!   when `query` matches the `name` or `aliases` of a live non-empty node
//!   (excluding `gone`, own system nodes, and name-only nodes with zero
//!   entries). Active and ongoing nodes first, then resting ones, then
//!   done or dropped ones; within each status tier, nodes matched by a
//!   `named` entry rank first (in the order their first named entry ranks),
//!   followed by nodes matched only by `query` (ordered newest-entry-first,
//!   then by id). A node gone (forgotten, or gone into another: its entries
//!   are on that one now) is never picked. Their own node (「关于你」) and
//!   hers (`her`) are never picked. At most [`Budget::picked`].
//! - **Each picked node's entries**: its ranked entries first, in rank
//!   order, then its newest to fill, at most [`Budget::entries`]; with two
//!   places or more its newest entry always among them (it takes the last
//!   place when the ranked fill them all), since what the words found may
//!   all be from before and the newest says how things are now.
//! - **How things stand** (`node.standing`): each picked node's, and
//!   their own node's, come before its entries and take none of their
//!   places: those not ended, the ones resting on an entry the words found
//!   first, then newest first. Ranking never finds them; they are how
//!   things are now, which entries only tell of. At most
//!   [`Budget::standing`] of each picked node's and [`Budget::resident`] of
//!   their own; how many more there are is said, so she knows not all of
//!   it is in front of her (none is cut short).
//! - **How their life stands, every time** (`resident`): what stands in
//!   someone's life on any of her nodes not gone, shown whatever the words
//!   found, since an old way of things on a node the words did find says
//!   the opposite of what is now. Each item once, not ended, with words,
//!   not a rule's or a repository's (only an item whose id says whose life
//!   it is), not on her own node (`her`, whose head the host shows every
//!   time), and not shown above already. Items on active and ongoing nodes
//!   first, then the rest; within each, the newest written first. At most
//!   [`Budget::resident`] items and [`Budget::resident_chars`] chars,
//!   counted on `node name：text` before any host-appended date suffix
//!   (hosts enforce any additional display room). One that does not
//!   fit in the room left is left out and the next one tried; one longer
//!   than all the room alone is cut short (ending in `…`) to the room left,
//!   and to half of all the room at most, keeping 12 chars of its words at
//!   least, else left out.
//!   How many were left out and how many cut is said.
//! - **What is due**: an item resting only on what goes on past the day
//!   she learned it (a plan, a deadline) is due on its last day
//!   ([`crate::stands::due_of`]); once the day has gone by
//!   ([`PickInput::today`]) it is no longer how things stand, above or every
//!   time. Nothing is written for it: its rows are as they were.
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
    /// Extra characters reserved per resident item for a host-appended suffix
    /// (such as `, since 2026-11`). Zero by default so core callers count only
    /// `node name：text` unless a host sets its display allowance.
    pub resident_suffix_chars: usize,
    /// Items of how things stand on each picked node, at most, so a node
    /// whose stands grew long over months does not crowd out the rest: by
    /// default 8, the default room of how their life stands every time
    /// (24) shared by the default nodes picked (3), set apart from both.
    /// Their own node's take [`Budget::resident`].
    pub standing: usize,
}

/// The fewest chars of an item's words worth showing cut short.
const RESIDENT_CUT_LEAST: usize = 12;

fn is_zero(value: &usize) -> bool {
    *value == 0
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
            resident_suffix_chars: 0,
            standing: 8,
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
    /// What its current version rests on (`memory:<id>`, as the node's
    /// rows cite it): one resting on an entry the words found comes first.
    #[serde(default)]
    pub because: Vec<String>,
    /// The memories its current version rests on, with their days
    /// ([`crate::stands::due_of`]): what was still to come when she learned
    /// it is due on that day, and once it has gone by it is no longer how
    /// things stand. Without them it is never due.
    #[serde(default)]
    pub rests_on: Vec<crate::stands::RestsOn>,
}

impl PickStanding {
    /// Whether it stands on `today`: not ended, and not due before it.
    fn holds(&self, today: Option<&str>) -> bool {
        !self.ended
            && !today.is_some_and(|today| {
                crate::stands::due_of(&self.rests_on)
                    .is_some_and(|due| crate::stands::due_passed(&due, today))
            })
    }
}

/// A node of the audience as recall sees it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PickNode {
    pub id: String,
    /// Its name, as the host shows it before an item of how things stand.
    #[serde(default)]
    pub name: Option<String>,
    /// Other names or aliases this node goes by in the same audience.
    #[serde(default)]
    pub aliases: Vec<String>,
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
            aliases: node
                .aliases
                .as_ref()
                .map(|aliases| aliases.current.value.clone())
                .unwrap_or_default(),
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
                    because: standing.current.value.because.clone(),
                    rests_on: Vec::new(),
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
    /// The day it is (`2026-11-15`), for what is due ([`PickStanding::rests_on`]).
    /// Without it nothing is due.
    #[serde(default)]
    pub today: Option<String>,
    /// The person's query words, used to match live non-empty nodes by name or alias.
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Picked {
    pub node: String,
    /// How things on it stand, by item, those its words found first, then
    /// newest first: shown before its entries.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub standing: Vec<String>,
    /// How many more stand on it than [`Budget::standing`] let in: all of
    /// them, whether shown or not; those [`Pick::resident`] has (its items
    /// of this node, which are never ones shown here) a host showing it
    /// takes off.
    #[serde(skip_serializing_if = "is_zero")]
    pub standing_left_out: usize,
    pub entries: Vec<String>,
}

/// One item of how their life stands, shown every time: on which node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resident {
    pub node: String,
    pub item: String,
    /// Its words as counted: the host shows these, cut short (ending in
    /// `…`) when the item alone is longer than all the room.
    pub text: String,
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
    /// How things stand on their own node, by item, those their words
    /// found first, then newest first: shown before its entries.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub about_them_standing: Vec<String>,
    /// How many more stand there than [`Budget::resident`] let in: all of
    /// them, whether shown or not; those [`Pick::resident`] has (its items
    /// of this node, which are never ones shown here) a host showing it
    /// takes off.
    #[serde(skip_serializing_if = "is_zero")]
    pub about_them_standing_left_out: usize,
    /// How their life stands, shown every time, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resident: Vec<Resident>,
    /// How many of those were left out for want of room.
    #[serde(skip_serializing_if = "is_zero")]
    pub resident_left_out: usize,
    /// How many of those were cut short.
    #[serde(skip_serializing_if = "is_zero")]
    pub resident_cut: usize,
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

/// How things on `node` stand: those not ended, the ones resting on an
/// entry the words found (`named`) first, then newest first (the same time:
/// by item); at most `most`, and the rest.
fn standing(
    node: &PickNode,
    today: Option<&str>,
    named: &BTreeSet<&str>,
    most: usize,
) -> (Vec<String>, Vec<String>) {
    let found = |one: &PickStanding| {
        one.because.iter().any(|reference| {
            reference
                .strip_prefix(MEMORY_REF)
                .is_some_and(|id| named.contains(id))
        })
    };
    let mut now: Vec<&PickStanding> = node
        .standing
        .iter()
        .filter(|one| one.holds(today))
        .collect();
    now.sort_by(|a, b| {
        found(b)
            .cmp(&found(a))
            .then_with(|| b.at.cmp(&a.at))
            .then_with(|| a.item.cmp(&b.item))
    });
    let mut seen = BTreeSet::new();
    let mut items: Vec<String> = now
        .into_iter()
        .filter(|one| seen.insert(one.item.as_str()))
        .map(|one| one.item.clone())
        .collect();
    let rest = items.split_off(most.min(items.len()));
    (items, rest)
}

/// How their life stands across `nodes` (not `her` own node), shown every
/// time, leaving out what `shown` (node, item) already has; within `budget`:
/// with how many were left out for want of room, and how many cut short.
fn resident(
    nodes: &[PickNode],
    her: Option<&str>,
    shown: &BTreeSet<(&str, &str)>,
    budget: &Budget,
    today: Option<&str>,
) -> (Vec<Resident>, usize, usize) {
    let mut candidates: Vec<(u8, i64, &PickNode, &PickStanding)> = Vec::new();
    // Her own node's head is the host's to show every time already.
    for node in nodes
        .iter()
        .filter(|node| !node.gone && Some(node.id.as_str()) != her)
    {
        let tier = u8::from(tier(node.status) > 0);
        let mut seen = BTreeSet::new();
        for one in &node.standing {
            if !one.holds(today)
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
    let (mut out, mut left_out, mut cut) = (Vec::new(), 0, 0);
    let mut chars = 0;
    for (_, _, node, one) in candidates {
        let name = node.name.as_deref().unwrap_or_default().trim();
        let words = one.text.trim();
        let head = name.chars().count() + 1 + budget.resident_suffix_chars;
        let line = head + words.chars().count();
        let room = budget.resident_chars.saturating_sub(chars);
        let text = if out.len() >= budget.resident {
            None
        } else if line <= room {
            Some(words.to_string())
        } else if line > budget.resident_chars
            && room.min(budget.resident_chars / 2) > head + RESIDENT_CUT_LEAST
        {
            // Longer than all the room alone: cut, rather than leave it out
            // (or, when it comes first, leave none); to half the room at
            // most, so one does not take the place of all the rest.
            cut += 1;
            let kept: String = words
                .chars()
                .take(room.min(budget.resident_chars / 2) - head - 1)
                .collect();
            Some(format!("{kept}…"))
        } else {
            None
        };
        let Some(text) = text else {
            left_out += 1;
            continue;
        };
        chars += head + text.chars().count();
        out.push(Resident {
            node: node.id.clone(),
            item: one.item.clone(),
            text,
        });
    }
    (out, left_out, cut)
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

fn latin_forms(word: &str) -> Vec<String> {
    let mut forms = vec![word.to_string()];
    if !word.is_ascii() {
        return forms;
    }
    if word.len() >= 5
        && let Some(prefix) = word.strip_suffix("ies")
    {
        forms.push(format!("{prefix}y"));
        forms.push(format!("{prefix}ie"));
        return forms;
    }
    for suffix in ["ing", "ed"] {
        if word.len() >= suffix.len() + 3
            && let Some(stem) = word.strip_suffix(suffix)
        {
            if stem.len() >= 3 {
                forms.push(format!("{stem}e"));
            }
            if stem.len() >= 4 {
                forms.push(stem.to_string());
            }
            return forms;
        }
    }
    if word.len() >= 5
        && let Some(stem) = word.strip_suffix("es")
    {
        forms.push(word[..word.len() - 1].to_string());
        if stem.ends_with("sh")
            || stem.ends_with("ch")
            || stem.ends_with('x')
            || stem.ends_with("ss")
            || stem.ends_with("zz")
            || stem.ends_with('o')
        {
            forms.push(stem.to_string());
        }
        return forms;
    }
    if word.len() >= 4
        && !word.ends_with("ss")
        && !word.ends_with("us")
        && !word.ends_with("is")
        && let Some(stem) = word.strip_suffix('s')
    {
        forms.push(stem.to_string());
    }
    forms
}

fn latin_token_matches(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if is_latin_stopword(a) || is_latin_stopword(b) {
        return false;
    }
    let fa = latin_forms(a);
    let fb = latin_forms(b);
    fa.iter()
        .any(|x| x.len() >= 3 && fb.iter().any(|y| x == y))
}

fn is_latin_stopword(word: &str) -> bool {
    matches!(
        word,
        "all"
            | "and"
            | "any"
            | "are"
            | "been"
            | "but"
            | "can"
            | "did"
            | "does"
            | "for"
            | "from"
            | "had"
            | "has"
            | "have"
            | "her"
            | "his"
            | "how"
            | "may"
            | "new"
            | "not"
            | "now"
            | "one"
            | "our"
            | "out"
            | "she"
            | "that"
            | "the"
            | "them"
            | "they"
            | "this"
            | "time"
            | "two"
            | "was"
            | "were"
            | "what"
            | "when"
            | "where"
            | "which"
            | "who"
            | "will"
            | "with"
            | "you"
            | "your"
    )
}

fn label_matches_query(label: &str, query_lower: &str, query_tokens: &[&str]) -> bool {
    let label_lower = label.trim().to_lowercase();
    if label_lower.is_empty() {
        return false;
    }
    let has_cjk = label_lower.chars().any(|ch| {
        matches!(
            ch,
            '\u{3040}'..='\u{30ff}'
                | '\u{3400}'..='\u{4dbf}'
                | '\u{4e00}'..='\u{9fff}'
                | '\u{ac00}'..='\u{d7af}'
        )
    });
    if has_cjk {
        return label_lower.chars().count() >= 2 && query_lower.contains(&label_lower);
    }
    let label_words: Vec<&str> = label_lower
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect();
    match label_words.as_slice() {
        [] => false,
        [single] => {
            single.chars().count() >= 3
                && !is_latin_stopword(single)
                && query_tokens.iter().any(|token| {
                    token.chars().count() >= 3 && latin_token_matches(single, token)
                })
        }
        phrase => {
            label_lower.chars().count() >= 3
                && query_tokens
                    .windows(phrase.len())
                    .any(|window| window == phrase)
        }
    }
}

fn node_matches_query(node: &PickNode, query_lower: &str, query_tokens: &[&str]) -> bool {
    node.name
        .iter()
        .map(String::as_str)
        .chain(node.aliases.iter().map(String::as_str))
        .any(|label| label_matches_query(label, query_lower, query_tokens))
}

pub fn pick(input: PickInput) -> Result<Pick, NodeError> {
    let today = input.today.as_deref();
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
    if let Some(query) = input.query.as_deref() {
        let query_lower = query.trim().to_lowercase();
        if !query_lower.is_empty() {
            let query_tokens: Vec<&str> = query_lower
                .split(|ch: char| !ch.is_alphanumeric())
                .filter(|token| !token.is_empty())
                .collect();
            let mut matched: Vec<(i64, usize)> = input
                .nodes
                .iter()
                .enumerate()
                .filter(|(at, node)| {
                    !node.gone
                        && !own(*at)
                        && !node.entries.is_empty()
                        && !by_rank.contains(at)
                        && node_matches_query(node, &query_lower, &query_tokens)
                })
                .map(|(at, node)| {
                    let newest_at = node
                        .entries
                        .iter()
                        .map(|entry| entry.at)
                        .max()
                        .unwrap_or(i64::MIN);
                    (newest_at, at)
                })
                .collect();
            matched.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| input.nodes[a.1].id.cmp(&input.nodes[b.1].id))
            });
            by_rank.extend(matched.into_iter().map(|(_, at)| at));
        }
    }
    let mut order = by_rank.clone();
    // Stable: within a tier, the rank order stands.
    order.sort_by_key(|at| tier(input.nodes[*at].status));
    order.truncate(input.budget.picked);
    let picked: Vec<Picked> = order
        .iter()
        .map(|at| {
            let (standing, more) =
                standing(&input.nodes[*at], today, &named, input.budget.standing);
            Picked {
                node: input.nodes[*at].id.clone(),
                standing,
                standing_left_out: more.len(),
                entries: best(&input.nodes[*at], &ranked, &on, *at, input.budget.entries),
            }
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
    let (about_them_standing, about_them_rest) = about_them_at
        .map(|at| standing(&input.nodes[at], today, &named, input.budget.resident))
        .unwrap_or_default();
    let about_them_standing_left_out = about_them_rest.len();
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
    let (resident, resident_left_out, resident_cut) = resident(
        nodes,
        input.her.as_deref(),
        &shown_standing,
        &input.budget,
        today,
    );
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
        about_them_standing_left_out,
        resident,
        resident_left_out,
        resident_cut,
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

    /// A picked node whose stands grew long shows at most its share of the
    /// room, the ones resting on what the words found first, then newest,
    /// and says how many more there are; their own node takes the whole
    /// room.
    #[test]
    fn a_long_list_of_how_things_stand_shows_its_share_and_says_the_rest() {
        let stands = |prefix: &str, count: i64, named: &[i64]| {
            json!(
                (1..=count)
                    .map(|n| {
                        let because = if named.contains(&n) {
                            vec![format!("memory:{prefix}{n}")]
                        } else {
                            Vec::new()
                        };
                        json!({"item": format!("{prefix}-s{n}"), "at": n, "because": because})
                    })
                    .collect::<Vec<_>>()
            )
        };
        let out = run(json!({
            "ranked": [{"id": "g2", "named": true}, {"id": "g3", "named": true}, {"id": "a1"}],
            "nodes": [
                {"id": "garden", "entries": entries("g", 3), "standing": stands("g", 10, &[2, 3])},
                {"id": "you", "entries": entries("a", 1), "standing": stands("a", 30, &[])}
            ],
            "aboutThem": "you"
        }));
        let garden = &out.picked[0];
        assert_eq!(
            garden.standing,
            [
                "g-s3", "g-s2", "g-s10", "g-s9", "g-s8", "g-s7", "g-s6", "g-s5"
            ],
            "what the words found first, then newest, eight at most"
        );
        assert_eq!(garden.standing_left_out, 2);
        assert_eq!(out.about_them_standing.len(), 24);
        assert_eq!(out.about_them_standing[0], "a-s30");
        assert_eq!(out.about_them_standing_left_out, 6);
        // Under its share nothing is said to be left out.
        let out = run(json!({
            "ranked": [{"id": "g1", "named": true}],
            "nodes": [{"id": "garden", "entries": entries("g", 1), "standing": stands("g", 4, &[])}]
        }));
        assert_eq!(out.picked[0].standing.len(), 4);
        assert_eq!(out.picked[0].standing_left_out, 0);
        let shown = serde_json::to_value(&out).unwrap();
        assert!(
            shown["picked"][0].get("standingLeftOut").is_none(),
            "{shown}"
        );
        // All left out are counted; one with words goes on to the list of
        // how their life stands, the only item of this node there.
        let mut many: Vec<serde_json::Value> = (3..=10)
            .map(|n| json!({"item": format!("g-s{n}"), "at": n}))
            .collect();
        many.push(json!({"item": "g-s2", "at": 2}));
        many.push(json!({"item": "them-0000000001", "at": 1, "text": "种了番茄"}));
        let out = run(json!({
            "ranked": [{"id": "g1", "named": true}],
            "nodes": [{"id": "garden", "entries": entries("g", 1), "standing": many}]
        }));
        assert_eq!(out.picked[0].standing.len(), 8);
        assert_eq!(
            out.resident
                .iter()
                .map(|one| (one.node.as_str(), one.item.as_str()))
                .collect::<Vec<_>>(),
            [("garden", "them-0000000001")]
        );
        assert_eq!(out.picked[0].standing_left_out, 2);
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
        // Within the room: one that does not fit is left out.
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

    #[test]
    fn one_that_does_not_fit_is_passed_and_one_too_long_is_cut() {
        let item = |node: &str| crate::stands::item_id("them", node, "memory:m", 1);
        let (long, job, mom) = (item("long"), item("job"), item("mom"));
        let out = run(json!({
            "ranked": [],
            "nodes": [
                {"id": "long", "name": "长", "standing": [
                    {"item": long, "at": 9, "text": "很".repeat(40)}
                ]},
                {"id": "job", "name": "工作", "standing": [
                    {"item": job, "at": 5, "text": "在宠物医疗公司做品牌"}
                ]},
                {"id": "mom", "name": "妈妈", "standing": [
                    {"item": mom, "at": 3, "text": "周六打电话"}
                ]}
            ],
            "budget": {"residentChars": 30}
        }));
        // The newest is longer than all the room: cut to half of it, 15
        // chars: 「长：」, 12 of its words and 「…」. 「工作：…」 (13) fits in
        // the 15 left; 「妈妈：周六打电话」 (8) does not.
        let shown: Vec<&str> = out.resident.iter().map(|one| one.item.as_str()).collect();
        assert_eq!(shown, [long.as_str(), job.as_str()]);
        assert_eq!(out.resident[0].text, format!("{}…", "很".repeat(12)));
        assert_eq!((out.resident_left_out, out.resident_cut), (1, 1));
        // After others, cut to the room they left: 50 chars, 「工作：…」 13,
        // then 「长」 to half (25): 「长：」, 22 words and 「…」; 12 left, and
        // 「妈妈：周六打电话」 fits in them.
        let after = |chars: usize| {
            run(json!({
                "ranked": [],
                "nodes": [
                    {"id": "job", "name": "工作", "standing": [
                        {"item": job, "at": 9, "text": "在宠物医疗公司做品牌"}
                    ]},
                    {"id": "long", "name": "长", "standing": [
                        {"item": long, "at": 5, "text": "很".repeat(60)}
                    ]},
                    {"id": "mom", "name": "妈妈", "standing": [
                        {"item": mom, "at": 3, "text": "周六打电话"}
                    ]}
                ],
                "budget": {"residentChars": chars}
            }))
        };
        let out = after(50);
        let texts: Vec<&str> = out.resident.iter().map(|one| one.text.as_str()).collect();
        let cut_words = format!("{}…", "很".repeat(22));
        assert_eq!(
            texts,
            ["在宠物医疗公司做品牌", cut_words.as_str(), "周六打电话"]
        );
        assert_eq!((out.resident_left_out, out.resident_cut), (0, 1));
        // Too little room left to keep 12 of its words: left out, not cut.
        // 28 chars: 13 for 「工作：…」, 15 left (and half is 14); 「长：」 and 12
        // words and 「…」 would take 15, more than 14.
        let out = after(28);
        assert_eq!((out.resident_left_out, out.resident_cut), (1, 0));
        assert_eq!(out.resident.len(), 2);
        // 30 chars: half is 15, room 17: 「长：」, 12 words and 「…」.
        let out = after(30);
        assert_eq!((out.resident_left_out, out.resident_cut), (1, 1));
        // With room for more, the one that does not fit is passed, not the
        // end of the list: 「工作：…」 (13) fits, 「长」 alone is 42, more
        // than the room left but not than all of it; 「妈妈：周六打电话」 (8)
        // fits after it.
        let out = run(json!({
            "ranked": [],
            "nodes": [
                {"id": "job", "name": "工作", "standing": [
                    {"item": job, "at": 9, "text": "在宠物医疗公司做品牌"}
                ]},
                {"id": "long", "name": "长", "standing": [
                    {"item": long, "at": 5, "text": "很".repeat(40)}
                ]},
                {"id": "mom", "name": "妈妈", "standing": [
                    {"item": mom, "at": 3, "text": "周六打电话"}
                ]}
            ],
            "budget": {"residentChars": 50}
        }));
        let shown: Vec<&str> = out.resident.iter().map(|one| one.item.as_str()).collect();
        assert_eq!(shown, [job.as_str(), mom.as_str()]);
        assert_eq!(out.resident[1].text, "周六打电话");
        assert_eq!((out.resident_left_out, out.resident_cut), (1, 0));
        // Past the count, the rest are left out and said so.
        let out = run(json!({
            "ranked": [],
            "nodes": [
                {"id": "job", "name": "工作", "standing": [
                    {"item": job, "at": 9, "text": "在宠物医疗公司做品牌"}
                ]},
                {"id": "mom", "name": "妈妈", "standing": [
                    {"item": mom, "at": 3, "text": "周六打电话"}
                ]}
            ],
            "budget": {"resident": 1}
        }));
        assert_eq!(out.resident.len(), 1);
        assert_eq!((out.resident_left_out, out.resident_cut), (1, 0));
    }

    #[test]
    fn what_is_due_stands_until_its_day_has_gone_by() {
        let item = |node: &str| crate::stands::item_id("them", node, "memory:m", 1);
        let (work, job) = (item("work"), item("job"));
        let rests = |from: &str, learned: &str| json!([{"from": from, "learned": learned}]);
        let out = |ranked: serde_json::Value, today: Option<&str>| {
            run(json!({
                "ranked": ranked,
                "nodes": [
                    {"id": "work", "name": "myriad", "entries": entries("w", 1), "standing": [
                        // A deadline: learned 11-13, due 11-14.
                        {"item": work, "at": 9, "text": "在做这一轮，定于 11-14 前做完",
                         "restsOn": rests("2026-11-14", "2026-11-13")}
                    ]},
                    {"id": "job", "name": "工作", "standing": [
                        // Begun before it was learned: never due.
                        {"item": job, "at": 5, "text": "在宠物医疗公司做品牌",
                         "restsOn": rests("2026-09-01", "2026-09-03")}
                    ]}
                ],
                "today": today
            }))
        };
        let found = || json!([{"id": "w1", "named": true}]);
        let every = |pick: &Pick| -> Vec<String> {
            pick.resident.iter().map(|one| one.item.clone()).collect()
        };
        // On its day, and without a day, it stands: above when its node is
        // picked, else every time.
        for today in [None, Some("2026-11-14")] {
            assert_eq!(
                out(found(), today).picked[0].standing,
                std::slice::from_ref(&work)
            );
            assert_eq!(
                every(&out(json!([]), today)),
                [work.clone(), job.clone()],
                "{today:?}"
            );
        }
        // The day after, it is no longer how things stand; the job still is.
        let today = Some("2026-11-15");
        assert!(out(found(), today).picked[0].standing.is_empty());
        // On their own node too.
        let about = |today: Option<&str>| {
            run(json!({
                "ranked": [],
                "nodes": [{"id": "you", "name": "你", "standing": [
                    {"item": work, "at": 9, "text": "在做这一轮，定于 11-14 前做完",
                     "restsOn": rests("2026-11-14", "2026-11-13")}
                ]}],
                "aboutThem": "you",
                "today": today
            }))
            .about_them_standing
        };
        assert_eq!(about(Some("2026-11-14")), std::slice::from_ref(&work));
        assert!(about(today).is_empty());
        assert_eq!(every(&out(json!([]), today)), std::slice::from_ref(&job));
    }

    #[test]
    fn query_matches_live_non_empty_nodes_by_name_or_alias_without_leaking_gone_or_empty() {
        let out = run(json!({
            "ranked": [],
            "query": "Did we go on any hikes or buy plants?",
            "nodes": [
                {"id": "n-hike", "name": "hiking", "entries": [{"id": "h1", "at": 10}]},
                {"id": "n-flora", "name": "botany", "aliases": ["plants"], "entries": [{"id": "p1", "at": 20}]},
                {"id": "n-gone", "name": "hiking club", "gone": true, "entries": [{"id": "g1", "at": 99}]},
                {"id": "n-empty", "name": "hike solo", "entries": [], "standing": [{"item": "them-0000000001", "at": 99, "text": "only standing"}]},
                {"id": "me", "name": "plants", "entries": [{"id": "m1", "at": 99}]},
            ],
            "aboutThem": "me"
        }));
        let picked_ids: Vec<&str> = out.picked.iter().map(|p| p.node.as_str()).collect();
        // Newest matched node comes first, excluding gone / name-only / own nodes.
        assert_eq!(picked_ids, ["n-flora", "n-hike"]);
        assert_eq!(out.picked[0].entries, ["p1"]);
        assert_eq!(out.picked[1].entries, ["h1"]);
        // CJK alias match too.
        let zh = run(json!({
            "ranked": [],
            "query": "我家里养了哪些绿植",
            "nodes": [{"id": "n-zh", "name": "家居", "aliases": ["绿植"], "entries": [{"id": "z1", "at": 5}]}]
        }));
        assert_eq!(zh.picked.len(), 1);
        assert_eq!(zh.picked[0].node, "n-zh");
        // Stopwords and stem/suffix collisions do not falsely match, while true
        // morphological variants (hobbies/hobby, exercises/exercise, sizes/size,
        // cookies/cookie, boxes/box) do match.
        let no_false = run(json!({
            "ranked": [],
            "query": "what time will I go and may not see parties cookies brownies?",
            "nodes": [
                {"id": "n-tim", "name": "Tim", "entries": [{"id": "e1", "at": 10}]},
                {"id": "n-notes", "name": "notes", "entries": [{"id": "e2", "at": 11}]},
                {"id": "n-will", "name": "Will", "entries": [{"id": "e3", "at": 12}]},
                {"id": "n-may", "name": "May", "entries": [{"id": "e4", "at": 13}]},
                {"id": "n-parts", "name": "parts", "entries": [{"id": "e5", "at": 14}]},
                {"id": "n-cook", "name": "cook", "entries": [{"id": "e6", "at": 15}]},
                {"id": "n-brown", "name": "Brown", "entries": [{"id": "e7", "at": 16}]}
            ]
        }));
        assert!(no_false.picked.is_empty(), "{:?}", no_false.picked);

        let morph = run(json!({
            "ranked": [],
            "query": "tell me about my hobbies exercises sizes cookies and boxes",
            "budget": {"picked": 5},
            "nodes": [
                {"id": "n-hobby", "name": "hobby", "entries": [{"id": "m1", "at": 10}]},
                {"id": "n-ex", "name": "exercise", "entries": [{"id": "m2", "at": 20}]},
                {"id": "n-size", "name": "size", "entries": [{"id": "m3", "at": 30}]},
                {"id": "n-cookie", "name": "cookie", "entries": [{"id": "m4", "at": 40}]},
                {"id": "n-box", "name": "box", "entries": [{"id": "m5", "at": 50}]}
            ]
        }));
        let morph_ids: Vec<&str> = morph.picked.iter().map(|p| p.node.as_str()).collect();
        assert_eq!(morph_ids, ["n-box", "n-cookie", "n-size", "n-ex", "n-hobby"]);

        // Ranking order: within active tier, entry-matched precedes query-matched;
        // query-matched active precedes resting.
        let ordered = run(json!({
            "ranked": [{"id": "e-active", "named": true}],
            "query": "camping",
            "nodes": [
                {"id": "n-rest", "name": "camping", "status": "resting", "entries": [{"id": "r1", "at": 99}]},
                {"id": "n-qact", "aliases": ["camping"], "entries": [{"id": "q1", "at": 80}]},
                {"id": "n-eact", "name": "other", "entries": [{"id": "e-active", "at": 10}]}
            ]
        }));
        let ordered_ids: Vec<&str> = ordered.picked.iter().map(|p| p.node.as_str()).collect();
        assert_eq!(ordered_ids, ["n-eact", "n-qact", "n-rest"]);
    }

    #[test]
    fn resident_suffix_chars_reserves_display_room_per_item() {
        let item = |n: &str| crate::stands::item_id("them", n, "memory:m", 1);
        let input = |suffix: usize| {
            json!({
                "ranked": [],
                "nodes": [
                    {"id": "n1", "name": "work", "standing": [{"item": item("n1"), "at": 9, "text": "builds software"}]},
                    {"id": "n2", "name": "home", "standing": [{"item": item("n2"), "at": 5, "text": "lives in tokyo"}]}
                ],
                "budget": {"residentChars": 45, "residentSuffixChars": suffix}
            })
        };
        // Without suffix allowance: 20 + 20 = 40 <= 45, both fit.
        let without = run(input(0));
        assert_eq!(without.resident.len(), 2);
        // With 14-char host date suffix allowance: (20+14)=34, second would need 68 > 45.
        let with_suffix = run(input(14));
        assert_eq!(with_suffix.resident.len(), 1);
        assert_eq!(with_suffix.resident_left_out, 1);
    }
}
