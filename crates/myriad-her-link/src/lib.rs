//! Her link's frames: what a frame must look like before it is sent or
//! taken, and its one byte form. The contract is
//! `shared/her_link_contract.json`, protocol 2, the only one spoken
//! (protocol 1 is only in its `history`). A frame of a family (`fact`,
//! `lease`…) is named `<family>.<name>`; the frames that came from
//! protocol 1 belong to none.
//!
//! A frame is one canonical JSON text: `{v, t, seq, ack, id?, at, body}`.
//! A sealed field (HPKE, RFC 9180) is checked for its shape only: opening
//! and sealing are the platform's, with its own keys.
//!
//! Pure: nothing here reads a clock, a file, the network or a key.

use std::{collections::BTreeMap, sync::LazyLock};

use base64::Engine as _;
use myriad_canonical::canonical_bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The contract this crate was built with.
pub const CONTRACT_JSON: &str = include_str!("../../../shared/her_link_contract.json");

pub mod now;

/// One frame type's rules.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSpec {
    pub from: String,
    pub delivery: String,
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    /// Pairs of fields of which exactly one is present.
    #[serde(default)]
    pub one_of: Vec<Vec<String>>,
    /// The most items an array field may hold.
    #[serde(default)]
    pub max_items: BTreeMap<String, usize>,
    /// Fields that may come sealed.
    #[serde(default)]
    pub sealed: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    required: Vec<String>,
    optional: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    max_frame_bytes: usize,
    /// What [`valid_id`] checks; read by the tests.
    #[cfg_attr(not(test), allow(dead_code))]
    id_pattern: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Encoding {
    frame_version: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sealing {
    pub suite: String,
    pub enc_bytes: usize,
    pub tag_bytes: usize,
    pub info: String,
}

#[derive(Debug, Deserialize)]
struct Sync {
    families: Vec<String>,
}

/// The fields an action's args or result take (`calls`).
#[derive(Debug, Clone, Deserialize)]
pub struct CallFields {
    pub required: Vec<String>,
    pub optional: Vec<String>,
    /// Fields whose value is one of a contract list: field → the list's
    /// name (`actStatuses`, `dropReasons`, `noticeKinds`, `sayKinds`,
    /// `workStatuses`).
    #[serde(default)]
    pub enums: BTreeMap<String, String>,
}

/// One action of `calls`: what one end may ask another to do for her.
#[derive(Debug, Clone, Deserialize)]
pub struct CallSpec {
    pub version: u64,
    /// The end that does it: `child` or `host`.
    pub by: String,
    /// The permission group the owner approves it by.
    pub group: String,
    /// The kinds of `for` it may be called for (`callAudiences`).
    #[serde(rename = "for")]
    pub audiences: Vec<String>,
    pub long: bool,
    pub streams: bool,
    pub blob: bool,
    pub args: CallFields,
    pub result: CallFields,
}

/// One state or event of `streams`: what a `stream.state` value or a
/// `stream.event` body of that name and version holds.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamSpec {
    pub version: u64,
    /// Who sends it: `child`, `host` or `either`.
    pub from: String,
    /// Whose it is: a kind of `callAudiences`, or `link`.
    #[serde(rename = "for")]
    pub audience: String,
    pub fields: CallFields,
}

#[derive(Debug, Deserialize)]
struct Streams {
    state: BTreeMap<String, StreamSpec>,
    event: BTreeMap<String, StreamSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Calls {
    default_approved: Vec<String>,
    fail_codes: Vec<String>,
    actions: BTreeMap<String, CallSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    protocol: u32,
    envelope: Envelope,
    limits: Limits,
    encoding: Encoding,
    sealing: Sealing,
    families: BTreeMap<String, String>,
    sync: Sync,
    frames: BTreeMap<String, FrameSpec>,
    error_codes: BTreeMap<String, String>,
    call_audiences: Vec<String>,
    calls: Calls,
    timing: CallTiming,
    say_kinds: Vec<String>,
    notice_kinds: Vec<String>,
    act_statuses: Vec<String>,
    work_statuses: Vec<String>,
    drop_reasons: Vec<String>,
    streams: Streams,
}

/// The longest a call may ask to be answered in.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CallTiming {
    call_deadline_max_ms: u64,
    long_call_max_ms: u64,
}

