//! What a node row's payload holds: one body per kind, each with its
//! version, and its one byte form (canonical JSON, as a row's).
//!
//! Every body carries `sync` and `saidBy` ([`crate::tags`]). A head change
//! carries `base`: the row ids of the versions it was written against
//! (empty for the first). What she concluded carries `because`: the ids of
//! its evidence (`span:<row>`, `entry:<row>`, `message:<id>`…), at least one.
//! Times are not in a body: a row's clock says when it was written.
//!
//! | kind | body (besides `version`, `sync`, `saidBy`) |
//! | --- | --- |
//! | `node.created` | `kind`, `key`, `name`, `object?` |
//! | `node.kind_changed` | `base`, `kind`, `reason`, `because` |
//! | `node.revised` | `base`, `field` (`name`, `summary`, `status`, `who_knows`, `object`, `progress`), `person?` (progress only), `value` |
//! | `node.view_changed` | `base`, `view`, `text`, `because` |
//! | `node.open_item` | `base`, `item`, `text`, `because`, `closed?` |
//! | `node.intent` | `base`, `intent`, `text`, `because`, `status`, `trigger?` |
//! | `node.span` | `audience`, `text`, `refs?`, `from?`, `to?` |
//! | `node.entry` | `audience`, `text`, `refs?`, `quoteOnly?` |
//! | `node.related` | `relation`, `to` (the other node's key), `reason`, `because` |
//! | `node.unrelated` | `base` (the related rows it takes back), `reason` |
//! | `node.done_when` | `base`, `text` (what "done" is, or null for none), `because` |
//! | `node.split_off` | `to` (the new node's key), `spans` (the rows of this node it takes), `reason`, `because` |
//! | `node.absorbed` | `into` (the key it goes into), `how` (`merged`, `folded_back`), `reason`, `because` |
//! | `node.used` | `via` (the related rows recall came along; empty: the node itself) |
//! | `node.upkept` | `run`, `seen` (per device, the newest row clock she had in view), `floor?`, `wrote?` (the rows the run wrote), `fallback?` |
//!
//! Every body refuses fields it does not know, and lists must be sorted
//! with no repeats, so each body has exactly one byte form. Node kinds the
//! core does not read (the native app's older rows) are not bodies here; the
//! fold passes their rows by.

use std::collections::BTreeMap;

use myriad_persona_sync::{
    Rev,
    canonical::{canonical_bytes, digest},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    NodeError, check_token,
    key::{Key, Kind},
    limits,
    tags::{SaidBy, Sync, check_ids, check_item_id, check_said_by, check_text},
};

/// The body format this core writes and the newest it reads.
pub const BODY_VERSION: u32 = 1;

/// Where a node stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    /// A task that never ends but is always being done (每周发版).
    Ongoing,
    /// 歇着: not in the main view, recalled later; a new span wakes it.
    Resting,
    Done,
    Dropped,
}

impl Status {
    /// A person is never done.
    pub fn fits(self, kind: Kind) -> bool {
        kind != Kind::Person || matches!(self, Self::Active | Self::Resting)
    }
}

/// Where something she means to say stands: 等着、说了、完了、放下.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    Waiting,
    Said,
    Done,
    Dropped,
}

/// The five ways two nodes relate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    /// 属于: this must be done or dropped before that one is done.
    PartOf,
    /// 挡着: this is why that one is stuck.
    Blocks,
    /// 撞上: the two do the same thing, or pull opposite ways.
    Clashes,
    /// 牵涉: that person or object is in this one's state.
    Involves,
    /// 相关: she finds them related, none of the above.
    Related,
}

impl Relation {
    /// Whether `a → b` is the same relation as `b → a`.
    pub fn symmetric(self) -> bool {
        matches!(self, Self::Clashes | Self::Related)
    }
}

/// What an outside object is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// A repository: only the node of its `repo:` key.
    Repo,
    /// A work: a show, a book, an album, a game.
    Work,
    Place,
    Other,
}

