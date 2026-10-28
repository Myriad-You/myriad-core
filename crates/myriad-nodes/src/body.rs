//! What a node row's payload holds: one body per kind, each carrying its
//! version, and its one byte form (canonical JSON, as a row's).
//!
//! A body is complete for its row: a moment's line is in the row that
//! raised or noted it, not left on a node document, so the node folds back
//! from its rows alone. Fields are the native app's (`AgentNodeEvent`) with
//! what the fold needs added; `node.said` and `decided.happened` come from
//! the site. Times are not in a body: a row's clock says when.
//!
//! Every body refuses fields it does not know, so two sides never drift
//! apart silently. Bodies of node kinds this batch does not read are kept
//! whole ([`Body::Later`]).

use myriad_persona_sync::canonical::{canonical_bytes, digest};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{NodeError, check_token, id};

/// The body format this core writes and the newest it reads.
pub const BODY_VERSION: u32 = 1;

/// What a node is. The native app reads a node without one as a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    /// A repository the owner works in; keyed by its name.
    Repo,
    /// A thread that kept coming back (`topic:<id>`).
    Topic,
    /// One day's loose ends (`loose:<day>`).
    Loose,
    /// What she watched, read or saw talked about with the owner
    /// (`together:<topic>`).
    Together,
}

/// `node.created`: the node was made. Whose it is, its key, form and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Created {
    pub version: u32,
    pub owner: String,
    pub key: String,
    pub form: Form,
    /// May be empty: the side that shows it names it.
    #[serde(default)]
    pub title: String,
}

/// `node.decided`: what she decided about one event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Decided {
    pub version: u32,
    /// The event she decided about; the node refers to it.
    pub event: String,
    /// The fact it came from, where the writer has one (the native app).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fact: Option<String>,
    /// What had happened (`safeFacts.happened`); may be empty.
    #[serde(default)]
    pub happened: String,
    /// ignore, remember, speak, ask, propose_work…
    pub action: String,
    /// Her reason, as the deciding side gives it (a code or a few words).
    #[serde(default)]
    pub reason: String,
}

/// `node.raised`: a line she had for the owner, not said yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Raised {
    pub version: u32,
    pub event: String,
    #[serde(default)]
    pub happened: String,
    pub action: String,
    /// In her words.
    pub line: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub cannot_wait: bool,
}

/// `node.noted`: what she noted about the owner on this node (a moment
/// she kept as a memory); it counts as said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Noted {
    pub version: u32,
    pub event: String,
    #[serde(default)]
    pub happened: String,
    pub line: String,
}

/// `node.said` (the site's): her line about `event` was said in the end;
/// `line` is what she said, when it differs from what she raised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Said {
    pub version: u32,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
}

/// `node.opened` (the owner opened her raised hand) and `node.notified`
/// (a notification said only that she has something): the event alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Event {
    pub version: u32,
    pub event: String,
}

/// `node.shown`: the gist of her line appeared beside the owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Shown {
    pub version: u32,
    pub event: String,
    /// The pause it was shown at (`cannot_wait`, `none`, a breakpoint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause: Option<String>,
}

/// `node.faded`: her line went unsaid, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Faded {
    pub version: u32,
    pub event: String,
    pub why: String,
}

/// `node.observed`: she saw a fact about the node (the native app).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observed {
    pub version: u32,
    pub fact: String,
    #[serde(default)]
    pub happened: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One row's body, by its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Created(Created),
    Decided(Decided),
    Raised(Raised),
    Noted(Noted),
    Said(Said),
    Opened(Event),
    Notified(Event),
    Shown(Shown),
    Faded(Faded),
    Observed(Observed),
    /// A node kind this batch does not read (her upkeep, views, links, a
    /// talk, a re-tag, an act): kept whole, folded only as a change.
    Later(Value),
}

/// The kinds whose bodies this batch reads, sorted.
pub const READ: &[&str] = &[
    "node.created",
    "node.decided",
    "node.faded",
    "node.noted",
    "node.notified",
    "node.observed",
    "node.opened",
    "node.raised",
    "node.said",
    "node.shown",
];

fn typed<T: DeserializeOwned>(value: &Value) -> Result<T, NodeError> {
    // The message names the field, never the value.
    serde_json::from_value(value.clone()).map_err(|error| {
        let message = error.to_string();
        let named = message
            .split(", expected")
            .next()
            .unwrap_or("")
            .split(" at line")
            .next()
            .unwrap_or("");
        if message.starts_with("unknown field") || message.starts_with("missing field") {
            NodeError::new(format!("body: {named}"))
        } else {
            NodeError::new("body does not fit its kind")
        }
    })
}