/// The contract list a call's field value is one of, by its name.
fn named_list(name: &str) -> Option<&'static [String]> {
    match name {
        "actStatuses" => Some(&CONTRACT.act_statuses),
        "dropReasons" => Some(&CONTRACT.drop_reasons),
        "noticeKinds" => Some(&CONTRACT.notice_kinds),
        "sayKinds" => Some(&CONTRACT.say_kinds),
        "workStatuses" => Some(&CONTRACT.work_statuses),
        _ => None,
    }
}

/// The longest a call id may be: its result's id (`<callId>.result`) still
/// matches `limits.idPattern`.
const CALL_ID_CHARS: usize = 64 - ".result".len();

/// The earliest a time in milliseconds since the epoch may be (2001): one
/// below it was written in seconds.
const EPOCH_MS_FLOOR: u64 = 1_000_000_000_000;

static CONTRACT: LazyLock<Contract> =
    LazyLock::new(|| serde_json::from_str(CONTRACT_JSON).expect("her_link_contract.json"));

/// The protocol these rules check: the only one spoken.
pub fn protocol() -> u32 {
    CONTRACT.protocol
}

/// The sealing suite every sealed field uses.
pub fn sealing() -> &'static Sealing {
    &CONTRACT.sealing
}

/// The HPKE `info` for a sealed field of frame type `t`.
pub fn seal_info(t: &str) -> String {
    format!("{} {t}", CONTRACT.sealing.info)
}

/// The family of frame type `t`: the part before its first dot, when that
/// names a family. The frames that came from protocol 1 have none (`say`,
/// with no dot, is not of the family `say`).
pub fn family(t: &str) -> Option<&'static str> {
    let (family, _) = t.split_once('.')?;
    CONTRACT
        .families
        .get_key_value(family)
        .map(|(name, _)| name.as_str())
}

/// The action `name` of `calls`, when the contract has it.
pub fn call_spec(name: &str) -> Option<&'static CallSpec> {
    CONTRACT.calls.actions.get(name)
}

/// The permission groups that are on for a device until its owner turns
/// them off.
pub fn calls_default_approved() -> &'static [String] {
    &CONTRACT.calls.default_approved
}

/// Whether `action` may be called for `of` (`call.ask`'s `for`): the caller
/// asks this before it asks, the receiver again on receipt. The error is
/// the code its `call.result` fails with.
pub fn call_allowed(action: &str, of: &Value) -> Result<(), &'static str> {
    let spec = call_spec(action).ok_or("call_unknown")?;
    let kind = of["kind"].as_str().ok_or("wrong_audience")?;
    if spec.audiences.iter().any(|allowed| allowed == kind) {
        Ok(())
    } else {
        Err("wrong_audience")
    }
}

/// An action's args or result: exactly its fields, each of `enums` one of
/// its list.
fn check_call_fields(what: &str, value: &Value, fields_of: &CallFields) -> Result<(), Refusal> {
    let have = fields(value, what)?;
    check_fields(what, &have, &fields_of.required, &fields_of.optional)?;
    for (field, list) in &fields_of.enums {
        let Some(given) = value.get(field) else {
            continue;
        };
        let known = named_list(list).unwrap_or(&[]);
        if !given
            .as_str()
            .is_some_and(|given| known.iter().any(|one| one == given))
        {
            return Err(refuse(
                "bad_frame",
                format!("{what}.{field} is one of {list}"),
            ));
        }
    }
    Ok(())
}

/// A call id: the frame's own id (`call.ask`) or the call it answers, short
/// enough for its result's id.
fn check_call_id(body: &Value) -> Result<&str, Refusal> {
    match body["callId"].as_str() {
        Some(id) if valid_id(id) && id.chars().count() <= CALL_ID_CHARS => Ok(id),
        _ => Err(refuse(
            "bad_frame",
            format!("a callId matches limits.idPattern in at most {CALL_ID_CHARS} characters"),
        )),
    }
}