/// The outside thing a topic (or a task) is about. Its text, lyrics or
/// comments are never kept: only its name, aliases, and how far each
/// person has got (a separate field, so nobody's progress spoils another's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Object {
    pub kind: ObjectKind,
    pub name: String,
    /// Other names it goes by, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

impl Object {
    fn check(&self) -> Result<(), NodeError> {
        check_text("object name", &self.name, limits::NAME_CHARS)?;
        if self.aliases.len() > limits::ALIASES {
            return Err(NodeError::new(format!(
                "aliases hold more than {}",
                limits::ALIASES
            )));
        }
        for alias in &self.aliases {
            check_text("alias", alias, limits::NAME_CHARS)?;
        }
        if !self.aliases.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(NodeError::new("aliases are not sorted or repeat"));
        }
        Ok(())
    }
}

/// One sentence of a summary and what it rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sentence {
    pub text: String,
    pub because: Vec<String>,
}

/// When an intent comes due: an absolute time and the zone it was meant in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Trigger {
    /// Unix microseconds.
    pub at: i64,
    /// An IANA name (`Asia/Tokyo`) or a fixed offset (`+09:00`).
    pub zone: String,
}

impl Trigger {
    fn check(&self) -> Result<(), NodeError> {
        let fixed = self.zone.len() == 6
            && self.zone.starts_with(['+', '-'])
            && self.zone.as_bytes()[3] == b':'
            && self
                .zone
                .bytes()
                .enumerate()
                .all(|(at, byte)| matches!(at, 0 | 3) || byte.is_ascii_digit());
        let named = !self.zone.is_empty()
            && self.zone.len() <= 64
            && self.zone.as_bytes()[0].is_ascii_alphabetic()
            && self.zone.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+')
            });
        if !fixed && !named {
            return Err(NodeError::new("trigger zone is an IANA name or ±hh:mm"));
        }
        Ok(())
    }
}

/// `node.created`: the node was made, with its first kind, name and object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Created {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub kind: Kind,
    pub key: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<Object>,
    /// Where its id was made, when that is not its row's audience and key
    /// now: the partner's id changed and the node moved with its audience
    /// (`crate::migrate`). Its id stays the one made from these.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub born: Option<Born>,
}

/// The audience and key a moved node's id was made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Born {
    pub audience: String,
    pub key: String,
}

/// `node.kind_changed`: she (or the partner) changed what the node is.
/// History, not a relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KindChanged {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub base: Vec<String>,
    pub kind: Kind,
    /// Which of the two judgments, in her words.
    pub reason: String,
    pub because: Vec<String>,
}

/// Which head field a `node.revised` row changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    /// `value`: a text.
    Name,
    /// `value`: `[{text, because}]`, every sentence with its evidence.
    Summary,
    /// `value`: a [`Status`].
    Status,
    /// `value`: the sorted ids of who knows, only people of this audience.
    WhoKnows,
    /// `value`: an [`Object`], or null for none.
    Object,
    /// `value`: how far `person` has got with the object, or null.
    Progress,
}

/// `node.revised`: one head field, revised against `base`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Revised {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub base: Vec<String>,
    pub field: Field,
    /// Whose progress, for `progress` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    pub value: Value,
}

/// A `node.revised` value, read by its field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Name(String),
    Summary(Vec<Sentence>),
    Status(Status),
    WhoKnows(Vec<String>),
    Object(Option<Object>),
    Progress(String, Option<String>),
}

impl Change {
    /// Its value as the format writes it.
    pub fn to_value(&self) -> Value {
        let value = match self {
            Self::Name(name) => serde_json::to_value(name),
            Self::Summary(sentences) => serde_json::to_value(sentences),
            Self::Status(status) => serde_json::to_value(status),
            Self::WhoKnows(people) => serde_json::to_value(people),
            Self::Object(object) => serde_json::to_value(object),
            Self::Progress(_, progress) => serde_json::to_value(progress),
        };
        value.expect("a change serializes")
    }
}

