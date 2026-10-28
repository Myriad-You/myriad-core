//! What a decision about one event writes: the node it files under and
//! the rows to append, each with its body.
//!
//! A decision is filed once: the node made the first time (`created`), the
//! decision kept with a reference to its event (`decided`), and her line
//! raised (`raised`) or her note kept (`noted`) when she had one. The same
//! event again writes nothing. A forgotten node takes nothing more: its id
//! is made from its key, and a forget absorbs every row of it, later ones
//! too.
//!
//! Rows come back as drafts: the caller stamps each with the log's clock,
//! its count and its payload reference (`log.clock_next`, `dseq`), keeps
//! the body under that reference, and writes them in this order in one
//! transaction. Ids are the caller's.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_log::{Author, Replicate, row::Audience};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{self, BODY_VERSION, Body, Form},
    check_token,
    fold::Node,
    id::{self, Place, node_id, private},
};

/// What she decided about one event.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decision {
    pub event: String,
    #[serde(default)]
    pub fact: Option<String>,
    #[serde(default)]
    pub happened: String,
    /// ignore, remember, speak, ask, propose_work…
    pub action: String,
    #[serde(default)]
    pub reason: String,
    /// Her line (what she would say or ask) or her note (`remember`).
    #[serde(default)]
    pub line: Option<String>,
    #[serde(default)]
    pub cannot_wait: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileInput {
    /// The owner whose node it is.
    pub owner: String,
    /// The node's audience; the owner's private one when absent.
    #[serde(default)]
    pub audience: Option<String>,
    /// A moment's kind and facts, to find the node from (`id::place`)…
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub safe_facts: Option<BTreeMap<String, String>>,
    /// …or the node's place itself (a side that knows more than the facts).
    #[serde(default)]
    pub place: Option<Place>,
    /// The node as it folds now (`nodes.fold`); absent when there is none.
    #[serde(default)]
    pub node: Option<Node>,
    pub decision: Decision,
    /// Ids for the rows, used in order; three are always enough.
    pub ids: Vec<String>,
}

/// One row to append: the envelope fields the rules decide, and its body.
/// The caller adds `version`, `origin`, `epoch`, `dseq`, `hlc` and
/// `payload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub id: String,
    pub audience: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    pub object: String,
    pub kind: String,
    pub author: Author,
    pub replicate: Replicate,
    pub body: Value,
    /// The body's one byte form, as a string.
    pub canonical: String,
    pub digest: String,
}

/// The node a thing files under, with its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placed {
    pub id: String,
    pub audience: String,
    pub form: Form,
    pub key: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Filed {
    /// Absent when the thing files under no node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<Placed>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Draft>,
    /// The node already holds this event: nothing is written again.
    #[serde(skip_serializing_if = "is_false")]
    pub already: bool,
    /// The node was forgotten: nothing is written.
    #[serde(skip_serializing_if = "is_false")]
    pub forgotten: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Who a node row is about: the owner, in an audience that is theirs.
fn subject(owner: &str, audience: &str) -> Option<String> {
    match Audience::parse(audience) {
        Ok(Audience::Personal(members)) if members.split(',').any(|one| one == owner) => {
            Some(owner.to_string())
        }
        _ => None,
    }
}

/// Whether a node row may leave the side that wrote it, by the native
/// app's default (`LogEnvelopeRules.nodeReplicate`): a group's stays, and so
/// does a node born of other agents' records or of the screen (a repository,
/// a together node); her topics and loose ends go.
pub fn replicate(audience: &str, form: Form) -> Replicate {
    match (Audience::parse(audience), form) {
        (Ok(Audience::Group(_)) | Err(_), _) => Replicate::Local,
        (_, Form::Repo | Form::Together) => Replicate::Local,
        (_, Form::Topic | Form::Loose) => Replicate::Shared,
    }
}

/// The audience given, or the owner's private one; checked.
fn audience_of(owner: &str, audience: Option<String>) -> Result<String, NodeError> {
    id::check_owner(owner)?;
    let audience = audience.unwrap_or_else(|| private(owner));
    check_token("audience", &audience)?;
    Audience::parse(&audience)
        .map_err(|_| NodeError::new("audience is not personal:, own: or group:"))?;
    Ok(audience)
}

struct Writer<'a> {
    owner: &'a str,
    audience: &'a str,
    object: String,
    form: Form,
    ids: std::slice::Iter<'a, String>,
}