/// `call.ask`: its id is its callId, its `for` a kind of `callAudiences` (a
/// group's naming it, and only a group's), its deadline no further
/// from its `at` than the contract's, its args those of its action at that version. An action or
/// version this end does not know passes: the receiver answers it with
/// `call_unknown` or `version_unsupported`.
fn check_ask(frame: &Value, body: &Value) -> Result<(), Refusal> {
    let id = check_call_id(body)?;
    if frame["id"].as_str() != Some(id) {
        return Err(refuse("bad_frame", "call.ask's id is its callId"));
    }
    let of = &body["for"];
    let of_fields = fields(of, "call.ask's for")?;
    check_fields(
        "call.ask's for",
        &of_fields,
        &["kind".to_string()],
        &["groupId".to_string()],
    )?;
    let Some(kind) = of["kind"].as_str() else {
        return Err(refuse("bad_frame", "call.ask's for has no kind"));
    };
    if !CONTRACT.call_audiences.iter().any(|one| one == kind) {
        return Err(refuse(
            "bad_frame",
            format!("call.ask's for is of no kind {kind:?}"),
        ));
    }
    match of.get("groupId") {
        Some(group) if kind == "group" && group.as_str().is_some_and(valid_id) => {}
        None if kind != "group" => {}
        _ => {
            return Err(refuse(
                "bad_frame",
                "call.ask's for names a groupId only and always for a group",
            ));
        }
    }
    let (Some(action), Some(version)) = (body["action"].as_str(), body["version"].as_u64()) else {
        return Err(refuse("bad_frame", "call.ask names its action and version"));
    };
    let spec = call_spec(action);
    let most = if spec.is_some_and(|spec| !spec.long) {
        CONTRACT.timing.call_deadline_max_ms
    } else {
        CONTRACT.timing.long_call_max_ms
    };
    // An ask that comes after its deadline passes: its receiver answers it
    // call_timeout.
    let at = frame["at"].as_u64().unwrap_or(0);
    if !body["deadlineAt"]
        .as_u64()
        .is_some_and(|by| by >= EPOCH_MS_FLOOR && by <= at.saturating_add(most))
    {
        return Err(refuse(
            "bad_frame",
            format!("call.ask's deadlineAt is milliseconds, at most {most} after its at"),
        ));
    }
    match spec {
        Some(spec) if spec.version == version => {
            check_call_fields(&format!("{action}'s args"), &body["args"], &spec.args)
        }
        _ => fields(&body["args"], "call.ask's args").map(|_| ()),
    }
}

/// `call.result`: its id is its callId's and `.result`; done carries
/// exactly the result fields of its action at that version (another
/// version passes), failed an error with one of `calls.failCodes`, unknown
/// neither. An action this end does not know only fails.
fn check_result(frame: &Value, body: &Value) -> Result<(), Refusal> {
    let id = check_call_id(body)?;
    if frame["id"].as_str() != Some(format!("{id}.result").as_str()) {
        return Err(refuse(
            "bad_frame",
            "call.result's id is its callId and .result",
        ));
    }
    let (Some(action), Some(version)) = (body["action"].as_str(), body["version"].as_u64()) else {
        return Err(refuse(
            "bad_frame",
            "call.result names its action and version",
        ));
    };
    let spec = call_spec(action);
    let result = body.get("result");
    let error = body.get("error");
    let outcome = body["outcome"].as_str();
    if spec.is_none() && outcome != Some("failed") {
        return Err(refuse(
            "bad_frame",
            format!("{action} is not an action: its call only fails"),
        ));
    }
    match outcome {
        Some("done") => {
            if error.is_some() {
                return Err(refuse("bad_frame", "a done call.result has no error"));
            }
            let Some(result) = result else {
                return Err(refuse("bad_frame", "a done call.result carries its result"));
            };
            match spec {
                Some(spec) if spec.version == version => {
                    check_call_fields(&format!("{action}'s result"), result, &spec.result)?;
                }
                _ => {
                    fields(result, "call.result's result")?;
                }
            }
        }
        Some("failed") => {
            if result.is_some() {
                return Err(refuse("bad_frame", "a failed call.result has no result"));
            }
            let Some(error) = error else {
                return Err(refuse(
                    "bad_frame",
                    "a failed call.result carries its error",
                ));
            };
            check_fields(
                "call.result's error",
                &fields(error, "call.result's error")?,
                &["code".to_string()],
                &[],
            )?;
            if !error["code"]
                .as_str()
                .is_some_and(|code| CONTRACT.calls.fail_codes.iter().any(|one| one == code))
            {
                return Err(refuse(
                    "bad_frame",
                    "call.result's error is one of calls.failCodes",
                ));
            }
        }
        Some("unknown") => {
            if result.is_some() || error.is_some() {
                return Err(refuse(
                    "bad_frame",
                    "an unknown call.result carries neither result nor error",
                ));
            }
        }
        _ => {
            return Err(refuse(
                "bad_frame",
                "call.result's outcome is done, failed or unknown",
            ));
        }
    }
    if body.get("blobs").is_some() && !spec.is_some_and(|spec| spec.blob) {
        return Err(refuse("bad_frame", format!("{action} has no blobs")));
    }
    Ok(())
}