impl Revised {
    /// Its value, read by its field and checked.
    pub fn change(&self) -> Result<Change, NodeError> {
        if self.person.is_some() != (self.field == Field::Progress) {
            return Err(NodeError::new(
                "person is given for progress, and only for it",
            ));
        }
        Ok(match self.field {
            Field::Name => {
                let name: String = typed(&self.value)?;
                check_text("name", &name, limits::NAME_CHARS)?;
                Change::Name(name)
            }
            Field::Summary => {
                let sentences: Vec<Sentence> = typed(&self.value)?;
                if sentences.len() > limits::SUMMARY_SENTENCES {
                    return Err(NodeError::new(format!(
                        "summary holds more than {} sentences",
                        limits::SUMMARY_SENTENCES
                    )));
                }
                for sentence in &sentences {
                    check_text("sentence", &sentence.text, limits::SENTENCE_CHARS)?;
                    check_because(&sentence.because)?;
                }
                Change::Summary(sentences)
            }
            Field::Status => Change::Status(typed(&self.value)?),
            Field::WhoKnows => {
                let people: Vec<String> = typed(&self.value)?;
                check_ids("whoKnows", &people, false, false, limits::WHO_KNOWS)?;
                Change::WhoKnows(people)
            }
            Field::Object => {
                let object: Option<Object> = typed(&self.value)?;
                if let Some(object) = &object {
                    object.check()?;
                }
                Change::Object(object)
            }
            Field::Progress => {
                let person = self.person.clone().unwrap_or_default();
                check_token("person", &person)?;
                let progress: Option<String> = typed(&self.value)?;
                if let Some(progress) = &progress {
                    check_text("progress", progress, limits::PROGRESS_CHARS)?;
                }
                Change::Progress(person, progress)
            }
        })
    }
}

/// `node.view_changed`: one of her views on it, revised against `base`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewChanged {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub base: Vec<String>,
    /// The view's id, minted when it was first written.
    pub view: String,
    pub text: String,
    pub because: Vec<String>,
}

/// `node.open_item`: something not wrapped up yet (没收尾的事), or closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenItem {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub base: Vec<String>,
    pub item: String,
    pub text: String,
    pub because: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub closed: bool,
}

/// `node.intent`: something she means to say (想说没说的话). Only the
/// partner's words or her own judgment make one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Intent {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub base: Vec<String>,
    pub intent: String,
    pub text: String,
    pub because: Vec<String>,
    pub status: IntentStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<Trigger>,
}

/// `node.span`: one span of experience, about this one thing, in her words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Span {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub audience: String,
    pub text: String,
    /// Where the originals are (`message:<id>`, `record:<id>`…), sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    /// When it happened, Unix microseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<i64>,
}

/// `node.entry`: a fact or a preference taken from the originals. Without
/// a reference it is marked quote-only (只有引文): the original was not
/// found again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub audience: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub quote_only: bool,
}

/// `node.related`: this node relates to the node of `to` in the same
/// audience. Written on the node it goes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Related {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub relation: Relation,
    /// The other node's key: its id is made in this row's audience, so a
    /// relation never crosses audiences.
    pub to: String,
    pub reason: String,
    pub because: Vec<String>,
}

/// `node.unrelated`: the `node.related` rows in `base`, on this node, are
/// taken back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Unrelated {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub base: Vec<String>,
    pub reason: String,
}

/// `node.done_when`: what "done" is for it, in her words, or none. With
/// one it is something in progress; with none it is a topic, or, still
/// always being done, something ongoing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DoneWhen {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub base: Vec<String>,
    pub text: Option<String>,
    pub because: Vec<String>,
}

/// `node.split_off`: written on the node a part was split from; the spans
/// it names (rows of this node) are on the new node of `to` now. It stays
/// on this node so that forgetting the new node still knows which spans
/// went with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SplitOff {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub to: String,
    pub spans: Vec<String>,
    pub reason: String,
    pub because: Vec<String>,
}

/// How a node went into another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    /// The two were one thing (并入).
    Merged,
    /// A part with no state of its own any more, back into its whole as
    /// experience (收回).
    FoldedBack,
}

