//! Her node rows on the wire, with their bodies: what one host core sends
//! another, and what it takes in.
//!
//! A batch is one audience's rows, with the payloads they point at: the
//! body of a row is its canonical bytes, base64, under the row's own
//! `payload.ref` (`{audience, rows, payloads: [{ref, data}], withheld}`,
//! the body of the link's `log.rows` frame, and a log page of the export
//! file). The row and its body are byte for byte what the writing side
//! stores.
//!
//! **Sending** ([`send`]): the rows the other side lacks (`log.answer`:
//! forgets first, local rows never), and none at all to a side that may not
//! hold the audience (`myriad_persona_log::holder`); then the rows it holds
//! but asked again for the bodies of (`ask`). Each row goes; its body goes
//! only when it is one the core reads and passes the checks below,
//! otherwise the row travels alone and the batch says why in `withheld`.
//! The row always goes because the other side's version vector counts it: a
//! missing row would stop that side's count there for good.
//!
//! **Taking** ([`take`]): the batch is checked as `log.admit` checks it
//! (clock, envelope, local, audience, members, epochs, the ledger, and the
//! audiences this side may hold); then each accepted row's body passes the
//! same checks as on sending, and must be exactly its canonical bytes. A
//! body that fails is dropped (the row stays, its body `withheld`): the
//! fold passes a row without a body by (`no_body`). A row this side asked
//! again for comes back as it holds it (`held`): it is not admitted again,
//! only its body taken.
//!
//! **What every side folds.** Every side that holds a row's body holds the
//! same bytes of it. A body withheld for good ([`Why::lasting`]: it is
//! local, keeps a key or a path, has no shape here, is too large, or is
//! somewhere it may not be) stays on the sides that wrote it, so those fold
//! it and the others pass its row by; that is the whole difference, and it
//! is listed by reason. A body withheld for now (`no_body`, `bad_body`: the
//! sender lacks it, or it was spoilt on the way) is asked for again
//! ([`Taken::ask`], the link's `log.vector` `ask`), at most once per link
//! session; once it comes, both sides fold the same.
//!
//! **Taken back from sync** (a `retract`, `myriad_persona_log::forget`):
//! a retracted node's rows go only as stubs, from every side, its home too,
//! so no body of it leaves the side that keeps it, in a batch or a file;
//! the retract goes first with the forgets. The rows still go, so vectors
//! count them. Every name of the node counts: the nodes that went into it
//! and the spans a split moved to it, as the rows' envelopes name them
//! (`to`, `takes`) on the ledger and in the rows, and as the bodies given
//! show for rows written before envelopes named them. A side takes a
//! retract as a forget unless it keeps the node (its own `retract.home`,
//! on its ledger), and rows of the node that come later as stubs.
//!
//! What a body must be to travel:
//!
//! - **One the core reads**: a node kind of [`crate::body::READ`], or a
//!   kind whose payload shape is in `myriad_persona_log::payloads`
//!   (segments, memories, mood), with no field its shape does not name
//!   (`bad_body`: a field the core cannot read is a field it cannot check).
//!   Anything else (the native app's older node kinds, judged cases until
//!   they have a shape) goes as a row only (`no_shape`).
//! - **Not too large**: at most [`MAX_BODY_BYTES`] canonical bytes, so it
//!   and its row fit one batch (`too_large`); a larger one would be refused
//!   by the link every time and hold the audience up behind it.
//! - **Shared**: its `sync` tag is `shared` (a local tag never leaves its
//!   device, even on a row marked shared: `not_shared`).
//! - **Of its row's audience**: a span or an entry tagged with another
//!   audience is one the fold passes by, unless it is a group's that the
//!   partner said in their own private talk (`audience_tag`).
//! - **No local path**: no reference or evidence is a `file:` reference or
//!   a path (`/…`, `~…`, `C:\…`). A raw record's id (`record:<session>#<n>`)
//!   travels as an id; the record itself stays on its device (`local_ref`).
//! - **Not someone else's node in hers**: no row of a node whose created
//!   row puts a person or a group in her own audience (`own:`), which every
//!   PC holds. Such a node is refused where it is made (`nodes.id`) and
//!   passed by in the fold; this keeps what a side wrote anyway off the
//!   wire (`person_elsewhere`). A side knows such a node by its created
//!   body, or by the sender's word that it withheld that body for this
//!   reason, taken only for a node the receiving side itself makes from a
//!   person or group key it knows (`people`, its partner, a key in the
//!   batch's bodies) in this audience: no sender's word takes a body of an
//!   ordinary node away. The created row and the node's later rows may
//!   come in different batches, so `take` lists the nodes it learnt of
//!   ([`Taken::elsewhere`]) and the caller keeps them and gives them to
//!   every later `take` and `send` (`elsewhere`): no body of such a node
//!   is taken or sent again, whatever batch it comes in.
//! - **No secret** (`secret`): none of her texts in it holds what the host
//!   filter's secret scan would wipe, and no other string in it (an id, a
//!   token, a key of a map) holds a key of a known shape. Ids and digests
//!   read as random, so only her texts are held to the random-run rule.
//!   Writers scrub before they write (`filter.scan_secrets`); this is the
//!   backstop, so a key that slipped into a body never leaves the device in
//!   it.
//!
//! Batches keep the link's limits: at most [`BATCH_ROWS`] rows,
//! [`BATCH_PAYLOADS`] payloads and about [`BATCH_BYTES`] bytes each, in
//! answer order.

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine as _;
use myriad_perception_filter::secrets::{self, SecretKind};
use myriad_persona_log::{
    AdmitInput, Admitted, Holder, Replicate, Row, Vector, admit,
    admit::{Epochs, Reason, Refusal},
    forget::{self, Entry},
    payloads, vector,
};
use myriad_persona_sync::canonical::canonical_bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{Body, Change, READ},
    key::Key,
    tags::{Sync, audience_fits},
};

