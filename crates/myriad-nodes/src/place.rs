//! Placing a talk: the pure half. When a stretch of talk (or an agent
//! session, or a stretch of the screen) comes to a pause, she cuts it into
//! spans, one thing each, and puts each span on a node or in the inbox.
//! Which node is hers to judge, by the model; what goes in and how its
//! answer is read are here, so the Swift and the Rust host send the same
//! thing and read the answer the same way. The prompt itself comes later.
//!
//! - [`prepare`] cuts the items and the candidate nodes to the limits in
//!   [`crate::limits`]: items in order, each cut to
//!   [`PLACE_ITEM_CHARS`], until [`PLACE_ITEMS`] or [`PLACE_TOTAL_CHARS`]
//!   (the first item always goes; `taken` says how many did, and the rest
//!   go in the next call); candidates in the order given, at most
//!   [`PLACE_CANDIDATES`], labelled `n1`, `n2`… so no id goes to the model.
//! - [`read`] reads the model's answer: `{"spans": [{"start": 1, "node":
//!   "n2", "gist": "…"}, …]}`, items numbered from 1. Each span runs from its
//!   start to the next one's; the first starts at 1; starts only go up;
//!   `node` is a label, or null for the inbox; the gist is her few words
//!   for the span, cut to [`PLACE_GIST_CHARS`]. A fenced or chatty answer is
//!   read from its first `{` to its last `}`. Anything else wrong and the
//!   whole stretch goes to the inbox as one span, with `fallback` saying
//!   why: nothing is lost, and nothing is guessed.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError, check_token,
    key::Kind,
    limits::{
        PLACE_CANDIDATES, PLACE_GIST_CHARS, PLACE_ITEM_CHARS, PLACE_ITEMS, PLACE_NAME_CHARS,
        PLACE_OUTPUT_BYTES, PLACE_SUMMARY_CHARS, PLACE_TOTAL_CHARS, cut,
    },
    tags::SaidBy,
};

/// One item of the talk: a message, an agent's step, a look at the screen.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Item {
    pub id: String,
    pub said_by: SaidBy,
    pub text: String,
}

/// A node it might go on, most likely first.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Candidate {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrepareInput {
    pub items: Vec<Item>,
    #[serde(default)]
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownItem {
    /// Its number for the model, from 1.
    pub n: usize,
    pub id: String,
    pub said_by: SaidBy,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offered {
    pub label: String,
    pub id: String,
    pub kind: Kind,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub items: Vec<ShownItem>,
    /// How many of the items given are in `items`: the first `taken`.
    pub taken: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Offered>,
}

/// Control characters (and, `flat`, line breaks) to spaces, trimmed.
fn clean(text: &str, flat: bool) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_control() && (flat || ch != '\n') {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn check_unique<'a>(what: &str, ids: impl Iterator<Item = &'a str>) -> Result<(), NodeError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        check_token(what, id)?;
        if !seen.insert(id) {
            return Err(NodeError::new(format!("{what}s repeat")));
        }
    }
    Ok(())
}

pub fn prepare(input: PrepareInput) -> Result<Prepared, NodeError> {
    if input.items.is_empty() {
        return Err(NodeError::new("no items to place"));
    }
    check_unique("item id", input.items.iter().map(|item| item.id.as_str()))?;
    check_unique(
        "candidate id",
        input
            .candidates
            .iter()
            .map(|candidate| candidate.id.as_str()),
    )?;
    let mut items = Vec::new();
    let mut total = 0;
    for item in input.items.into_iter().take(PLACE_ITEMS) {
        let text = cut(&clean(&item.text, false), PLACE_ITEM_CHARS);
        let chars = text.chars().count();
        if !items.is_empty() && total + chars > PLACE_TOTAL_CHARS {
            break;
        }
        total += chars;
        items.push(ShownItem {
            n: items.len() + 1,
            id: item.id,
            said_by: item.said_by,
            text,
        });
    }
    let candidates = input
        .candidates
        .into_iter()
        .take(PLACE_CANDIDATES)
        .enumerate()
        .map(|(at, candidate)| Offered {
            label: format!("n{}", at + 1),
            id: candidate.id,
            kind: candidate.kind,
            name: cut(&clean(&candidate.name, true), PLACE_NAME_CHARS),
            summary: cut(&clean(&candidate.summary, true), PLACE_SUMMARY_CHARS),
        })
        .collect();
    Ok(Prepared {
        taken: items.len(),
        items,
        candidates,
    })
}

