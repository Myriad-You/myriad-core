//! Her upkeep of one node: the pure half. When the queue ([`crate::queue`])
//! hands out a node, she looks at it with what is one relation away and
//! the inbox, and judges: the two judgments (does a part have a state of
//! its own; must it be done before its whole is) and what follows from
//! them. The judging is hers, by the model, in her words; here is only what
//! she is shown and how her answer becomes rows, so the Swift and the Rust
//! host write the same rows from the same answer.
//!
//! - [`prepare`] folds the rows and shows her the node (its head, its newest
//!   spans, what the partner set, what was changed on both sides), its
//!   neighbours one relation away, and the inbox, each cut to the limits in
//!   [`crate::limits`] and labelled (`n1` the node, `n2`… neighbours, `s1`…
//!   its spans, `i1`… inbox items) so no id goes to the model.
//! - [`read`] reads her answer, `{"actions": [{"do": …}, …]}`, into the rows
//!   to write, in order, and last the `node.upkept` mark. What each action
//!   writes:
//!
//! | `do` | what she says | rows |
//! | --- | --- | --- |
//! | `create` | a new node `as` (`new…`), `name`, `done?`, `ongoing?`, `because` (立节点) | `node.created`; `node.done_when` with a done; status `ongoing` |
//! | `split` | a part `as` with its `spans`, `name`, `done?`, `ongoing?`, `reason`, `because` (拆分) | as `create`; `node.split_off` on this node; `part_of` from the part |
//! | `place` | inbox `items` onto `node`, in her `text` (收件箱归位) | `node.span`, its refs the items |
//! | `absorb` | this node `into` a neighbour, `how` `merged` or `folded_back`, `reason`, `because` (并入、收回) | `node.absorbed`; its open items, waiting intents and relations again on the other |
//! | `relate` | `from?` (this node) → `to`, `relation`, `reason`, `because` (挂两处 is a second `part_of`) | `node.related` |
//! | `unrelate` | `from?` → `to`, `relation`, `reason` (独立 is a `part_of` taken back) | `node.unrelated` on each node holding it |
//! | `kind` | `done?` and `ongoing?`, `reason`, `because` (进行中 ↔ 话题) | `node.done_when`, `node.kind_changed`, status, as they change |
//! | `status` | `status` (`active`, `resting`, `done`, `dropped`, `ongoing`) | `node.revised` |
//! | `hand` | `text`, `because`: a hand raised to the partner | `node.intent`, waiting |
//!
//! The kind follows from her `done` alone: with a "done" it is something in
//! progress; without one it is a topic, unless it is `ongoing` (always being
//! done, 每周发版), which is something in progress whose status is ongoing.
//!
//! What the partner set stands: an action that would change a head field
//! whose current version is the partner's, take back a relation the
//! partner made, or merge away a node whose own row, relation, open item or
//! waiting intent is the partner's (a merge writes those again, as hers) is
//! held (`held`), and she may only raise a hand. Every other action of the
//! answer stands.
//!
//! A `part_of` she writes is checked as the fold will check it: the fold's
//! rule (in log order, one that would close a loop is passed by) replayed
//! over every `part_of` row it weighs, with what the answer takes back and
//! adds. Each row to write comes with its id, from the run's token; the
//! mark lists them, so the queue knows what the run wrote and nothing else
//! written meanwhile is taken as seen.
//!
//! An answer that cannot be read as it stands changes nothing: no row but
//! the `node.upkept` mark, which says why (`fallback`) so the queue moves
//! on; the node is due again at its next change.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::row::Author;
use myriad_persona_sync::{Rev, canonical::sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{
        Absorbed, Body, Created, DoneWhen, Field, How, Intent, IntentStatus, KindChanged, OpenItem,
        Related, Relation, Revised, Span, SplitOff, Status, Unrelated, Upkept,
    },
    check_token,
    fold::{FoldInput, Full, Item, Node, Parts, fold_full},
    key::{Key, Kind, node_id},
    limits::{
        BECAUSE_IDS, NAME_CHARS, PLACE_SUMMARY_CHARS, REASON_CHARS, REFS, SEEN_DEVICES, TEXT_CHARS,
        UPKEEP_ACTIONS, UPKEEP_HEAD_ITEMS, UPKEEP_INBOX, UPKEEP_INBOX_TOTAL_CHARS,
        UPKEEP_NEIGHBOURS, UPKEEP_OUTPUT_BYTES, UPKEEP_SPAN_CHARS, UPKEEP_SPANS,
        UPKEEP_TOTAL_CHARS, UPKEEP_WRITES, cut,
    },
    place::{clean, object_in},
    queue::{Reason, Watch},
    tags::{SaidBy, Sync, check_said_by},
};

/// One inbox item of this audience: what was not on a node yet.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InboxItem {
    /// `<type>:<id>`: what a span placed from it refers to.
    pub id: String,
    pub said_by: Vec<SaidBy>,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpkeepInput {
    /// The rows of the node and of the nodes around it, as for the fold.
    pub rows: Vec<myriad_persona_log::Row>,
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    pub audience: String,
    /// The node she keeps.
    pub node: String,
    /// Inbox items of this audience, oldest first.
    #[serde(default)]
    pub inbox: Vec<InboxItem>,
}

