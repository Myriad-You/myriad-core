//! Listing all of a kind: asked "which ones" or "how many", code lists
//! what she keeps of the kind and she only judges which count, instead of
//! counting a few dozen lines of text herself.
//!
//! An item of a kind (a shirt to pick up, a project, a purchase) is an
//! entry on the topic of that kind, maybe with when it happened and where
//! it stands (`node.entry`'s `happened`, `state`, `open`). The recall
//! judgment names the stretch of time the question is about
//! (`myriad_merope::remembering`, a [`Range`]); [`range_dates`] makes days
//! of it from the host's now and her zone. [`list`] then goes through the
//! entries of the nodes of that kind:
//!
//! - **Once each**: an entry is one by the memory row it is the place of
//!   (or its own row, without one), so a fact on two nodes, or written
//!   again, is listed once, the newest kept. Two with the same text and the
//!   same words of theirs are one too. The same words alone are not: one
//!   sentence can tell of two things ("the shirt and the coat went to the
//!   cleaner's").
//! - **Newest first, up to [`LIMIT`]**: it looks at that many and says how
//!   many there are (`looked` of `of`), so she knows when not all are here.
//! - **In the stretch asked about**: an entry whose `happened` falls
//!   outside it is left out (`outside`); one only partly in it is kept and
//!   marked; one with no `happened` is kept and marked, for her to judge by
//!   what it says and the day she learned it.
//! - **In the state asked about** (`open`): an entry known to be the other
//!   way is left out (`otherState`); one with no `open` is kept.
//! - **In order**: by when it happened, else the day she learned it,
//!   oldest first.
//!
//! What was retired (replaced, faded, forgotten) is the host's to leave out
//! before calling, as for [`crate::recall::pick`]. Their words beside a fact
//! are the host's to give, only their own (a fact they told her); they go
//! beside it only when they add something, cut to
//! [`THEIR_WORDS_CHARS`]. The list comes with the section she reads before
//! she answers (`section`), the rule of answering from a list in it
//! ([`myriad_merope::speaking::LISTED_RULE`]).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use myriad_merope::happened::{Happened, Range, span_of, state_of};
use myriad_merope::speaking::{
    ListedFact, Listing, THEIR_WORDS_CHARS, format_listed_section, listed_line,
};
use serde::{Deserialize, Serialize};

use crate::{NodeError, body::MEMORY_REF, check_token, fold::Node, intents::Zone};

/// Entries looked at, at most, by default.
pub const LIMIT: usize = 80;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RangeInput {
    pub range: Range,
    /// Unix microseconds.
    pub now: i64,
    /// Her zone: an IANA name or `±hh:mm`.
    pub zone: String,
}

/// A range as days: her today, and the first and last day (either absent
/// when the range is open at that end).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeDates {
    pub today: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

/// The days of `range`, from `now` in her `zone`.
pub fn range_dates(input: RangeInput) -> Result<RangeDates, NodeError> {
    let today = Zone::parse(&input.zone)?.day(input.now)?;
    input.range.check().map_err(NodeError::new)?;
    let (from, to) = input
        .range
        .dates(today)
        .ok_or_else(|| NodeError::new("range does not hold"))?;
    Ok(RangeDates {
        today: today.to_string(),
        from: from.map(|day| day.to_string()),
        to: to.map(|day| day.to_string()),
    })
}

/// One entry of a node, as listing sees it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListEntry {
    /// The entry's id as the host shows it (its row's).
    pub id: String,
    /// When it was written (said), Unix microseconds.
    pub at: i64,
    pub text: String,
    /// The memory row it is the place of.
    #[serde(default)]
    pub memory: Option<String>,
    /// Their words it was taken from, when they are their own.
    #[serde(default)]
    pub quote: Option<String>,
    #[serde(default)]
    pub happened: Option<Happened>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub open: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNode {
    pub id: String,
    /// Forgotten, or gone into another node.
    #[serde(default)]
    pub gone: bool,
    #[serde(default)]
    pub entries: Vec<ListEntry>,
}

