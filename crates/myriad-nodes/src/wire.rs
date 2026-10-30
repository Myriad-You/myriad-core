//! Her node rows on the wire, with their bodies: what one host core sends
//! another, and what it takes in.
//!
//! A batch is one audience's rows, with the payloads they point at: the
//! body of a row is its canonical bytes, base64, under the row's own
//! `payload.ref` (`{audience, rows, payloads: [{ref, data}]}`, the body of
//! the link's `log.rows` frame, and a log page of the export file). The row
//! and its body are byte for byte what the writing side stores, so every
//! side folds the same bytes.
//!
//! **Sending** ([`send`]): the rows the other side lacks (`log.answer`:
//! forgets first, local rows never), and none at all to a side that may not
//! hold the audience (`myriad_persona_log::holder`). Each row goes; its
//! body goes only when it is one the core reads and passes the checks
//! below, otherwise the row travels alone and the body is listed in
//! `withheld`. The row always goes because the other side's version vector
//! counts it: a missing row would stop that side's count there for good.
//!
//! **Taking** ([`take`]): the batch is checked as `log.admit` checks it
//! (clock, envelope, local, audience, members, epochs, the ledger, and the
//! audiences this side may hold); then each accepted row's body passes the
//! same checks as on sending, and must be exactly its canonical bytes. A
//! body that fails is dropped (the row stays, its body `withheld`): the
//! fold passes a row without a body by (`no_body`).
//!
//! What a body must be to travel:
//!
//! - **One the core reads**: a node kind of [`crate::body::READ`], or a
//!   kind whose payload shape is in `myriad_persona_log::payloads`
//!   (segments, memories, mood). Anything else (the native app's older node
//!   kinds, judged cases until they have a shape) goes as a row only
//!   (`no_shape`).
//! - **Shared**: its `sync` tag is `shared` (a local tag never leaves its
//!   device, even on a row marked shared: `not_shared`).
//! - **No local path**: no reference or evidence is a `file:` reference or
//!   a path (`/…`, `~…`, `C:\…`). A raw record's id (`record:<session>#<n>`)
//!   travels as an id; the record itself stays on its device (`local_ref`).
//! - **No secret**: none of her texts in it holds what the host filter's
//!   secret scan would wipe (`secret`). Writers scrub before they write
//!   (`filter.scan_secrets`); this is the backstop, so a key that slipped
//!   into a body never leaves the device in it.
//!
//! Batches keep the link's limits: at most [`BATCH_ROWS`] rows,
//! [`BATCH_PAYLOADS`] payloads and about [`BATCH_BYTES`] bytes each, in
//! answer order.

use std::collections::BTreeMap;

use base64::Engine as _;
use myriad_persona_log::{
    AdmitInput, Admitted, Holder, Row, Vector, admit,
    admit::{Epochs, Reason, Refusal},
    forget::Entry,
    payloads, vector,
};
use myriad_persona_sync::canonical::canonical_bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    NodeError,
    body::{Body, Change, READ},
    tags::Sync,
};

/// Rows in one batch: the link's `log.rows` `maxItems.rows`.
pub const BATCH_ROWS: usize = 256;
/// Payloads in one batch: the link's `log.rows` `maxItems.payloads`.
pub const BATCH_PAYLOADS: usize = 64;
/// About how many bytes of rows and payloads one batch holds, under the
/// link's frame limit (256 KiB) with room for the frame around it.
pub const BATCH_BYTES: usize = 192 << 10;

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
}