/// `node.absorbed`: this node went into the node of `into`; its spans and
/// entries are on that one now, and it reads as gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Absorbed {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub into: String,
    pub how: How,
    pub reason: String,
    pub because: Vec<String>,
}

/// `node.used`: recall brought this node in, along the related rows in
/// `via` (none: by itself). What fades is what is not used, not what is
/// old.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Used {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub via: Vec<String>,
}

/// `node.upkept`: her upkeep looked at this node. `seen` holds, per
/// device, the newest clock of the rows she had in view; past
/// [`limits::SEEN_DEVICES`] devices the oldest go, and `floor` (Unix ms)
/// stands for them: a row of a device not in `seen` counts as seen up to
/// it. `wrote` lists the rows the run itself wrote, which count as seen, so
/// what she wrote does not make the node due again (and nothing else
/// written meanwhile hides behind it). A `fallback` run changed nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Upkept {
    pub version: u32,
    pub sync: Sync,
    pub said_by: Vec<SaidBy>,
    pub run: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub seen: BTreeMap<String, Rev>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wrote: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One row's body, by its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Created(Created),
    KindChanged(KindChanged),
    Revised(Revised),
    ViewChanged(ViewChanged),
    OpenItem(OpenItem),
    Intent(Intent),
    Span(Span),
    Entry(Entry),
    Related(Related),
    Unrelated(Unrelated),
    DoneWhen(DoneWhen),
    SplitOff(SplitOff),
    Absorbed(Absorbed),
    Used(Used),
    Upkept(Upkept),
}

/// The kinds whose bodies the core reads, sorted.
pub const READ: &[&str] = &[
    "node.absorbed",
    "node.created",
    "node.done_when",
    "node.entry",
    "node.intent",
    "node.kind_changed",
    "node.open_item",
    "node.related",
    "node.revised",
    "node.span",
    "node.split_off",
    "node.unrelated",
    "node.upkept",
    "node.used",
    "node.view_changed",
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
    if version == 0 || version > u64::from(BODY_VERSION) {
        return Err(NodeError::new(format!(
            "body version is not one this core reads (1 to {BODY_VERSION})"
        )));
    }
    Ok(version)
}

fn check_because(because: &[String]) -> Result<(), NodeError> {
    check_ids("because", because, true, true, limits::BECAUSE_IDS)
}

fn check_base(base: &[String]) -> Result<(), NodeError> {
    check_ids("base", base, false, false, limits::BASES)
}

fn check_audience(audience: &str) -> Result<(), NodeError> {
    check_token("audience", audience)?;
    myriad_persona_log::row::Audience::parse(audience)
        .map(|_| ())
        .map_err(|_| NodeError::new("audience is not personal:, own: or group:"))
}

impl Body {
    /// Reads and checks the body of a row of `kind`.
    pub fn read(kind: &str, value: &Value) -> Result<Self, NodeError> {
        if !kind.starts_with("node.") || myriad_persona_log::kinds::rule(kind).is_none() {
            return Err(NodeError::new("kind is not a node kind the log holds"));
        }
        if !READ.contains(&kind) {
            return Err(NodeError::new("kind is not one the core reads"));
        }
        version_of(value)?;
        let body = match kind {
            "node.absorbed" => Self::Absorbed(typed(value)?),
            "node.created" => Self::Created(typed(value)?),
            "node.done_when" => Self::DoneWhen(typed(value)?),
            "node.entry" => Self::Entry(typed(value)?),
            "node.intent" => Self::Intent(typed(value)?),
            "node.kind_changed" => Self::KindChanged(typed(value)?),
            "node.open_item" => Self::OpenItem(typed(value)?),
            "node.related" => Self::Related(typed(value)?),
            "node.revised" => {
                // One byte form: the value as its field writes it (an
                // object's empty aliases left out).
                let mut revised: Revised = typed(value)?;
                revised.value = revised.change()?.to_value();
                Self::Revised(revised)
            }
            "node.span" => Self::Span(typed(value)?),
            "node.split_off" => Self::SplitOff(typed(value)?),
            "node.unrelated" => Self::Unrelated(typed(value)?),
            "node.upkept" => Self::Upkept(typed(value)?),
            "node.used" => Self::Used(typed(value)?),
            "node.view_changed" => Self::ViewChanged(typed(value)?),
            _ => unreachable!("READ lists every kind read here"),
        };
        body.check()?;
        Ok(body)
    }

