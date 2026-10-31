//! Her upkeep of one node: the pure half. When the queue ([`crate::queue`])
//! hands out a node, she looks at it with what is one relation away and
//! the inbox, and judges: the two judgments (does a part have a state of
//! its own; must it be done before its whole is) and what follows from
//! them. The judging is hers, by the model, in her words; here is only what
//! she is shown and how her answer becomes rows, so the Swift and the Rust
//! host write the same rows from the same answer.
//!
//! - [`prepare`] folds the rows and shows her the node (its head, its newest
//!   spans and entries, what the partner set, what was changed on both
//!   sides), its neighbours one relation away, the other nodes of the
//!   audience the caller offers (candidates: what might be the same thing,
//!   or where an inbox item belongs), and the inbox, each cut to the limits
//!   in [`crate::limits`] and labelled (`n1` the node, `n2`… neighbours and
//!   then candidates, `s1`… its spans, `e1`… its entries, `t1`… its summary
//!   sentences, `o1`… its open items, `v1`… its views, `i1`… inbox items)
//!   so no id goes to the model. With no node kept she only files the
//!   inbox, onto candidates or new nodes.
//! - [`read`] reads her answer, `{"actions": [{"do": …}, …]}`, into the rows
//!   to write, in order, and last the `node.upkept` mark (none when no node
//!   is kept). What each action writes:
//!
//! | `do` | what she says | rows |
//! | --- | --- | --- |
//! | `create` | a new node `as` (`new…`), `name`, `done?`, `ongoing?`, `because` (立节点) | `node.created`; `node.done_when` with a done; status `ongoing` |
//! | `split` | a part `as` with its `spans`, `name`, `done?`, `ongoing?`, `reason`, `because` (拆分) | as `create`; `node.split_off` on this node; `part_of` from the part |
//! | `place` | inbox `items` onto `node`, in her `text` (收件箱归位) | `node.span`, its refs the items |
//! | `absorb` | this node `into` a neighbour or a candidate, `how` `merged` or `folded_back`, `reason`, `because` (并入、收回) | `node.absorbed`; its open items, waiting intents and relations again on the other |
//! | `relate` | `from?` (this node) → `to`, `relation`, `reason`, `because` (挂两处 is a second `part_of`) | `node.related` |
//! | `unrelate` | `from?` → `to`, `relation`, `reason` (独立 is a `part_of` taken back) | `node.unrelated` on each node holding it |
//! | `kind` | `done?` and `ongoing?`, `reason`, `because` (进行中 ↔ 话题) | `node.done_when`, `node.kind_changed`, status, as they change |
//! | `status` | `status` (`active`, `resting`, `done`, `dropped`, `ongoing`) | `node.revised` |
//! | `hand` | `text`, `because`: a hand raised to the partner | `node.intent`, waiting |
//! | `summary` | `sentences`, each `{text, because}` (改写概括) | `node.revised` of the summary |
//! | `view` | `view?` (a `v…`; none for a new one), `text`, `because` (看法变了) | `node.view_changed` |
//! | `open` | `item?` (an `o…`; none for a new one), `text?`, `closed?`, `because` (没收尾的事) | `node.open_item` |
//!
//! Evidence (`because`) is what she was shown: a span, an entry or an inbox
//! item is itself; a summary sentence, an open item or a view stands for
//! what it rests on, so a node with no span of its own can still be
//! judged. A view changes only on what counts (算数): every piece of it a
//! span, an entry or an inbox item said only by the partner or her, and not
//! in a group; anything else is held (`not_counted`).
//!
//! The kind follows from her `done` alone: with a "done" it is something in
//! progress; without one it is a topic, unless it is `ongoing` (always being
//! done, 每周发版), which is something in progress whose status is ongoing.
//!
//! What the partner set stands: an action that would change a head field,
//! an open item or a view whose current version is the partner's, take
//! back a relation the partner made, or merge away a node with anything of
//! the partner's on it (its own row, a relation, a head field, a view, an
//! open item or a waiting intent: a merge writes some of those again as
//! hers and leaves the rest behind) is held (`held`), and she may only
//! raise a hand.
//! Every other action of the answer stands. What she watched or read with
//! the partner (`minted:together-…`) only ever takes others in: merging it
//! away changes nothing, as for a person or a repository (it may still be
//! split).
//!
//! A `part_of` she writes is checked as the fold will check it: the fold's
//! rule (in log order, one that would close a loop is passed by) replayed
//! over every `part_of` row it weighs, with what the answer takes back and
//! adds. Each row to write comes with its id, from the run's token; the
//! mark lists them, so the queue knows what the run wrote and nothing else
//! written meanwhile is taken as seen.
//!
//! A node taken back from sync (撤回同步) is out of shared upkeep on every
//! side. Only its home folds it whole, and there a run on it writes local
//! rows only (`local` in what is prepared: the caller passes a local
//! `sync`, or `read` refuses the call) and names no node but the ones this
//! side keeps. A shared run is never shown one, so never merges into it: a
//! preference, not a rule of the log (a merge of a node with stubs in it is
//! harmless, and a retract never reaches through one). Nor is a shared run
//! shown a span or an entry any retract reaches, wherever the fold shows it
//! (split off into a node not taken back, or in a node it went into since),
//! so nothing the home keeps goes into shared text.
//!
//! An answer that cannot be read as it stands changes nothing: no row but
//! the `node.upkept` mark, which says why (`fallback`) so the queue moves
//! on; the node is due again at its next change.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::{forget, row::Author};
use myriad_persona_sync::{Rev, canonical::sha256_hex};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{
        Absorbed, Body, Created, DoneWhen, Field, How, Intent, IntentStatus, KindChanged, OpenItem,
        Related, Relation, Revised, Sentence, Span, SplitOff, Status, Unrelated, Upkept,
        ViewChanged,
    },
    check_token,
    fold::{FoldInput, Full, Item, Node, Parts, fold_full},
    key::{Key, Kind, node_id},
    limits::{
        BECAUSE_IDS, NAME_CHARS, PLACE_SUMMARY_CHARS, REASON_CHARS, REFS, SEEN_DEVICES,
        SENTENCE_CHARS, SUMMARY_SENTENCES, TEXT_CHARS, UPKEEP_ACTIONS, UPKEEP_CANDIDATES,
        UPKEEP_ENTRIES, UPKEEP_ENTRIES_TOTAL_CHARS, UPKEEP_HEAD_ITEMS, UPKEEP_INBOX,
        UPKEEP_INBOX_TOTAL_CHARS, UPKEEP_NEIGHBOURS, UPKEEP_OUTPUT_BYTES, UPKEEP_SPAN_CHARS,
        UPKEEP_SPANS, UPKEEP_TOTAL_CHARS, UPKEEP_WRITES, cut,
    },
    place::{clean, object_in},
    queue::{Reason, Watch},
    tags::{SaidBy, Sync, audience_fits, check_said_by},
};