/// A head field, as the partner may have set it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Head {
    Kind,
    Name,
    Summary,
    Status,
    DoneWhen,
    WhoKnows,
    Object,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownText {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownIntent {
    pub text: String,
    pub status: IntentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownNode {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_when: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub summary: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub open: Vec<ShownText>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub intents: Vec<ShownIntent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ShownText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Parts>,
    /// The partner made this node: only a hand against merging it away.
    #[serde(skip_serializing_if = "is_false")]
    pub made_by_partner: bool,
    /// Head fields the partner set: only a hand against them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub partner_set: Vec<Head>,
    /// Head fields two sides changed without seeing each other: hers to
    /// settle.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub both_changed: Vec<Head>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownSpan {
    pub label: String,
    pub said_by: Vec<SaidBy>,
    pub text: String,
}

/// Which way a relation runs from the node she keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Toward {
    /// From the node she keeps to the neighbour.
    Out,
    /// From the neighbour to the node she keeps.
    In,
    /// Either way (撞上, 相关).
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownRelation {
    pub relation: Relation,
    pub toward: Toward,
    #[serde(skip_serializing_if = "is_false")]
    pub by_partner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownNeighbour {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub status: Status,
    /// Its summary's first sentence, cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gist: Option<String>,
    pub relations: Vec<ShownRelation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownInbox {
    pub label: String,
    pub said_by: Vec<SaidBy>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Taken {
    /// Its newest spans shown, of all it has.
    pub spans: usize,
    /// The first inbox items shown, of those given.
    pub inbox: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub node: ShownNode,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<ShownSpan>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub neighbours: Vec<ShownNeighbour>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inbox: Vec<ShownInbox>,
    /// Why it is due now.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub due: Vec<Reason>,
    pub taken: Taken,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn by_partner<T>(item: &Option<Item<T>>) -> bool {
    item.as_ref().is_some_and(|item| {
        item.current.author == Author::Owner
            || item
                .rivals
                .iter()
                .any(|rival| rival.author == Author::Owner)
    })
}

fn status_of(node: &Node) -> Status {
    node.status
        .as_ref()
        .map_or(Status::Active, |status| status.current.value)
}

/// What one upkeep call stands on: the fold and the labels.
struct Context {
    full: Full,
    audience: String,
    /// Index of the node she keeps in the fold.
    at: usize,
    /// Label → index in the fold: `n1` the node, `n2`… its neighbours.
    nodes: BTreeMap<String, usize>,
    /// `s…` → span row.
    spans: BTreeMap<String, String>,
    /// `i…` → index in the inbox.
    inbox: BTreeMap<String, usize>,
    items: Vec<InboxItem>,
    prepared: Prepared,
}

fn context(input: UpkeepInput) -> Result<Context, NodeError> {
    check_token("node", &input.node)?;
    let mut seen = BTreeSet::new();
    for item in &input.inbox {
        check_token("inbox id", &item.id)?;
        if myriad_persona_log::row::object_type(&item.id).is_none() {
            return Err(NodeError::new("an inbox id is <type>:<id>"));
        }
        if !seen.insert(item.id.as_str()) {
            return Err(NodeError::new("inbox ids repeat"));
        }
        check_said_by(&item.said_by)?;
    }
    let audience = input.audience;
    check_token("audience", &audience)?;
    myriad_persona_log::row::Audience::parse(&audience)
        .map_err(|_| NodeError::new("audience is not personal:, own: or group:"))?;
    let full = fold_full(FoldInput {
        rows: input.rows,
        bodies: input.bodies,
    })?;
    let at = full
        .folded
        .nodes
        .iter()
        .position(|node| node.audience == audience && node.id == input.node)
        .ok_or_else(|| NodeError::new("the node is not in the rows given"))?;
    let node = &full.folded.nodes[at];
    if node.forgotten || node.key.is_none() || node.absorbed_into.is_some() {
        return Err(NodeError::new(
            "the node is forgotten, gone into another, or not created yet",
        ));
    }

    let mut partner_set = Vec::new();
    let mut both_changed = Vec::new();
    let mut head = |field: Head, partner: bool, both: bool| {
        if partner {
            partner_set.push(field);
        }
        if both {
            both_changed.push(field);
        }
    };
    let both = |changed: Option<bool>| changed.unwrap_or(false);
    head(
        Head::Kind,
        by_partner(&node.kind),
        both(node.kind.as_ref().map(|i| i.both_changed)),
    );
    head(
        Head::Name,
        by_partner(&node.name),
        both(node.name.as_ref().map(|i| i.both_changed)),
    );
    head(
        Head::Summary,
        by_partner(&node.summary),
        both(node.summary.as_ref().map(|i| i.both_changed)),
    );
    head(
        Head::Status,
        by_partner(&node.status),
        both(node.status.as_ref().map(|i| i.both_changed)),
    );
    head(
        Head::DoneWhen,
        by_partner(&node.done_when),
        both(node.done_when.as_ref().map(|i| i.both_changed)),
    );
    head(
        Head::WhoKnows,
        by_partner(&node.who_knows),
        both(node.who_knows.as_ref().map(|i| i.both_changed)),
    );
    head(
        Head::Object,
        by_partner(&node.object),
        both(node.object.as_ref().map(|i| i.both_changed)),
    );
    let shown_text = |text: &str| cut(&clean(text, false), UPKEEP_SPAN_CHARS);
    let shown = ShownNode {
        label: "n1".to_string(),
        kind: node.kind.as_ref().map(|kind| kind.current.value.kind),
        name: node.name.as_ref().map(|name| name.current.value.clone()),
        status: status_of(node),
        done_when: node
            .done_when
            .as_ref()
            .and_then(|done| done.current.value.as_deref().map(shown_text)),
        summary: node
            .summary
            .as_ref()
            .map(|summary| {
                summary
                    .current
                    .value
                    .iter()
                    .map(|sentence| sentence.text.clone())
                    .collect()
            })
            .unwrap_or_default(),
        open: node
            .open
            .iter()
            .filter(|(_, item)| !item.current.value.closed)
            .take(UPKEEP_HEAD_ITEMS)
            .map(|(_, item)| ShownText {
                text: shown_text(&item.current.value.text),
            })
            .collect(),
        intents: node
            .intents
            .iter()
            .filter(|(_, intent)| {
                matches!(
                    intent.current.value.status,
                    IntentStatus::Waiting | IntentStatus::Said
                )
            })
            .take(UPKEEP_HEAD_ITEMS)
            .map(|(_, intent)| ShownIntent {
                text: shown_text(&intent.current.value.text),
                status: intent.current.value.status,
            })
            .collect(),
        views: node
            .views
            .iter()
            .take(UPKEEP_HEAD_ITEMS)
            .map(|(_, view)| ShownText {
                text: shown_text(&view.current.value.text),
            })
            .collect(),
        parts: node.parts,
        made_by_partner: node.created_by == Some(Author::Owner),
        partner_set,
        both_changed,
    };

    // Its newest spans, shown oldest first.
    let mut spans = Vec::new();
    let mut total = 0;
    for span in node.spans.iter().rev().take(UPKEEP_SPANS) {
        let text = shown_text(&span.text);
        let chars = text.chars().count();
        if !spans.is_empty() && total + chars > UPKEEP_TOTAL_CHARS {
            break;
        }
        total += chars;
        spans.push((span.row.clone(), span.said_by.clone(), text));
    }
    spans.reverse();
    let mut span_labels = BTreeMap::new();
    let shown_spans: Vec<ShownSpan> = spans
        .into_iter()
        .enumerate()
        .map(|(n, (row, said_by, text))| {
            let label = format!("s{}", n + 1);
            span_labels.insert(label.clone(), row);
            ShownSpan {
                label,
                said_by,
                text,
            }
        })
        .collect();

    // One relation away.
    let index: BTreeMap<&str, usize> = full
        .folded
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, other)| {
            other.audience == audience
                && !other.forgotten
                && other.key.is_some()
                && other.absorbed_into.is_none()
        })
        .map(|(at, other)| (other.id.as_str(), at))
        .collect();
    let mut around: BTreeMap<usize, Vec<ShownRelation>> = BTreeMap::new();
    for relation in &full.folded.relations {
        if relation.audience != audience {
            continue;
        }
        let (other, toward) = if relation.from == node.id {
            (&relation.to, Toward::Out)
        } else if relation.to == node.id {
            (&relation.from, Toward::In)
        } else {
            continue;
        };
        let Some(&other) = index.get(other.as_str()) else {
            continue;
        };
        around.entry(other).or_default().push(ShownRelation {
            relation: relation.relation,
            toward: if relation.relation.symmetric() {
                Toward::Both
            } else {
                toward
            },
            by_partner: relation.author == Author::Owner,
        });
    }
    let mut around: Vec<(usize, Vec<ShownRelation>)> = around
        .into_iter()
        .map(|(other, mut relations)| {
            relations.sort_by_key(|shown| (shown.relation, shown.toward));
            relations.dedup();
            (other, relations)
        })
        .collect();
    around.sort_by(|(a, ra), (b, rb)| {
        let first = |relations: &[ShownRelation]| (relations[0].relation, relations[0].toward);
        (first(ra), &full.folded.nodes[*a].id).cmp(&(first(rb), &full.folded.nodes[*b].id))
    });
    around.truncate(UPKEEP_NEIGHBOURS);
    let mut nodes = BTreeMap::from([("n1".to_string(), at)]);
    let neighbours: Vec<ShownNeighbour> = around
        .into_iter()
        .enumerate()
        .map(|(n, (other, relations))| {
            let label = format!("n{}", n + 2);
            nodes.insert(label.clone(), other);
            let other = &full.folded.nodes[other];
            ShownNeighbour {
                label,
                kind: other.kind.as_ref().map(|kind| kind.current.value.kind),
                name: other.name.as_ref().map(|name| name.current.value.clone()),
                status: status_of(other),
                gist: other.summary.as_ref().and_then(|summary| {
                    summary
                        .current
                        .value
                        .first()
                        .map(|sentence| cut(&clean(&sentence.text, true), PLACE_SUMMARY_CHARS))
                }),
                relations,
            }
        })
        .collect();

    // The inbox, oldest first.
    let mut inbox = Vec::new();
    let mut inbox_labels = BTreeMap::new();
    let mut total = 0;
    for (n, item) in input.inbox.iter().enumerate().take(UPKEEP_INBOX) {
        let text = shown_text(&item.text);
        let chars = text.chars().count();
        if !inbox.is_empty() && total + chars > UPKEEP_INBOX_TOTAL_CHARS {
            break;
        }
        total += chars;
        let label = format!("i{}", n + 1);
        inbox_labels.insert(label.clone(), n);
        inbox.push(ShownInbox {
            label,
            said_by: item.said_by.clone(),
            text,
        });
    }
    let due = Watch::new(&full)
        .due(&audience, &node.id)
        .map(|due| due.why)
        .unwrap_or_default();
    let prepared = Prepared {
        node: shown,
        taken: Taken {
            spans: shown_spans.len(),
            inbox: inbox.len(),
        },
        spans: shown_spans,
        neighbours,
        inbox,
        due,
    };
    Ok(Context {
        full,
        audience,
        at,
        nodes,
        spans: span_labels,
        inbox: inbox_labels,
        items: input.inbox,
        prepared,
    })
}

pub fn prepare(input: UpkeepInput) -> Result<Prepared, NodeError> {
    Ok(context(input)?.prepared)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadInput {
    /// What `prepare` was given.
    pub input: UpkeepInput,
    /// This run's token, minted once by the caller: new keys and ids come
    /// from it, so one answer writes the same rows on every host.
    pub run: String,
    /// The sync tag of the rows she writes.
    pub sync: Sync,
    /// The model's answer, as it came.
    pub output: String,
}

/// One row to write: its id, its node, its kind, its body. The caller
/// writes it under this id (the mark lists the ids, so the queue knows what
/// the run wrote), adds the rest of the envelope (clock, device), and keeps
/// the order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Write {
    pub id: String,
    pub node: String,
    pub kind: String,
    pub body: Value,
}

/// Why an action was held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeldWhy {
    /// The partner set it: only a hand.
    PartnerSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Held {
    /// The action's place in the answer, from 1.
    pub action: usize,
    pub why: HeldWhy,
}

/// Why an answer could not be read as it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    /// Longer than [`UPKEEP_OUTPUT_BYTES`].
    TooLong,
    /// No JSON object in it.
    NotJson,
    /// No `actions` list.
    NoActions,
    /// More than [`UPKEEP_ACTIONS`].
    TooManyActions,
    /// A `do` that is none of the actions.
    UnknownAction,
    /// A node, span or inbox label she was not shown, or a new one twice.
    UnknownLabel,
    /// A field missing, empty, or not what it should be.
    BadField,
    /// No evidence, or evidence she was not shown.
    NoEvidence,
    /// A `part_of` that would close a loop.
    Cycle,
    /// Something the rules never allow: a person split or merged away, a
    /// kind its key does not have, a span on 「关于你」, an item placed
    /// twice, a field written twice, folding back into what it is not part
    /// of, taking back a relation that is not there, a new node with nothing
    /// on it.
    NotAllowed,
}

impl Fallback {
    fn name(self) -> &'static str {
        match self {
            Self::TooLong => "too_long",
            Self::NotJson => "not_json",
            Self::NoActions => "no_actions",
            Self::TooManyActions => "too_many_actions",
            Self::UnknownAction => "unknown_action",
            Self::UnknownLabel => "unknown_label",
            Self::BadField => "bad_field",
            Self::NoEvidence => "no_evidence",
            Self::Cycle => "cycle",
            Self::NotAllowed => "not_allowed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpkeepRead {
    /// In order; the last is always the `node.upkept` mark.
    pub writes: Vec<Write>,
    /// Inbox items now on a node, sorted: the caller takes them out.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub placed: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub held: Vec<Held>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Fallback>,
}

/// A node an answer can name: one in the fold, or one it creates.
#[derive(Debug, Clone)]
struct Named {
    id: String,
    key: String,
    /// Index in the fold; none for a new node.
    at: Option<usize>,
}

/// Reading one answer: the context, and what it has written so far.
struct Reader<'a> {
    context: &'a Context,
    run: &'a str,
    sync: Sync,
    writes: Vec<Write>,
    held: Vec<Held>,
    placed: BTreeSet<String>,
    news: BTreeMap<String, Named>,
    used: BTreeSet<String>,
    written: BTreeSet<(String, &'static str)>,
    /// Every `part_of` row the fold weighs, in log order, as (from, to,
    /// row), with what the answer adds and takes back: the fold's rule is
    /// replayed over it, so what stands here is what will stand there.
    part_of: Vec<(String, String, String)>,
}

fn field<'v>(action: &'v Value, name: &str) -> Option<&'v Value> {
    action.get(name).filter(|value| !value.is_null())
}

fn text(action: &Value, name: &str, max: usize, flat: bool) -> Result<String, Fallback> {
    let text = field(action, name)
        .and_then(Value::as_str)
        .map(|text| clean(text, flat))
        .filter(|text| !text.is_empty())
        .ok_or(Fallback::BadField)?;
    Ok(cut(&text, max))
}

fn optional_text(action: &Value, name: &str, max: usize) -> Result<Option<String>, Fallback> {
    match field(action, name) {
        None => Ok(None),
        Some(_) => text(action, name, max, false).map(Some),
    }
}

fn flag(action: &Value, name: &str) -> Result<bool, Fallback> {
    match field(action, name) {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(Fallback::BadField),
    }
}

fn labels(action: &Value, name: &str) -> Result<Vec<String>, Fallback> {
    let list = field(action, name)
        .and_then(Value::as_array)
        .ok_or(Fallback::BadField)?;
    list.iter()
        .map(|label| {
            label
                .as_str()
                .map(|label| label.trim().to_string())
                .ok_or(Fallback::BadField)
        })
        .collect()
}

fn enum_of<T: serde::de::DeserializeOwned>(action: &Value, name: &str) -> Result<T, Fallback> {
    field(action, name)
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .ok_or(Fallback::BadField)
}

fn minted(seed: &str) -> String {
    sha256_hex(seed.as_bytes())[..24].to_string()
}

fn kind_for(done: &Option<String>, ongoing: bool) -> Kind {
    if done.is_some() || ongoing {
        Kind::Task
    } else {
        Kind::Topic
    }
}

impl<'a> Reader<'a> {
    fn new(context: &'a Context, run: &'a str, sync: Sync) -> Self {
        let part_of = context
            .full
            .part_of
            .iter()
            .filter(|(audience, ..)| *audience == context.audience)
            .map(|(_, from, to, row)| (from.clone(), to.clone(), row.clone()))
            .collect();
        Self {
            context,
            run,
            sync,
            writes: Vec::new(),
            held: Vec::new(),
            placed: BTreeSet::new(),
            news: BTreeMap::new(),
            used: BTreeSet::new(),
            written: BTreeSet::new(),
            part_of,
        }
    }

    fn node(&self, at: usize) -> &'a Node {
        &self.context.full.folded.nodes[at]
    }

    fn me(&self) -> &'a Node {
        self.node(self.context.at)
    }

    fn named(&mut self, label: &str) -> Result<Named, Fallback> {
        let label = label.trim();
        if let Some(named) = self.news.get(label) {
            self.used.insert(label.to_string());
            return Ok(named.clone());
        }
        let at = *self
            .context
            .nodes
            .get(label)
            .ok_or(Fallback::UnknownLabel)?;
        let node = self.node(at);
        Ok(Named {
            id: node.id.clone(),
            key: node.key.clone().ok_or(Fallback::UnknownLabel)?,
            at: Some(at),
        })
    }

    fn label_of(&mut self, action: &Value, name: &str) -> Result<Named, Fallback> {
        let label = field(action, name)
            .and_then(Value::as_str)
            .ok_or(Fallback::BadField)?
            .to_string();
        self.named(&label)
    }

    fn source(&mut self, action: &Value) -> Result<Named, Fallback> {
        match field(action, "from") {
            None => self.named("n1"),
            Some(_) => self.label_of(action, "from"),
        }
    }

    fn evidence(&self, action: &Value) -> Result<Vec<String>, Fallback> {
        let given = field(action, "because")
            .and_then(Value::as_array)
            .ok_or(Fallback::NoEvidence)?;
        let mut ids = BTreeSet::new();
        for label in given {
            let label = label.as_str().ok_or(Fallback::NoEvidence)?.trim();
            let id = if let Some(row) = self.context.spans.get(label) {
                format!("span:{row}")
            } else if let Some(&at) = self.context.inbox.get(label) {
                self.context.items[at].id.clone()
            } else {
                return Err(Fallback::NoEvidence);
            };
            ids.insert(id);
        }
        if ids.is_empty() || ids.len() > BECAUSE_IDS {
            return Err(Fallback::NoEvidence);
        }
        Ok(ids.into_iter().collect())
    }

    fn write(&mut self, node: &str, body: Body) -> Result<(), Fallback> {
        let kind = match &body {
            Body::Created(_) => "node.created",
            Body::KindChanged(_) => "node.kind_changed",
            Body::Revised(_) => "node.revised",
            Body::ViewChanged(_) => "node.view_changed",
            Body::OpenItem(_) => "node.open_item",
            Body::Intent(_) => "node.intent",
            Body::Span(_) => "node.span",
            Body::Entry(_) => "node.entry",
            Body::Related(_) => "node.related",
            Body::Unrelated(_) => "node.unrelated",
            Body::DoneWhen(_) => "node.done_when",
            Body::SplitOff(_) => "node.split_off",
            Body::Absorbed(_) => "node.absorbed",
            Body::Used(_) => "node.used",
            Body::Upkept(_) => "node.upkept",
        };
        // What it writes reads back as it was written, or nothing is written.
        let body = Body::read(kind, &body.to_value()).map_err(|_| Fallback::BadField)?;
        self.writes.push(Write {
            id: row_id(self.run, self.writes.len()),
            node: node.to_string(),
            kind: kind.to_string(),
            body: body.to_value(),
        });
        Ok(())
    }

    /// One write per head field per node in one answer: a second would not
    /// know the first's row.
    fn once(&mut self, node: &str, what: &'static str) -> Result<(), Fallback> {
        if self.written.insert((node.to_string(), what)) {
            Ok(())
        } else {
            Err(Fallback::NotAllowed)
        }
    }

    fn her(&self) -> Vec<SaidBy> {
        vec![SaidBy::Her]
    }

    /// The `part_of` rows that stand, by the fold's rule: in log order, one
    /// that would close a loop with those taken so far is passed by.
    fn standing(&self) -> (BTreeSet<(String, String)>, BTreeSet<String>) {
        let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
        let mut rows = BTreeSet::new();
        for (from, to, row) in &self.part_of {
            if !reaches(&edges, to, from) {
                edges.insert((from.clone(), to.clone()));
                rows.insert(row.clone());
            }
        }
        (edges, rows)
    }

    /// Adds a `part_of` the answer writes, if it will stand once folded.
    fn add_part_of(&mut self, from: &str, to: &str) -> bool {
        let row = format!("new:{}", self.part_of.len());
        self.part_of
            .push((from.to_string(), to.to_string(), row.clone()));
        if self.standing().1.contains(&row) {
            true
        } else {
            self.part_of.pop();
            false
        }
    }

    fn relate(
        &mut self,
        from: &Named,
        to: &Named,
        relation: Relation,
        reason: &str,
        because: &[String],
    ) -> Result<(), Fallback> {
        if from.id == to.id {
            return Err(Fallback::NotAllowed);
        }
        if relation == Relation::PartOf && !self.add_part_of(&from.id, &to.id) {
            return Err(Fallback::Cycle);
        }
        self.write(
            &from.id,
            Body::Related(Related {
                version: 1,
                sync: self.sync.clone(),
                said_by: self.her(),
                relation,
                to: to.key.clone(),
                reason: reason.to_string(),
                because: because.to_vec(),
            }),
        )
    }

    /// A new node from `create` or `split`, and its label.
    fn mint(&mut self, action: &Value) -> Result<(String, Named), Fallback> {
        let label = field(action, "as")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|label| label.starts_with("new") && label.len() > 3)
            .ok_or(Fallback::BadField)?
            .to_string();
        if self.news.contains_key(&label) || self.context.nodes.contains_key(&label) {
            return Err(Fallback::UnknownLabel);
        }
        let name = text(action, "name", NAME_CHARS, true)?;
        let done = optional_text(action, "done", TEXT_CHARS)?;
        let ongoing = flag(action, "ongoing")?;
        let because = self.evidence(action)?;
        let key = format!(
            "minted:u{}",
            minted(&format!("upkeep\u{1f}{}\u{1f}{label}", self.run))
        );
        let id = node_id(&self.context.audience, &key).map_err(|_| Fallback::BadField)?;
        let kind = kind_for(&done, ongoing);
        self.write(
            &id,
            Body::Created(Created {
                version: 1,
                sync: self.sync.clone(),
                said_by: self.her(),
                kind,
                key: key.clone(),
                name,
                object: None,
            }),
        )?;
        if done.is_some() {
            self.write(
                &id,
                Body::DoneWhen(DoneWhen {
                    version: 1,
                    sync: self.sync.clone(),
                    said_by: self.her(),
                    base: Vec::new(),
                    text: done,
                    because,
                }),
            )?;
        }
        if ongoing {
            self.status_write(&id, Vec::new(), Status::Ongoing)?;
        }
        let named = Named { id, key, at: None };
        self.news.insert(label.clone(), named.clone());
        Ok((label, named))
    }

    fn status_write(
        &mut self,
        node: &str,
        base: Vec<String>,
        status: Status,
    ) -> Result<(), Fallback> {
        self.once(node, "status")?;
        self.write(
            node,
            Body::Revised(Revised {
                version: 1,
                sync: self.sync.clone(),
                said_by: self.her(),
                base,
                field: Field::Status,
                person: None,
                value: serde_json::to_value(status).map_err(|_| Fallback::BadField)?,
            }),
        )
    }

    fn hold(&mut self, n: usize) {
        self.held.push(Held {
            action: n,
            why: HeldWhy::PartnerSet,
        });
    }

    fn action(&mut self, n: usize, action: &Value) -> Result<(), Fallback> {
        let what = field(action, "do")
            .and_then(Value::as_str)
            .ok_or(Fallback::UnknownAction)?;
        match what.trim() {
            "create" => {
                self.mint(action)?;
            }
            "split" => {
                let me = self.me();
                if me
                    .kind
                    .as_ref()
                    .is_some_and(|kind| kind.current.value.kind == Kind::Person)
                {
                    return Err(Fallback::NotAllowed);
                }
                let reason = text(action, "reason", REASON_CHARS, true)?;
                let because = self.evidence(action)?;
                let mut rows = BTreeSet::new();
                for label in labels(action, "spans")? {
                    rows.insert(
                        self.context
                            .spans
                            .get(&label)
                            .ok_or(Fallback::UnknownLabel)?
                            .clone(),
                    );
                }
                if rows.is_empty() || rows.len() > REFS {
                    return Err(Fallback::BadField);
                }
                let (label, part) = self.mint(action)?;
                self.used.insert(label);
                let whole = self.named("n1")?;
                self.write(
                    &whole.id,
                    Body::SplitOff(SplitOff {
                        version: 1,
                        sync: self.sync.clone(),
                        said_by: self.her(),
                        to: part.key.clone(),
                        spans: rows.into_iter().collect(),
                        reason: reason.clone(),
                        because: because.clone(),
                    }),
                )?;
                self.relate(&part, &whole, Relation::PartOf, &reason, &because)?;
            }
            "place" => {
                let target = self.label_of(action, "node")?;
                if Key::parse(&target.key).is_ok_and(|key| key.is_about_you(&self.context.audience))
                {
                    return Err(Fallback::NotAllowed);
                }
                let text = text(action, "text", TEXT_CHARS, false)?;
                let mut refs = BTreeSet::new();
                let mut said_by = BTreeSet::new();
                for label in labels(action, "items")? {
                    let &at = self
                        .context
                        .inbox
                        .get(&label)
                        .ok_or(Fallback::UnknownLabel)?;
                    let item = &self.context.items[at];
                    if !self.placed.insert(item.id.clone()) {
                        return Err(Fallback::NotAllowed);
                    }
                    refs.insert(item.id.clone());
                    said_by.extend(item.said_by.iter().copied());
                }
                if refs.is_empty() || refs.len() > REFS {
                    return Err(Fallback::BadField);
                }
                self.write(
                    &target.id,
                    Body::Span(Span {
                        version: 1,
                        sync: self.sync.clone(),
                        said_by: said_by.into_iter().collect(),
                        audience: self.context.audience.clone(),
                        text,
                        refs: refs.into_iter().collect(),
                        from: None,
                        to: None,
                    }),
                )?;
            }
            "absorb" => self.absorb(n, action)?,
            "relate" => {
                let from = self.source(action)?;
                let to = self.label_of(action, "to")?;
                let relation: Relation = enum_of(action, "relation")?;
                let reason = text(action, "reason", REASON_CHARS, true)?;
                let because = self.evidence(action)?;
                self.relate(&from, &to, relation, &reason, &because)?;
            }
            "unrelate" => self.unrelate(n, action)?,
            "kind" => self.kind(n, action)?,
            "status" => {
                let status: Status = enum_of(action, "status")?;
                let me = self.me();
                let kind = me
                    .kind
                    .as_ref()
                    .map_or(Kind::Topic, |kind| kind.current.value.kind);
                if !status.fits(kind) || (status == Status::Ongoing && kind != Kind::Task) {
                    return Err(Fallback::NotAllowed);
                }
                if status == status_of(me) {
                    return Ok(());
                }
                if by_partner(&me.status) {
                    self.hold(n);
                    return Ok(());
                }
                let base = me
                    .status
                    .as_ref()
                    .map(|item| item.base.clone())
                    .unwrap_or_default();
                let id = me.id.clone();
                self.status_write(&id, base, status)?;
            }
            "hand" => {
                let text = text(action, "text", TEXT_CHARS, false)?;
                let because = self.evidence(action)?;
                let id = self.me().id.clone();
                self.write(
                    &id,
                    Body::Intent(Intent {
                        version: 1,
                        sync: self.sync.clone(),
                        said_by: self.her(),
                        base: Vec::new(),
                        intent: format!("h{}", minted(&format!("hand\u{1f}{}\u{1f}{n}", self.run))),
                        text,
                        because,
                        status: IntentStatus::Waiting,
                        trigger: None,
                    }),
                )?;
            }
            _ => return Err(Fallback::UnknownAction),
        }
        Ok(())
    }

    fn absorb(&mut self, n: usize, action: &Value) -> Result<(), Fallback> {
        let me = self.me();
        let into = self.label_of(action, "into")?;
        let Some(into_at) = into.at else {
            return Err(Fallback::UnknownLabel);
        };
        let how: How = enum_of(action, "how")?;
        let reason = text(action, "reason", REASON_CHARS, true)?;
        let because = self.evidence(action)?;
        let about_you =
            Key::parse(&into.key).is_ok_and(|key| key.is_about_you(&self.context.audience));
        if !me
            .key
            .as_deref()
            .is_some_and(|key| key.starts_with("minted:"))
            || into_at == self.context.at
            || about_you
        {
            return Err(Fallback::NotAllowed);
        }
        if how == How::FoldedBack
            && !self
                .standing()
                .0
                .contains(&(me.id.clone(), into.id.clone()))
        {
            return Err(Fallback::NotAllowed);
        }
        // What the merge would write again as hers: a node, a relation, an
        // open item or an intent the partner wrote stays theirs.
        let partner_rows = self
            .context
            .full
            .folded
            .relations
            .iter()
            .filter(|relation| {
                relation.audience == self.context.audience
                    && (relation.from == me.id || relation.to == me.id)
            })
            .flat_map(|relation| relation.rows.iter())
            .filter_map(|row| self.context.full.rows.iter().find(|known| known.id == *row))
            .any(|row| row.author == Author::Owner);
        let partner_items = me
            .open
            .values()
            .filter(|open| !open.current.value.closed)
            .any(|open| open.current.author == Author::Owner)
            || me
                .intents
                .values()
                .filter(|said| said.current.value.status == IntentStatus::Waiting)
                .any(|said| said.current.author == Author::Owner);
        if me.created_by == Some(Author::Owner) || partner_rows || partner_items {
            self.hold(n);
            return Ok(());
        }
        let id = me.id.clone();
        self.once(&id, "absorbed")?;
        self.write(
            &id,
            Body::Absorbed(Absorbed {
                version: 1,
                sync: self.sync.clone(),
                said_by: self.her(),
                into: into.key.clone(),
                how,
                reason: reason.clone(),
                because: because.clone(),
            }),
        )?;
        // What is still open, and what she still means to say, goes on.
        for (item, open) in &me.open {
            if open.current.value.closed {
                continue;
            }
            self.write(
                &into.id,
                Body::OpenItem(OpenItem {
                    version: 1,
                    sync: self.sync.clone(),
                    said_by: open.current.said_by.clone(),
                    base: Vec::new(),
                    item: format!("m{}", minted(&format!("open\u{1f}{id}\u{1f}{item}"))),
                    text: open.current.value.text.clone(),
                    because: open.current.value.because.clone(),
                    closed: false,
                }),
            )?;
        }
        for (intent, said) in &me.intents {
            if !matches!(said.current.value.status, IntentStatus::Waiting) {
                continue;
            }
            self.write(
                &into.id,
                Body::Intent(Intent {
                    version: 1,
                    sync: self.sync.clone(),
                    said_by: said.current.said_by.clone(),
                    base: Vec::new(),
                    intent: format!("m{}", minted(&format!("intent\u{1f}{id}\u{1f}{intent}"))),
                    text: said.current.value.text.clone(),
                    because: said.current.value.because.clone(),
                    status: IntentStatus::Waiting,
                    trigger: said.current.value.trigger.clone(),
                }),
            )?;
        }
        // Its relations, again from the node it goes into.
        let audience = self.context.audience.clone();
        let relations: Vec<_> = self
            .context
            .full
            .folded
            .relations
            .iter()
            .filter(|relation| {
                relation.audience == audience && (relation.from == id || relation.to == id)
            })
            .cloned()
            .collect();
        let standing: BTreeSet<(String, Relation, String)> = self
            .context
            .full
            .folded
            .relations
            .iter()
            .filter(|relation| relation.audience == audience)
            .map(|relation| {
                (
                    relation.from.clone(),
                    relation.relation,
                    relation.to.clone(),
                )
            })
            .collect();
        for relation in relations {
            let outward = relation.from == id;
            let other_id = if outward {
                &relation.to
            } else {
                &relation.from
            };
            if *other_id == into.id {
                continue;
            }
            let Some(other_at) = self
                .context
                .full
                .folded
                .nodes
                .iter()
                .position(|node| node.audience == audience && node.id == *other_id)
            else {
                continue;
            };
            let other = self.node(other_at);
            let (Some(other_key), false, None) =
                (other.key.clone(), other.forgotten, &other.absorbed_into)
            else {
                continue;
            };
            let other = Named {
                id: other.id.clone(),
                key: other_key,
                at: Some(other_at),
            };
            let (from, to) = if outward {
                (&into, &other)
            } else {
                (&other, &into)
            };
            let known = standing.contains(&(from.id.clone(), relation.relation, to.id.clone()))
                || (relation.relation.symmetric()
                    && standing.contains(&(to.id.clone(), relation.relation, from.id.clone())));
            if known {
                continue;
            }
            if relation.relation == Relation::PartOf {
                // One that would not stand is left out, not written.
                if !self.add_part_of(&from.id, &to.id) {
                    continue;
                }
                self.part_of.pop();
            }
            self.relate(from, to, relation.relation, &reason, &because)?;
        }
        Ok(())
    }

    fn unrelate(&mut self, n: usize, action: &Value) -> Result<(), Fallback> {
        let from = self.source(action)?;
        let to = self.label_of(action, "to")?;
        let relation: Relation = enum_of(action, "relation")?;
        let reason = text(action, "reason", REASON_CHARS, true)?;
        let audience = &self.context.audience;
        let found = self.context.full.folded.relations.iter().find(|standing| {
            standing.audience == *audience
                && standing.relation == relation
                && ((standing.from == from.id && standing.to == to.id)
                    || (relation.symmetric() && standing.from == to.id && standing.to == from.id))
        });
        let Some(found) = found else {
            return Err(Fallback::NotAllowed);
        };
        // The rows of it, by the node each is on.
        let mut on: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut partner = false;
        for row in &found.rows {
            let Some(row) = self.context.full.rows.iter().find(|known| known.id == *row) else {
                continue;
            };
            partner |= row.author == Author::Owner;
            let node = row
                .object
                .strip_prefix("node:")
                .unwrap_or(&row.object)
                .to_string();
            on.entry(node).or_default().insert(row.id.clone());
        }
        if partner {
            self.hold(n);
            return Ok(());
        }
        if relation == Relation::PartOf {
            self.part_of.retain(|(_, _, row)| !found.rows.contains(row));
        }
        for (node, rows) in on {
            self.write(
                &node,
                Body::Unrelated(Unrelated {
                    version: 1,
                    sync: self.sync.clone(),
                    said_by: self.her(),
                    base: rows.into_iter().collect(),
                    reason: reason.clone(),
                }),
            )?;
        }
        Ok(())
    }

    fn kind(&mut self, n: usize, action: &Value) -> Result<(), Fallback> {
        let me = self.me();
        let done = optional_text(action, "done", TEXT_CHARS)?;
        let ongoing = flag(action, "ongoing")?;
        let reason = text(action, "reason", REASON_CHARS, true)?;
        let because = self.evidence(action)?;
        let kind = kind_for(&done, ongoing);
        let key = me.key.as_deref().and_then(|key| Key::parse(key).ok());
        if key.is_some_and(|key| !key.kinds().contains(&kind)) {
            return Err(Fallback::NotAllowed);
        }
        let current_done = me
            .done_when
            .as_ref()
            .and_then(|done| done.current.value.clone());
        let current_kind = me.kind.as_ref().map(|kind| kind.current.value.kind);
        let status = status_of(me);
        let new_status = match (ongoing, status == Status::Ongoing) {
            (true, false) => Some(Status::Ongoing),
            (false, true) => Some(Status::Active),
            _ => None,
        };
        let changes_done = done != current_done;
        let changes_kind = current_kind != Some(kind);
        if (changes_done && by_partner(&me.done_when))
            || (changes_kind && by_partner(&me.kind))
            || (new_status.is_some() && by_partner(&me.status))
        {
            self.hold(n);
            return Ok(());
        }
        let id = me.id.clone();
        if changes_done {
            self.once(&id, "done_when")?;
            self.write(
                &id,
                Body::DoneWhen(DoneWhen {
                    version: 1,
                    sync: self.sync.clone(),
                    said_by: self.her(),
                    base: me
                        .done_when
                        .as_ref()
                        .map(|item| item.base.clone())
                        .unwrap_or_default(),
                    text: done,
                    because: because.clone(),
                }),
            )?;
        }
        if changes_kind {
            self.once(&id, "kind")?;
            self.write(
                &id,
                Body::KindChanged(KindChanged {
                    version: 1,
                    sync: self.sync.clone(),
                    said_by: self.her(),
                    base: me
                        .kind
                        .as_ref()
                        .map(|item| item.base.clone())
                        .unwrap_or_default(),
                    kind,
                    reason,
                    because,
                }),
            )?;
        }
        if let Some(status) = new_status {
            let base = me
                .status
                .as_ref()
                .map(|item| item.base.clone())
                .unwrap_or_default();
            self.status_write(&id, base, status)?;
        }
        Ok(())
    }
}