    fn check(&self) -> Result<(), NodeError> {
        let (sync, said_by) = self.tags();
        sync.check()?;
        check_said_by(said_by)?;
        check_base(self.base())?;
        match self {
            Self::Created(body) => {
                let key = Key::parse(&body.key)?;
                if !key.kinds().contains(&body.kind) {
                    return Err(NodeError::new("kind does not fit the key"));
                }
                check_text("name", &body.name, limits::NAME_CHARS)?;
                if let Some(object) = &body.object {
                    object.check()?;
                }
                let repo_object = body
                    .object
                    .as_ref()
                    .is_some_and(|object| object.kind == ObjectKind::Repo);
                if matches!(key, Key::Repo(_)) != repo_object {
                    return Err(NodeError::new(
                        "a repo key has a repo object, and only a repo key",
                    ));
                }
                if let Some(born) = &body.born {
                    check_audience(&born.audience)?;
                    Key::parse(&born.key)?;
                }
            }
            Self::KindChanged(body) => {
                if body.kind == Kind::Person {
                    return Err(NodeError::new("a node never becomes a person"));
                }
                check_text("reason", &body.reason, limits::REASON_CHARS)?;
                check_because(&body.because)?;
            }
            Self::Revised(body) => {
                body.change()?;
            }
            Self::ViewChanged(body) => {
                check_item_id("view", &body.view)?;
                check_text("text", &body.text, limits::TEXT_CHARS)?;
                check_because(&body.because)?;
            }
            Self::OpenItem(body) => {
                check_item_id("item", &body.item)?;
                check_text("text", &body.text, limits::TEXT_CHARS)?;
                check_because(&body.because)?;
            }
            Self::Intent(body) => {
                check_item_id("intent", &body.intent)?;
                check_text("text", &body.text, limits::TEXT_CHARS)?;
                check_because(&body.because)?;
                if let Some(trigger) = &body.trigger {
                    trigger.check()?;
                }
            }
            Self::Span(body) => {
                check_audience(&body.audience)?;
                check_text("text", &body.text, limits::TEXT_CHARS)?;
                check_ids("refs", &body.refs, false, true, limits::REFS)?;
                if let (Some(from), Some(to)) = (body.from, body.to)
                    && from > to
                {
                    return Err(NodeError::new("a span ends before it starts"));
                }
            }
            Self::Entry(body) => {
                check_audience(&body.audience)?;
                check_text("text", &body.text, limits::TEXT_CHARS)?;
                check_ids("refs", &body.refs, false, true, limits::REFS)?;
                if body.quote_only != body.refs.is_empty() {
                    return Err(NodeError::new(
                        "an entry has refs, or is quote-only without them",
                    ));
                }
            }
            Self::Related(body) => {
                Key::parse(&body.to)?;
                check_text("reason", &body.reason, limits::REASON_CHARS)?;
                check_because(&body.because)?;
            }
            Self::Unrelated(body) => {
                if body.base.is_empty() {
                    return Err(NodeError::new("base is empty"));
                }
                check_text("reason", &body.reason, limits::REASON_CHARS)?;
            }
            Self::DoneWhen(body) => {
                if let Some(text) = &body.text {
                    check_text("text", text, limits::TEXT_CHARS)?;
                }
                check_because(&body.because)?;
            }
            Self::SplitOff(body) => {
                if !matches!(Key::parse(&body.to)?, Key::Minted(_)) {
                    return Err(NodeError::new("a part split off has a minted key"));
                }
                check_ids("spans", &body.spans, true, false, limits::REFS)?;
                check_text("reason", &body.reason, limits::REASON_CHARS)?;
                check_because(&body.because)?;
            }
            Self::Absorbed(body) => {
                Key::parse(&body.into)?;
                check_text("reason", &body.reason, limits::REASON_CHARS)?;
                check_because(&body.because)?;
            }
            Self::Used(body) => {
                check_ids("via", &body.via, false, false, limits::BASES)?;
            }
            Self::Upkept(body) => {
                check_token("run", &body.run)?;
                if body.seen.len() > limits::SEEN_DEVICES {
                    return Err(NodeError::new(format!(
                        "seen holds more than {} devices",
                        limits::SEEN_DEVICES
                    )));
                }
                if body.seen.iter().any(|(device, rev)| *device != rev.node) {
                    return Err(NodeError::new("seen names each clock by its own device"));
                }
                check_ids("wrote", &body.wrote, false, false, limits::UPKEEP_WRITES)?;
                if let Some(fallback) = &body.fallback {
                    check_item_id("fallback", fallback)?;
                }
            }
        }
        Ok(())
    }