impl Writer<'_> {
    fn row(&mut self, kind: &str, author: Author, body: Body) -> Result<Draft, NodeError> {
        let id = self
            .ids
            .next()
            .ok_or_else(|| NodeError::new("too few ids for the rows"))?;
        // Bodies written here are read back by the same rules.
        Body::read(kind, &body.to_value())?;
        Ok(Draft {
            id: id.clone(),
            audience: self.audience.to_string(),
            subject: subject(self.owner, self.audience),
            object: self.object.clone(),
            kind: kind.to_string(),
            author,
            replicate: replicate(self.audience, self.form),
            canonical: String::from_utf8(body.canonical_bytes())
                .map_err(|_| NodeError::new("canonical bytes are UTF-8"))?,
            digest: body.digest(),
            body: body.to_value(),
        })
    }
}

fn check_ids(ids: &[String]) -> Result<(), NodeError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        check_token("id", id)?;
        if !seen.insert(id) {
            return Err(NodeError::new("ids repeat"));
        }
    }
    Ok(())
}

fn check_node(node: &Node, id: &str, audience: &str) -> Result<(), NodeError> {
    if node.id != id || node.audience != audience {
        return Err(NodeError::new("node given is another node"));
    }
    Ok(())
}

/// Her line, trimmed; none when it is empty.
fn line_of(line: Option<&str>) -> Option<String> {
    line.map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

/// A moment, to find its node from before she decides (so what she
/// already made of it can go with it).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlaceInput {
    pub owner: String,
    #[serde(default)]
    pub audience: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub safe_facts: BTreeMap<String, String>,
}

/// The node a moment files under, with its id; none for a moment that
/// files under no node.
pub fn locate(input: PlaceInput) -> Result<Option<Placed>, NodeError> {
    let audience = audience_of(&input.owner, input.audience)?;
    let Some(place) = id::place(&input.kind, &input.safe_facts)? else {
        return Ok(None);
    };
    placed(&input.owner, audience, place).map(Some)
}

fn placed(owner: &str, audience: String, place: Place) -> Result<Placed, NodeError> {
    id::check_place(&place, owner, &audience)?;
    Ok(Placed {
        id: node_id(owner, &audience, &place.key)?,
        audience,
        form: place.form,
        key: place.key,
        title: place.title,
    })
}