/// Whether `from` reaches `to` along `edges`.
fn reaches(edges: &BTreeSet<(String, String)>, from: &str, to: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut next = vec![from.to_string()];
    while let Some(at) = next.pop() {
        if at == to {
            return true;
        }
        if !seen.insert(at.clone()) {
            continue;
        }
        next.extend(
            edges
                .iter()
                .filter(|(one, _)| *one == at)
                .map(|(_, whole)| whole.clone()),
        );
    }
    false
}

/// The id of a run's `n`th row.
fn row_id(run: &str, n: usize) -> String {
    format!("u{}", minted(&format!("row\u{1f}{run}\u{1f}{n}")))
}

fn actions(output: &str) -> Result<Vec<Value>, Fallback> {
    if output.len() > UPKEEP_OUTPUT_BYTES {
        return Err(Fallback::TooLong);
    }
    let value = object_in(output).ok_or(Fallback::NotJson)?;
    let actions = value
        .get("actions")
        .and_then(Value::as_array)
        .ok_or(Fallback::NoActions)?;
    if actions.len() > UPKEEP_ACTIONS {
        return Err(Fallback::TooManyActions);
    }
    Ok(actions.clone())
}

pub fn read(input: ReadInput) -> Result<UpkeepRead, NodeError> {
    check_token("run", &input.run)?;
    input.sync.check()?;
    let context = context(input.input)?;
    let mut reader = Reader::new(&context, &input.run, input.sync.clone());
    let read = actions(&input.output).and_then(|actions| {
        for (at, action) in actions.iter().enumerate() {
            reader.action(at + 1, action)?;
        }
        // A new node no later action puts anything on, or relates, is a
        // node with nothing on it.
        if reader.news.keys().any(|label| !reader.used.contains(label)) {
            return Err(Fallback::NotAllowed);
        }
        if reader.writes.len() > UPKEEP_WRITES {
            return Err(Fallback::TooManyActions);
        }
        Ok(())
    });
    let me = &context.full.folded.nodes[context.at];
    // What she saw: the newest clock per device of what the node watches,
    // the newest devices kept, the rest under one floor.
    let mut seen: Vec<(String, Rev)> = Watch::new(&context.full)
        .newest(&context.audience, &me.id)
        .into_iter()
        .collect();
    seen.sort_by(|a, b| b.1.cmp(&a.1));
    let floor = seen
        .iter()
        .skip(SEEN_DEVICES)
        .map(|(_, rev)| rev.wall_ms)
        .max();
    seen.truncate(SEEN_DEVICES);
    let (mut writes, placed, held, fallback) = match read {
        Ok(()) => (
            reader.writes,
            reader.placed.into_iter().collect(),
            reader.held,
            None,
        ),
        Err(why) => (Vec::new(), Vec::new(), Vec::new(), Some(why)),
    };
    let mut wrote: Vec<String> = writes.iter().map(|write| write.id.clone()).collect();
    wrote.sort();
    let upkept = Body::Upkept(Upkept {
        version: 1,
        sync: input.sync,
        said_by: vec![SaidBy::Her],
        run: input.run.clone(),
        seen: seen.into_iter().collect(),
        floor,
        wrote,
        fallback: fallback.map(|why| why.name().to_string()),
    });
    // The mark reads back as it was written, or the queue would never move.
    let upkept = Body::read("node.upkept", &upkept.to_value())?;
    writes.push(Write {
        id: row_id(&input.run, writes.len()),
        node: me.id.clone(),
        kind: "node.upkept".to_string(),
        body: upkept.to_value(),
    });
    Ok(UpkeepRead {
        writes,
        placed,
        held,
        fallback,
    })
}