    /// Its sync and who-said-it tags.
    pub fn tags(&self) -> (&Sync, &[SaidBy]) {
        match self {
            Self::Created(Created { sync, said_by, .. })
            | Self::KindChanged(KindChanged { sync, said_by, .. })
            | Self::Revised(Revised { sync, said_by, .. })
            | Self::ViewChanged(ViewChanged { sync, said_by, .. })
            | Self::OpenItem(OpenItem { sync, said_by, .. })
            | Self::Intent(Intent { sync, said_by, .. })
            | Self::Span(Span { sync, said_by, .. })
            | Self::Entry(Entry { sync, said_by, .. })
            | Self::Related(Related { sync, said_by, .. })
            | Self::Unrelated(Unrelated { sync, said_by, .. })
            | Self::DoneWhen(DoneWhen { sync, said_by, .. })
            | Self::SplitOff(SplitOff { sync, said_by, .. })
            | Self::Absorbed(Absorbed { sync, said_by, .. })
            | Self::Used(Used { sync, said_by, .. })
            | Self::Upkept(Upkept { sync, said_by, .. }) => (sync, said_by),
        }
    }

    /// The versions it was written against; empty for a body without any.
    pub fn base(&self) -> &[String] {
        match self {
            Self::KindChanged(KindChanged { base, .. })
            | Self::Revised(Revised { base, .. })
            | Self::ViewChanged(ViewChanged { base, .. })
            | Self::OpenItem(OpenItem { base, .. })
            | Self::Intent(Intent { base, .. })
            | Self::Unrelated(Unrelated { base, .. })
            | Self::DoneWhen(DoneWhen { base, .. }) => base,
            Self::Created(_)
            | Self::Span(_)
            | Self::Entry(_)
            | Self::Related(_)
            | Self::SplitOff(_)
            | Self::Absorbed(_)
            | Self::Used(_)
            | Self::Upkept(_) => &[],
        }
    }

    /// Its JSON, as the format writes it.
    pub fn to_value(&self) -> Value {
        let value = match self {
            Self::Created(body) => serde_json::to_value(body),
            Self::KindChanged(body) => serde_json::to_value(body),
            Self::Revised(body) => serde_json::to_value(body),
            Self::ViewChanged(body) => serde_json::to_value(body),
            Self::OpenItem(body) => serde_json::to_value(body),
            Self::Intent(body) => serde_json::to_value(body),
            Self::Span(body) => serde_json::to_value(body),
            Self::Entry(body) => serde_json::to_value(body),
            Self::Related(body) => serde_json::to_value(body),
            Self::Unrelated(body) => serde_json::to_value(body),
            Self::DoneWhen(body) => serde_json::to_value(body),
            Self::SplitOff(body) => serde_json::to_value(body),
            Self::Absorbed(body) => serde_json::to_value(body),
            Self::Used(body) => serde_json::to_value(body),
            Self::Upkept(body) => serde_json::to_value(body),
        };
        // Strings, numbers, booleans, enums and JSON: always serializes.
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