/// Rows in one batch: the link's `log.rows` `maxItems.rows` (and
/// `maxItems.withheld`).
pub const BATCH_ROWS: usize = 256;
/// Payloads in one batch: the link's `log.rows` `maxItems.payloads`.
pub const BATCH_PAYLOADS: usize = 64;
/// About how many bytes of rows and payloads one batch holds, under the
/// link's frame limit (256 KiB) with room for the frame around it.
pub const BATCH_BYTES: usize = 192 << 10;
/// The most canonical bytes a body may have to travel: in base64 (about
/// 171 KiB) it fits one batch with its row.
pub const MAX_BODY_BYTES: usize = 128 << 10;
/// Rows one side asks again for at a time: the link's `log.vector`
/// `maxItems.ask`.
pub const ASK_ROWS: usize = 64;

/// A payload carried beside the rows: the row's `payload.ref` and the
/// body's canonical bytes, base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Carried {
    #[serde(rename = "ref")]
    pub reference: String,
    pub data: String,
}

/// One audience's rows and the payloads they point at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Batch {
    pub audience: String,
    pub rows: Vec<Row>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payloads: Vec<Carried>,
    /// The rows here that point at a payload and go without it, and why:
    /// the sender's word, so the other side knows which not to ask again
    /// for.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub withheld: Vec<Withheld>,
}

/// Why a body did not travel (or was not taken) with its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// The row points at a payload that is not here.
    NoBody,
    /// The body is not of its kind's shape, holds a field its shape does
    /// not name, or is not its canonical bytes.
    BadBody,
    /// The core has no shape for this kind's payload.
    NoShape,
    /// The body's sync tag keeps it on its device.
    NotShared,
    /// A reference or evidence is a local file or path.
    LocalRef,
    /// One of her texts holds a secret, or another string a key.
    Secret,
    /// A row of a person's or a group's node in her own audience.
    PersonElsewhere,
    /// More than [`MAX_BODY_BYTES`] canonical bytes.
    TooLarge,
    /// A span or an entry tagged with an audience its row's may not hold.
    AudienceTag,
}