/// Why a body did not travel (or was not taken) with its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// The row points at a payload that is not here.
    NoBody,
    /// The body is not of its kind's shape, or not its canonical bytes.
    BadBody,
    /// The core has no shape for this kind's payload.
    NoShape,
    /// The body's sync tag keeps it on its device.
    NotShared,
    /// A reference or evidence is a local file or path.
    LocalRef,
    /// One of her texts holds a secret.
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
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
    /// past `theirs`).
    pub rows: Vec<Row>,
    /// Each row's body, by row id: what its payload resolves to.
    #[serde(default)]
    pub bodies: BTreeMap<String, Value>,
    /// What the other side holds of the audience (its `log.vector`).
    #[serde(default)]
    pub theirs: Vector,
    /// The other side.
    pub holder: Holder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    /// In order; each one `log.rows` frame.
    pub batches: Vec<Batch>,
    /// Rows that go without their body, in answer order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub withheld: Vec<Withheld>,
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
    pub members: BTreeMap<String, std::collections::BTreeSet<String>>,
    #[serde(default)]
    pub ledger: Vec<Entry>,
    #[serde(default)]
    pub epochs: Epochs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Taken {
    #[serde(flatten)]
    pub admitted: Admitted,
    /// The accepted rows' bodies, by row id, as read: what to store beside
    /// them (and give `nodes.fold`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub bodies: BTreeMap<String, Value>,
    /// Accepted rows whose body is not taken, in log order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub withheld: Vec<Withheld>,
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

fn secret(text: &str) -> bool {
    !myriad_perception_filter::secrets::scan(text)
        .wiped
        .is_empty()
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
        Body::Used(_) | Body::Upkept(_) => {}
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

fn judge(texts: &[&str], ids: &[&str]) -> Result<(), Why> {
    if ids.iter().any(|id| local_path(id)) {
        return Err(Why::LocalRef);
    }
    if texts.iter().any(|text| secret(text)) {
        return Err(Why::Secret);
    }
    Ok(())
}

/// The body of `row` checked for the wire: its canonical bytes, and the
/// body as read; or why it does not travel.
pub fn carry(row: &Row, body: &Value) -> Result<(Vec<u8>, Value), Why> {
    if row.kind.starts_with("node.") {
        if !READ.contains(&row.kind.as_str()) {
            return Err(Why::NoShape);
        }
        let body = Body::read(&row.kind, body).map_err(|_| Why::BadBody)?;
        if body.tags().0 != &Sync::Shared {
            return Err(Why::NotShared);
        }
        let (texts, ids) = node_parts(&body);
        judge(&texts, &ids)?;
        if let Body::Revised(revised) = &body {
            let change = revised.change().map_err(|_| Why::BadBody)?;
            let (texts, ids) = revised_parts(&change);
            let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
            let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
            judge(&texts, &ids)?;
        }
        return Ok((body.canonical_bytes(), body.to_value()));
    }
    match payloads::check(&row.kind, body) {
        Ok(true) => {
            let text = body.get("text").and_then(Value::as_str);
            judge(&text.into_iter().collect::<Vec<_>>(), &[])?;
            Ok((canonical_bytes(body), body.clone()))
        }
        Ok(false) => Err(Why::NoShape),
        Err(_) => Err(Why::BadBody),
    }
}

/// What to send a side holding `theirs` of one audience, in batches.
pub fn send(input: SendInput) -> Result<Sent, NodeError> {
    let SendInput {
        audience,
        rows,
        bodies,
        theirs,
        holder,
    } = input;
    holder
        .check()
        .map_err(|error| NodeError::new(error.to_string()))?;
    myriad_persona_log::row::Audience::parse(&audience)
        .map_err(|_| NodeError::new("audience is not personal:, own: or group:"))?;
    if rows.iter().any(|row| row.audience != audience) {
        return Err(NodeError::new("a row is not of the audience"));
    }
    if !holder.holds(&audience) {
        return Ok(Sent {
            batches: Vec::new(),
            withheld: Vec::new(),
            not_held: true,
        });
    }
    let answered =
        vector::answer(&rows, &theirs).map_err(|error| NodeError::new(error.to_string()))?;
    let engine = base64::engine::general_purpose::STANDARD;
    let mut batches: Vec<Batch> = Vec::new();
    let mut withheld = Vec::new();
    let mut bytes = 0;
    for row in answered {
        row.check()
            .map_err(|_| NodeError::new("a row's envelope is not whole"))?;
        let carried = match (&row.payload, row.forgotten) {
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
        let carried = carried.unwrap_or_else(|why| {
            withheld.push(Withheld {
                id: row.id.clone(),
                why,
            });
            None
        });
        let size = row.canonical_bytes().len()
            + carried
                .as_ref()
                .map_or(0, |one| one.reference.len() + one.data.len() + 24);
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
            });
            bytes = 0;
        }
        let batch = batches.last_mut().expect("a batch was just made");
        bytes += size;
        batch.rows.push(row);
        batch.payloads.extend(carried);
    }
    Ok(Sent {
        batches,
        withheld,
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
    let (rows, astray): (Vec<Row>, Vec<Row>) = batch
        .rows
        .iter()
        .cloned()
        .partition(|row| row.audience == batch.audience);
    let mut admitted = admit(AdmitInput {
        rows,
        now,
        known,
        members,
        ledger,
        epochs,
        holder: Some(holder),
    });
    admitted
        .refused
        .extend(astray.into_iter().map(|row| Refusal {
            id: row.id,
            reason: Reason::Envelope,
            detail: Some("not of the batch's audience".into()),
        }));
    let engine = base64::engine::general_purpose::STANDARD;
    let mut bodies = BTreeMap::new();
    let mut withheld = Vec::new();
    for row in &admitted.accepted {
        let Some(payload) = row.payload.as_ref().filter(|_| !row.forgotten) else {
            continue;
        };
        let read = carried
            .get(payload.reference.as_str())
            .ok_or(Why::NoBody)
            .and_then(|data| engine.decode(data).map_err(|_| Why::BadBody))
            .and_then(|data| {
                let body: Value = serde_json::from_slice(&data).map_err(|_| Why::BadBody)?;
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
    Ok(Taken {
        admitted,
        bodies,
        withheld,
    })
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
}