fn version_of(value: &Value) -> Result<u64, NodeError> {
    let version = value
        .as_object()
        .ok_or_else(|| NodeError::new("a body is an object"))?
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(|| NodeError::new("a body carries its version"))?;
    if version == 0 {
        return Err(NodeError::new("a body's version starts at 1"));
    }
    Ok(version)
}

fn check_line(line: &str) -> Result<(), NodeError> {
    if line.trim().is_empty() {
        return Err(NodeError::new("line is empty"));
    }
    Ok(())
}

fn check_action(action: &str) -> Result<(), NodeError> {
    check_token("action", action)?;
    if !action
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
    {
        return Err(NodeError::new("action is lowercase a-z and _"));
    }
    Ok(())
}

impl Body {
    /// Reads and checks the body of a row of `kind`.
    pub fn read(kind: &str, value: &Value) -> Result<Self, NodeError> {
        if !kind.starts_with("node.") || myriad_persona_log::kinds::rule(kind).is_none() {
            return Err(NodeError::new("kind is not a node kind the log holds"));
        }
        let version = version_of(value)?;
        if !READ.contains(&kind) {
            return Ok(Self::Later(value.clone()));
        }
        if version > u64::from(BODY_VERSION) {
            return Err(NodeError::new(format!(
                "body version is not one this core reads (1 to {BODY_VERSION})"
            )));
        }
        let body = match kind {
            "node.created" => Self::Created(typed(value)?),
            "node.decided" => Self::Decided(typed(value)?),
            "node.raised" => Self::Raised(typed(value)?),
            "node.noted" => Self::Noted(typed(value)?),
            "node.said" => Self::Said(typed(value)?),
            "node.opened" => Self::Opened(typed(value)?),
            "node.notified" => Self::Notified(typed(value)?),
            "node.shown" => Self::Shown(typed(value)?),
            "node.faded" => Self::Faded(typed(value)?),
            "node.observed" => Self::Observed(typed(value)?),
            _ => unreachable!("READ lists every kind read here"),
        };
        body.check()?;
        Ok(body)
    }

    fn check(&self) -> Result<(), NodeError> {
        if let Some(event) = self.event() {
            check_token("event", event)?;
        }
        match self {
            Self::Created(created) => {
                id::check_owner(&created.owner)?;
                id::check_key(&created.key)?;
            }
            Self::Decided(decided) => {
                check_action(&decided.action)?;
                if let Some(fact) = &decided.fact {
                    check_token("fact", fact)?;
                }
            }
            Self::Raised(raised) => {
                check_action(&raised.action)?;
                check_line(&raised.line)?;
            }
            Self::Noted(noted) => check_line(&noted.line)?,
            Self::Said(said) => {
                if let Some(line) = &said.line {
                    check_line(line)?;
                }
            }
            Self::Faded(faded) => check_token("why", &faded.why)?,
            Self::Observed(observed) => check_token("fact", &observed.fact)?,
            Self::Opened(_) | Self::Notified(_) | Self::Shown(_) | Self::Later(_) => {}
        }
        Ok(())
    }

    /// The event a body is about, when it is about one.
    pub fn event(&self) -> Option<&str> {
        match self {
            Self::Decided(Decided { event, .. })
            | Self::Raised(Raised { event, .. })
            | Self::Noted(Noted { event, .. })
            | Self::Said(Said { event, .. })
            | Self::Opened(Event { event, .. })
            | Self::Notified(Event { event, .. })
            | Self::Shown(Shown { event, .. })
            | Self::Faded(Faded { event, .. }) => Some(event),
            Self::Created(_) | Self::Observed(_) | Self::Later(_) => None,
        }
    }

    /// Its JSON, as the format writes it.
    pub fn to_value(&self) -> Value {
        let value = match self {
            Self::Created(body) => serde_json::to_value(body),
            Self::Decided(body) => serde_json::to_value(body),
            Self::Raised(body) => serde_json::to_value(body),
            Self::Noted(body) => serde_json::to_value(body),
            Self::Said(body) => serde_json::to_value(body),
            Self::Opened(body) | Self::Notified(body) => serde_json::to_value(body),
            Self::Shown(body) => serde_json::to_value(body),
            Self::Faded(body) => serde_json::to_value(body),
            Self::Observed(body) => serde_json::to_value(body),
            Self::Later(value) => Ok(value.clone()),
        };
        // Strings, numbers, booleans and an enum: always serializes.
        value.expect("a body serializes")
    }

    /// Its one byte form: canonical JSON.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        canonical_bytes(&self.to_value())
    }

    /// The first 16 bytes of sha256 of its canonical bytes, in hex.
    pub fn digest(&self) -> String {
        digest(&self.to_value())
    }
}