/// One inbox item of this audience: what was not on a node yet.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InboxItem {
    /// `<type>:<id>`: what a span placed from it refers to.
    pub id: String,
    pub said_by: Vec<SaidBy>,
    pub text: String,
    /// Where it was said, when not in this audience: only the partner's
    /// own words in a group, kept in their private audience.
    #[serde(default)]
    pub audience: Option<String>,
}

impl InboxItem {
    fn said_in<'a>(&'a self, audience: &'a str) -> &'a str {
        self.audience.as_deref().unwrap_or(audience)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpkeepInput {
    /// The rows of the node and of the nodes around it, as for the fold.
    pub rows: Vec<myriad_persona_log::Row>,
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    /// This side's ledger, as for the fold.
    #[serde(default)]
    pub ledger: Vec<myriad_persona_log::forget::Entry>,
    pub audience: String,
    /// The node she keeps; none when she only files the inbox (the
    /// audience has no node yet, or none is due).
    #[serde(default)]
    pub node: Option<String>,
    /// Other nodes of this audience, by id, ranked by the caller (by what
    /// recall finds near the node or the inbox): what might be the same
    /// thing, or where an inbox item belongs. Their rows are in `rows`.
    #[serde(default)]
    pub candidates: Vec<String>,
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

/// An open item or a view of the node, labelled (`o…`, `v…`) so she can
/// revise it, or cite what it rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownItem {
    pub label: String,
    pub text: String,
    /// Its current version is the partner's: only a hand against it.
    #[serde(skip_serializing_if = "is_false")]
    pub by_partner: bool,
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
    /// Its sentences in order; the `n`th is `t<n>` when she cites what it
    /// rests on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub summary: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub open: Vec<ShownItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub intents: Vec<ShownIntent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ShownItem>,
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

/// A node of the audience the caller offered, with no relation to the node
/// she keeps (or no node kept).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShownCandidate {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub status: Status,
    /// Its summary's first sentence, cut.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gist: Option<String>,
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
    /// Its newest entries shown, of all it has.
    pub entries: usize,
    /// The first inbox items shown, of those given.
    pub inbox: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    /// None when she only files the inbox.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<ShownNode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<ShownSpan>,
    /// Its entries (facts and preferences), newest kept, labelled `e…`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<ShownSpan>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub neighbours: Vec<ShownNeighbour>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<ShownCandidate>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inbox: Vec<ShownInbox>,
    /// Why it is due now.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub due: Vec<Reason>,
    pub taken: Taken,
    /// The node is taken back from sync and this side is its home: the
    /// run writes local rows only (`read` is given a local `sync`).
    #[serde(skip_serializing_if = "is_false")]
    pub local: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn by_partner<T>(item: &Option<Item<T>>) -> bool {
    item.as_ref().is_some_and(partners)
}

/// Whether the partner wrote a current version of it.
fn partners<T>(item: &Item<T>) -> bool {
    item.current.author == Author::Owner
        || item
            .rivals
            .iter()
            .any(|rival| rival.author == Author::Owner)
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
    /// Index of the node she keeps in the fold; none when she only files
    /// the inbox.
    at: Option<usize>,
    /// Label → index in the fold: `n1` the node, `n2`… its neighbours,
    /// then the candidates.
    nodes: BTreeMap<String, usize>,
    /// `s…` → span row.
    spans: BTreeMap<String, String>,
    /// `e…` → entry row.
    entries: BTreeMap<String, String>,
    /// `t…`, `o…`, `v…` → what that sentence, open item or view rests on.
    heads: BTreeMap<String, Vec<String>>,
    /// `o…` → the open item's id.
    open: BTreeMap<String, String>,
    /// `v…` → the view's id.
    views: BTreeMap<String, String>,
    /// `i…` → index in the inbox.
    inbox: BTreeMap<String, usize>,
    items: Vec<InboxItem>,
    prepared: Prepared,
}

/// Whether a node is one she can be shown: created, not forgotten, not
/// gone into another.
fn alive(node: &Node) -> bool {
    !node.forgotten && node.key.is_some() && node.absorbed_into.is_none()
}

fn gist(node: &Node) -> Option<String> {
    node.summary.as_ref().and_then(|summary| {
        summary
            .current
            .value
            .first()
            .map(|sentence| cut(&clean(&sentence.text, true), PLACE_SUMMARY_CHARS))
    })
}

fn shown_text(text: &str) -> String {
    cut(&clean(text, false), UPKEEP_SPAN_CHARS)
}

/// The newest of `texts` (row, said by, text), within `count` and `budget`
/// chars, shown oldest first and labelled `<prefix>1`….
fn newest<'t>(
    texts: impl DoubleEndedIterator<Item = (&'t str, &'t [SaidBy], &'t str)>,
    prefix: &str,
    count: usize,
    budget: usize,
) -> (Vec<ShownSpan>, BTreeMap<String, String>) {
    let mut taken = Vec::new();
    let mut total = 0;
    for (row, said_by, text) in texts.rev().take(count) {
        let text = shown_text(text);
        let chars = text.chars().count();
        if !taken.is_empty() && total + chars > budget {
            break;
        }
        total += chars;
        taken.push((row.to_string(), said_by.to_vec(), text));
    }
    taken.reverse();
    let mut labels = BTreeMap::new();
    let shown = taken
        .into_iter()
        .enumerate()
        .map(|(n, (row, said_by, text))| {
            let label = format!("{prefix}{}", n + 1);
            labels.insert(label.clone(), row);
            ShownSpan {
                label,
                said_by,
                text,
            }
        })
        .collect();
    (shown, labels)
}

/// The node she keeps, as she sees its head, and the labels of its head.
struct Kept {
    shown: ShownNode,
    heads: BTreeMap<String, Vec<String>>,
    open: BTreeMap<String, String>,
    views: BTreeMap<String, String>,
}

fn kept(node: &Node) -> Kept {
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
    let mut heads = BTreeMap::new();
    let summary = node
        .summary
        .as_ref()
        .map(|summary| {
            summary
                .current
                .value
                .iter()
                .enumerate()
                .map(|(n, sentence)| {
                    heads.insert(format!("t{}", n + 1), sentence.because.clone());
                    sentence.text.clone()
                })
                .collect()
        })
        .unwrap_or_default();
    let mut open_labels = BTreeMap::new();
    let open = node
        .open
        .iter()
        .filter(|(_, item)| !item.current.value.closed)
        .take(UPKEEP_HEAD_ITEMS)
        .enumerate()
        .map(|(n, (id, item))| {
            let label = format!("o{}", n + 1);
            heads.insert(label.clone(), item.current.value.because.clone());
            open_labels.insert(label.clone(), id.clone());
            ShownItem {
                label,
                text: shown_text(&item.current.value.text),
                by_partner: partners(item),
            }
        })
        .collect();
    let mut view_labels = BTreeMap::new();
    let views = node
        .views
        .iter()
        .take(UPKEEP_HEAD_ITEMS)
        .enumerate()
        .map(|(n, (id, view))| {
            let label = format!("v{}", n + 1);
            heads.insert(label.clone(), view.current.value.because.clone());
            view_labels.insert(label.clone(), id.clone());
            ShownItem {
                label,
                text: shown_text(&view.current.value.text),
                by_partner: partners(view),
            }
        })
        .collect();
    let shown = ShownNode {
        label: "n1".to_string(),
        kind: node.kind.as_ref().map(|kind| kind.current.value.kind),
        name: node.name.as_ref().map(|name| name.current.value.clone()),
        status: status_of(node),
        done_when: node
            .done_when
            .as_ref()
            .and_then(|done| done.current.value.as_deref().map(shown_text)),
        summary,
        open,
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
        views,
        parts: node.parts,
        made_by_partner: node.created_by == Some(Author::Owner),
        partner_set,
        both_changed,
    };
    Kept {
        shown,
        heads,
        open: open_labels,
        views: view_labels,
    }
}

fn context(input: UpkeepInput) -> Result<Context, NodeError> {
    if let Some(node) = &input.node {
        check_token("node", node)?;
    } else if input.inbox.is_empty() {
        return Err(NodeError::new("with no node, there is an inbox to file"));
    }
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
        if let Some(said_in) = &item.audience {
            check_token("inbox audience", said_in)?;
            if !audience_fits(said_in, &input.audience, &item.said_by) {
                return Err(NodeError::new(
                    "an inbox item said elsewhere is the partner's own words from a group",
                ));
            }
        }
    }
    let mut seen = BTreeSet::new();
    for candidate in &input.candidates {
        check_token("candidate", candidate)?;
        if !seen.insert(candidate.as_str()) {
            return Err(NodeError::new("candidate ids repeat"));
        }
    }
    let audience = input.audience;
    check_token("audience", &audience)?;
    myriad_persona_log::row::Audience::parse(&audience)
        .map_err(|_| NodeError::new("audience is not personal:, own: or group:"))?;
    let ledger = forget::joined(&input.ledger, &forget::ledger(&input.rows));
    let full = fold_full(FoldInput {
        rows: input.rows,
        bodies: input.bodies,
        ledger: input.ledger,
    })?;
    let place = |id: &str| {
        full.folded
            .nodes
            .iter()
            .position(|node| node.audience == audience && node.id == id)
    };
    let at = match &input.node {
        None => None,
        Some(id) => {
            let at =
                place(id).ok_or_else(|| NodeError::new("the node is not in the rows given"))?;
            if !alive(&full.folded.nodes[at]) {
                return Err(NodeError::new(
                    "the node is forgotten, gone into another, or not created yet",
                ));
            }
            Some(at)
        }
    };
    // A run on a node taken back from sync (on its home) is local; a shared
    // one is never shown such a node.
    let local = at.is_some_and(|at| !full.folded.nodes[at].retracted.is_empty());
    // Nor is a shared run shown a span or an entry a retract reaches,
    // wherever the fold shows it (split off into a node not taken back, or
    // in one the node went into since): what the home keeps goes into no
    // shared text. A local run sees them.
    let view = forget::View::new(&ledger);
    let by_id: BTreeMap<&str, &myriad_persona_log::Row> =
        full.rows.iter().map(|row| (row.id.as_str(), row)).collect();
    let withdrawn = |row: &str| {
        !local
            && by_id.get(row).is_some_and(|row| {
                view.reaching(row)
                    .iter()
                    .any(|entry| entry.is_retract() || entry.kind == forget::Kind::Home)
            })
    };
    // Every candidate is a node of this audience in the rows: one from
    // another audience is never shown, and is the caller's mistake.
    let candidates = input
        .candidates
        .iter()
        .map(|id| {
            place(id).ok_or_else(|| {
                NodeError::new("a candidate is not a node of this audience in the rows given")
            })
        })
        .collect::<Result<Vec<usize>, _>>()?;

    let mut kept_node = None;
    let mut spans = Vec::new();
    let mut span_labels = BTreeMap::new();
    let mut entries = Vec::new();
    let mut entry_labels = BTreeMap::new();
    let mut nodes = BTreeMap::new();
    let mut neighbours = Vec::new();
    let mut due = Vec::new();
    if let Some(at) = at {
        let node = &full.folded.nodes[at];
        kept_node = Some(kept(node));
        // Its newest spans and entries, shown oldest first.
        (spans, span_labels) = newest(
            node.spans
                .iter()
                .filter(|span| !withdrawn(&span.row))
                .map(|span| {
                    (
                        span.row.as_str(),
                        span.said_by.as_slice(),
                        span.text.as_str(),
                    )
                }),
            "s",
            UPKEEP_SPANS,
            UPKEEP_TOTAL_CHARS,
        );
        (entries, entry_labels) = newest(
            node.entries
                .iter()
                .filter(|entry| !withdrawn(&entry.row))
                .map(|entry| {
                    (
                        entry.row.as_str(),
                        entry.said_by.as_slice(),
                        entry.text.as_str(),
                    )
                }),
            "e",
            UPKEEP_ENTRIES,
            UPKEEP_ENTRIES_TOTAL_CHARS,
        );

        // One relation away.
        let index: BTreeMap<&str, usize> = full
            .folded
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, other)| {
                other.audience == audience && alive(other) && local != other.retracted.is_empty()
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
        nodes.insert("n1".to_string(), at);
        neighbours = around
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
                    gist: gist(other),
                    relations,
                }
            })
            .collect();
        due = Watch::new(&full)
            .due(&audience, &node.id)
            .map(|due| due.why)
            .unwrap_or_default();
    }

    // The candidates after the neighbours (after `n1` when no node is
    // kept), in the caller's order: one already shown, or not alive, is
    // passed by.
    let first = if at.is_some() { nodes.len() + 1 } else { 2 };
    let shown_candidates: Vec<ShownCandidate> = candidates
        .into_iter()
        .filter(|other| alive(&full.folded.nodes[*other]))
        .filter(|other| local != full.folded.nodes[*other].retracted.is_empty())
        .filter(|other| !nodes.values().any(|shown| shown == other))
        .take(UPKEEP_CANDIDATES)
        .collect::<Vec<usize>>()
        .into_iter()
        .enumerate()
        .map(|(n, other)| {
            let label = format!("n{}", first + n);
            nodes.insert(label.clone(), other);
            let other = &full.folded.nodes[other];
            ShownCandidate {
                label,
                kind: other.kind.as_ref().map(|kind| kind.current.value.kind),
                name: other.name.as_ref().map(|name| name.current.value.clone()),
                status: status_of(other),
                gist: gist(other),
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
    let (shown, heads, open, views) = match kept_node {
        Some(kept) => (Some(kept.shown), kept.heads, kept.open, kept.views),
        None => (None, BTreeMap::new(), BTreeMap::new(), BTreeMap::new()),
    };
    let prepared = Prepared {
        node: shown,
        taken: Taken {
            spans: spans.len(),
            entries: entries.len(),
            inbox: inbox.len(),
        },
        spans,
        entries,
        neighbours,
        candidates: shown_candidates,
        inbox,
        due,
        local,
    };
    Ok(Context {
        full,
        audience,
        at,
        nodes,
        spans: span_labels,
        entries: entry_labels,
        heads,
        open,
        views,
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
/// the order. A merge or a split also names the other node in the
/// envelope: the caller writes `to` and `takes` into the row as they are
/// (`myriad_persona_log::Row`), so a stub of it keeps them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Write {
    pub id: String,
    pub node: String,
    pub kind: String,
    pub body: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub takes: Vec<String>,
}

/// Why an action was held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeldWhy {
    /// The partner set it: only a hand.
    PartnerSet,
    /// A view that does not rest only on what counts (算数): the partner's
    /// or her own words, outside a group, shown to her as they are.
    NotCounted,
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
    /// Something the rules never allow: a person split, a person, a
    /// repository or what she watched with the partner merged away, a kind
    /// its key does not have, a span on 「关于你」, an item placed twice, a
    /// field written twice, folding back into what it is not part of,
    /// taking back a relation that is not there (or any, with no node
    /// kept), a new node with nothing on it.
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
    /// In order; the last is the `node.upkept` mark, when a node is kept.
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
    written: BTreeSet<(String, String)>,
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

    /// The node she keeps: an action on it with none kept names a label
    /// she was not shown.
    fn me(&self) -> Result<&'a Node, Fallback> {
        self.context
            .at
            .map(|at| self.node(at))
            .ok_or(Fallback::UnknownLabel)
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
        // A shared run names no node taken back from sync; a local run (on
        // one, at its home) names only those this side keeps.
        if node.retracted.is_empty() == self.context.prepared.local {
            return Err(Fallback::NotAllowed);
        }
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
        Ok(self.weighed(action)?.0)
    }

    /// What `because` names: a span (`s…`), an entry (`e…`) or an inbox
    /// item (`i…`) is itself; a summary sentence (`t…`), an open item
    /// (`o…`) or a view (`v…`) is what it rests on. And whether all of it
    /// counts (算数) for a view: shown as it is (not through the head),
    /// said only by the partner or her, and not in a group.
    fn weighed(&self, given: &Value) -> Result<(Vec<String>, bool), Fallback> {
        let given = field(given, "because")
            .and_then(Value::as_array)
            .ok_or(Fallback::NoEvidence)?;
        let counts = |said_by: &[SaidBy], audience: &str| {
            said_by
                .iter()
                .all(|said| matches!(said, SaidBy::Partner | SaidBy::Her))
                && !matches!(
                    myriad_persona_log::row::Audience::parse(audience),
                    Ok(myriad_persona_log::row::Audience::Group(_)) | Err(_)
                )
        };
        let mut ids = BTreeSet::new();
        let mut counted = true;
        for label in given {
            let label = label.as_str().ok_or(Fallback::NoEvidence)?.trim();
            if let Some(row) = self.context.spans.get(label) {
                let span = self
                    .me()?
                    .spans
                    .iter()
                    .find(|span| span.row == *row)
                    .ok_or(Fallback::NoEvidence)?;
                counted &= counts(&span.said_by, &span.audience);
                ids.insert(format!("span:{row}"));
            } else if let Some(row) = self.context.entries.get(label) {
                let entry = self
                    .me()?
                    .entries
                    .iter()
                    .find(|entry| entry.row == *row)
                    .ok_or(Fallback::NoEvidence)?;
                counted &= counts(&entry.said_by, &entry.audience);
                ids.insert(format!("entry:{row}"));
            } else if let Some(&at) = self.context.inbox.get(label) {
                let item = &self.context.items[at];
                counted &= counts(&item.said_by, item.said_in(&self.context.audience));
                ids.insert(item.id.clone());
            } else if let Some(rests_on) = self.context.heads.get(label) {
                counted = false;
                ids.extend(rests_on.iter().cloned());
            } else {
                return Err(Fallback::NoEvidence);
            }
        }
        if ids.is_empty() || ids.len() > BECAUSE_IDS {
            return Err(Fallback::NoEvidence);
        }
        Ok((ids.into_iter().collect(), counted))
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
        let audience = self.context.audience.as_str();
        let other = |key: &str| {
            node_id(audience, key)
                .map(|id| format!("node:{id}"))
                .map_err(|_| Fallback::BadField)
        };
        let (to, takes) = match &body {
            Body::Absorbed(absorbed) => (Some(other(&absorbed.into)?), Vec::new()),
            Body::SplitOff(split) => {
                let takes: BTreeSet<&String> = split.spans.iter().collect();
                (
                    Some(other(&split.to)?),
                    takes.into_iter().cloned().collect(),
                )
            }
            _ => (None, Vec::new()),
        };
        self.writes.push(Write {
            id: row_id(self.run, self.writes.len()),
            node: node.to_string(),
            kind: kind.to_string(),
            body: body.to_value(),
            to,
            takes,
        });
        Ok(())
    }

    /// One write per head field per node in one answer: a second would not
    /// know the first's row.
    fn once(&mut self, node: &str, what: &str) -> Result<(), Fallback> {
        if self.written.insert((node.to_string(), what.to_string())) {
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

    fn hold(&mut self, n: usize, why: HeldWhy) {
        self.held.push(Held { action: n, why });
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
                let me = self.me()?;
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
                let mut said_in = BTreeSet::new();
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
                    said_in.insert(item.said_in(&self.context.audience).to_string());
                }
                if refs.is_empty() || refs.len() > REFS {
                    return Err(Fallback::BadField);
                }
                // One span is said in one place: items from a group and from
                // here are placed apart.
                let [said_in] = <[String; 1]>::try_from(said_in.into_iter().collect::<Vec<_>>())
                    .map_err(|_| Fallback::NotAllowed)?;
                self.write(
                    &target.id,
                    Body::Span(Span {
                        version: 1,
                        sync: self.sync.clone(),
                        said_by: said_by.into_iter().collect(),
                        audience: said_in,
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
                let me = self.me()?;
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
                    self.hold(n, HeldWhy::PartnerSet);
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
                let id = self.me()?.id.clone();
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
            "summary" => self.summary(n, action)?,
            "view" => self.view(n, action)?,
            "open" => self.open(n, action)?,
            _ => return Err(Fallback::UnknownAction),
        }
        Ok(())
    }

    /// Her summary rewritten, every sentence with what it rests on.
    fn summary(&mut self, n: usize, action: &Value) -> Result<(), Fallback> {
        let me = self.me()?;
        let given = field(action, "sentences")
            .and_then(Value::as_array)
            .filter(|given| !given.is_empty() && given.len() <= SUMMARY_SENTENCES)
            .ok_or(Fallback::BadField)?;
        let mut sentences = Vec::new();
        for sentence in given {
            sentences.push(Sentence {
                text: text(sentence, "text", SENTENCE_CHARS, true)?,
                because: self.evidence(sentence)?,
            });
        }
        if by_partner(&me.summary) {
            self.hold(n, HeldWhy::PartnerSet);
            return Ok(());
        }
        let id = me.id.clone();
        self.once(&id, "summary")?;
        self.write(
            &id,
            Body::Revised(Revised {
                version: 1,
                sync: self.sync.clone(),
                said_by: self.her(),
                base: me
                    .summary
                    .as_ref()
                    .map(|item| item.base.clone())
                    .unwrap_or_default(),
                field: Field::Summary,
                person: None,
                value: serde_json::to_value(sentences).map_err(|_| Fallback::BadField)?,
            }),
        )
    }

    /// One of her views, changed (`view`, a label she was shown) or new.
    /// It changes only on what counts: anything else is held.
    fn view(&mut self, n: usize, action: &Value) -> Result<(), Fallback> {
        let me = self.me()?;
        let text = text(action, "text", TEXT_CHARS, false)?;
        let (because, counted) = self.weighed(action)?;
        let (view, current) = match field(action, "view") {
            None => (
                format!("v{}", minted(&format!("view\u{1f}{}\u{1f}{n}", self.run))),
                None,
            ),
            Some(label) => {
                let label = label.as_str().ok_or(Fallback::BadField)?.trim();
                let view = self
                    .context
                    .views
                    .get(label)
                    .ok_or(Fallback::UnknownLabel)?;
                (view.clone(), me.views.get(view))
            }
        };
        if current.is_some_and(partners) {
            self.hold(n, HeldWhy::PartnerSet);
            return Ok(());
        }
        if !counted {
            self.hold(n, HeldWhy::NotCounted);
            return Ok(());
        }
        let id = me.id.clone();
        self.once(&id, &format!("view\u{1f}{view}"))?;
        self.write(
            &id,
            Body::ViewChanged(ViewChanged {
                version: 1,
                sync: self.sync.clone(),
                said_by: self.her(),
                base: current.map(|item| item.base.clone()).unwrap_or_default(),
                view,
                text,
                because,
            }),
        )
    }

    /// Something not wrapped up yet: new, reworded (`item`, a label she was
    /// shown), or closed.
    fn open(&mut self, n: usize, action: &Value) -> Result<(), Fallback> {
        let me = self.me()?;
        let closed = flag(action, "closed")?;
        let because = self.evidence(action)?;
        let (item, current) = match field(action, "item") {
            None => (
                format!("o{}", minted(&format!("open\u{1f}{}\u{1f}{n}", self.run))),
                None,
            ),
            Some(label) => {
                let label = label.as_str().ok_or(Fallback::BadField)?.trim();
                let item = self.context.open.get(label).ok_or(Fallback::UnknownLabel)?;
                (item.clone(), me.open.get(item))
            }
        };
        let text = match (optional_text(action, "text", TEXT_CHARS)?, current) {
            (Some(text), _) => text,
            (None, Some(current)) => current.current.value.text.clone(),
            (None, None) => return Err(Fallback::BadField),
        };
        if current.is_none() && closed {
            return Err(Fallback::BadField);
        }
        if current.is_some_and(partners) {
            self.hold(n, HeldWhy::PartnerSet);
            return Ok(());
        }
        let id = me.id.clone();
        self.once(&id, &format!("open\u{1f}{item}"))?;
        self.write(
            &id,
            Body::OpenItem(OpenItem {
                version: 1,
                sync: self.sync.clone(),
                said_by: self.her(),
                base: current.map(|item| item.base.clone()).unwrap_or_default(),
                item,
                text,
                because,
                closed,
            }),
        )
    }

    fn absorb(&mut self, n: usize, action: &Value) -> Result<(), Fallback> {
        let me = self.me()?;
        let into = self.label_of(action, "into")?;
        let Some(into_at) = into.at else {
            return Err(Fallback::UnknownLabel);
        };
        let how: How = enum_of(action, "how")?;
        let reason = text(action, "reason", REASON_CHARS, true)?;
        let because = self.evidence(action)?;
        let about_you =
            Key::parse(&into.key).is_ok_and(|key| key.is_about_you(&self.context.audience));
        // Only a minted node goes into another, and not what she watched or
        // read with the partner: that only ever takes others in.
        let goes = me
            .key
            .as_deref()
            .and_then(|key| Key::parse(key).ok())
            .is_some_and(|key| matches!(key, Key::Minted(_)) && !key.is_together());
        if !goes || Some(into_at) == self.context.at || about_you {
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
        // Anything else the partner set that the node going away would take
        // with it: a head field, a view, an open item or a waiting intent,
        // any current version of it theirs.
        let partner_items = by_partner(&me.kind)
            || by_partner(&me.name)
            || by_partner(&me.summary)
            || by_partner(&me.status)
            || by_partner(&me.done_when)
            || by_partner(&me.who_knows)
            || by_partner(&me.object)
            || me.progress.values().any(partners)
            || me.views.values().any(partners)
            || me
                .open
                .values()
                .filter(|open| !open.current.value.closed)
                .any(partners)
            || me
                .intents
                .values()
                .filter(|said| said.current.value.status == IntentStatus::Waiting)
                .any(partners);
        if me.created_by == Some(Author::Owner) || partner_rows || partner_items {
            self.hold(n, HeldWhy::PartnerSet);
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
        // Filing the inbox alone, she sees no relation to take back.
        if self.context.at.is_none() {
            return Err(Fallback::NotAllowed);
        }
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
            self.hold(n, HeldWhy::PartnerSet);
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
        let me = self.me()?;
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
            self.hold(n, HeldWhy::PartnerSet);
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
    if context.prepared.local && input.sync == Sync::Shared {
        return Err(NodeError::new(
            "a node taken back from sync is kept with local rows only",
        ));
    }
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
    let (mut writes, placed, held, fallback) = match read {
        Ok(()) => (
            reader.writes,
            reader.placed.into_iter().collect(),
            reader.held,
            None,
        ),
        Err(why) => (Vec::new(), Vec::new(), Vec::new(), Some(why)),
    };
    // Filing the inbox with no node kept leaves no mark: the inbox items
    // she placed leave the inbox, and the nodes they went on are due.
    if let Some(at) = context.at {
        let me = &context.full.folded.nodes[at];
        // What she saw: the newest clock per device of what the node
        // watches and of the nodes shown with it, the newest devices kept,
        // the rest under one floor.
        let shown: Vec<&str> = context
            .nodes
            .values()
            .map(|&other| context.full.folded.nodes[other].id.as_str())
            .collect();
        let mut seen: Vec<(String, Rev)> = Watch::new(&context.full)
            .newest(&context.audience, &me.id, &shown)
            .into_iter()
            .collect();
        seen.sort_by(|a, b| b.1.cmp(&a.1));
        let floor = seen
            .iter()
            .skip(SEEN_DEVICES)
            .map(|(_, rev)| rev.wall_ms)
            .max();
        seen.truncate(SEEN_DEVICES);
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
        // The mark reads back as it was written, or the queue would never
        // move.
        let upkept = Body::read("node.upkept", &upkept.to_value())?;
        writes.push(Write {
            id: row_id(&input.run, writes.len()),
            node: me.id.clone(),
            kind: "node.upkept".to_string(),
            body: upkept.to_value(),
            to: None,
            takes: Vec::new(),
        });
    }
    Ok(UpkeepRead {
        writes,
        placed,
        held,
        fallback,
    })
}