impl Why {
    /// Whether the body stays withheld however often it is asked for: all
    /// but `no_body` and `bad_body`, which may come right on another try.
    pub fn lasting(self) -> bool {
        !matches!(self, Why::NoBody | Why::BadBody)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Withheld {
    pub id: String,
    pub why: Why,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendInput {
    /// The audience the rows are of.
    pub audience: String,
    /// This side's rows of that audience (all of them, or every shared row
    /// past `theirs` and every row in `ask`).
    pub rows: Vec<Row>,
    /// Each row's body, by row id: what its payload resolves to.
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    /// What the other side holds of the audience (its `log.vector`).
    #[serde(default)]
    pub theirs: Vector,
    /// Rows the other side holds and asks again for the bodies of (its
    /// `log.vector` `ask`, at most [`ASK_ROWS`]): they go again after what
    /// it lacks. An id this side does not hold as a shared row with a
    /// payload is passed by.
    #[serde(default)]
    pub ask: Vec<String>,
    /// The other side.
    pub holder: Holder,
    /// This side's ledger: a node it forgot or took back from sync goes as
    /// stubs under every name it has, whatever rows are given.
    #[serde(default)]
    pub ledger: Vec<Entry>,
    /// The nodes (`node:<id>`) this side knows are a person's or a group's
    /// in her own audience ([`Taken::elsewhere`], kept): no body of theirs
    /// goes, though their created row's body is not among `bodies`.
    #[serde(default)]
    pub elsewhere: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    /// In order; each one `log.rows` frame, with the rows in it that go
    /// without their body.
    pub batches: Vec<Batch>,
    /// The other side may not hold this audience: nothing goes.
    #[serde(skip_serializing_if = "is_false")]
    pub not_held: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TakeInput {
    pub batch: Batch,
    /// This side.
    pub holder: Holder,
    /// This side's clock, as for `log.admit`.
    pub now: i64,
    #[serde(default)]
    pub known: BTreeMap<String, String>,
    #[serde(default)]
    pub members: BTreeMap<String, BTreeSet<String>>,
    /// This side's ledger, as for `log.admit`: refused without once a
    /// retract is in the batch.
    #[serde(default)]
    pub ledger: Option<Vec<Entry>>,
    #[serde(default)]
    pub epochs: Epochs,
    /// The rows this side holds and asked again for the bodies of: one that
    /// comes back the same is not admitted again (it was), only its body
    /// taken; one that comes back otherwise is refused.
    #[serde(default)]
    pub held: Vec<Row>,
    /// The nodes this side already knows are a person's or a group's in her
    /// own audience (earlier [`Taken::elsewhere`]): no body of theirs is
    /// taken, in this batch or any later one.
    #[serde(default)]
    pub elsewhere: Vec<String>,
    /// Person and group keys this side knows (`person:<id>`, `group:<id>`):
    /// the sender's word that it kept a node's body back as a person's or
    /// a group's (`person_elsewhere`) holds only for a node one of them, or
    /// a key in the batch's bodies, makes in this audience.
    #[serde(default)]
    pub people: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Taken {
    #[serde(flatten)]
    pub admitted: Admitted,
    /// The accepted rows' bodies, and the asked-for ones', by row id, as
    /// read: what to store beside them (and give `nodes.fold`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bodies: BTreeMap<String, Value>,
    /// Those rows whose body is not taken, in log order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub withheld: Vec<Withheld>,
    /// Of `withheld`, the rows whose body may yet come: to ask for again
    /// (`log.vector` `ask`), at most once per link session.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ask: Vec<String>,
    /// Nodes this batch showed to be a person's or a group's in her own
    /// audience (by a created body, or the sender's `person_elsewhere` for
    /// a node this side can tie to such a key), not among the `elsewhere`
    /// given: the caller keeps them for every later `take` and `send`, and
    /// drops any body of theirs it stored before.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub elsewhere: Vec<String>,
}

/// Whether an id is a local file or path, after its `<type>:`.
fn local_path(id: &str) -> bool {
    let (kind, rest) = id.split_once(':').unwrap_or(("", id));
    let drive = rest.as_bytes().get(1..3) == Some(b":\\".as_slice())
        || rest.as_bytes().get(1..3) == Some(b":/".as_slice());
    kind == "file"
        || rest.starts_with('/')
        || rest.starts_with('~')
        || rest.starts_with('\\')
        || drive
}

/// Whether one of her texts holds anything the secret scan wipes.
fn secret(text: &str) -> bool {
    !secrets::scan(text).wiped.is_empty()
}

/// Whether a string that is not her words (an id, a token, a map's key)
/// holds a key of a known shape. Random-looking runs pass: ids and digests
/// read as random.
fn keyed_text(text: &str) -> bool {
    secrets::scan(text)
        .wiped
        .iter()
        .any(|kind| *kind != SecretKind::HighEntropy)
}

/// Whether any string anywhere in `value`, keys of maps too, holds a key of
/// a known shape.
fn keyed(value: &Value) -> bool {
    match value {
        Value::String(text) => keyed_text(text),
        Value::Array(items) => items.iter().any(keyed),
        Value::Object(fields) => fields
            .iter()
            .any(|(key, value)| keyed_text(key) || keyed(value)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

/// Her texts in a node body, and the ids that point at what it rests on.
fn node_parts(body: &Body) -> (Vec<&str>, Vec<&str>) {
    let mut texts: Vec<&str> = Vec::new();
    let mut ids: Vec<&str> = Vec::new();
    match body {
        Body::Created(body) => {
            texts.push(&body.name);
            if let Some(object) = &body.object {
                texts.push(&object.name);
                texts.extend(object.aliases.iter().map(String::as_str));
            }
        }
        Body::KindChanged(body) => {
            texts.push(&body.reason);
            ids.extend(body.because.iter().map(String::as_str));
        }
        Body::Revised(_) => {}
        Body::ViewChanged(body) => {
            texts.push(&body.text);
            ids.extend(body.because.iter().map(String::as_str));
        }
        Body::OpenItem(body) => {
            texts.push(&body.text);
            ids.extend(body.because.iter().map(String::as_str));
        }
        Body::Intent(body) => {
            texts.push(&body.text);
            ids.extend(body.because.iter().map(String::as_str));
        }
        Body::Span(body) => {
            texts.push(&body.text);
            ids.extend(body.refs.iter().map(String::as_str));
        }
        Body::Entry(body) => {
            texts.push(&body.text);
            ids.extend(body.refs.iter().map(String::as_str));
        }
        Body::Related(body) => {
            texts.push(&body.reason);
            ids.extend(body.because.iter().map(String::as_str));
        }
        Body::Unrelated(body) => texts.push(&body.reason),
        Body::DoneWhen(body) => {
            texts.extend(body.text.as_deref());
            ids.extend(body.because.iter().map(String::as_str));
        }
        Body::SplitOff(body) => {
            texts.push(&body.reason);
            ids.extend(body.because.iter().map(String::as_str));
        }
        Body::Absorbed(body) => {
            texts.push(&body.reason);
            ids.extend(body.because.iter().map(String::as_str));
        }
        Body::Used(body) => ids.extend(body.via.iter().map(String::as_str)),
        Body::Upkept(_) => {}
    }
    (texts, ids)
}

/// A revision's texts and evidence: read from its value by its field.
fn revised_parts(change: &Change) -> (Vec<String>, Vec<String>) {
    match change {
        Change::Name(name) => (vec![name.clone()], Vec::new()),
        Change::Summary(sentences) => (
            sentences.iter().map(|one| one.text.clone()).collect(),
            sentences
                .iter()
                .flat_map(|one| one.because.iter().cloned())
                .collect(),
        ),
        Change::Object(Some(object)) => {
            let mut texts = vec![object.name.clone()];
            texts.extend(object.aliases.iter().cloned());
            (texts, Vec::new())
        }
        Change::Progress(_, Some(progress)) => (vec![progress.clone()], Vec::new()),
        Change::Status(_)
        | Change::WhoKnows(_)
        | Change::Object(None)
        | Change::Progress(_, None) => (Vec::new(), Vec::new()),
    }
}

/// Her texts and the ids a body rests on, then every string in the whole
/// body.
fn judge(texts: &[&str], ids: &[&str], whole: &Value) -> Result<(), Why> {
    if ids.iter().any(|id| local_path(id)) {
        return Err(Why::LocalRef);
    }
    if texts.iter().any(|text| secret(text)) || keyed(whole) {
        return Err(Why::Secret);
    }
    Ok(())
}

/// The fields each payload shape names (`myriad_persona_log::payloads`).
/// On the wire a payload holds no other: a field the core does not know is
/// one it cannot check.
const PAYLOAD_FIELDS: [(&str, &[&str]); 7] = [
    ("segment.open", &["at", "place"]),
    ("segment.line", &["at", "text", "line", "replyTo"]),
    ("segment.close", &["at"]),
    ("memory.write", &["at", "text", "kind", "replaces"]),
    ("memory.retire", &["at", "why", "by"]),
    ("memory.restore", &["at"]),
    (
        "affect.change",
        &["at", "valence", "arousal", "band", "cause"],
    ),
];

/// The payload fields that are ids, not words: held to the path rule and
/// the known-key rule, not the random-run one.
const PAYLOAD_IDS: [&str; 4] = ["line", "replyTo", "replaces", "by"];

/// A payload of a shaped kind checked: its shape, no field it does not
/// name, then its words and ids.
fn payload(kind: &str, body: &Value) -> Result<(), Why> {
    match payloads::check(kind, body) {
        Ok(true) => {}
        Ok(false) => return Err(Why::NoShape),
        Err(_) => return Err(Why::BadBody),
    }
    let names = PAYLOAD_FIELDS
        .iter()
        .find(|(shaped, _)| *shaped == kind)
        .map(|(_, names)| *names)
        .ok_or(Why::NoShape)?;
    let fields = body.as_object().ok_or(Why::BadBody)?;
    if fields.keys().any(|key| !names.contains(&key.as_str())) {
        return Err(Why::BadBody);
    }
    let mut texts: Vec<&str> = Vec::new();
    let mut ids: Vec<&str> = Vec::new();
    for (key, value) in fields {
        let strings: Vec<&str> = match value {
            Value::String(text) => vec![text.as_str()],
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if PAYLOAD_IDS.contains(&key.as_str()) {
            ids.extend(strings);
        } else {
            texts.extend(strings);
        }
    }
    judge(&texts, &ids, body)
}

/// The nodes whose created row (among `created`, row and body) puts a
/// person or a group in her own audience.
fn elsewhere<'a>(created: impl Iterator<Item = (&'a Row, &'a Value)>) -> BTreeSet<String> {
    created
        .filter(|(row, _)| row.kind == "node.created")
        .filter(|(row, body)| {
            body.get("key")
                .and_then(Value::as_str)
                .and_then(|key| Key::parse(key).ok())
                .is_some_and(|key| !key.fits(&row.audience))
        })
        .map(|(row, _)| row.object.clone())
        .collect()
}

/// The body of `row` checked for the wire: its canonical bytes, and the
/// body as read; or why it does not travel.
pub fn carry(row: &Row, body: &Value) -> Result<(Vec<u8>, Value), Why> {
    if elsewhere(std::iter::once((row, body))).contains(&row.object) {
        return Err(Why::PersonElsewhere);
    }
    if !row.kind.starts_with("node.") {
        payload(&row.kind, body)?;
        let bytes = canonical_bytes(body);
        if bytes.len() > MAX_BODY_BYTES {
            return Err(Why::TooLarge);
        }
        return Ok((bytes, body.clone()));
    }
    if !READ.contains(&row.kind.as_str()) {
        return Err(Why::NoShape);
    }
    let body = Body::read(&row.kind, body).map_err(|_| Why::BadBody)?;
    let bytes = body.canonical_bytes();
    if bytes.len() > MAX_BODY_BYTES {
        return Err(Why::TooLarge);
    }
    if body.tags().0 != &Sync::Shared {
        return Err(Why::NotShared);
    }
    let fits = match &body {
        Body::Span(span) => audience_fits(&span.audience, &row.audience, &span.said_by),
        Body::Entry(entry) => audience_fits(&entry.audience, &row.audience, &entry.said_by),
        _ => true,
    };
    if !fits {
        return Err(Why::AudienceTag);
    }
    let (mut texts, mut ids) = node_parts(&body);
    let revised = match &body {
        Body::Revised(revised) => revised_parts(&revised.change().map_err(|_| Why::BadBody)?),
        _ => (Vec::new(), Vec::new()),
    };
    texts.extend(revised.0.iter().map(String::as_str));
    ids.extend(revised.1.iter().map(String::as_str));
    let value = body.to_value();
    judge(&texts, &ids, &value)?;
    Ok((bytes, value))
}

/// A payload as it arrived: its bytes and the JSON they read as.
type Decoded = Result<(Vec<u8>, Value), Why>;

/// What to send a side holding `theirs` of one audience, in batches.
pub fn send(input: SendInput) -> Result<Sent, NodeError> {
    let SendInput {
        audience,
        rows,
        bodies,
        theirs,
        ask,
        holder,
        ledger,
        elsewhere: known_away,
    } = input;
    holder
        .check()
        .map_err(|error| NodeError::new(error.to_string()))?;
    myriad_persona_log::row::Audience::parse(&audience)
        .map_err(|_| NodeError::new("audience is not personal:, own: or group:"))?;
    if rows.iter().any(|row| row.audience != audience) {
        return Err(NodeError::new("a row is not of the audience"));
    }
    if ask.len() > ASK_ROWS {
        return Err(NodeError::new(format!(
            "asks again for more than {ASK_ROWS} rows"
        )));
    }
    if !holder.holds(&audience) {
        return Ok(Sent {
            batches: Vec::new(),
            not_held: true,
        });
    }
    // Nothing of a forgotten or retracted node leaves this side but stubs,
    // under any name of it: the other side keeps nothing of this side's.
    let ledger = forget::joined(
        &forget::joined(&ledger, &forget::ledger(&rows)),
        &crate::fold::legacy(&rows, &bodies),
    );
    let (rows, _) = forget::absorb(rows, &ledger, forget::Side::Away);
    let mut away = elsewhere(
        rows.iter()
            .filter_map(|row| bodies.get(&row.id).map(|body| (row, body))),
    );
    away.extend(known_away);
    let answered =
        vector::answer(&rows, &theirs).map_err(|error| NodeError::new(error.to_string()))?;
    let lacked: BTreeSet<&str> = answered.iter().map(|row| row.id.as_str()).collect();
    let asked: BTreeSet<&str> = ask.iter().map(String::as_str).collect();
    let mut again: Vec<Row> = rows
        .iter()
        .filter(|row| asked.contains(row.id.as_str()) && !lacked.contains(row.id.as_str()))
        .filter(|row| row.replicate == Replicate::Shared && !row.forgotten)
        .filter(|row| row.payload.is_some())
        .cloned()
        .collect();
    myriad_persona_log::row::sort(&mut again);
    let engine = base64::engine::general_purpose::STANDARD;
    let mut batches: Vec<Batch> = Vec::new();
    let mut bytes = 0;
    for row in answered.into_iter().chain(again) {
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
        let carried = match (&row.payload, row.forgotten) {
            (Some(_), false) if away.contains(&row.object) => Err(Why::PersonElsewhere),
            (Some(payload), false) => match bodies.get(&row.id) {
                None => Err(Why::NoBody),
                Some(body) => carry(&row, body).map(|(bytes, _)| Carried {
                    reference: payload.reference.clone(),
                    data: engine.encode(bytes),
                }),
            }
            .map(Some),
            _ => Ok(None),
        };
        let (carried, withheld) = match carried {
            Ok(carried) => (carried, None),
            Err(why) => (
                None,
                Some(Withheld {
                    id: row.id.clone(),
                    why,
                }),
            ),
        };
        let size = row.canonical_bytes().len()
            + carried
                .as_ref()
                .map_or(0, |one| one.reference.len() + one.data.len() + 24)
            + withheld.as_ref().map_or(0, |one| one.id.len() + 40);
        let full = batches.last().is_none_or(|batch| {
            batch.rows.len() >= BATCH_ROWS
                || (carried.is_some() && batch.payloads.len() >= BATCH_PAYLOADS)
                || (bytes + size > BATCH_BYTES && !batch.rows.is_empty())
        });
        if full {
            batches.push(Batch {
                audience: audience.clone(),
                rows: Vec::new(),
                payloads: Vec::new(),
                withheld: Vec::new(),
            });
            bytes = 0;
        }
        let batch = batches.last_mut().expect("a batch was just made");
        bytes += size;
        batch.rows.push(row);
        batch.payloads.extend(carried);
        batch.withheld.extend(withheld);
    }
    Ok(Sent {
        batches,
        not_held: false,
    })
}

/// A batch from another side, checked: the rows to merge and their bodies.
pub fn take(input: TakeInput) -> Result<Taken, NodeError> {
    let TakeInput {
        batch,
        holder,
        now,
        known,
        members,
        ledger,
        epochs,
        held,
        elsewhere: known_away,
        people,
    } = input;
    holder
        .check()
        .map_err(|error| NodeError::new(error.to_string()))?;
    let mut carried: BTreeMap<&str, &str> = BTreeMap::new();
    for one in &batch.payloads {
        if carried
            .insert(one.reference.as_str(), one.data.as_str())
            .is_some()
        {
            return Err(NodeError::new("a payload is in the batch twice"));
        }
    }
    let mut said: BTreeMap<&str, Why> = BTreeMap::new();
    for one in &batch.withheld {
        let row = batch
            .rows
            .iter()
            .find(|row| row.id == one.id)
            .ok_or_else(|| NodeError::new("a withheld body is of no row in the batch"))?;
        let is_carried = row
            .payload
            .as_ref()
            .is_some_and(|payload| carried.contains_key(payload.reference.as_str()));
        if is_carried || said.insert(one.id.as_str(), one.why).is_some() {
            return Err(NodeError::new(
                "a body is said to be withheld twice, or carried too",
            ));
        }
    }
    let held: BTreeMap<&str, &Row> = held
        .iter()
        .filter(|row| row.audience == batch.audience)
        .map(|row| (row.id.as_str(), row))
        .collect();
    let mut astray: Vec<Refusal> = Vec::new();
    let mut again: Vec<Row> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    for row in &batch.rows {
        if row.audience != batch.audience {
            astray.push(Refusal {
                id: row.id.clone(),
                reason: Reason::Envelope,
                detail: Some("not of the batch's audience".into()),
            });
        } else if let Some(mine) = held.get(row.id.as_str()) {
            if *mine == row {
                again.push(row.clone());
            } else {
                astray.push(Refusal {
                    id: row.id.clone(),
                    reason: Reason::Envelope,
                    detail: Some("not the row this side holds".into()),
                });
            }
        } else {
            rows.push(row.clone());
        }
    }
    // What the batch's own bodies show, read as sent: the names of rows
    // from before envelopes held them (so a retract taken now reaches the
    // rows under them), and the person and group keys a node id may be
    // made of.
    let shown: BTreeMap<String, Value> = batch
        .rows
        .iter()
        .filter_map(|row| {
            let payload = row.payload.as_ref().filter(|_| !row.forgotten)?;
            let data = base64::engine::general_purpose::STANDARD
                .decode(carried.get(payload.reference.as_str())?)
                .ok()?;
            Some((row.id.clone(), serde_json::from_slice(&data).ok()?))
        })
        .collect();
    let checked: Vec<Row> = batch
        .rows
        .iter()
        .filter(|row| row.audience == batch.audience && row.check().is_ok())
        .cloned()
        .collect();
    let legacy = crate::fold::legacy(&checked, &shown);
    // The person and group nodes this side can tie a node of this audience
    // to: only for one of them is the sender's `person_elsewhere` taken.
    let mut keys: BTreeSet<String> = people.into_iter().collect();
    keys.insert(format!("person:{}", holder.owner));
    for body in shown.values() {
        strings(body, &mut keys);
    }
    let ties: BTreeSet<String> = keys
        .iter()
        .filter(|key| Key::parse(key).is_ok_and(|key| !key.fits(&batch.audience)))
        .map(|key| format!("node:{}", crate::key::made_id(&batch.audience, key)))
        .collect();
    forget::given(ledger.as_deref(), &batch.rows)
        .map_err(|error| NodeError::new(error.to_string()))?;
    let mut admitted = admit(AdmitInput {
        rows,
        now,
        known,
        members,
        ledger: Some(forget::joined(&ledger.unwrap_or_default(), &legacy)),
        epochs,
        holder: Some(holder),
    })
    .map_err(|error| NodeError::new(error.to_string()))?;
    admitted.refused.extend(astray);
    // A row asked again for is taken as the ledger stands now too: no
    // body of a node forgotten or taken back from this side comes in.
    let (again, _) = forget::absorb(again, &admitted.ledger, forget::Side::Here);
    let engine = base64::engine::general_purpose::STANDARD;
    // The most base64 one body may take.
    let most = MAX_BODY_BYTES.div_ceil(3) * 4;
    // Each accepted or asked-for row's payload, decoded: its bytes and
    // what they read as.
    let decoded: Vec<(&Row, Decoded)> = admitted
        .accepted
        .iter()
        .chain(&again)
        .filter_map(|row| {
            let payload = row.payload.as_ref().filter(|_| !row.forgotten)?;
            let read = match carried.get(payload.reference.as_str()) {
                None => Err(said.get(row.id.as_str()).copied().unwrap_or(Why::NoBody)),
                Some(data) if data.len() > most => Err(Why::TooLarge),
                Some(data) => engine
                    .decode(data)
                    .map_err(|_| Why::BadBody)
                    .and_then(|data| {
                        let body = serde_json::from_slice(&data).map_err(|_| Why::BadBody)?;
                        Ok((data, body))
                    }),
            };
            Some((row, read))
        })
        .collect();
    let mut away = elsewhere(
        decoded
            .iter()
            .filter_map(|(row, read)| read.as_ref().ok().map(|(_, body)| (*row, body))),
    );
    // The sender's word that it kept a body back for this reason marks the
    // node too (the created body that would show it may never come), but
    // only a node this side itself ties to a person or a group.
    away.extend(
        batch
            .rows
            .iter()
            .filter(|row| said.get(row.id.as_str()) == Some(&Why::PersonElsewhere))
            .filter(|row| ties.contains(&row.object))
            .map(|row| row.object.clone()),
    );
    let learnt: Vec<String> = away
        .iter()
        .filter(|node| !known_away.contains(node))
        .cloned()
        .collect();
    away.extend(known_away);
    let mut bodies = BTreeMap::new();
    let mut withheld = Vec::new();
    for (row, read) in decoded {
        let read = read.and_then(|(data, body)| {
            if away.contains(&row.object) {
                return Err(Why::PersonElsewhere);
            }
            let (canonical, body) = carry(row, &body)?;
            if canonical == data {
                Ok(body)
            } else {
                Err(Why::BadBody)
            }
        });
        match read {
            Ok(body) => {
                bodies.insert(row.id.clone(), body);
            }
            Err(why) => withheld.push(Withheld {
                id: row.id.clone(),
                why,
            }),
        }
    }
    let ask = withheld
        .iter()
        .filter(|one| !one.why.lasting())
        .map(|one| one.id.clone())
        .collect();
    Ok(Taken {
        admitted,
        bodies,
        withheld,
        ask,
        elsewhere: learnt,
    })
}

/// Every string in `value`, into `out`.
fn strings(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(one) => {
            out.insert(one.clone());
        }
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_local_and_an_id_is_not() {
        for id in [
            "file:///Users/me/notes.md",
            "record:/Users/me/.claude/s.jsonl#12",
            "record:~/s.jsonl",
            "record:C:\\Users\\me",
            "record:\\\\host\\share",
        ] {
            assert!(local_path(id), "{id}");
        }
        for id in [
            "record:s#12",
            "message:m-1",
            "span:r-1",
            "entry:e1",
            "web:example.com/a",
        ] {
            assert!(!local_path(id), "{id}");
        }
    }

    #[test]
    fn each_payload_shape_names_every_field_its_struct_writes() {
        use payloads::*;
        let full = [
            (
                "segment.open",
                serde_json::to_value(SegmentOpen {
                    at: 1,
                    place: "web".into(),
                }),
            ),
            (
                "segment.line",
                serde_json::to_value(SegmentLine {
                    at: 1,
                    text: "早".into(),
                    line: Some("m_1".into()),
                    reply_to: Some("m_0".into()),
                }),
            ),
            (
                "segment.close",
                serde_json::to_value(SegmentClose { at: 1 }),
            ),
            (
                "memory.write",
                serde_json::to_value(MemoryWrite {
                    at: 1,
                    text: "喜欢热可可".into(),
                    kind: "preference".into(),
                    replaces: vec!["memory:m-1".into()],
                }),
            ),
            (
                "memory.retire",
                serde_json::to_value(MemoryRetire {
                    at: 1,
                    why: "superseded".into(),
                    by: Some("memory:m-2".into()),
                }),
            ),
            (
                "memory.restore",
                serde_json::to_value(MemoryRestore { at: 1 }),
            ),
            (
                "affect.change",
                serde_json::to_value(AffectChange {
                    at: 1,
                    valence: 50.0,
                    arousal: 50.0,
                    band: "warm".into(),
                    cause: "chat".into(),
                }),
            ),
        ];
        assert_eq!(full.len(), PAYLOAD_FIELDS.len());
        for ((kind, value), (named, names)) in full.into_iter().zip(PAYLOAD_FIELDS) {
            assert_eq!(kind, named);
            let value = value.unwrap();
            let mut written: Vec<&str> = value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            let mut names = names.to_vec();
            written.sort();
            names.sort();
            assert_eq!(written, names, "{kind}");
        }
    }
}