/// A state (`stream.state`) or an event (`stream.event`) of `streams` by
/// its name, at the version registered.
pub fn stream_spec(event: bool, name: &str) -> Option<&'static StreamSpec> {
    let streams = &CONTRACT.streams;
    let of = if event { &streams.event } else { &streams.state };
    of.get(name)
}

/// `stream.state` / `stream.event`: a name and a version; one registered at
/// that version holds exactly its fields, another passes for an end to
/// ignore (`streams`).
fn check_stream(t: &str, body: &Value) -> Result<(), Refusal> {
    let (Some(name), Some(version)) = (body["name"].as_str(), body["version"].as_u64()) else {
        return Err(refuse("bad_frame", "a stream names its name and version"));
    };
    if !valid_id(name) || version < 1 {
        return Err(refuse(
            "bad_frame",
            "a stream's name matches limits.idPattern and its version is at least 1",
        ));
    }
    let event = t == "stream.event";
    let Some(spec) = stream_spec(event, name).filter(|spec| spec.version == version) else {
        return Ok(());
    };
    let held = if event { &body["body"] } else { &body["value"] };
    check_call_fields(name, held, &spec.fields)?;
    if name == "link.dropped" && !held["t"].as_str().is_some_and(valid_id) {
        return Err(refuse(
            "bad_frame",
            "link.dropped's t is a frame type (limits.idPattern) or unnamed",
        ));
    }
    Ok(())
}

/// `link.versions`: the core an end runs is its abi and the commit it was
/// built from.
fn check_versions(body: &Value) -> Result<(), Refusal> {
    let core = &body["core"];
    check_fields(
        "link.versions's core",
        &fields(core, "link.versions's core")?,
        &["abi".to_string(), "commit".to_string()],
        &[],
    )?;
    let commit = core["commit"].as_str().unwrap_or("");
    if core["abi"].as_u64().is_none()
        || commit.len() != 40
        || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(refuse(
            "bad_frame",
            "link.versions's core is its abi and its 40-hex-digit commit",
        ));
    }
    Ok(())
}

/// `call.progress`: its phase is a stage name, never content, and what it
/// waits for, if anything, the owner.
fn check_progress(body: &Value) -> Result<(), Refusal> {
    check_call_id(body)?;
    if !body["phase"].as_str().is_some_and(valid_id) {
        return Err(refuse(
            "bad_frame",
            "call.progress's phase is a stage name (limits.idPattern)",
        ));
    }
    if body
        .get("waiting")
        .is_some_and(|waiting| waiting != "owner")
    {
        return Err(refuse(
            "bad_frame",
            "call.progress waits only for the owner",
        ));
    }
    Ok(())
}

/// Whether frame type `t` flows only on a link whose welcome said sync on
/// (the contract's `sync.families`).
pub fn needs_sync(t: &str) -> bool {
    family(t).is_some_and(|family| CONTRACT.sync.families.iter().any(|one| one == family))
}

/// The additional data a sealed field of frame `t` is bound to: canonical
/// JSON `{from, t, to}`.
pub fn seal_aad(from: &str, t: &str, to: &str) -> Vec<u8> {
    canonical_bytes(&serde_json::json!({ "from": from, "t": t, "to": to }))
}