/// A candidate as `read` needs it: its label and its id.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Label {
    pub label: String,
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadInput {
    /// The ids of the items the model saw, in order (`prepare`'s items).
    pub items: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<Label>,
    /// The model's answer, as it came.
    pub output: String,
}

/// One span: its items, the node it goes on (absent: the inbox), her gist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedSpan {
    pub items: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    pub gist: String,
}

/// Why the answer could not be read as it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    /// Longer than [`PLACE_OUTPUT_BYTES`].
    TooLong,
    /// No JSON object in it.
    NotJson,
    /// No `spans`, or none.
    NoSpans,
    /// More spans than items.
    TooManySpans,
    /// A start that is not a number, not from 1, not going up, or past the
    /// last item.
    BadStart,
    /// A node that is not one of the labels, or not text.
    UnknownNode,
    /// A span without her few words for it.
    NoGist,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Read {
    pub spans: Vec<PlacedSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Fallback>,
}

pub fn read(input: ReadInput) -> Result<Read, NodeError> {
    if input.items.is_empty() {
        return Err(NodeError::new("no items to place"));
    }
    check_unique("item id", input.items.iter().map(String::as_str))?;
    check_unique(
        "label",
        input.candidates.iter().map(|known| known.label.as_str()),
    )?;
    for known in &input.candidates {
        check_token("candidate id", &known.id)?;
    }
    let labels: BTreeMap<&str, &str> = input
        .candidates
        .iter()
        .map(|known| (known.label.as_str(), known.id.as_str()))
        .collect();
    Ok(match spans(&input.output, &input.items, &labels) {
        Ok(spans) => Read {
            spans,
            fallback: None,
        },
        Err(why) => Read {
            spans: vec![PlacedSpan {
                items: input.items,
                node: None,
                gist: String::new(),
            }],
            fallback: Some(why),
        },
    })
}

/// The JSON object in an answer: inside a fence, or from its first `{` to
/// its last `}`.
fn object_in(output: &str) -> Option<Value> {
    let text = output.trim();
    let text = match text.strip_prefix("```") {
        Some(fenced) => {
            let body = fenced.split_once('\n').map_or("", |(_, body)| body);
            body.trim_end().strip_suffix("```").unwrap_or(body)
        }
        None => text,
    };
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&text[start..=end]).ok()
}

fn spans(
    output: &str,
    items: &[String],
    labels: &BTreeMap<&str, &str>,
) -> Result<Vec<PlacedSpan>, Fallback> {
    if output.len() > PLACE_OUTPUT_BYTES {
        return Err(Fallback::TooLong);
    }
    let value = object_in(output).ok_or(Fallback::NotJson)?;
    let given = value
        .get("spans")
        .and_then(Value::as_array)
        .filter(|spans| !spans.is_empty())
        .ok_or(Fallback::NoSpans)?;
    if given.len() > items.len() {
        return Err(Fallback::TooManySpans);
    }
    let mut read: Vec<(usize, Option<String>, String)> = Vec::new();
    for span in given {
        let start = span
            .get("start")
            .and_then(Value::as_u64)
            .and_then(|start| usize::try_from(start).ok())
            .ok_or(Fallback::BadStart)?;
        let after = read.last().map_or(0, |(last, _, _)| *last);
        let first_is_one = !read.is_empty() || start == 1;
        if !first_is_one || start <= after || start > items.len() {
            return Err(Fallback::BadStart);
        }
        let node = match span.get("node") {
            None | Some(Value::Null) => None,
            Some(Value::String(label)) => Some(
                labels
                    .get(label.trim())
                    .ok_or(Fallback::UnknownNode)?
                    .to_string(),
            ),
            Some(_) => return Err(Fallback::UnknownNode),
        };
        let gist = span
            .get("gist")
            .and_then(Value::as_str)
            .map(|gist| clean(gist, true))
            .filter(|gist| !gist.is_empty())
            .ok_or(Fallback::NoGist)?;
        read.push((start, node, cut(&gist, PLACE_GIST_CHARS)));
    }
    let mut out = Vec::new();
    for (at, (start, node, gist)) in read.iter().enumerate() {
        let end = read
            .get(at + 1)
            .map_or(items.len(), |(next, _, _)| next - 1);
        out.push(PlacedSpan {
            items: items[start - 1..end].to_vec(),
            node: node.clone(),
            gist: gist.clone(),
        });
    }
    Ok(out)
}