impl ListNode {
    /// A folded node as listing sees it: each entry with the memory row it
    /// is the place of (`memory:<id>`); their words are the host's to add.
    pub fn of(node: &Node) -> Self {
        Self {
            id: node.id.clone(),
            gone: node.forgotten || node.absorbed_into.is_some(),
            entries: node
                .entries
                .iter()
                .map(|entry| ListEntry {
                    id: entry.row.clone(),
                    at: entry.at,
                    text: entry.text.clone(),
                    memory: entry
                        .refs
                        .iter()
                        .find_map(|reference| reference.strip_prefix(MEMORY_REF))
                        .map(str::to_string),
                    quote: None,
                    happened: entry.happened.clone(),
                    state: entry.state.clone(),
                    open: entry.open,
                })
                .collect(),
        }
    }
}

fn limit() -> usize {
    LIMIT
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListInput {
    /// The nodes of the kind.
    pub nodes: Vec<ListNode>,
    /// Her zone, for the day each entry was said.
    pub zone: String,
    /// The first and last day asked about ([`range_dates`]); a month or a
    /// year is its first or last day. Neither: any time.
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    /// Only what is still open (true), or only what is over (false).
    #[serde(default)]
    pub open: Option<bool>,
    #[serde(default = "limit")]
    pub limit: usize,
}

/// How an entry falls in the stretch asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    In,
    Partly,
    /// It has no `happened`.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub node: String,
    pub text: String,
    /// The day it was said, in her zone.
    pub said: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub happened: Option<Happened>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open: Option<bool>,
    /// Their words, when they add something to the fact.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    /// Only when a stretch of time was asked about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fit: Option<Fit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listed {
    pub items: Vec<Item>,
    /// Entries looked at, once each.
    pub looked: usize,
    /// Entries she keeps of the kind, once each.
    pub of: usize,
    /// Left out: they happened outside the stretch asked about.
    pub outside: usize,
    /// Left out: they are known to be the other way from what was asked.
    pub other_state: usize,
    /// What she reads before she answers; absent when she keeps nothing of
    /// the kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

fn words(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn bound(day: Option<&str>, last: bool) -> Result<Option<NaiveDate>, NodeError> {
    day.map(|day| {
        span_of(day)
            .map(|(first, end)| if last { end } else { first })
            .ok_or_else(|| NodeError::new("from and to are a day, a month or a year"))
    })
    .transpose()
}

pub fn list(input: ListInput) -> Result<Listed, NodeError> {
    if input.limit == 0 {
        return Err(NodeError::new("limit is at least 1"));
    }
    let zone = Zone::parse(&input.zone)?;
    let from = bound(input.from.as_deref(), false)?;
    let to = bound(input.to.as_deref(), true)?;
    if let (Some(from), Some(to)) = (from, to)
        && to < from
    {
        return Err(NodeError::new("to comes before from"));
    }
    let ranged = from.is_some() || to.is_some();
    let mut nodes = BTreeSet::new();
    let mut all: Vec<(&str, &ListEntry)> = Vec::new();
    for node in &input.nodes {
        check_token("node id", &node.id)?;
        if !nodes.insert(node.id.as_str()) {
            return Err(NodeError::new("node ids repeat"));
        }
        for entry in &node.entries {
            check_token("entry id", &entry.id)?;
            if let Some(memory) = &entry.memory {
                check_token("memory", memory)?;
            }
            if !node.gone {
                all.push((&node.id, entry));
            }
        }
    }
    // Newest first; the same time, by id.
    all.sort_by(|(_, a), (_, b)| b.at.cmp(&a.at).then_with(|| a.id.cmp(&b.id)));
    // One thing: entries that share a memory row, or a text and their
    // words, however they chain; the newest of each stands for it.
    let mut first: BTreeMap<(bool, String, String), usize> = BTreeMap::new();
    let mut joined: Vec<usize> = (0..all.len()).collect();
    fn root(joined: &mut [usize], at: usize) -> usize {
        let mut at = at;
        while joined[at] != at {
            joined[at] = joined[joined[at]];
            at = joined[at];
        }
        at
    }
    for (at, (_, entry)) in all.iter().enumerate() {
        let quote = entry.quote.as_deref().map(words).unwrap_or_default();
        let mut keys = vec![(
            false,
            entry.memory.as_deref().unwrap_or(&entry.id).to_string(),
            String::new(),
        )];
        if !quote.is_empty() {
            keys.push((true, words(&entry.text), quote));
        }
        for key in keys {
            let other = *first.entry(key).or_insert(at);
            let (a, b) = (root(&mut joined, at), root(&mut joined, other));
            // The newer (earlier here) stands for both.
            joined[a.max(b)] = a.min(b);
        }
    }
    // Each thing as its newest says it, with what an older one knew and
    // the newest left out (when it happened, where it stands, their words).
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for at in 0..all.len() {
        groups.entry(root(&mut joined, at)).or_default().push(at);
    }
    let mut once: Vec<(&str, ListEntry)> = groups
        .into_values()
        .map(|members| {
            let (node, newest) = all[members[0]];
            let mut entry = newest.clone();
            let older = || members.iter().map(|at| all[*at].1);
            entry.happened = older().find_map(|one| {
                one.happened
                    .clone()
                    .filter(|happened| happened.check().is_ok())
            });
            if let Some(one) = older().find(|one| one.state.as_deref().and_then(state_of).is_some())
            {
                entry.state = one.state.clone();
                entry.open = one.open;
            }
            entry.quote =
                older().find_map(|one| one.quote.clone().filter(|quote| !words(quote).is_empty()));
            (node, entry)
        })
        .collect();
    let of = once.len();
    once.truncate(input.limit);
    let looked = once.len();
    let (mut outside, mut other_state) = (0, 0);
    let mut kept: Vec<(NaiveDate, i64, Item)> = Vec::new();
    for (node, entry) in &once {
        let said = zone.day(entry.at)?;
        let happened = entry
            .happened
            .clone()
            .filter(|happened| happened.check().is_ok());
        let span = happened.as_ref().and_then(Happened::span);
        let fit = match (ranged, span) {
            (false, _) => None,
            (true, None) => Some(Fit::Unknown),
            (true, Some((first, last))) => {
                let starts = from.unwrap_or(NaiveDate::MIN);
                let ends = to.unwrap_or(NaiveDate::MAX);
                if last < starts || first > ends {
                    outside += 1;
                    continue;
                }
                Some(if first >= starts && last <= ends {
                    Fit::In
                } else {
                    Fit::Partly
                })
            }
        };
        let state = entry.state.as_deref().and_then(state_of);
        let open = entry.open.filter(|_| state.is_some());
        if input.open.is_some() && open.is_some() && open != input.open {
            other_state += 1;
            continue;
        }
        let text = words(&entry.text);
        let quote = entry
            .quote
            .as_deref()
            .map(words)
            .filter(|quote| !quote.is_empty() && !text.contains(quote.as_str()))
            .map(|quote| quote.chars().take(THEIR_WORDS_CHARS).collect());
        kept.push((
            span.map_or(said, |(first, _)| first),
            entry.at,
            Item {
                id: entry.id.clone(),
                node: node.to_string(),
                text,
                said: said.to_string(),
                memory: entry.memory.clone(),
                happened,
                state,
                open,
                quote,
                fit,
            },
        ));
    }
    kept.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then_with(|| a.2.id.cmp(&b.2.id))
    });
    let items: Vec<Item> = kept.into_iter().map(|(_, _, item)| item).collect();
    let lines: Vec<String> = items
        .iter()
        .map(|item| {
            listed_line(&ListedFact {
                said: &item.said,
                text: &item.text,
                happened: item.happened.as_ref().map(Happened::shown),
                state: item.state.as_deref(),
                open: item.open,
                partly: item.fit == Some(Fit::Partly),
                time_unknown: item.fit == Some(Fit::Unknown),
                their_words: item.quote.as_deref(),
            })
        })
        .collect();
    let range = match (from, to) {
        (Some(from), Some(to)) => Some(format!("between {from} and {to}")),
        (Some(from), None) => Some(format!("since {from}")),
        (None, Some(to)) => Some(format!("up to {to}")),
        (None, None) => None,
    };
    let section = format_listed_section(&Listing {
        lines: &lines,
        range,
        open: input.open,
        looked,
        of,
        outside,
        other_state,
    });
    Ok(Listed {
        items,
        looked,
        of,
        outside,
        other_state,
        section,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const DAY: i64 = 86_400_000_000;
    /// 2026-10-01T00:00Z.
    const OCT_1: i64 = 1_790_812_800_000_000;

    fn run(input: Value) -> Listed {
        list(serde_json::from_value(input).unwrap()).unwrap()
    }

    fn ids(listed: &Listed) -> Vec<&str> {
        listed.items.iter().map(|item| item.id.as_str()).collect()
    }

    #[test]
    fn each_is_listed_once_by_its_memory_or_by_text_and_words() {
        let out = run(json!({
            "zone": "+00:00",
            "nodes": [
                {"id": "clothes", "entries": [
                    {"id": "e1", "at": OCT_1, "text": "送洗衬衫", "memory": "m1", "quote": "衬衫和外套都送去干洗了"},
                    {"id": "e2", "at": OCT_1 + DAY, "text": "送洗外套", "memory": "m2", "quote": "衬衫和外套都送去干洗了"},
                    {"id": "e3", "at": OCT_1 + 2 * DAY, "text": "送洗衬衫", "memory": "m3", "quote": "衬衫和外套都送去干洗了"}
                ]},
                {"id": "errands", "entries": [
                    {"id": "e4", "at": OCT_1 + 3 * DAY, "text": "送洗衬衫 ", "memory": "m1"}
                ]},
                {"id": "old", "gone": true, "entries": [
                    {"id": "e5", "at": OCT_1, "text": "退了鞋", "memory": "m5"}
                ]}
            ]
        }));
        // m1 twice: the newest stays. m3 says what m1 said from the same
        // words; m2 is another thing from them.
        assert_eq!(ids(&out), ["e2", "e4"]);
        assert_eq!(out.items[1].memory.as_deref(), Some("m1"));
        // The newest of m1 has no words of theirs; an older one does.
        assert_eq!(
            out.items[1].quote.as_deref(),
            Some("衬衫和外套都送去干洗了")
        );
        assert_eq!((out.looked, out.of), (2, 2));
        assert_eq!(out.items[1].node, "errands");
        assert_eq!(
            out.items[0].quote.as_deref(),
            Some("衬衫和外套都送去干洗了")
        );
        let section = out.section.unwrap();
        assert!(section.contains("Code went through all 2 facts you keep of this kind."));
        assert!(
            section.contains("- [2026-10-02] 送洗外套 (their words: \"衬衫和外套都送去干洗了\")")
        );
    }

    #[test]
    fn a_stretch_of_time_leaves_out_what_fell_outside_it_and_marks_the_rest() {
        let out = run(json!({
            "zone": "Asia/Shanghai",
            "from": "2026-10-01",
            "to": "2026-10-31",
            "nodes": [{"id": "clothes", "entries": [
                {"id": "sep", "at": OCT_1, "text": "九月买的外套", "happened": {"from": "2026-09-20"}},
                {"id": "oct", "at": OCT_1 + 5 * DAY, "text": "取了衬衫", "happened": {"from": "2026-10-03"}, "state": "已取", "open": false},
                {"id": "edge", "at": OCT_1 + 6 * DAY, "text": "出差", "happened": {"from": "2026-09-28", "to": "2026-10-02"}},
                {"id": "none", "at": OCT_1 + 7 * DAY, "text": "退了一双鞋", "state": "已退"},
                {"id": "open", "at": OCT_1 + 8 * DAY, "text": "裤子待取", "happened": {"from": "2026-10-08", "about": true}, "state": "待取", "open": true}
            ]}]
        }));
        assert_eq!(out.outside, 1);
        // By when it happened, else the day it was said.
        assert_eq!(ids(&out), ["edge", "oct", "none", "open"]);
        let fit: Vec<Option<Fit>> = out.items.iter().map(|item| item.fit).collect();
        assert_eq!(
            fit,
            [
                Some(Fit::Partly),
                Some(Fit::In),
                Some(Fit::Unknown),
                Some(Fit::In)
            ]
        );
        // Said at 2026-10-08T00:00Z: the 8th in Shanghai.
        assert_eq!(out.items[2].said, "2026-10-08");
        let section = out.section.unwrap();
        assert!(section.contains("for what happened between 2026-10-01 and 2026-10-31"));
        assert!(section.contains("Left out: 1 that happened outside that time."));
        assert!(section.contains("退了一双鞋 (when it happened is not known; now 已退)"));
        assert!(section.contains("裤子待取 (happened around 2026-10-08; now 待取, still open)"));
        assert!(section.contains("only partly in that time"));
        // Only what is still open: the one over is left out, the one with
        // no open stays.
        let open = run(json!({
            "zone": "+08:00",
            "open": true,
            "nodes": [{"id": "clothes", "entries": [
                {"id": "a", "at": OCT_1, "text": "取了衬衫", "state": "已取", "open": false},
                {"id": "b", "at": OCT_1, "text": "裤子待取", "state": "待取", "open": true},
                {"id": "c", "at": OCT_1, "text": "鞋"}
            ]}]
        }));
        assert_eq!(ids(&open), ["b", "c"]);
        assert_eq!(open.other_state, 1);
        assert!(open.items.iter().all(|item| item.fit.is_none()));
    }

    #[test]
    fn past_the_limit_it_says_how_many_it_did_not_look_at() {
        let entries: Vec<Value> = (0..5)
            .map(|n| json!({"id": format!("e{n}"), "at": OCT_1 + n * DAY, "text": format!("第{n}件")}))
            .collect();
        let out =
            run(json!({"zone": "+00:00", "limit": 3, "nodes": [{"id": "t", "entries": entries}]}));
        assert_eq!(ids(&out), ["e2", "e3", "e4"]);
        assert_eq!((out.looked, out.of), (3, 5));
        assert!(out.section.unwrap().contains("the newest 3 of the 5 facts"));
        let none = run(json!({"zone": "+00:00", "nodes": []}));
        assert_eq!((none.of, none.section), (0, None));
        for bad in [
            json!({"zone": "+00:00", "limit": 0, "nodes": []}),
            json!({"zone": "nowhere zone", "nodes": []}),
            json!({"zone": "+00:00", "from": "2026-10-31", "to": "2026-10-01", "nodes": []}),
            json!({"zone": "+00:00", "from": "last month", "nodes": []}),
            json!({"zone": "+00:00", "nodes": [{"id": "t"}, {"id": "t"}]}),
        ] {
            assert!(
                list(serde_json::from_value(bad.clone()).unwrap()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_range_is_made_days_in_her_zone() {
        // 2026-10-31T20:00Z is already November in Tokyo.
        let now = OCT_1 + 30 * DAY + 20 * 3_600_000_000;
        let dates = |zone: &str| {
            range_dates(
                serde_json::from_value(json!({
                    "range": {"how": "previous", "unit": "month", "count": 1},
                    "now": now,
                    "zone": zone
                }))
                .unwrap(),
            )
            .unwrap()
        };
        let utc = dates("+00:00");
        assert_eq!(
            (utc.today.as_str(), utc.from.as_deref()),
            ("2026-10-31", Some("2026-09-01"))
        );
        let tokyo = dates("Asia/Tokyo");
        assert_eq!(tokyo.today, "2026-11-01");
        assert_eq!(
            (tokyo.from.as_deref(), tokyo.to.as_deref()),
            (Some("2026-10-01"), Some("2026-10-31"))
        );
        assert!(
            range_dates(
                serde_json::from_value(json!({"range": {"how": "past", "unit": "day", "count": 0}, "now": now, "zone": "+00:00"}))
                    .unwrap()
            )
            .is_err()
        );
    }
}