/// Why a frame was refused: one of the contract's `errorCodes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub code: String,
    pub detail: String,
}

fn refuse(code: &str, detail: impl Into<String>) -> Refusal {
    debug_assert!(CONTRACT.error_codes.contains_key(code), "{code}");
    Refusal {
        code: code.to_string(),
        detail: detail.into(),
    }
}

/// A frame that passed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checked {
    pub t: String,
    /// The family (`fact`, `lease`…), or `null` for a protocol-1 frame.
    pub family: Option<String>,
    pub delivery: String,
    /// Its one byte form.
    pub canonical: String,
}

/// `limits.idPattern`: `^[A-Za-z0-9_.:-]{1,64}$`.
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.:-".contains(&byte))
}

fn spec(t: &str) -> Option<(&'static FrameSpec, Option<String>)> {
    CONTRACT
        .frames
        .get(t)
        .map(|spec| (spec, family(t).map(str::to_string)))
}

fn decoded(what: &str, raw: &Value) -> Result<Vec<u8>, Refusal> {
    raw.as_str()
        .and_then(|raw| base64::engine::general_purpose::STANDARD.decode(raw).ok())
        .ok_or_else(|| refuse("bad_sealed", format!("{what} is not base64")))
}

/// Whether `value` is a sealed field of the contract's suite.
fn check_sealed(field: &str, value: &Value) -> Result<(), Refusal> {
    let sealing = &CONTRACT.sealing;
    let Some(object) = value.as_object() else {
        return Err(refuse("bad_sealed", format!("{field} is not an object")));
    };
    let keys: Vec<&str> = object.keys().map(String::as_str).collect();
    let mut wanted = ["ct", "enc", "keyId", "suite", "to"];
    wanted.sort_unstable();
    let mut have = keys.clone();
    have.sort_unstable();
    if have != wanted {
        return Err(refuse(
            "bad_sealed",
            format!("{field} has {have:?}, not {wanted:?}"),
        ));
    }
    if object["suite"].as_str() != Some(sealing.suite.as_str()) {
        return Err(refuse("bad_sealed", format!("{field} is of another suite")));
    }
    if !object["to"].as_str().is_some_and(valid_id) {
        return Err(refuse(
            "bad_sealed",
            format!("{field}.to is not a device id"),
        ));
    }
    let key_id = object["keyId"].as_str().unwrap_or_default();
    if key_id.len() != 32
        || !key_id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(refuse("bad_sealed", format!("{field}.keyId is not 32 hex")));
    }
    if decoded("enc", &object["enc"])?.len() != sealing.enc_bytes {
        return Err(refuse(
            "bad_sealed",
            format!("{field}.enc is not {} bytes", sealing.enc_bytes),
        ));
    }
    if decoded("ct", &object["ct"])?.len() < sealing.tag_bytes {
        return Err(refuse(
            "bad_sealed",
            format!("{field}.ct is shorter than a tag"),
        ));
    }
    Ok(())
}

fn fields(value: &Value, what: &str) -> Result<Vec<String>, Refusal> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .ok_or_else(|| refuse("bad_frame", format!("{what} is not an object")))
}

fn check_fields(
    what: &str,
    have: &[String],
    required: &[String],
    optional: &[String],
) -> Result<(), Refusal> {
    if let Some(missing) = required.iter().find(|field| !have.contains(field)) {
        return Err(refuse("bad_frame", format!("{what} has no {missing}")));
    }
    if let Some(extra) = have
        .iter()
        .find(|field| !required.contains(field) && !optional.contains(field))
    {
        return Err(refuse("bad_frame", format!("{what} takes no {extra}")));
    }
    Ok(())
}