pub fn file(input: FileInput) -> Result<Filed, NodeError> {
    let audience = audience_of(&input.owner, input.audience)?;
    check_ids(&input.ids)?;
    let place = match (input.place, input.kind) {
        (Some(place), None) if input.safe_facts.is_none() => place,
        (None, Some(kind)) => {
            let facts = input.safe_facts.unwrap_or_default();
            match id::place(&kind, &facts)? {
                Some(place) => place,
                None => {
                    return Ok(Filed {
                        place: None,
                        rows: Vec::new(),
                        already: false,
                        forgotten: false,
                    });
                }
            }
        }
        _ => return Err(NodeError::new("give a place, or a kind and its facts")),
    };
    let placed = placed(&input.owner, audience.clone(), place.clone())?;
    let id = placed.id.clone();
    let decision = input.decision;
    let mut filed = Filed {
        place: Some(placed),
        rows: Vec::new(),
        already: false,
        forgotten: false,
    };
    if let Some(node) = &input.node {
        check_node(node, &id, &audience)?;
        if node.forgotten {
            filed.forgotten = true;
            return Ok(filed);
        }
        if node.holds(&decision.event) {
            filed.already = true;
            return Ok(filed);
        }
    }

    let mut writer = Writer {
        owner: &input.owner,
        audience: &audience,
        object: format!("node:{id}"),
        form: place.form,
        ids: input.ids.iter(),
    };
    let made = input.node.as_ref().is_some_and(|node| node.key.is_some());
    if !made {
        let created = Body::Created(body::Created {
            version: BODY_VERSION,
            owner: input.owner.clone(),
            key: place.key,
            form: place.form,
            title: place.title,
        });
        filed
            .rows
            .push(writer.row("node.created", Author::Her, created)?);
    }
    let decided = Body::Decided(body::Decided {
        version: BODY_VERSION,
        event: decision.event.clone(),
        fact: decision.fact,
        happened: decision.happened.clone(),
        action: decision.action.clone(),
        reason: decision.reason,
    });
    filed
        .rows
        .push(writer.row("node.decided", Author::Her, decided)?);
    if let Some(line) = line_of(decision.line.as_deref()) {
        let (kind, body) = if decision.action == "remember" {
            (
                "node.noted",
                Body::Noted(body::Noted {
                    version: BODY_VERSION,
                    event: decision.event,
                    happened: decision.happened,
                    line,
                }),
            )
        } else {
            (
                "node.raised",
                Body::Raised(body::Raised {
                    version: BODY_VERSION,
                    event: decision.event,
                    happened: decision.happened,
                    action: decision.action,
                    line,
                    cannot_wait: decision.cannot_wait,
                }),
            )
        };
        filed.rows.push(writer.row(kind, Author::Her, body)?);
    }
    Ok(filed)
}

/// How her raised line ended up said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    /// She said it (the site, once it reached the owner): her row.
    Said,
    /// The owner opened it beside them (the Mac): theirs.
    Opened,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaidInput {
    pub owner: String,
    /// The node holding the moment, as it folds now.
    pub node: Node,
    pub event: String,
    pub how: How,
    /// What she said in the end, when it differs from her line (`said`).
    #[serde(default)]
    pub line: Option<String>,
    pub id: String,
}

/// The row saying her line about `event` was said. `said` writes one only
/// for a line she raised and has not said yet (a note is said already);
/// `opened` always writes one, as the Mac records each opening.
pub fn said(input: SaidInput) -> Result<Vec<Draft>, NodeError> {
    let audience = audience_of(&input.owner, Some(input.node.audience.clone()))?;
    check_ids(std::slice::from_ref(&input.id))?;
    check_token("event", &input.event)?;
    if let Some(key) = &input.node.key {
        check_node(
            &input.node,
            &node_id(&input.owner, &audience, key)?,
            &audience,
        )?;
    }
    if input.node.forgotten {
        return Ok(Vec::new());
    }
    let ids = [input.id];
    let mut writer = Writer {
        owner: &input.owner,
        audience: &audience,
        object: format!("node:{}", input.node.id),
        // A node whose created row has not arrived reads as a repository,
        // as the Mac reads one without a form.
        form: input.node.form.unwrap_or(Form::Repo),
        ids: ids.iter(),
    };
    let row = match input.how {
        How::Opened => writer.row(
            "node.opened",
            Author::Owner,
            Body::Opened(body::Event {
                version: BODY_VERSION,
                event: input.event,
            }),
        )?,
        How::Said => {
            let waiting = input.node.moments.iter().any(|moment| {
                moment.event == input.event && !moment.said && moment.action != "remember"
            });
            if !waiting {
                return Ok(Vec::new());
            }
            writer.row(
                "node.said",
                Author::Her,
                Body::Said(body::Said {
                    version: BODY_VERSION,
                    event: input.event,
                    line: line_of(input.line.as_deref()),
                }),
            )?
        }
    };
    Ok(vec![row])
}