/// Whether `frame` is one that may be sent or taken, from a sender with
/// `roles` (`device`, `host`, `mind`, `server`; empty: not checked).
pub fn check(frame: &Value, roles: &[String]) -> Result<Checked, Refusal> {
    let contract = &*CONTRACT;
    let bytes = canonical_bytes(frame);
    if bytes.len() > contract.limits.max_frame_bytes {
        return Err(refuse(
            "too_large",
            format!(
                "{} bytes, at most {}",
                bytes.len(),
                contract.limits.max_frame_bytes
            ),
        ));
    }
    let envelope = fields(frame, "the frame")?;
    check_fields(
        "the envelope",
        &envelope,
        &contract.envelope.required,
        &contract.envelope.optional,
    )?;
    if frame["v"].as_u64() != Some(contract.encoding.frame_version) {
        return Err(refuse(
            "version_unsupported",
            format!(
                "v is {}, this end reads {}",
                frame["v"], contract.encoding.frame_version
            ),
        ));
    }
    let Some(t) = frame["t"].as_str() else {
        return Err(refuse("bad_frame", "t is not text"));
    };
    let Some((spec, family)) = spec(t) else {
        return Err(refuse("unknown_frame", format!("no frame {t:?}")));
    };
    if !frame["seq"].as_u64().is_some_and(|seq| seq >= 1) {
        return Err(refuse("bad_frame", "seq counts from 1"));
    }
    if frame["ack"].as_u64().is_none() || frame["at"].as_u64().is_none() {
        return Err(refuse("bad_frame", "ack and at are whole numbers from 0"));
    }
    let reliable = spec.delivery == "reliable";
    match frame.get("id") {
        Some(id) if !reliable => {
            return Err(refuse(
                "bad_frame",
                format!("{t} is not reliable and takes no id: {id}"),
            ));
        }
        Some(id) if !id.as_str().is_some_and(valid_id) => {
            return Err(refuse(
                "bad_id",
                format!("id {id} does not match the pattern"),
            ));
        }
        None if reliable => {
            return Err(refuse(
                "bad_frame",
                format!("{t} is reliable and needs an id"),
            ));
        }
        _ => {}
    }
    if family.is_some() && !roles.is_empty() && spec.from != "either" && !roles.contains(&spec.from)
    {
        return Err(refuse(
            "bad_frame",
            format!("{t} comes from a {}, not from {roles:?}", spec.from),
        ));
    }
    let body = &frame["body"];
    let have = fields(body, "the body")?;
    check_fields(
        &format!("{t}'s body"),
        &have,
        &spec.required,
        &spec.optional,
    )?;
    for pair in &spec.one_of {
        let present = pair.iter().filter(|field| have.contains(field)).count();
        if present != 1 {
            return Err(refuse(
                "bad_frame",
                format!("{t} takes exactly one of {pair:?}"),
            ));
        }
    }
    for (field, most) in &spec.max_items {
        if let Some(items) = body.get(field) {
            let Some(items) = items.as_array() else {
                return Err(refuse("bad_frame", format!("{t}.{field} is not a list")));
            };
            if items.len() > *most {
                return Err(refuse(
                    "too_many",
                    format!("{t}.{field} has {}, at most {most}", items.len()),
                ));
            }
        }
    }
    for field in &spec.sealed {
        if let Some(value) = body.get(field) {
            check_sealed(field, value)?;
        }
    }
    match t {
        "call.ask" => check_ask(frame, body)?,
        "call.result" => check_result(frame, body)?,
        "call.progress" => check_progress(body)?,
        "stream.state" | "stream.event" => check_stream(t, body)?,
        "link.versions" => check_versions(body)?,
        "call.accepted" | "call.event" | "call.cancel" => {
            check_call_id(body)?;
        }
        _ => {}
    }
    Ok(Checked {
        t: t.to_string(),
        family,
        delivery: spec.delivery.clone(),
        canonical: String::from_utf8(bytes).expect("canonical JSON is UTF-8"),
    })
}

/// [`check`] for the bytes off the wire.
pub fn check_bytes(bytes: &[u8], roles: &[String]) -> Result<Checked, Refusal> {
    if bytes.len() > CONTRACT.limits.max_frame_bytes {
        return Err(refuse("too_large", format!("{} bytes", bytes.len())));
    }
    let frame: Value =
        serde_json::from_slice(bytes).map_err(|error| refuse("bad_frame", error.to_string()))?;
    check(&frame, roles)
}

/// Every family and what it carries.
pub fn families() -> &'static BTreeMap<String, String> {
    &CONTRACT.families
}

#[cfg(test)]
mod tests;
