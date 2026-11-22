//! The link's protocol as a value: numbering, acks, the reliable outbox,
//! which turns to resume, which run events and directives are new, the
//! heartbeat and what a close means. It began as the Android XR end's port of
//! the Mac's `HerLinkSession` and came here whole, so every end keeps one
//! copy; what only a computer does there (Work, experiences, moments, nodes,
//! proposals, acts, her host lease's moves) it does not read yet.

use std::collections::{BTreeMap, BTreeSet};

use myriad_her_link::valid_id;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::contract::{Contract, Timing};
use crate::frames;
use crate::whole;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Closed,
    Greeting,
    Open,
}

/// Why her authorization here is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    Revoked,
    OwnerChanged,
    TokenUnknown,
}

/// Why the link is not open, as the child sees it. No case carries a token or the site's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Down {
    Connecting,
    Network,
    Silent,
    HostUnavailable,
    NoOwner,
    Replaced,
    UnsupportedProtocol,
    RefusedHandshake,
    /// The shell fell too far behind in taking the reliable frames handed to it
    /// ([`Session::KEPT_PASSING`] wait): the link is dropped so that her host, which was
    /// acked none of them, sends again on the next what it sends again.
    Behind,
    /// She lives on another end now: the device's name, when the site said it.
    HostMoved(Option<String>),
    /// She is moving: no end has her until it is done or called off.
    HostFrozen,
    Redirected,
    UnsupportedSite,
    CredentialMissing,
    Stopped,
    Removed(Removal),
}

/// What the site's `{error, code}` said when it refused the upgrade, as far as the link acts on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Removed(Removal),
    NoOwner,
    HostFrozen,
    HostMoved(Option<String>),
}

impl Refusal {
    /// What the body of a refused upgrade says (contract `auth`): `{error, code}`, with
    /// `host: {kind, name}` when the code is `host_moved` and the site knows where she lives.
    /// None for a body that is not that, or a code the link does not act on.
    pub fn read(body: &Value) -> Option<Self> {
        let name = body.get("host").and_then(|host| host.get("name"));
        Self::of(body.get("code")?.as_str()?, name.and_then(Value::as_str))
    }

    /// The contract's `refusals` code, and the device she lives on when `host_moved` names it.
    pub fn of(code: &str, device_name: Option<&str>) -> Option<Self> {
        Some(match code {
            "device_revoked" => Self::Removed(Removal::Revoked),
            "device_token_required" | "device_token_invalid" => {
                Self::Removed(Removal::TokenUnknown)
            }
            "owner_changed" => Self::Removed(Removal::OwnerChanged),
            "site_owner_required" => Self::NoOwner,
            "host_frozen" => Self::HostFrozen,
            "host_moved" => Self::HostMoved(device_name.map(str::to_string)),
            _ => return None,
        })
    }
}

/// What to do once the socket has closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After {
    Reconnect(Down),
    /// No automatic reconnect; the owner can retry.
    Stop(Down),
    /// The authorization is gone: the credential goes, nothing reconnects.
    Forget(Removal),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Delivery {
    /// `lineage` and `host_epoch` are her host lease's (none while the site has no her).
    Welcome {
        device_id: String,
        host: String,
        lineage: Option<String>,
        host_epoch: Option<i64>,
        /// Whether the sync families flow on this link (`welcome.sync` is `on`).
        sync: bool,
        /// The partner's one id, when her host said it (an id, not a credential).
        partner: Option<String>,
    },
    Accepted {
        turn_id: String,
        run_id: String,
        session_id: Option<String>,
        duplicate: bool,
    },
    Rejected {
        turn_id: String,
        code: String,
    },
    Event {
        turn_id: String,
        run_id: String,
        sequence: u64,
        event: Value,
    },
    End {
        turn_id: String,
        run_id: String,
    },
    /// `live`: her director's refinement while the reply is said (`perform.live`).
    Perform {
        turn_id: Option<String>,
        run_id: Option<String>,
        directive: Value,
        live: Option<u64>,
    },
    /// Her director's refinement of the `say` line `say_id`, to play only while that line is said.
    PerformSay {
        say_id: String,
        directive: Value,
    },
    /// Her reaction to the contact `touch_id`, or none.
    TouchReaction {
        touch_id: String,
        reaction: Option<String>,
    },
    /// The set of her wardrobe she wears here, and its rig's (and picture's) content ids.
    Outfit {
        outfit_id: String,
        rig_asset_id: String,
        portrait_asset_id: Option<String>,
    },
    /// Her wardrobe on her host, read only, in her order (`wardrobe`, with `wardrobe.list`): to
    /// choose here what she wears on this device.
    Wardrobe(Vec<WardrobeItem>),
    Mood(Value),
    /// A line she says here on her own (greeting, reaching out, face to face), once per id.
    Say {
        id: String,
        body: Value,
    },
    /// Her recent conversations with the owner, to show here, newest first.
    Conversations(Vec<Value>),
    /// One of them changed (`conversations.live`): as it stands now.
    Conversation(Value),
    /// One of the site's notifications about her (`notice`).
    Notice(Value),
    HostError {
        code: String,
    },
    /// Her host asks this end to do one thing for her (`call.ask`), already held to what
    /// this end offered, to whom the action may be for, and to its deadline. The shell writes
    /// `call_id`, `action` and `version` down before it starts (see [`Session::lost`]), does
    /// it, and answers once with [`Session::call_done`] or [`Session::call_failed`].
    /// `deadline_at` is by her host's clock, milliseconds: the session ends the call itself
    /// when it passes ([`Delivery::CallStop`]). `of` is whom it is for (`{kind, groupId?}`).
    Call {
        call_id: String,
        action: String,
        version: u64,
        args: Value,
        of: Value,
        deadline_at: i64,
    },
    /// Stop doing the call `call_id`: `why` is `cancelled` (her host no longer wants it) or
    /// `timeout` (its deadline passed). The session has answered her host already; what the
    /// shell says of this call afterwards is not sent.
    CallStop {
        call_id: String,
        why: String,
    },
    /// Of a call this end asked of her host ([`Session::call`]): her host has the long
    /// action in hand.
    CallAccepted {
        call_id: String,
    },
    /// Where that long action is: a stage name of the action's own, and whether her host
    /// waits for the owner. The newest counts.
    CallProgress {
        call_id: String,
        phase: String,
        waiting_for_owner: bool,
    },
    /// The next piece of its result, in order, each once; one that was lost with a link is
    /// not made up, and the whole still comes in the result.
    CallPiece {
        call_id: String,
        sequence: u64,
        piece: Value,
    },
    /// What came of it, once: `outcome` is `done` (with `result`), `failed` (with `code`,
    /// one of the contract's `calls.failCodes`) or `unknown` (her host lost track: it may
    /// have happened, and no end does it again by itself). `here` says the session said it
    /// itself, at most once for a call: no answer came by a minute after the deadline
    /// (`call_timeout`), or her host does not provide the action on the link the ask waited
    /// for (`call_not_granted`). A result of her host's that still comes after is delivered
    /// too, and is what happened.
    CallResult {
        call_id: String,
        outcome: String,
        result: Option<Value>,
        code: Option<String>,
        blobs: Vec<String>,
        here: bool,
    },
    /// Calls this end asked that it no longer keeps: each was told timed out, not granted or
    /// cancelled here, had no result of her host's since, and more than
    /// [`Session::KEPT_ANSWERS`] such were kept. A result of her host's that still comes
    /// for one is acked and not delivered. Their ids are not to be used again.
    CallsForgotten {
        call_ids: Vec<String>,
    },
    /// Something her host tells as it changes (`stream.state`), of a name the contract
    /// registers at that version. Only the newest counts.
    StreamState {
        name: String,
        version: u64,
        at: i64,
        value: Value,
    },
    /// Something that happened, as her host tells it (`stream.event`), of a name the
    /// contract registers at that version; once per id.
    StreamEvent {
        id: String,
        name: String,
        version: u64,
        at: i64,
        body: Value,
    },
    /// What her host runs, as its own `link.versions` says (`{contract, core, app, actions}`),
    /// for the shell to show beside its own.
    HostVersions(Value),
    /// Her host's reading of this end's `link.versions`: `usable` is `full`, `partial`
    /// (`stopped` names what this link will not carry) or `refused` (the host closes the link
    /// after it). For the shell to show.
    Verdict {
        usable: String,
        stopped: Vec<String>,
    },
    /// What may be called on this link now (`call.granted`), each `(name, version)`:
    /// `actions` are those of this end's offer the owner approved and that are granted now;
    /// `providing` are her host's own that this end may call. The newest stands.
    Granted {
        actions: Vec<(String, u64)>,
        providing: Vec<(String, u64)>,
    },
    /// A frame of this end's that will not go: its type and why, never its body. Either the
    /// shared core would not send it (its refusal code; a reliable one has left the outbox,
    /// and an `input`'s turn has ended), or the shell gave what cannot be a frame: an id that
    /// is not one or is already used here for something else (`bad_id`), a part that is not
    /// the object it must be (`bad_frame`). Then nothing was kept, and a turn that id was
    /// used for before stands as it was.
    Unsent {
        t: String,
        code: String,
        /// The id the shell gave it (the turn's, for an `input`), when it is a reliable
        /// frame and the id is one (`limits.idPattern`): which words, which touch. None for
        /// a frame that is not reliable, and for an id that is not an id.
        id: Option<String>,
    },
    /// A frame of her host's that the session does not read, handed to a shell that takes
    /// such frames ([`Session::passes`]): its type, its id when it has one, its number on
    /// this link (`seq`), which link of this session's it came on (`link`, counted from 1),
    /// and how the contract delivers its kind (`reliable`, `answer`, `latest`, `request`).
    /// Nothing of what it carried: it is the text the shell has just handed to
    /// [`Session::received`], which the shared core took. A `reliable` one counts as taken
    /// only when the shell says so ([`Session::took`]); any other counts at once.
    ///
    /// `again` is true for a frame the shell took before that her host sends again, of the
    /// few kinds whose sender counts them delivered on an answer of the shell's and not on
    /// this end's ack (`myriad_her_link::TAKEN_BY_ANSWER`: `lease.grant`, `lease.deputy`):
    /// there is nothing to take (it counts already), the shell answers it again. Any other
    /// reliable frame it took before is only acked, and not handed over again.
    Frame {
        t: String,
        id: Option<String>,
        seq: u64,
        link: u64,
        delivery: String,
        again: bool,
    },
    /// Her host has the result of the call `call_id` it asked of this end: its ack covers
    /// the `call.result`. Only now is the call nothing the shell need keep on disk.
    CallSettled {
        call_id: String,
    },
    /// A frame from the host this end could not read, so the two are not of one version: why,
    /// as the shared core's refusal code (`unknown_frame`, `bad_frame`, `version_unsupported`…),
    /// never its body. `t` is its type when the contract this end was built with names it, none
    /// for a type it does not know or a text that is no envelope. Nothing of what the frame
    /// carried is delivered. The link's own bookkeeping still counts it: it was heard, its ack
    /// is taken, a ping is answered, a reliable frame is acked, and a `welcome` that cannot be
    /// read still drops the link. One comes for every such frame, however often her host
    /// sends it: the shell says or logs it, and bounds how often.
    Unread {
        t: Option<String>,
        code: String,
    },
}

/// One set of her wardrobe as her host lists it (the Mac's `HerLinkChild.WardrobeItem`): wearable
/// here only when it has a rig; its name is its id when it has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WardrobeItem {
    pub outfit_id: String,
    pub name: String,
    pub full_body: bool,
    pub wearable: bool,
    pub rig_asset_id: Option<String>,
    pub portrait_asset_id: Option<String>,
}

impl WardrobeItem {
    fn read(value: &Value) -> Option<Self> {
        let outfit_id = string(value, "outfitId").filter(|id| !id.is_empty())?;
        let asset = |key: &str| string(value, key).filter(|id| asset_id(id));
        let rig_asset_id = asset("rigAssetId");
        Some(WardrobeItem {
            name: string(value, "name")
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| outfit_id.clone()),
            outfit_id,
            full_body: value["profile"] == "fullBody",
            wearable: value["wearable"] == true && rig_asset_id.is_some(),
            rig_asset_id,
            portrait_asset_id: asset("portraitAssetId"),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Send(String),
    Deliver(Delivery),
    Drop(Down),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Turn {
    pub turn_id: String,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    /// The highest run sequence taken; 0 before any.
    #[serde(deserialize_with = "whole::u64")]
    pub last_sequence: u64,
    saw_start: bool,
    /// The mark of what was said under this id ([`mark`]); empty in a state kept before
    /// turns had one.
    #[serde(default)]
    said: String,
    /// The highest sequence a directive of this turn was played at.
    #[serde(default, deserialize_with = "whole::maybe_u64")]
    pub performed: Option<u64>,
    /// The highest version of her director's refinement played while the reply is said; once
    /// there is one, the run's own directives no longer replace it.
    #[serde(default, deserialize_with = "whole::maybe_u64")]
    pub live: Option<u64>,
    pub answered: bool,
    pub ended: bool,
}

impl Turn {
    fn new(turn_id: &str) -> Self {
        Self {
            turn_id: turn_id.to_string(),
            run_id: None,
            session_id: None,
            last_sequence: 0,
            saw_start: false,
            said: String::new(),
            performed: None,
            live: None,
            answered: false,
            ended: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Pending {
    id: String,
    t: String,
    body: Value,
    #[serde(default, deserialize_with = "whole::maybe_u64")]
    seq: Option<u64>,
    /// Whether it is a frame of the shell's own ([`Session::pass`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pass: bool,
    /// The capability her host must have accepted for it to go, when the shell named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    needs: Option<String>,
}

/// A reliable frame handed to the shell that it has not said it took.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Passing {
    #[serde(deserialize_with = "whole::u64")]
    seq: u64,
    id: String,
}

/// A frame as it crosses the link: `{t, seq, ack, id?, at, body}` (contract `envelope`).
struct Envelope {
    t: String,
    seq: u64,
    ack: u64,
    id: Option<String>,
    body: Value,
}

impl Envelope {
    /// None unless it is an envelope with every required field.
    fn parse(text: &str) -> Option<Self> {
        let Value::Object(o) = serde_json::from_str::<Value>(text).ok()? else {
            return None;
        };
        let t = o.get("t")?.as_str().filter(|t| !t.is_empty())?.to_string();
        let seq = unsigned(o.get("seq")?)?;
        let ack = unsigned(o.get("ack")?)?;
        unsigned(o.get("at")?)?;
        let body = o.get("body").filter(|body| body.is_object())?.clone();
        let id = match o.get("id") {
            None | Some(Value::Null) => None,
            Some(Value::String(id)) => Some(id.clone()),
            Some(_) => return None,
        };
        Some(Self {
            t,
            seq,
            ack,
            id,
            body,
        })
    }
}

/// A frame's id, when it is written as one.
fn id_of(envelope: &Envelope) -> Option<String> {
    envelope.id.clone().filter(|id| valid_id(id))
}

/// A frame of this end's that is not kept or sent, told at once: its type, why, and the id
/// the shell gave it when it is a reliable one.
fn unsent(t: &str, code: &str, id: Option<&str>) -> Vec<Output> {
    vec![Output::Deliver(Delivery::Unsent {
        t: t.to_string(),
        code: code.to_string(),
        // Only an id that is one: what is not may be anything the shell had in hand, and
        // nothing told here repeats what came in.
        id: id.filter(|id| valid_id(id)).map(str::to_string),
    })]
}

/// What was said under a turn's id, as a short mark of it (64-bit FNV-1a, in hex): enough
/// to tell the same words given again from other words under an id already used, without
/// keeping the words. Not a secret and not for anything else.
fn mark(text: &str, session_id: Option<&str>, images: &[String]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut take = |bytes: &[u8]| {
        for byte in bytes.iter().chain(&[0xff]) {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    take(text.as_bytes());
    take(session_id.unwrap_or_default().as_bytes());
    for image in images {
        take(image.as_bytes());
    }
    format!("{hash:016x}")
}

/// How the id of a call's answer ends: the call's id and this (`call.result`'s idempotency).
const RESULT: &str = ".result";

/// A `call.result` that says the call was not done, and why.
fn failed(call_id: &str, action: &str, version: u64, code: &str) -> Value {
    json!({
        "callId": call_id,
        "action": action,
        "version": version,
        "outcome": "failed",
        "error": { "code": code },
    })
}

/// What an end may offer of `actions`: those the contract's `calls` names at that version
/// and a child does, each once, in order.
fn offered(actions: impl Iterator<Item = (String, u64)>) -> Vec<(String, u64)> {
    let mut offers: Vec<(String, u64)> = actions
        .filter(|(name, version)| {
            myriad_her_link::call_spec(name)
                .is_some_and(|spec| spec.by == "child" && spec.version == *version)
        })
        .collect();
    offers.sort();
    offers.dedup();
    offers
}

/// The `[{name, version}]` under `key`, each as `(name, version)`; none when it is not that.
fn named(body: &Value, key: &str) -> Option<Vec<(String, u64)>> {
    body.get(key)?
        .as_array()?
        .iter()
        .map(|one| Some((string(one, "name")?, unsigned(one.get("version")?)?)))
        .collect()
}

/// A whole number JSON can carry exactly.
fn unsigned(value: &Value) -> Option<u64> {
    match value.as_u64() {
        Some(n) => (n <= WHOLE).then_some(n),
        None => {
            let n = value.as_f64()?;
            (n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n <= WHOLE as f64).then_some(n as u64)
        }
    }
}

/// The largest whole number every JSON reader keeps exactly (2^53). No count or time of the
/// link is past it: a frame that says one is no envelope, and a state that holds one is not
/// a session's, so what a session gives back survives a reader that keeps numbers as doubles.
const WHOLE: u64 = 1 << 53;

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

/// Whether what a kept hello says of this end is what [`Session::new`] and a welcome write:
/// an app with a name and a version, a lineage that is text or null, an epoch that is a
/// count or null. One that is not would be refused by every host, on every reconnect.
fn hello_holds(hello: &Map<String, Value>) -> bool {
    let text_or_null = |value: Option<&Value>| value.is_none_or(|v| v.is_null() || v.is_string());
    let app = hello.get("app").and_then(Value::as_object);
    app.is_some_and(|app| {
        app.len() == 2
            && app.get("name").is_some_and(Value::is_string)
            && app.get("version").is_some_and(Value::is_string)
    }) && text_or_null(hello.get("lineage"))
        && hello
            .get("hostEpoch")
            .is_none_or(|epoch| epoch.is_null() || unsigned(epoch).is_some())
        && hello.keys().all(|key| {
            matches!(
                key.as_str(),
                "app" | "capabilities" | "hostEpoch" | "lineage" | "protocol" | "role" | "surface"
            )
        })
}

/// [`Session::state`]: every field that is not the contract's, turns in the order said.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct State {
    #[serde(deserialize_with = "whole::u32")]
    version: u32,
    hello: Map<String, Value>,
    phase: String,
    #[serde(deserialize_with = "whole::u64")]
    sent: u64,
    #[serde(deserialize_with = "whole::u64")]
    heard: u64,
    #[serde(deserialize_with = "whole::i64")]
    opened_at_ms: i64,
    #[serde(deserialize_with = "whole::i64")]
    last_heard_ms: i64,
    #[serde(deserialize_with = "whole::i64")]
    last_sent_ms: i64,
    #[serde(deserialize_with = "whole::i64")]
    heartbeat_ms: i64,
    outbox: Vec<Pending>,
    turns: Vec<Turn>,
    current_turn: Option<String>,
    mood_revision: Option<f64>,
    taken: Vec<String>,
    host_capabilities: Vec<String>,
    /// None in a state kept before a session recorded it: then it is what the hello holds.
    #[serde(default)]
    said: Option<Vec<String>>,
    #[serde(default)]
    offers: Vec<Named>,
    #[serde(default)]
    core: Option<Runs>,
    #[serde(default)]
    told_dropped: Vec<(String, String)>,
    #[serde(default, deserialize_with = "whole::u64")]
    drops: u64,
    #[serde(default)]
    doing: Vec<Doing>,
    #[serde(default)]
    answered: Vec<Answered>,
    #[serde(default, deserialize_with = "whole::i64")]
    host_ahead_ms: i64,
    #[serde(default)]
    granted: Vec<Named>,
    #[serde(default)]
    heard_granted: bool,
    #[serde(default)]
    held: Vec<Held>,
    #[serde(default)]
    providing: Vec<Named>,
    #[serde(default)]
    asking: Vec<Asking>,
    #[serde(default)]
    cancels: Vec<String>,
    #[serde(default)]
    passes: bool,
    #[serde(default, deserialize_with = "whole::u64")]
    links: u64,
    #[serde(default)]
    sync: bool,
    #[serde(default)]
    passing: Vec<Passing>,
    #[serde(default, deserialize_with = "whole::maybe_u64")]
    stalled: Option<u64>,
    #[serde(default, deserialize_with = "whole::u64")]
    acked: u64,
}

/// A call this end is doing for her host.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Doing {
    call_id: String,
    action: String,
    #[serde(deserialize_with = "whole::u64")]
    version: u64,
    /// By her host's clock, milliseconds.
    #[serde(deserialize_with = "whole::i64")]
    deadline_at: i64,
    /// The sequence of the last piece of its result sent (`call.event`); 0 before any.
    #[serde(default, deserialize_with = "whole::u64")]
    pieces: u64,
}

/// A call this end asked of her host and has no result of yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Asking {
    call_id: String,
    /// By her host's clock, milliseconds.
    #[serde(deserialize_with = "whole::i64")]
    deadline_at: i64,
    /// The sequence of the last piece of its result taken; 0 before any.
    #[serde(default, deserialize_with = "whole::u64")]
    pieces: u64,
    /// Whether the session has said how it ended here (timed out, not granted, cancelled
    /// before it went again): a result may still come.
    #[serde(default)]
    late: bool,
    /// The action asked; none in a state kept before it was kept, and such a call is taken
    /// for a long one.
    #[serde(default)]
    action: Option<String>,
}

/// A call that came before her host said what is granted on this link: not judged yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Held {
    call_id: String,
    action: String,
    #[serde(deserialize_with = "whole::u64")]
    version: u64,
    args: Value,
    of: Value,
    #[serde(deserialize_with = "whole::i64")]
    deadline_at: i64,
}

/// A call this end has answered, and with what: asked again, it is answered the same.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Answered {
    call_id: String,
    /// The `call.result` body.
    result: Value,
}

/// An action and its version, as a state keeps it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Named {
    name: String,
    #[serde(deserialize_with = "whole::u64")]
    version: u64,
}

/// [`Session::runs`], as a state keeps it.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Runs {
    #[serde(deserialize_with = "whole::u64")]
    abi: u64,
    commit: String,
}

impl State {
    const VERSION: u32 = 1;
}

pub struct Session {
    timing: Timing,
    schema_version: u32,
    hello: Map<String, Value>,
    contract_capabilities: Vec<String>,
    /// The frame types the contract names: only these are told in an `Unread`.
    known_frames: BTreeSet<String>,
    phase: Phase,
    sent: u64,
    heard: u64,
    opened_at_ms: i64,
    last_heard_ms: i64,
    last_sent_ms: i64,
    heartbeat_ms: i64,
    outbox: Vec<Pending>,
    turns: BTreeMap<String, Turn>,
    order: Vec<String>,
    /// The newest turn said here; a directive of any other turn is old.
    current_turn: Option<String>,
    mood_revision: Option<f64>,
    /// Ids of the host's reliable frames already taken, newest last (dedup across resends).
    taken: Vec<String>,
    /// What the host accepted of this child's capabilities, as its welcome said.
    host_capabilities: BTreeSet<String>,
    /// What this end's hello said on this link: the capabilities as they were when the
    /// link came up.
    said: BTreeSet<String>,
    /// What this end may be asked to do ([`Session::offer`]), each `(name, version)`.
    offers: Vec<(String, u64)>,
    /// The shared core this end runs, as `link.versions` says it ([`Session::runs`]).
    core: Option<(u64, String)>,
    /// What her host has been told this end dropped on this link, each `(type, reason)`:
    /// each is told once a link.
    told_dropped: BTreeSet<(String, String)>,
    /// How many such reports this session has made, for their ids: it never starts over.
    drops: u64,
    /// The calls this end is doing for her host, oldest first.
    doing: Vec<Doing>,
    /// The calls it has answered, newest last: asked again, each is answered the same.
    answered: Vec<Answered>,
    /// How far her host's clock is ahead of the time the shell gives, as the last welcome
    /// showed (`serverTime` less the time it came): a deadline is read by her host's clock.
    host_ahead_ms: i64,
    /// What her host said is granted on this link (`call.granted.actions`), the newest.
    /// Only a grant said on this link decides: nothing is done by one from a link before.
    granted: Vec<(String, u64)>,
    /// Whether her host has said what is granted on this link yet.
    heard_granted: bool,
    /// The calls that came before it did: neither handed to the shell nor answered until it
    /// says, then judged by what it says. They wait across a link that goes, for the next
    /// link's word; each is still held to its own deadline.
    held: Vec<Held>,
    /// Her host's own actions this end may call (`call.granted.providing`), as said on this
    /// link; none before it has said.
    providing: Vec<(String, u64)>,
    /// The calls this end asked of her host and has no result of yet, oldest first.
    asking: Vec<Asking>,
    /// The long calls this end cancelled while their ask waited to go again: her host may
    /// have had the ask on the link before, so it is told to stop (`call.cancel`) on the
    /// next link where both ends listed `calls`, after its word on what it provides there.
    cancels: Vec<String>,
    /// Whether the shell takes the frames of her host's that the session does not read.
    passes: bool,
    /// How many links this session has had; the one now is this number.
    links: u64,
    /// Whether this link's welcome said sync is on.
    sync: bool,
    /// The reliable frames handed to the shell on this link that it has not said it took:
    /// what this end acks stops short of the lowest.
    passing: Vec<Passing>,
    /// The lowest number of a reliable frame of this link that could not be handed over
    /// (too many wait): what this end acks stays short of it while the link lasts, so her
    /// host sends it again on the next.
    stalled: Option<u64>,
    /// The highest number this end has acked on this link: what it acks never goes back,
    /// whatever numbers her host's frames come under.
    acked: u64,
}

impl Session {
    /// The most kinds of dropped frame told by their own type on one link.
    pub const KEPT_DROPPED: usize = 32;
    pub const KEPT_TURNS: usize = 64;
    pub const KEPT_TAKEN: usize = 256;
    /// The most `stream.event` frames and frames of the shell's own ([`Session::pass`])
    /// kept until acked, together. The session's own `link.dropped` is at most
    /// [`Self::KEPT_DROPPED`] kinds on a link: this is eight links' worth of them never acked,
    /// and as many ids as an end remembers of what it took ([`Self::KEPT_TAKEN`]). The shell's
    /// `log.refused` is at most six for a batch the host sent, again only as often as the host
    /// sends that batch again.
    pub const KEPT_EVENTS: usize = 256;

    /// The host's frame types this session reads; any other is passed by
    /// (`crate::capabilities::unread` compares an end's capabilities with it). A `pong` is
    /// read only as a sign of life.
    pub const READS: &[&str] = &[
        "call.accepted",
        "call.ask",
        "call.cancel",
        "call.event",
        "call.granted",
        "call.progress",
        "call.result",
        "conversation",
        "conversations",
        "error",
        "link.verdict",
        "link.versions",
        "mood",
        "notice",
        "outfit",
        "perform",
        "ping",
        "pong",
        "run.accepted",
        "run.end",
        "run.event",
        "run.rejected",
        "say",
        "stream.event",
        "stream.state",
        "touch.reaction",
        "wardrobe",
        "welcome",
    ];

    /// `lineage` and `host_epoch` are her host lease as this child last heard it (none the
    /// first time); each welcome replaces them, so a reconnect says what it knows.
    pub fn new(
        contract: &Contract,
        capabilities: &[&str],
        app: (&str, &str),
        lineage: Option<&str>,
        host_epoch: Option<i64>,
    ) -> Self {
        let mut hello = Map::new();
        hello.insert("protocol".into(), json!(contract.protocol));
        hello.insert("role".into(), json!("child"));
        hello.insert("surface".into(), json!("app"));
        hello.insert(
            "capabilities".into(),
            json!(
                capabilities
                    .iter()
                    .filter(|c| contract.capabilities.iter().any(|known| known == *c))
                    .collect::<Vec<_>>()
            ),
        );
        hello.insert("app".into(), json!({ "name": app.0, "version": app.1 }));
        // An epoch is a count: one that is not is not said. Each of the two is said as
        // given: the device she lives on says an epoch and no lineage.
        let host_epoch =
            host_epoch.filter(|epoch| u64::try_from(*epoch).is_ok_and(|epoch| epoch <= WHOLE));
        hello.insert("lineage".into(), json!(lineage));
        hello.insert("hostEpoch".into(), json!(host_epoch));
        Self {
            timing: contract.timing,
            schema_version: contract.schema_version,
            hello,
            contract_capabilities: contract.capabilities.clone(),
            known_frames: contract.frames.keys().cloned().collect(),
            phase: Phase::Closed,
            sent: 0,
            heard: 0,
            opened_at_ms: 0,
            last_heard_ms: 0,
            last_sent_ms: 0,
            heartbeat_ms: contract.timing.heartbeat_ms,
            outbox: Vec::new(),
            turns: BTreeMap::new(),
            order: Vec::new(),
            current_turn: None,
            mood_revision: None,
            taken: Vec::new(),
            host_capabilities: BTreeSet::new(),
            said: BTreeSet::new(),
            offers: Vec::new(),
            core: None,
            told_dropped: BTreeSet::new(),
            drops: 0,
            doing: Vec::new(),
            answered: Vec::new(),
            host_ahead_ms: 0,
            granted: Vec::new(),
            heard_granted: false,
            held: Vec::new(),
            providing: Vec::new(),
            asking: Vec::new(),
            cancels: Vec::new(),
            passes: false,
            links: 0,
            sync: false,
            passing: Vec::new(),
            stalled: None,
            acked: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn host_capabilities(&self) -> &BTreeSet<String> {
        &self.host_capabilities
    }

    /// The calls in hand whose bytes the shell may put at her host at `now_ms`
    /// (`transports.blobs`): a blob action's, before its deadline, on an open link where her
    /// host has said it grants the action at that version. A picture or a recording is what
    /// was seen or heard (`calls.seen`), and once its bytes are at her host, taking the action
    /// back cannot call them back: the shell puts them only for a call named here, and asks
    /// again before each put.
    ///
    /// The bytes go first, then the answer ([`Session::call_done`] with their names): a call
    /// answered is no longer in hand and is never named here again. One that is not named
    /// waits, neither put nor answered done: it is named once this link grants it, or her
    /// host cancels it (a blob action is long, and her host cancels a long one when it takes
    /// the action back), or its deadline passes; the session then hands the shell its stop.
    pub fn may_put(&self, now_ms: i64) -> Vec<String> {
        if self.phase != Phase::Open || !self.both("calls") || !self.heard_granted {
            return Vec::new();
        }
        let host_now = self.host_now(now_ms);
        self.doing
            .iter()
            .filter(|call| {
                host_now < call.deadline_at
                    && myriad_her_link::call_spec(&call.action).is_some_and(|spec| spec.blob)
                    && self.granted.contains(&(call.action.clone(), call.version))
            })
            .map(|call| call.call_id.clone())
            .collect()
    }

    /// Whether the host takes what the owner shows her (`images`) with what they say.
    pub fn shows(&self) -> bool {
        self.host_capabilities.contains("images")
    }

    /// What this end says it can do, as its hello says it.
    pub fn capabilities(&self) -> Vec<String> {
        self.hello
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|all| {
                all.iter()
                    .filter_map(|c| c.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// What the next hello says this end can do.
    pub fn set_capabilities(&mut self, capabilities: &[&str]) {
        let known: Vec<&str> = capabilities
            .iter()
            .copied()
            .filter(|c| self.contract_capabilities.iter().any(|known| known == c))
            .collect();
        self.hello.insert("capabilities".into(), json!(known));
    }

    /// What this end may be asked to do for her, each `(name, version)` of the contract's
    /// `calls`: said in `call.offer` after every welcome on a link where both ends listed
    /// `calls`. An action the contract does not name at that version, or one her host does
    /// (`by: host`), is left out: an end offers what it can do, in the contract's words.
    pub fn offer(&mut self, actions: &[(&str, u64)]) {
        self.offers = offered(
            actions
                .iter()
                .map(|(name, version)| (name.to_string(), *version)),
        );
    }

    /// The shared core this end runs, for `link.versions`: its ABI, and the myriad-core
    /// commit it was built from (40 hex digits; the end's pin). Without it no
    /// `link.versions` is sent; with a commit that is not one, the core refuses the frame
    /// and the shell is told (`Unsent`) after every welcome. A session restored from a
    /// kept state says what the state said: an end says it again after [`Session::restore`],
    /// so that after an update it is the core it runs now.
    pub fn runs(&mut self, abi: u64, build: &str) {
        self.core = Some((abi.min(WHOLE), build.to_string()));
    }

    /// What this end said it runs ([`Session::runs`]), when it did.
    pub fn running(&self) -> Option<(u64, &str)> {
        self.core
            .as_ref()
            .map(|(abi, build)| (*abi, build.as_str()))
    }

    /// Whether both ends listed `capability` on this link: this end in the hello it sent
    /// when the link came up, her host in its welcome. What the shell has since set for the
    /// next hello ([`Session::set_capabilities`]) opens nothing on this one.
    fn both(&self, capability: &str) -> bool {
        self.host_capabilities.contains(capability) && self.said.contains(capability)
    }

    /// What goes first on every new link once it is open: this end's versions, and what it
    /// may be asked to do.
    fn introduce(&mut self, schema_version: u32, now_ms: i64) -> Vec<Output> {
        let mut out = Vec::new();
        if let (true, Some((abi, build))) = (self.both("versions"), self.core.clone()) {
            let actions: Map<String, Value> = self
                .offers
                .iter()
                .map(|(name, version)| (name.clone(), json!(version)))
                .collect();
            let body = json!({
                "contract": schema_version,
                "core": { "abi": abi, "commit": build },
                "app": self.hello.get("app").cloned().unwrap_or(Value::Null),
                "actions": actions,
            });
            out.extend(self.send("link.versions", &body, None, now_ms));
        }
        if self.both("calls") {
            let actions: Vec<Value> = self
                .offers
                .iter()
                .map(|(name, version)| json!({ "name": name, "version": version }))
                .collect();
            out.extend(self.send("call.offer", &json!({ "actions": actions }), None, now_ms));
        }
        out
    }

    pub fn turn(&self, turn_id: &str) -> Option<&Turn> {
        self.turns.get(turn_id)
    }

    pub fn current_turn(&self) -> Option<&str> {
        self.current_turn.as_deref()
    }

    pub fn unfinished(&self) -> Vec<&Turn> {
        self.order
            .iter()
            .filter_map(|id| self.turns.get(id))
            .filter(|turn| !turn.ended)
            .collect()
    }

    // As a value

    /// The whole session as JSON, for an end that keeps it between calls (the C entry's
    /// `link.child_step`): [`Session::restore`] of it is this session again. What comes from
    /// the contract is read again from the contract when it is restored. It holds what the
    /// owner said and has not been acked; an end keeps it where it keeps that. Every count
    /// and time in it is a whole number no greater than 2^53, and is read back whether it
    /// comes as `3` or as `3.0` (some encoders write every number as a double);
    /// `moodRevision` is her host's own number, kept as it came.
    pub fn state(&self) -> Value {
        let state = State {
            version: State::VERSION,
            hello: self.hello.clone(),
            phase: self.phase.name().to_string(),
            sent: self.sent,
            heard: self.heard,
            opened_at_ms: self.opened_at_ms,
            last_heard_ms: self.last_heard_ms,
            last_sent_ms: self.last_sent_ms,
            heartbeat_ms: self.heartbeat_ms,
            outbox: self.outbox.clone(),
            turns: self
                .order
                .iter()
                .filter_map(|id| self.turns.get(id).cloned())
                .collect(),
            current_turn: self.current_turn.clone(),
            mood_revision: self.mood_revision,
            taken: self.taken.clone(),
            host_capabilities: self.host_capabilities.iter().cloned().collect(),
            said: Some(self.said.iter().cloned().collect()),
            offers: self
                .offers
                .iter()
                .map(|(name, version)| Named {
                    name: name.clone(),
                    version: *version,
                })
                .collect(),
            core: self.core.clone().map(|(abi, commit)| Runs { abi, commit }),
            told_dropped: self.told_dropped.iter().cloned().collect(),
            drops: self.drops,
            doing: self.doing.clone(),
            answered: self.answered.clone(),
            host_ahead_ms: self.host_ahead_ms,
            granted: self
                .granted
                .iter()
                .map(|(name, version)| Named {
                    name: name.clone(),
                    version: *version,
                })
                .collect(),
            heard_granted: self.heard_granted,
            held: self.held.clone(),
            providing: self
                .providing
                .iter()
                .map(|(name, version)| Named {
                    name: name.clone(),
                    version: *version,
                })
                .collect(),
            asking: self.asking.clone(),
            cancels: self.cancels.clone(),
            passes: self.passes,
            links: self.links,
            sync: self.sync,
            passing: self.passing.clone(),
            stalled: self.stalled,
            acked: self.acked,
        };
        serde_json::to_value(state).expect("a session is plain data")
    }

    /// The session [`Session::state`] gave, under `contract`. Why not, when `state` is not
    /// one: never with anything of what it holds.
    pub fn restore(contract: &Contract, state: &Value) -> Result<Self, String> {
        let no = || "not a session's state".to_string();
        let mut state = State::deserialize(state).map_err(|_| no())?;
        if state.version != State::VERSION {
            return Err("a session's state of another version".into());
        }
        let phase = [Phase::Closed, Phase::Greeting, Phase::Open]
            .into_iter()
            .find(|phase| phase.name() == state.phase)
            .ok_or_else(no)?;
        // Every count and time is one a session could have reached, so no step after this
        // overflows or reads a time before the epoch.
        let time = |ms: i64| u64::try_from(ms).is_ok_and(|ms| ms <= WHOLE);
        let counts = [state.sent, state.heard]
            .into_iter()
            .chain(state.outbox.iter().filter_map(|pending| pending.seq))
            .chain(state.turns.iter().flat_map(|turn| {
                [Some(turn.last_sequence), turn.performed, turn.live]
                    .into_iter()
                    .flatten()
            }));
        let counts = counts
            .chain([state.drops])
            .chain(
                state
                    .doing
                    .iter()
                    .flat_map(|call| [call.version, call.pieces]),
            )
            .chain(state.offers.iter().map(|named| named.version))
            .chain(state.granted.iter().map(|named| named.version))
            .chain(state.held.iter().map(|call| call.version))
            .chain(state.providing.iter().map(|named| named.version))
            .chain(state.asking.iter().map(|call| call.pieces))
            .chain(state.core.iter().map(|runs| runs.abi))
            .chain([state.links])
            .chain(state.passing.iter().map(|passing| passing.seq))
            .chain(state.stalled)
            .chain([state.acked]);
        let beat = 50..=contract.timing.heartbeat_ms.max(50);
        let mut ids = BTreeSet::new();
        // `sent` counts up by one with each frame, so it stops one short.
        if counts.into_iter().any(|count| count > WHOLE)
            || state.sent >= WHOLE
            || ![state.opened_at_ms, state.last_heard_ms, state.last_sent_ms]
                .into_iter()
                .all(time)
            || !beat.contains(&state.heartbeat_ms)
            || !state.doing.iter().all(|call| time(call.deadline_at))
            || !state.held.iter().all(|call| time(call.deadline_at))
            || !state.asking.iter().all(|call| time(call.deadline_at))
            || state.host_ahead_ms.unsigned_abs() > WHOLE
            || !state.turns.iter().all(|turn| ids.insert(&turn.turn_id))
            || state
                .mood_revision
                .is_some_and(|revision| !revision.is_finite())
            || !hello_holds(&state.hello)
        {
            return Err(no());
        }
        // What the hello says of the contract is this contract's, whichever one the state
        // was kept under: a state kept across an update says the new protocol.
        let said: Vec<Value> = state
            .hello
            .get("capabilities")
            .and_then(Value::as_array)
            .ok_or_else(no)?
            .iter()
            .filter(|said| {
                said.as_str()
                    .is_some_and(|said| contract.capabilities.iter().any(|known| known == said))
            })
            .cloned()
            .collect();
        // What the hello of the link said, when the state was kept under a session that
        // recorded it; one from before did not, and then it is what its hello holds.
        let said_on_link: BTreeSet<String> = match state.said.take() {
            Some(said) => said.into_iter().collect(),
            None => said
                .iter()
                .filter_map(|one| one.as_str().map(str::to_string))
                .collect(),
        };
        state
            .hello
            .insert("capabilities".into(), Value::Array(said));
        // The epoch as the count it is, however the state's numbers were written.
        let epoch = state.hello.get("hostEpoch").and_then(unsigned);
        state.hello.insert("hostEpoch".into(), json!(epoch));
        let lineage = state.hello.get("lineage").cloned().unwrap_or(Value::Null);
        state.hello.insert("lineage".into(), lineage);
        state
            .hello
            .insert("protocol".into(), json!(contract.protocol));
        state.hello.insert("role".into(), json!("child"));
        state.hello.insert("surface".into(), json!("app"));
        Ok(Self {
            timing: contract.timing,
            schema_version: contract.schema_version,
            hello: state.hello,
            contract_capabilities: contract.capabilities.clone(),
            known_frames: contract.frames.keys().cloned().collect(),
            phase,
            sent: state.sent,
            heard: state.heard,
            opened_at_ms: state.opened_at_ms,
            last_heard_ms: state.last_heard_ms,
            last_sent_ms: state.last_sent_ms,
            heartbeat_ms: state.heartbeat_ms,
            outbox: state.outbox,
            order: state
                .turns
                .iter()
                .map(|turn| turn.turn_id.clone())
                .collect(),
            turns: state
                .turns
                .into_iter()
                .map(|turn| (turn.turn_id.clone(), turn))
                .collect(),
            current_turn: state.current_turn,
            mood_revision: state.mood_revision,
            taken: state.taken,
            host_capabilities: state.host_capabilities.into_iter().collect(),
            said: said_on_link,
            // By this contract, whichever one the state was kept under: an action it no
            // longer names, or names at another version, is not offered.
            offers: offered(
                state
                    .offers
                    .into_iter()
                    .map(|named| (named.name, named.version)),
            ),
            core: state.core.map(|runs| (runs.abi, runs.commit)),
            told_dropped: state.told_dropped.into_iter().collect(),
            drops: state.drops,
            doing: state.doing,
            answered: state.answered,
            host_ahead_ms: state.host_ahead_ms,
            granted: state
                .granted
                .into_iter()
                .map(|named| (named.name, named.version))
                .collect(),
            heard_granted: state.heard_granted,
            held: state.held,
            providing: state
                .providing
                .into_iter()
                .map(|named| (named.name, named.version))
                .collect(),
            // Only of a call still asked, each once, in the order kept.
            cancels: {
                let mut cancels: Vec<String> = Vec::new();
                for id in state.cancels {
                    if state.asking.iter().any(|call| call.call_id == id) && !cancels.contains(&id)
                    {
                        cancels.push(id);
                    }
                }
                cancels
            },
            asking: state.asking,
            passes: state.passes,
            links: state.links,
            sync: state.sync,
            passing: state.passing,
            stalled: state.stalled,
            acked: state.acked,
        })
    }

    // Connection

    /// The socket is up: numbering starts again and hello goes first.
    pub fn connected(&mut self, now_ms: i64) -> Vec<Output> {
        self.phase = Phase::Greeting;
        self.sent = 0;
        self.heard = 0;
        self.opened_at_ms = now_ms;
        self.last_heard_ms = now_ms;
        self.heartbeat_ms = self.timing.heartbeat_ms;
        self.host_capabilities.clear();
        self.said = self.capabilities().into_iter().collect();
        self.told_dropped.clear();
        // What is granted is said anew on every link: nothing of the last one's decides.
        self.granted.clear();
        self.providing.clear();
        self.heard_granted = false;
        // Numbers start again: what waited for the shell's word on the last link is her
        // host's to send again.
        self.links = self.links.saturating_add(1).min(WHOLE);
        self.sync = false;
        self.passing.clear();
        self.stalled = None;
        self.acked = 0;
        for pending in &mut self.outbox {
            pending.seq = None;
        }
        let hello = Value::Object(self.hello.clone());
        self.send("hello", &hello, None, now_ms)
    }

    /// The socket is gone; reliable frames wait for the next connection.
    pub fn disconnected(&mut self) {
        self.phase = Phase::Closed;
        self.passing.clear();
        self.stalled = None;
        for pending in &mut self.outbox {
            pending.seq = None;
        }
    }

    // From the child

    /// Something the owner said here: a reliable `input` whose id is the turn. Sent now when
    /// the link is open, otherwise with the next welcome. `images` is what they showed her with
    /// it (data URLs), sent only to a host that accepted `images` ([`Session::shows`]).
    ///
    /// Nothing the owner said is let fall in silence: when `turn_id` is not an id
    /// (`limits.idPattern`), or was already used here for other words (or with another
    /// session or other pictures), or is one another reliable frame waits under, the words
    /// are not kept and the shell is told at once, `Unsent { t: "input", code: "bad_id", id }`.
    /// The same call made again under its id (the same words, session and pictures), whether
    /// the turn still waits, was sent, was answered or was rejected, is the same turn:
    /// nothing comes back, and nothing more goes. To ask again after a rejection is a new
    /// turn, under a new id.
    ///
    /// Whether the pictures go with the words is settled here, when it is said, by what the
    /// host of the last welcome accepted ([`Session::shows`]; nothing before a first welcome):
    /// when that is not `images`, the words are kept without them, and go so on whatever
    /// link they go. The shell reads `shows` before it calls and says so to the owner.
    #[allow(clippy::too_many_arguments)]
    pub fn say(
        &mut self,
        turn_id: &str,
        text: &str,
        session_id: Option<&str>,
        rig_state: Option<&Value>,
        images: &[String],
        now_ms: i64,
    ) -> Vec<Output> {
        // An id that ends `.result` is a call's answer's, never a turn's.
        if !valid_id(turn_id) || turn_id.ends_with(RESULT) {
            return unsent("input", "bad_id", Some(turn_id));
        }
        // The mark is of what the shell gave, pictures and all, not of what this host takes:
        // the same call made again is the same turn whatever link it is made on.
        let said = mark(text, session_id, images);
        let shown: &[String] = if self.shows() { images } else { &[] };
        if let Some(turn) = self.turns.get(turn_id) {
            // The same words again under their id are the same turn, said once: nothing more
            // goes and nothing is lost. Other words under an id already used would be lost,
            // and are told.
            // (A turn from a state kept before turns had a mark has none: it is taken for the
            // same, which sends nothing twice.)
            return if turn.said == said || turn.said.is_empty() {
                Vec::new()
            } else {
                unsent("input", "bad_id", Some(turn_id))
            };
        }
        // Nor may the id be one another reliable frame waits under: its answer would take
        // both out of the outbox.
        if self.outbox.iter().any(|pending| pending.id == turn_id) {
            return unsent("input", "bad_id", Some(turn_id));
        }
        let mut body = json!({ "surface": "app", "text": text });
        if let Some(session_id) = session_id {
            body["sessionId"] = json!(session_id);
        }
        if !shown.is_empty() {
            body["images"] = json!(shown);
        }
        if let Some(rig_state) = rig_state.filter(|state| state.is_object()) {
            body["rigState"] = rig_state.clone();
        }
        let mut turn = Turn::new(turn_id);
        turn.said = said;
        turn.session_id = session_id.map(str::to_string);
        self.remember(turn);
        self.current_turn = Some(turn_id.to_string());
        self.outbox.push(Pending {
            id: turn_id.to_string(),
            t: "input".into(),
            body,
            seq: None,
            pass: false,
            needs: None,
        });
        if self.phase == Phase::Open {
            self.flush(now_ms)
        } else {
            Vec::new()
        }
    }

    /// Whether the owner is with her here; dropped while the link is down (a latest value).
    pub fn presence(&mut self, body: &Value, now_ms: i64) -> Vec<Output> {
        if self.phase != Phase::Open {
            return Vec::new();
        }
        if !body.get("presence").is_some_and(Value::is_object) {
            return unsent("presence", "bad_frame", None);
        }
        self.send("presence", body, None, now_ms)
    }

    /// Where saying the run's reply has got to (`playback.observe`, a latest value).
    pub fn observe_playback(
        &mut self,
        run_id: &str,
        upcoming_text: &str,
        rig: &Value,
        now_ms: i64,
    ) -> Vec<Output> {
        if self.phase != Phase::Open {
            return Vec::new();
        }
        if run_id.is_empty() || !rig.is_object() {
            return unsent("playback.observe", "bad_frame", None);
        }
        let body = json!({ "runId": run_id, "upcomingText": upcoming_text, "rig": rig });
        self.send("playback.observe", &body, None, now_ms)
    }

    /// The run's reply has been said, or stopped being said (`playback.end`, a request).
    pub fn end_playback(&mut self, run_id: &str, now_ms: i64) -> Vec<Output> {
        if self.phase != Phase::Open {
            return Vec::new();
        }
        if run_id.is_empty() {
            return unsent("playback.end", "bad_frame", None);
        }
        self.send("playback.end", &json!({ "runId": run_id }), None, now_ms)
    }

    /// How much of a reply or line was said aloud (`speech.delivered`, reliable).
    pub fn delivered(&mut self, id: &str, body: &Value, now_ms: i64) -> Vec<Output> {
        self.reliable_out(id, "speech.delivered", body, now_ms)
    }

    /// Contact going on with her figure here (`touch.appraise`, a request: nothing while down).
    pub fn appraise_touch(
        &mut self,
        touch_id: &str,
        touch: &Value,
        rig_state: Option<&Value>,
        now_ms: i64,
    ) -> Vec<Output> {
        if self.phase != Phase::Open {
            return Vec::new();
        }
        if !valid_id(touch_id) {
            return unsent("touch.appraise", "bad_id", None);
        }
        if !touch.is_object() {
            return unsent("touch.appraise", "bad_frame", None);
        }
        let mut body = json!({ "touchId": touch_id, "touch": touch });
        if let Some(rig_state) = rig_state.filter(|state| state.is_object()) {
            body["rigState"] = rig_state.clone();
        }
        self.send("touch.appraise", &body, None, now_ms)
    }

    /// A contact with her figure here ended (`touch`, reliable, its id the contact's).
    pub fn touched(&mut self, touch_id: &str, touch: &Value, now_ms: i64) -> Vec<Output> {
        if !touch.is_object() {
            return unsent("touch", "bad_frame", Some(touch_id));
        }
        self.reliable_out(touch_id, "touch", &json!({ "touch": touch }), now_ms)
    }

    /// The owner put the set `outfit_id` on her here (`outfit.wear`, a request).
    pub fn wear(&mut self, outfit_id: &str, now_ms: i64) -> Vec<Output> {
        if self.phase != Phase::Open {
            return Vec::new();
        }
        if outfit_id.is_empty() {
            return unsent("outfit.wear", "bad_frame", None);
        }
        self.send(
            "outfit.wear",
            &json!({ "outfitId": outfit_id }),
            None,
            now_ms,
        )
    }

    fn reliable_out(&mut self, id: &str, t: &str, body: &Value, now_ms: i64) -> Vec<Output> {
        if !valid_id(id) || id.ends_with(RESULT) {
            return unsent(t, "bad_id", Some(id));
        }
        // The same frame again under the id it waits under is the same frame; another under
        // that id would be lost, and is told. An id a turn was said under is not free either.
        if let Some(pending) = self.outbox.iter().find(|pending| pending.id == id) {
            return if pending.t == t && pending.body == *body {
                Vec::new()
            } else {
                unsent(t, "bad_id", Some(id))
            };
        }
        if self.turns.contains_key(id) {
            return unsent(t, "bad_id", Some(id));
        }
        self.outbox.push(Pending {
            id: id.to_string(),
            t: t.to_string(),
            body: body.clone(),
            seq: None,
            pass: false,
            needs: None,
        });
        if self.phase == Phase::Open {
            self.flush(now_ms)
        } else {
            Vec::new()
        }
    }

    // From the host

    /// A text frame from the host. One this end cannot read is told as [`Delivery::Unread`]
    /// before whatever else comes of it.
    pub fn received(&mut self, text: &str, now_ms: i64) -> Vec<Output> {
        let Some(envelope) = Envelope::parse(text) else {
            let unread = Output::Deliver(Delivery::Unread {
                t: None,
                code: "bad_frame".into(),
            });
            return match self.phase {
                Phase::Greeting => vec![unread, Output::Drop(Down::RefusedHandshake)],
                Phase::Open => {
                    // Its type, if it came with one, and why the core refuses it, as far
                    // as either can be read of what is no envelope.
                    let t = serde_json::from_str::<Value>(text)
                        .ok()
                        .and_then(|frame| string(&frame, "t"))
                        .unwrap_or_default();
                    let code = frames::refused(text).unwrap_or_else(|| "bad_frame".into());
                    let mut out = vec![unread];
                    out.extend(self.dropped(&t, &code, now_ms));
                    out
                }
                Phase::Closed => Vec::new(),
            };
        };
        let refused = frames::refused(text);
        let mut out: Vec<Output> = refused
            .iter()
            .filter(|_| self.phase != Phase::Closed)
            .map(|code| {
                Output::Deliver(Delivery::Unread {
                    t: Some(envelope.t.clone()).filter(|t| self.known_frames.contains(t)),
                    code: code.clone(),
                })
            })
            .collect();
        out.extend(self.read(&envelope, refused.is_none(), now_ms));
        if let Some(code) = &refused {
            out.extend(self.dropped(&envelope.t, code, now_ms));
        }
        out
    }

    /// Her host is told what this end drops (`stream.event` `link.dropped`), on a link that
    /// is open and where both ends listed `streams`: the frame's type as it came when it is
    /// written as an id (`unnamed` otherwise) and why, never anything it carried; once per
    /// type and reason on a link. The contract's reasons are four: every refusal of the
    /// core's that is none of the others is `bad_frame`.
    fn dropped(&mut self, t: &str, code: &str, now_ms: i64) -> Vec<Output> {
        if self.phase != Phase::Open || !self.both("streams") {
            return Vec::new();
        }
        let t = if valid_id(t) { t } else { "unnamed" };
        let reason = match code {
            "unknown_frame" | "version_unsupported" | "too_large" => code,
            _ => "bad_frame",
        };
        // Her host sends the types; this end does not keep a list as long as it likes. Past
        // so many kinds on one link, a new type is told as unnamed.
        let t = if self.told_dropped.len() >= Self::KEPT_DROPPED
            && !self.told_dropped.iter().any(|(told, _)| told == t)
        {
            "unnamed"
        } else {
            t
        };
        if !self
            .told_dropped
            .insert((t.to_string(), reason.to_string()))
        {
            return Vec::new();
        }
        // Kept within the same bound as the shell's events: with no room, not told.
        let Some(mut out) = self.room_for_event() else {
            return Vec::new();
        };
        // An id of this end's own making, from what it was given: when this link came up
        // and how many reports this session has made, which never starts over; and never
        // one a turn or a waiting frame already has.
        let id = loop {
            self.drops += 1;
            let id = format!("ld_{}_{}", self.opened_at_ms.max(0), self.drops);
            let taken =
                self.turns.contains_key(&id) || self.outbox.iter().any(|pending| pending.id == id);
            if !taken {
                break id;
            }
        };
        self.outbox.push(Pending {
            id,
            t: "stream.event".into(),
            body: json!({
                "name": "link.dropped",
                "version": 1,
                "at": now_ms.max(0),
                "body": { "t": t, "reason": reason },
            }),
            seq: None,
            pass: false,
            needs: None,
        });
        out.extend(self.flush(now_ms));
        out
    }

    fn read(&mut self, envelope: &Envelope, taken: bool, now_ms: i64) -> Vec<Output> {
        self.last_heard_ms = now_ms;
        self.heard = self.heard.max(envelope.seq);
        let acked = |pending: &Pending| pending.seq.is_some_and(|seq| seq <= envelope.ack);
        // Her host has the result of a call it asked: the shell is told, which keeps the
        // call on disk until then.
        let mut out: Vec<Output> = self
            .outbox
            .iter()
            .filter(|pending| pending.t == "call.result" && acked(pending))
            .filter_map(|pending| pending.id.strip_suffix(RESULT))
            .map(|call_id| {
                Output::Deliver(Delivery::CallSettled {
                    call_id: call_id.to_string(),
                })
            })
            .collect();
        self.outbox.retain(|pending| !acked(pending));
        out.extend(self.read_frame(envelope, taken, now_ms));
        out
    }

    fn read_frame(&mut self, envelope: &Envelope, taken: bool, now_ms: i64) -> Vec<Output> {
        // Whether the frame is taken is the shared core's; the numbering, acks and heartbeat are
        // the link's own, and count a refused frame too. One it refuses is read as carrying
        // nothing, which every reader below turns down: a ping is still answered and a reliable
        // one still acked.
        let body = if taken {
            envelope.body.clone()
        } else {
            Value::Null
        };
        if self.phase == Phase::Greeting {
            return match envelope.t.as_str() {
                "welcome" => self.welcome(&body, now_ms),
                "error" => self.host_error(&body),
                "ping" => self.send("pong", &json!({}), None, now_ms),
                _ => Vec::new(),
            };
        }
        if self.phase != Phase::Open {
            return Vec::new();
        }
        match envelope.t.as_str() {
            "ping" => self.send("pong", &json!({}), None, now_ms),
            "run.accepted" => self.accepted(&body, now_ms),
            "run.rejected" => self.rejected(&body),
            "run.event" => self.event(&body),
            "run.end" => {
                let Some(id) = string(&body, "turnId") else {
                    return Vec::new();
                };
                let Some(turn) = self.turns.get_mut(&id).filter(|turn| !turn.ended) else {
                    return Vec::new();
                };
                turn.ended = true;
                let run_id = string(&body, "runId")
                    .or_else(|| turn.run_id.clone())
                    .unwrap_or_default();
                vec![Output::Deliver(Delivery::End {
                    turn_id: id,
                    run_id,
                })]
            }
            "perform" => self.perform(&body),
            "mood" => {
                if !body.get("mood").is_some_and(Value::is_object) {
                    return Vec::new();
                }
                let revision = body["mood"]
                    .get("revision")
                    .and_then(Value::as_f64)
                    .filter(|r| r.is_finite())
                    .unwrap_or(0.0);
                if self.mood_revision.is_some_and(|shown| revision < shown) {
                    return Vec::new();
                }
                self.mood_revision = Some(revision);
                vec![Output::Deliver(Delivery::Mood(body))]
            }
            "error" => self.host_error(&body),
            "call.ask" => self.asked(envelope, &body, taken, now_ms),
            "call.accepted" | "call.progress" | "call.event" => {
                self.heard_of_call(&envelope.t, &body)
            }
            "call.result" => self.answered_to(envelope, &body, taken, now_ms),
            "stream.state" => match self.stream(false, &body) {
                Some((name, version, at)) => vec![Output::Deliver(Delivery::StreamState {
                    name,
                    version,
                    at,
                    value: body["value"].clone(),
                })],
                None => Vec::new(),
            },
            "stream.event" => self.stream_event_in(envelope, &body, taken, now_ms),
            "call.cancel" => match string(&body, "callId") {
                Some(call_id) => self.cancelled(&call_id, now_ms),
                None => Vec::new(),
            },
            "link.versions" if body.is_object() => {
                vec![Output::Deliver(Delivery::HostVersions(body))]
            }
            "link.verdict" => match string(&body, "usable") {
                Some(usable) => vec![Output::Deliver(Delivery::Verdict {
                    usable,
                    stopped: body
                        .get("stopped")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|what| what.as_str().map(str::to_string))
                        .collect(),
                })],
                None => Vec::new(),
            },
            "call.granted" => match (named(&body, "actions"), named(&body, "providing")) {
                (Some(actions), Some(providing)) => {
                    let delivery = Delivery::Granted {
                        actions: actions.clone(),
                        providing: providing.clone(),
                    };
                    let mut out = vec![Output::Deliver(delivery)];
                    // It decides only on a link where both ends listed `calls`: on another
                    // no call is taken, whatever is said to be granted.
                    if self.both("calls") {
                        self.granted = actions;
                        self.providing = providing;
                        let first = !self.heard_granted;
                        self.heard_granted = true;
                        // What came before her host said: judged now, by what it says.
                        for call in std::mem::take(&mut self.held) {
                            out.extend(self.judge(call, now_ms));
                        }
                        // And what this end waits to ask of it: asked now if it provides
                        // the action here, told not granted if it does not.
                        out.extend(self.settle_asks(first, now_ms));
                    }
                    out
                }
                _ => Vec::new(),
            },
            // One the core refuses is not handed to the shell with nothing in it. Her host
            // counts it delivered all the same: the link's ack is of everything heard (the
            // `ack` every frame of this end's carries, and the one sent here), so it is not
            // sent again. That this end could not read it reaches her host only as
            // `link.dropped`, where both ends listed `streams`. Its id is not kept, which
            // matters only to a host that would send the line anew under it.
            "say" if !taken => match envelope.id.as_deref().filter(|id| valid_id(id)) {
                Some(_) => self.send("ack", &json!({}), None, now_ms),
                None => Vec::new(),
            },
            "say" => self.reliable_in(envelope, body, now_ms, |id, body| Delivery::Say {
                id,
                body,
            }),
            "outfit" => {
                let (Some(outfit_id), Some(rig_asset_id)) =
                    (string(&body, "outfitId"), string(&body, "rigAssetId"))
                else {
                    return Vec::new();
                };
                if outfit_id.is_empty() || !asset_id(&rig_asset_id) {
                    return Vec::new();
                }
                let portrait_asset_id = string(&body, "portraitAssetId").filter(|id| asset_id(id));
                vec![Output::Deliver(Delivery::Outfit {
                    outfit_id,
                    rig_asset_id,
                    portrait_asset_id,
                })]
            }
            "wardrobe" => match body.get("outfits").and_then(Value::as_array) {
                Some(outfits) => vec![Output::Deliver(Delivery::Wardrobe(
                    outfits.iter().filter_map(WardrobeItem::read).collect(),
                ))],
                None => Vec::new(),
            },
            "touch.reaction" => match string(&body, "touchId").filter(|id| valid_id(id)) {
                Some(touch_id) => vec![Output::Deliver(Delivery::TouchReaction {
                    touch_id,
                    reaction: string(&body, "reaction"),
                })],
                None => Vec::new(),
            },
            "conversations" => match body.get("sessions") {
                Some(Value::Array(sessions)) => {
                    vec![Output::Deliver(Delivery::Conversations(sessions.clone()))]
                }
                _ => Vec::new(),
            },
            "conversation" => match body.get("session") {
                Some(session @ Value::Object(_))
                    if session.get("sessionId").is_some_and(Value::is_string) =>
                {
                    vec![Output::Deliver(Delivery::Conversation(session.clone()))]
                }
                _ => Vec::new(),
            },
            "notice" => {
                let ok = string(&body, "id").is_some_and(|id| valid_id(&id))
                    && body.get("kind").is_some_and(Value::is_string);
                if ok {
                    vec![Output::Deliver(Delivery::Notice(body))]
                } else {
                    Vec::new()
                }
            }
            _ => self.unread_frame(envelope, taken, now_ms),
        }
    }

    /// The most reliable frames handed to the shell that may wait for its word at once,
    /// counted apart from the frames this end keeps to send. A shell that reads its socket
    /// no faster than it takes frames never comes to it.
    pub const KEPT_PASSING: usize = 256;

    /// A frame of a type the session does not read. One the core refuses was told already
    /// (`Unread`). One it takes goes to a shell that takes such frames; to any other it is
    /// told as not read (`Unread` `unknown_frame`, and `link.dropped` to her host where both
    /// ends listed `streams`), so that it is not let fall in silence.
    fn unread_frame(&mut self, envelope: &Envelope, taken: bool, now_ms: i64) -> Vec<Output> {
        // One the session reads that brings nothing here (a pong) is read all the same.
        if !taken || Self::READS.contains(&envelope.t.as_str()) {
            return Vec::new();
        }
        let Some(rule) = crate::contract::contract().frames.get(&envelope.t) else {
            return Vec::new();
        };
        if !self.passes {
            let mut out = vec![Output::Deliver(Delivery::Unread {
                t: Some(envelope.t.clone()),
                code: "unknown_frame".into(),
            })];
            out.extend(self.dropped(&envelope.t, "unknown_frame", now_ms));
            return out;
        }
        let id = envelope.id.clone().filter(|id| valid_id(id));
        let frame = |id: Option<String>, link: u64, again: bool| {
            Output::Deliver(Delivery::Frame {
                t: envelope.t.clone(),
                id,
                seq: envelope.seq,
                link,
                delivery: rule.delivery.clone(),
                again,
            })
        };
        // Only a reliable frame waits for the shell's word: the contract promises no other
        // kind is delivered.
        let Some(id) = id.filter(|_| rule.delivery == "reliable") else {
            return vec![frame(id_of(envelope), self.links, false)];
        };
        // One the shell took before (her host did not hear this end's ack): acked, not
        // handed over again. But a frame its sender counts delivered only on the shell's
        // answer comes again because that answer was not heard: the shell is handed it
        // again, to answer again; it counts already, and waits for nothing.
        if self.taken.contains(&id) {
            let mut out = Vec::new();
            if myriad_her_link::TAKEN_BY_ANSWER.contains(&envelope.t.as_str()) {
                out.push(frame(Some(id), self.links, true));
            }
            out.extend(self.send("ack", &json!({}), None, now_ms));
            return out;
        }
        // The same frame again on this link, still waiting: nothing more. The same number
        // under another id is another frame (her host's numbers are not this end's to trust).
        let same = |passing: &Passing| passing.seq == envelope.seq && passing.id == id;
        if self.passing.iter().any(same) {
            return Vec::new();
        }
        // Too many wait for the shell. This one is neither handed over nor counted: what
        // this end acks stays short of it, the shell is told, and the link is dropped, so
        // that her host sends again on the next link what it sends again.
        if self.passing.len() >= Self::KEPT_PASSING {
            self.stalled = Some(self.stalled.map_or(envelope.seq, |at| at.min(envelope.seq)));
            return vec![
                Output::Deliver(Delivery::Unread {
                    t: Some(envelope.t.clone()),
                    code: "too_many".into(),
                }),
                Output::Drop(Down::Behind),
            ];
        }
        self.passing.push(Passing {
            seq: envelope.seq,
            id: id.clone(),
        });
        vec![frame(Some(id), self.links, false)]
    }

    /// The shell takes the frames of her host's that the session does not read
    /// ([`Delivery::Frame`]), or does not (as a session starts): then such a frame counts as
    /// heard and is told as not read.
    pub fn passes(&mut self, passes: bool) {
        self.passes = passes;
    }

    /// The shell has taken the reliable frames it was handed as `(link, seq)`, in any
    /// order: each counts now, and what this end acks moves up to just short of the lowest
    /// still waiting, said to her host at once with an `ack` when it moved. One that does
    /// not wait (taken before, of a link before this one, never handed over) is told,
    /// `Unsent` `ack` `bad_id`: a frame of a link before is her host's to send again, and
    /// is taken by its new number.
    pub fn took(&mut self, frames: &[(u64, u64)], now_ms: i64) -> Vec<Output> {
        let before = self.acks();
        let mut out = Vec::new();
        for (link, seq) in frames {
            let waits = self
                .passing
                .iter()
                .position(|passing| *link == self.links && passing.seq == *seq);
            let Some(index) = waits else {
                out.extend(unsent("ack", "bad_id", None));
                continue;
            };
            let passing = self.passing.remove(index);
            self.taken.push(passing.id);
            if self.taken.len() > Self::KEPT_TAKEN {
                let over = self.taken.len() - Self::KEPT_TAKEN;
                self.taken.drain(..over);
            }
        }
        if self.phase == Phase::Open && self.acks() > before {
            out.extend(self.send("ack", &json!({}), None, now_ms));
        }
        out
    }

    /// What this end acks: every frame of her host's up to this number is done with. A
    /// frame the session reads is done with when read; a reliable one handed to the shell,
    /// when the shell says it took it.
    fn acks(&self) -> u64 {
        self.passing
            .iter()
            .map(|passing| passing.seq)
            .chain(self.stalled)
            .min()
            .map_or(self.heard, |lowest| {
                lowest.saturating_sub(1).min(self.heard)
            })
            // Never back: a host whose numbers go back does not take back what was acked.
            .max(self.acked)
    }

    /// The frame types the session sends itself, each by a call of its own: not the
    /// shell's to send by [`Session::pass`].
    pub const OWNS: &[&str] = &[
        "ack",
        "hello",
        "input",
        "outfit.wear",
        "ping",
        "playback.end",
        "playback.observe",
        "pong",
        "presence",
        "run.resume",
        "speech.delivered",
        "touch",
        "touch.appraise",
    ];

    /// Send a frame of the shell's own: one the session has no call for. The session
    /// numbers it, says what it acks, and writes it as the shared core does; the core's
    /// check decides whether it is a frame at all. `needs` names a capability her host must
    /// have accepted on the link for the frame to go.
    ///
    /// Who sends it again is said here, once, and does not change: a reliable frame is
    /// kept until her host acks it and goes again after a reconnect, with the same id,
    /// unless `keep` is false; then it goes now, on an open link, and nothing of it is
    /// kept: sending it again is the shell's. A frame that is not reliable is never kept.
    ///
    /// What does not go is told, `Unsent` with the frame's type, its id and why: the
    /// core's refusal code; `owned`, a type the session sends itself (also every `call.*`,
    /// `link.*` and `stream.*`); `not_sender`, a type only her host's side sends;
    /// `sync_off`, a frame of the sync families on a link whose welcome said sync off;
    /// `not_accepted`, her host did not accept the capability `needs` names; `link_down`,
    /// there is no open link and the frame is not kept; `passthrough_off`, the shell did
    /// not say it takes the frames the session does not read ([`Session::passes`]).
    pub fn pass(
        &mut self,
        t: &str,
        id: Option<&str>,
        body: &Value,
        keep: Option<bool>,
        needs: Option<&str>,
        now_ms: i64,
    ) -> Vec<Output> {
        // Nothing told here repeats what came in: a type is said back only when it is one
        // the contract names.
        let named = if self.known_frames.contains(t) {
            t
        } else {
            "unnamed"
        };
        let untold = |code: &str| unsent(named, code, id);
        // A shell that does not take the frames the session does not read does not send
        // them either: it would not be handed her host's answers to them.
        if !self.passes {
            return untold("passthrough_off");
        }
        let owned = Self::OWNS.contains(&t)
            || ["call.", "link.", "stream."]
                .iter()
                .any(|family| t.starts_with(family));
        if owned {
            return untold("owned");
        }
        if let Err(refused) = frames::text(t, WHOLE, WHOLE, id, now_ms.max(0), body) {
            return untold(&refused.code);
        }
        let Some(rule) = crate::contract::contract().frames.get(t) else {
            return untold("unknown_frame");
        };
        // Her host's side alone sends these; a frame of the sync families between the ends
        // that keep her log is this end's too.
        let sync = myriad_her_link::needs_sync(t);
        let hers = match rule.from.as_str() {
            "server" => true,
            "host" => !sync,
            _ => false,
        };
        if hers {
            return untold("not_sender");
        }
        let reliable = rule.delivery == "reliable";
        if (keep == Some(true) && !reliable) || needs.is_some_and(|needs| !valid_id(needs)) {
            return untold("bad_frame");
        }
        let kept = reliable && keep != Some(false);
        let open = self.phase == Phase::Open;
        if !kept && !open {
            return untold("link_down");
        }
        // What an open link's welcome said settles it now; a kept frame handed over with
        // no open link is settled by the next welcome.
        if open && sync && !self.sync {
            return untold("sync_off");
        }
        if open && needs.is_some_and(|needs| !self.both(needs)) {
            return untold("not_accepted");
        }
        if !kept {
            return self.send(t, body, id, now_ms);
        }
        let Some(id) = id else {
            return untold("bad_id");
        };
        if !valid_id(id) || id.ends_with(RESULT) || self.turns.contains_key(id) {
            return untold("bad_id");
        }
        // The same frame again under the id it waits under is the same frame.
        if let Some(pending) = self.outbox.iter().find(|pending| pending.id == id) {
            return if pending.t == t && pending.body == *body {
                Vec::new()
            } else {
                untold("bad_id")
            };
        }
        let Some(mut out) = self.room_for_event() else {
            return untold("too_many");
        };
        self.outbox.push(Pending {
            id: id.to_string(),
            t: t.to_string(),
            body: body.clone(),
            seq: None,
            pass: true,
            needs: needs.map(str::to_string),
        });
        if open {
            out.extend(self.flush(now_ms));
        }
        out
    }

    /// This link's welcome is in: a frame of the shell's that waited and may not go on
    /// this link is let go and told, and does not wait for another.
    fn settle_passed(&mut self) -> Vec<Output> {
        let mut out = Vec::new();
        let mut kept = Vec::new();
        for pending in std::mem::take(&mut self.outbox) {
            let why = if !pending.pass {
                None
            } else if myriad_her_link::needs_sync(&pending.t) && !self.sync {
                Some("sync_off")
            } else if pending
                .needs
                .as_deref()
                .is_some_and(|needs| !self.both(needs))
            {
                Some("not_accepted")
            } else {
                None
            };
            match why {
                Some(code) => out.extend(unsent(&pending.t, code, Some(&pending.id))),
                None => kept.push(pending),
            }
        }
        self.outbox = kept;
        out
    }

    fn host_error(&self, body: &Value) -> Vec<Output> {
        string(body, "code")
            .map(|code| vec![Output::Deliver(Delivery::HostError { code })])
            .unwrap_or_default()
    }

    /// A reliable frame from the host: acked at once, delivered once per id however often it is resent.
    fn reliable_in(
        &mut self,
        envelope: &Envelope,
        body: Value,
        now_ms: i64,
        delivery: impl FnOnce(String, Value) -> Delivery,
    ) -> Vec<Output> {
        let Some(id) = envelope.id.clone().filter(|id| valid_id(id)) else {
            return Vec::new();
        };
        let ack = self.send("ack", &json!({}), None, now_ms);
        if self.taken.contains(&id) {
            return ack;
        }
        self.taken.push(id.clone());
        if self.taken.len() > Self::KEPT_TAKEN {
            let over = self.taken.len() - Self::KEPT_TAKEN;
            self.taken.drain(..over);
        }
        let mut out = vec![Output::Deliver(delivery(id, body))];
        out.extend(ack);
        out
    }

    fn welcome(&mut self, body: &Value, now_ms: i64) -> Vec<Output> {
        let Some(device_id) = string(body, "deviceId").filter(|id| !id.is_empty()) else {
            return vec![Output::Drop(Down::RefusedHandshake)];
        };
        // Every end speaks one protocol and neither falls back: a welcome that says
        // another than this end's hello did is not a link to go on with.
        // Read as a number however it is written (`2`, `2.0`), like every count here.
        let protocol = |from: Option<&Value>| from.and_then(unsigned);
        if protocol(body.get("protocol")) != protocol(self.hello.get("protocol")) {
            return vec![
                Output::Deliver(Delivery::Unread {
                    t: Some("welcome".into()),
                    code: "unsupported_protocol".into(),
                }),
                Output::Drop(Down::UnsupportedProtocol),
            ];
        }
        self.phase = Phase::Open;
        if let Some(beat) = body
            .get("heartbeatMs")
            .and_then(unsigned)
            .filter(|b| *b >= 50)
        {
            self.heartbeat_ms = self.timing.heartbeat_ms.min(beat as i64);
        }
        self.host_capabilities = body
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|all| {
                all.iter()
                    .filter_map(|c| c.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let lineage = string(body, "lineage");
        let host_epoch = body
            .get("hostEpoch")
            .and_then(unsigned)
            .map(|epoch| epoch.min(i64::MAX as u64) as i64);
        if let (Some(lineage), Some(epoch)) = (&lineage, host_epoch) {
            self.hello.insert("lineage".into(), json!(lineage));
            self.hello.insert("hostEpoch".into(), json!(epoch));
        }
        self.sync = body.get("sync").is_some_and(|sync| sync == "on");
        let mut out = vec![Output::Deliver(Delivery::Welcome {
            device_id,
            host: string(body, "host").unwrap_or_else(|| "site".into()),
            lineage,
            host_epoch,
            sync: self.sync,
            partner: string(body, "partner").filter(|partner| !partner.is_empty()),
        })];
        out.extend(self.settle_passed());
        // A report of what this end dropped is for a host that listed `streams`: one still
        // waiting is not carried to a link where it did not.
        let report = |pending: &Pending| {
            pending.t == "stream.event" && pending.body["name"] == "link.dropped"
        };
        if !self.both("streams") {
            self.outbox.retain(|pending| !report(pending));
        }
        // What waits from the link before goes again on this one, and is this link's telling
        // of that kind: it is not told a second time here.
        let waiting: Vec<(String, String)> = self
            .outbox
            .iter()
            .filter(|pending| report(pending))
            .filter_map(|pending| {
                let body = &pending.body["body"];
                Some((string(body, "t")?, string(body, "reason")?))
            })
            .collect();
        self.told_dropped.extend(waiting);
        // Her host's clock, for the deadlines of what it asks: how far ahead of the shell's.
        if let Some(server) = body.get("serverTime").and_then(unsigned) {
            self.host_ahead_ms = (server as i64).saturating_sub(now_ms);
        }
        out.extend(self.introduce(self.schema_version, now_ms));
        // Unanswered inputs again (same id, new seq); then what was missed of each unfinished turn.
        // One whose input her host acked and never answered has no run yet: it is asked for by
        // its turn alone, and a host that does not have it rejects it (`turn_unknown`), so the
        // owner is told it was not sent instead of waiting on it for good.
        out.extend(self.flush(now_ms));
        let waiting: BTreeSet<String> = self.outbox.iter().map(|p| p.id.clone()).collect();
        let resumes: Vec<(String, Option<String>, u64)> = self
            .order
            .iter()
            .filter_map(|id| self.turns.get(id))
            .filter(|turn| !turn.ended && !waiting.contains(&turn.turn_id))
            .map(|turn| {
                (
                    turn.turn_id.clone(),
                    turn.run_id.clone(),
                    turn.last_sequence,
                )
            })
            .collect();
        for (turn_id, run_id, after) in resumes {
            out.extend(self.resume(&turn_id, run_id.as_deref(), after, now_ms));
        }
        out
    }

    fn accepted(&mut self, body: &Value, now_ms: i64) -> Vec<Output> {
        let (Some(id), Some(run_id)) = (string(body, "turnId"), string(body, "runId")) else {
            return Vec::new();
        };
        if run_id.is_empty() || !self.turns.contains_key(&id) {
            return Vec::new();
        }
        self.outbox.retain(|pending| pending.id != id);
        let duplicate = body.get("duplicate") == Some(&Value::Bool(true));
        let turn = self.turns.get_mut(&id).expect("checked above");
        turn.run_id = Some(run_id.clone());
        turn.answered = true;
        if let Some(session) = string(body, "sessionId") {
            turn.session_id = Some(session);
        }
        let (session_id, ended, after) = (turn.session_id.clone(), turn.ended, turn.last_sequence);
        let mut out = vec![Output::Deliver(Delivery::Accepted {
            turn_id: id.clone(),
            run_id: run_id.clone(),
            session_id,
            duplicate,
        })];
        // A resend the host already had: nothing streams until it is asked for.
        if duplicate && !ended {
            out.extend(self.resume(&id, Some(&run_id), after, now_ms));
        }
        out
    }

    fn rejected(&mut self, body: &Value) -> Vec<Output> {
        let Some(id) = string(body, "turnId") else {
            return Vec::new();
        };
        if !self.turns.contains_key(&id) {
            return Vec::new();
        }
        self.outbox.retain(|pending| pending.id != id);
        let turn = self.turns.get_mut(&id).expect("checked above");
        if turn.ended {
            return Vec::new();
        }
        turn.ended = true;
        turn.answered = true;
        let code = string(body, "code").unwrap_or_else(|| "internal_error".into());
        vec![Output::Deliver(Delivery::Rejected { turn_id: id, code })]
    }

    fn event(&mut self, body: &Value) -> Vec<Output> {
        let (Some(id), Some(run_id), Some(sequence)) = (
            string(body, "turnId"),
            string(body, "runId"),
            body.get("sequence").and_then(unsigned),
        ) else {
            return Vec::new();
        };
        if !body.get("event").is_some_and(Value::is_object) {
            return Vec::new();
        }
        let Some(turn) = self.turns.get_mut(&id) else {
            return Vec::new();
        };
        if turn.ended || turn.run_id.as_ref().is_some_and(|known| *known != run_id) {
            return Vec::new();
        }
        if sequence == 0 {
            if turn.saw_start || turn.last_sequence != 0 {
                return Vec::new();
            }
            turn.saw_start = true;
        } else {
            if sequence <= turn.last_sequence {
                return Vec::new();
            }
            turn.last_sequence = sequence;
        }
        // A turn asked for by its turn alone is answered by its run's events, with no
        // `run.accepted`: the first of them says which run it is.
        turn.run_id = Some(run_id.clone());
        turn.answered = true;
        vec![Output::Deliver(Delivery::Event {
            turn_id: id,
            run_id,
            sequence,
            event: body["event"].clone(),
        })]
    }

    fn perform(&mut self, body: &Value) -> Vec<Output> {
        let Some(directive) = body.get("directive").filter(|d| d.is_object()).cloned() else {
            return Vec::new();
        };
        if let Some(say) = body.get("sayId").and_then(Value::as_str) {
            if !valid_id(say) {
                return Vec::new();
            }
            return vec![Output::Deliver(Delivery::PerformSay {
                say_id: say.to_string(),
                directive,
            })];
        }
        let Some(id) = string(body, "turnId") else {
            return vec![Output::Deliver(Delivery::Perform {
                turn_id: None,
                run_id: None,
                directive,
                live: None,
            })];
        };
        let current = self.current_turn.as_deref() == Some(id.as_str());
        let Some(turn) = self.turns.get_mut(&id) else {
            return Vec::new();
        };
        if let Some(live) = body.get("live").and_then(unsigned) {
            // Her director's refinement while the reply is said: also after the run's end, the
            // higher version newer; another turn's is old.
            if !current || turn.live.is_some_and(|played| live <= played) {
                return Vec::new();
            }
            turn.live = Some(live);
            let run_id = string(body, "runId").or_else(|| turn.run_id.clone());
            return vec![Output::Deliver(Delivery::Perform {
                turn_id: Some(id),
                run_id,
                directive,
                live: Some(live),
            })];
        }
        // Within a turn the higher sequence is newer; another turn's directive is old, and so is
        // the run's own once her director has refined it.
        if turn.ended || !current || turn.live.is_some() {
            return Vec::new();
        }
        let sequence = body.get("sequence").and_then(unsigned).unwrap_or(0);
        if turn.performed.is_some_and(|played| sequence <= played) {
            return Vec::new();
        }
        turn.performed = Some(sequence);
        let run_id = string(body, "runId").or_else(|| turn.run_id.clone());
        vec![Output::Deliver(Delivery::Perform {
            turn_id: Some(id),
            run_id,
            directive,
            live: None,
        })]
    }

    fn resume(
        &mut self,
        turn_id: &str,
        run_id: Option<&str>,
        after: u64,
        now_ms: i64,
    ) -> Vec<Output> {
        let mut body = json!({ "turnId": turn_id, "afterSeq": after });
        if let Some(run_id) = run_id {
            body["runId"] = json!(run_id);
        }
        self.send("run.resume", &body, None, now_ms)
    }

    // Calls: her host asks this end to do one thing for her

    pub const KEPT_ANSWERS: usize = 64;

    /// The time by her host's clock, as the last welcome set it right.
    fn host_now(&self, now_ms: i64) -> i64 {
        now_ms.saturating_add(self.host_ahead_ms)
    }

    /// `call.ask`: acked, and then answered at once when it is not this end's to do, handed
    /// to the shell when it is, or answered again as before when it was asked already.
    fn asked(
        &mut self,
        envelope: &Envelope,
        body: &Value,
        taken: bool,
        now_ms: i64,
    ) -> Vec<Output> {
        // Only on a link where both ends listed `calls`: on another it passes by, as any
        // frame this end does not read.
        if !self.both("calls") {
            return Vec::new();
        }
        let Some(call_id) = envelope.id.clone().filter(|id| valid_id(id)) else {
            return Vec::new();
        };
        let mut out = self.send("ack", &json!({}), None, now_ms);
        // One the core refuses is acked, so it is not sent for good, and no more: there is
        // no saying what it asked.
        if !taken {
            return out;
        }
        // Asked again while it is in hand (her host did not hear the first time): not done
        // again, and a long one is said to be in hand again, as it was the first time.
        if let Some(call) = self.doing.iter().find(|call| call.call_id == call_id) {
            if myriad_her_link::call_spec(&call.action).is_some_and(|spec| spec.long) {
                out.extend(self.send("call.accepted", &json!({ "callId": call_id }), None, now_ms));
            }
            return out;
        }
        if let Some(before) = self.answered.iter().find(|a| a.call_id == call_id).cloned() {
            out.extend(self.answer(&before.call_id, before.result, now_ms));
            return out;
        }
        // Asked again while it waits for her host's word on what is granted: it waits on.
        if self.held.iter().any(|call| call.call_id == call_id) {
            return out;
        }
        let (Some(action), Some(version)) = (
            string(body, "action"),
            body.get("version").and_then(unsigned),
        ) else {
            return out;
        };
        let call = Held {
            call_id,
            action,
            version,
            args: body.get("args").cloned().unwrap_or_else(|| json!({})),
            of: body.get("for").cloned().unwrap_or(Value::Null),
            // As it came; what it is held to is the core's to say (`call_receive`).
            deadline_at: body
                .get("deadlineAt")
                .and_then(unsigned)
                .map_or(0, |at| at.min(i64::MAX as u64) as i64),
        };
        out.extend(self.judge(call, now_ms));
        out
    }

    /// A call her host asks, or one that waited for its word on what is granted: handed to
    /// the shell, kept waiting, or answered why not.
    fn judge(&mut self, call: Held, now_ms: i64) -> Vec<Output> {
        // One that was answered while it waited (handed back as lost) stays answered, and
        // one already in hand is not handed over again.
        if self.doing.iter().any(|doing| doing.call_id == call.call_id) {
            return Vec::new();
        }
        if let Some(before) = self
            .answered
            .iter()
            .find(|a| a.call_id == call.call_id)
            .cloned()
        {
            return self.answer(&before.call_id, before.result, now_ms);
        }
        // What is done with it is the shared core's to say, the same for every end
        // (`myriad_her_link::call_receive`): by what this end offers now (it may have changed
        // while the call waited), by her host's word on this link on what is granted, and
        // only that, and by the deadline on her host's clock. Before her host has said, the
        // call waits: not handed to the shell, which would be acting on a grant that may
        // have been taken back, and not answered, which would be refusing what may be
        // granted. A deadline further than its action may be given is cut short, not refused.
        let received = myriad_her_link::call_receive(
            &call.action,
            call.version,
            &call.of,
            call.deadline_at.max(0) as u64,
            self.host_now(now_ms).max(0) as u64,
            &self.offers,
            &[],
            self.heard_granted.then_some(&self.granted[..]),
        );
        let mut call = call;
        call.deadline_at = received.deadline_at.min(i64::MAX as u64) as i64;
        match received.verdict {
            myriad_her_link::CallVerdict::Do => {}
            myriad_her_link::CallVerdict::Hold => {
                self.held.push(call);
                return Vec::new();
            }
            // The version said back is the one this end does.
            myriad_her_link::CallVerdict::Fail(code) => {
                let result = failed(&call.call_id, &call.action, received.version, code);
                return self.answer(&call.call_id, result, now_ms);
            }
        }
        self.doing.push(Doing {
            call_id: call.call_id.clone(),
            action: call.action.clone(),
            version: call.version,
            deadline_at: call.deadline_at,
            pieces: 0,
        });
        let long = myriad_her_link::call_spec(&call.action).is_some_and(|spec| spec.long);
        let accepted = json!({ "callId": call.call_id });
        let mut out = vec![Output::Deliver(Delivery::Call {
            call_id: call.call_id,
            action: call.action,
            version: call.version,
            args: call.args,
            of: call.of,
            deadline_at: call.deadline_at,
        })];
        // A long one is said to be in hand at once (within `callAcceptMs`); with no link
        // now, it is said when her host asks again.
        if long && self.phase == Phase::Open {
            out.extend(self.send("call.accepted", &accepted, None, now_ms));
        }
        out
    }

    /// `call.cancel`: a call in hand is stopped and answered `call_cancelled`; one already
    /// answered is answered again as it was; one this end does not know is passed by.
    fn cancelled(&mut self, call_id: &str, now_ms: i64) -> Vec<Output> {
        if !self.both("calls") {
            return Vec::new();
        }
        if let Some(call) = self.take_doing(call_id) {
            let result = failed(&call.call_id, &call.action, call.version, "call_cancelled");
            let mut out = vec![Output::Deliver(Delivery::CallStop {
                call_id: call.call_id.clone(),
                why: "cancelled".into(),
            })];
            out.extend(self.answer(&call.call_id, result, now_ms));
            return out;
        }
        // One that waits was never the shell's: it is answered, and no one is stopped.
        if let Some(at) = self.held.iter().position(|call| call.call_id == call_id) {
            let call = self.held.remove(at);
            let result = failed(&call.call_id, &call.action, call.version, "call_cancelled");
            return self.answer(&call.call_id, result, now_ms);
        }
        match self.answered.iter().find(|a| a.call_id == call_id).cloned() {
            Some(before) => self.answer(&before.call_id, before.result, now_ms),
            None => Vec::new(),
        }
    }

    /// Every call whose deadline has passed: stopped, and answered `call_timeout`.
    fn expire(&mut self, now_ms: i64) -> Vec<Output> {
        let host_now = self.host_now(now_ms);
        let late: Vec<String> = self
            .doing
            .iter()
            .filter(|call| host_now >= call.deadline_at)
            .map(|call| call.call_id.clone())
            .collect();
        let mut out = Vec::new();
        for call_id in late {
            let Some(call) = self.take_doing(&call_id) else {
                continue;
            };
            out.push(Output::Deliver(Delivery::CallStop {
                call_id: call.call_id.clone(),
                why: "timeout".into(),
            }));
            let result = failed(&call.call_id, &call.action, call.version, "call_timeout");
            out.extend(self.answer(&call.call_id, result, now_ms));
        }
        out
    }

    /// Every call still waiting for her host's word whose deadline has passed: answered
    /// `call_timeout`. The shell never had it, so no one is stopped.
    fn expire_held(&mut self, now_ms: i64) -> Vec<Output> {
        let host_now = self.host_now(now_ms);
        let (late, held): (Vec<Held>, Vec<Held>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|call| host_now >= call.deadline_at);
        self.held = held;
        let mut out = Vec::new();
        for call in late {
            let result = failed(&call.call_id, &call.action, call.version, "call_timeout");
            out.extend(self.answer(&call.call_id, result, now_ms));
        }
        out
    }

    fn take_doing(&mut self, call_id: &str) -> Option<Doing> {
        let at = self.doing.iter().position(|call| call.call_id == call_id)?;
        Some(self.doing.remove(at))
    }

    /// Why the core will not send `body` as the `call.result` of `call_id`, if it will not:
    /// tried as the longest frame it can go out as (the largest numbering and time a frame
    /// carries), so one that passes here fits whenever it goes.
    fn unfit(call_id: &str, body: &Value, now_ms: i64) -> Option<String> {
        let id = format!("{call_id}{RESULT}");
        let _ = now_ms;
        frames::text("call.result", WHOLE, WHOLE, Some(&id), WHOLE as i64, body)
            .err()
            .map(|refused| refused.code)
    }

    /// The one `call.result` of `call_id`: kept, so that the call asked again is answered
    /// the same, and sent as a reliable frame whose id is the call's and `.result`.
    fn answer(&mut self, call_id: &str, result: Value, now_ms: i64) -> Vec<Output> {
        self.answered.retain(|a| a.call_id != call_id);
        self.answered.push(Answered {
            call_id: call_id.to_string(),
            result: result.clone(),
        });
        let id = format!("{call_id}{RESULT}");
        if !self.outbox.iter().any(|pending| pending.id == id) {
            self.outbox.push(Pending {
                id,
                t: "call.result".into(),
                body: result,
                seq: None,
                pass: false,
                needs: None,
            });
        }
        // Only so many answers are kept, the oldest let go first; but never one whose
        // result her host has not acked yet: asked again, that call would be done again.
        let mut over = self.answered.len().saturating_sub(Self::KEPT_ANSWERS);
        let outbox = &self.outbox;
        self.answered.retain(|kept| {
            let waiting = outbox
                .iter()
                .any(|pending| pending.id == format!("{}{RESULT}", kept.call_id));
            if over > 0 && !waiting {
                over -= 1;
                false
            } else {
                true
            }
        });
        if self.phase == Phase::Open {
            self.flush(now_ms)
        } else {
            Vec::new()
        }
    }

    /// The call `call_id` is done, with `result` (the action's result fields) and the ids of
    /// the blobs it put at her host, if it is a blob action. Answered once: a call that is
    /// not in hand (never asked, answered already, stopped) is told `Unsent`, as is a result
    /// the contract does not take for the action, and then her host is told the call failed
    /// here (`internal_error`), not left waiting.
    pub fn call_done(
        &mut self,
        call_id: &str,
        result: &Value,
        blobs: &[String],
        now_ms: i64,
    ) -> Vec<Output> {
        let Some(call) = self
            .doing
            .iter()
            .find(|call| call.call_id == call_id)
            .cloned()
        else {
            return unsent("call.result", "bad_id", Some(call_id));
        };
        let mut body = json!({
            "callId": call.call_id,
            "action": call.action,
            "version": call.version,
            "outcome": "done",
            "result": result,
        });
        if !blobs.is_empty() {
            body["blobs"] = json!(blobs);
        }
        let mut out = Vec::new();
        if let Some(code) = Self::unfit(call_id, &body, now_ms) {
            out.extend(unsent("call.result", &code, Some(call_id)));
            body = failed(&call.call_id, &call.action, call.version, "internal_error");
        }
        self.take_doing(call_id);
        out.extend(self.answer(call_id, body, now_ms));
        out
    }

    /// The call `call_id` was not done: `code` is one of the contract's `calls.failCodes`
    /// (`declined`, `not_now`, `call_not_granted`, `internal_error`…). Anything else is
    /// told `Unsent` and answered `internal_error`.
    pub fn call_failed(&mut self, call_id: &str, code: &str, now_ms: i64) -> Vec<Output> {
        let Some(call) = self.take_doing(call_id) else {
            return unsent("call.result", "bad_id", Some(call_id));
        };
        let mut body = failed(&call.call_id, &call.action, call.version, code);
        let mut out = Vec::new();
        if let Some(code) = Self::unfit(call_id, &body, now_ms) {
            out.extend(unsent("call.result", &code, Some(call_id)));
            body = failed(&call.call_id, &call.action, call.version, "internal_error");
        }
        out.extend(self.answer(call_id, body, now_ms));
        out
    }

    /// Where the long call `call_id` is: `phase` is a stage name of the action's own, never
    /// content; `waiting_for_owner` while this end waits for the owner to answer on the
    /// device. Only the newest counts, and none is kept while the link is down.
    pub fn call_progress(
        &mut self,
        call_id: &str,
        phase: &str,
        waiting_for_owner: bool,
        now_ms: i64,
    ) -> Vec<Output> {
        if !self.doing.iter().any(|call| call.call_id == call_id) {
            return unsent("call.progress", "bad_id", Some(call_id));
        }
        if self.phase != Phase::Open || !self.both("calls") {
            return Vec::new();
        }
        let mut body = json!({ "callId": call_id, "phase": phase });
        if waiting_for_owner {
            body["waiting"] = json!("owner");
        }
        self.send("call.progress", &body, None, now_ms)
    }

    /// The next piece of the result of the call `call_id`, for an action whose result comes
    /// in pieces. Numbered here, from 1. Lost with the link: the whole still goes in
    /// [`Session::call_done`].
    pub fn call_piece(&mut self, call_id: &str, piece: &Value, now_ms: i64) -> Vec<Output> {
        let Some(call) = self.doing.iter_mut().find(|call| call.call_id == call_id) else {
            return unsent("call.event", "bad_id", Some(call_id));
        };
        call.pieces += 1;
        let body = json!({ "callId": call_id, "sequence": call.pieces, "piece": piece });
        if self.phase != Phase::Open || !self.both("calls") {
            return Vec::new();
        }
        self.send("call.event", &body, None, now_ms)
    }

    /// Calls this end had in hand when it lost track of them: it was restarted, and its
    /// session with it. Each is `(call_id, action, version)` as the shell wrote it down
    /// before it began ([`Delivery::Call`]). Each is answered `unknown`, which is the truth
    /// (it may have happened), and is never handed to the shell again: asked again under
    /// that id, it is answered the same. No end does it again by itself. The shell hands
    /// them back before it connects: an ask that comes first under one of these ids on a
    /// session that does not know it would be taken for a new call.
    pub fn lost(&mut self, calls: &[(&str, &str, u64)], now_ms: i64) -> Vec<Output> {
        let mut out = Vec::new();
        for (call_id, action, version) in calls {
            // One this session still has in hand (its state was kept across the restart) is
            // lost all the same: the shell is not doing it. One answered already stands.
            if self.answered.iter().any(|a| a.call_id == *call_id) {
                out.extend(unsent("call.result", "bad_id", Some(call_id)));
                continue;
            }
            // One that still waits for her host's word in a kept state was handed to the
            // shell after that state was kept: it waits no more.
            let waited = self
                .held
                .iter()
                .position(|call| call.call_id == *call_id)
                .map(|at| self.held.remove(at));
            let (action, version) = match (self.take_doing(call_id), waited) {
                (Some(call), _) => (call.action, call.version),
                (None, Some(call)) => (call.action, call.version),
                (None, None) => (action.to_string(), *version),
            };
            let body = json!({
                "callId": call_id,
                "action": action,
                "version": version,
                "outcome": "unknown",
            });
            match Self::unfit(call_id, &body, now_ms) {
                Some(code) => out.extend(unsent("call.result", &code, Some(call_id))),
                None => out.extend(self.answer(call_id, body, now_ms)),
            }
        }
        out
    }

    // Calls: this end asks her host to do one thing

    /// How long after its deadline a call with no answer is counted timed out (the
    /// contract's `call.ask`: "a minute after deadlineAt").
    pub const CALL_GRACE_MS: i64 = 60_000;

    /// Ask her host to do `action` at `version` with `args`, for `of` (`{kind, groupId?}`),
    /// to be answered within `within_ms` from now, cut short to the most a call of the
    /// action may be given (`timing.callDeadlineMaxMs`, `longCallMaxMs` for a long one); the
    /// deadline goes out by her host's clock, the frame's own time by this end's. `call_id`
    /// is the shell's, made once (at
    /// most 57 characters): the ask is a reliable frame under it, kept and sent again with
    /// the same deadline until her host acks it. Not asked, and told `Unsent`, when there is
    /// no open link (`call_unavailable`), when the two ends did not both list `calls` on it
    /// or her host has not said it provides the action at that version here
    /// (`call_not_granted`), when the action is not one the contract registers
    /// (`call_unknown`) or not for `of` (`wrong_audience`), when the
    /// id is already a call's or another frame's (`bad_id`), or when the frame is not one
    /// the contract takes. An ask not acked when the link goes waits for the next link's
    /// word on what her host provides there: it goes again if it does, and ends
    /// `call_not_granted` (told here) if it does not. What comes
    /// of it is delivered: [`Delivery::CallAccepted`], `CallProgress`, `CallPiece`, and one
    /// [`Delivery::CallResult`].
    #[allow(clippy::too_many_arguments)]
    pub fn call(
        &mut self,
        call_id: &str,
        action: &str,
        version: u64,
        args: &Value,
        of: &Value,
        within_ms: i64,
        now_ms: i64,
    ) -> Vec<Output> {
        let refuse = |code: &str| unsent("call.ask", code, Some(call_id));
        // Her host is asked only while it is linked, and only for what it has said, on this
        // link, that it provides.
        if self.phase != Phase::Open {
            return refuse("call_unavailable");
        }
        if !self.both("calls") || !self.providing.contains(&(action.to_string(), version)) {
            return refuse("call_not_granted");
        }
        // A call's id is made once: not one whose result has already come.
        if self.taken.contains(&format!("{call_id}{RESULT}")) {
            return refuse("bad_id");
        }
        // Not an action the contract registers (`call_unknown`), or not for `of`.
        if let Err(code) = myriad_her_link::call_allowed(action, of) {
            return refuse(code);
        }
        if self.asking.iter().any(|call| call.call_id == call_id) {
            return refuse("bad_id");
        }
        // The deadline is by her host's clock, whoever asks (this end's clock as set right by
        // the latest welcome), and no further than a call of the action may be given from
        // now: her host holds it to that anyway (`call_deadline`), so both hold the same.
        let asked_at = self.host_now(now_ms).max(0) as u64;
        let wanted = asked_at.saturating_add(within_ms.max(0) as u64);
        let deadline_at =
            myriad_her_link::call_deadline(action, wanted, asked_at).min(i64::MAX as u64) as i64;
        let body = json!({
            "callId": call_id,
            "action": action,
            "version": version,
            "args": args,
            "for": of,
            "deadlineAt": deadline_at,
        });
        // As her host will check it.
        if let Err(refused) = frames::text("call.ask", WHOLE, WHOLE, Some(call_id), now_ms, &body) {
            return refuse(&refused.code);
        }
        let mut out = self.reliable_out(call_id, "call.ask", &body, now_ms);
        // Whether this ask went, by what is told of it: not of another frame that waited.
        let refused = out.iter().any(|o| {
            matches!(o, Output::Deliver(Delivery::Unsent { t, id, .. })
                if t == "call.ask" && id.as_deref() == Some(call_id))
        });
        if !refused {
            self.asking.push(Asking {
                call_id: call_id.to_string(),
                deadline_at,
                pieces: 0,
                late: false,
                action: Some(action.to_string()),
            });
            // Calls told ended here with no result since are not kept without end: past so
            // many, the oldest are let go, and the shell is told which.
            let mut over = self
                .asking
                .iter()
                .filter(|call| call.late)
                .count()
                .saturating_sub(Self::KEPT_ANSWERS);
            let mut gone = Vec::new();
            self.asking.retain(|call| {
                if over > 0 && call.late {
                    over -= 1;
                    gone.push(call.call_id.clone());
                    false
                } else {
                    true
                }
            });
            if !gone.is_empty() {
                self.outbox
                    .retain(|pending| !(pending.t == "call.ask" && gone.contains(&pending.id)));
                self.cancels.retain(|id| !gone.contains(id));
                out.push(Output::Deliver(Delivery::CallsForgotten { call_ids: gone }));
            }
        }
        out
    }

    /// Whether her host has said, on this link, that it provides what the `call.ask` body
    /// asks: on a link where both ends listed `calls`.
    fn provided(&self, ask: &Value) -> bool {
        let named = string(ask, "action").zip(ask.get("version").and_then(unsigned));
        self.both("calls") && named.is_some_and(|named| self.providing.contains(&named))
    }

    /// Her host has just said what it provides on this link: an ask that waited from the
    /// link before goes now if it provides the action here; if it does not, the ask is let
    /// go and the shell is told the call was not granted, here.
    ///
    /// `first` is whether this is her host's first word on this link: the stops that wait
    /// from before go then. One made on this link after that is for the next link, and a
    /// later word of her host's on this one does not send it early.
    fn settle_asks(&mut self, first: bool, now_ms: i64) -> Vec<Output> {
        // Only asks that wait from a link before: one sent on this link is her host's to
        // answer, whatever it says it provides now.
        let refused: Vec<String> = self
            .outbox
            .iter()
            .filter(|pending| {
                pending.t == "call.ask" && pending.seq.is_none() && !self.provided(&pending.body)
            })
            .map(|pending| pending.id.clone())
            .collect();
        self.outbox
            .retain(|pending| pending.t != "call.ask" || !refused.contains(&pending.id));
        let mut out = Vec::new();
        for call in &mut self.asking {
            // Told here once (not again of one already told timed out), and kept as asked:
            // her host may have had the ask after all, and its result is what happened.
            if refused.contains(&call.call_id) && !call.late {
                call.late = true;
                out.push(Output::Deliver(Delivery::CallResult {
                    call_id: call.call_id.clone(),
                    outcome: "failed".into(),
                    result: None,
                    code: Some("call_not_granted".into()),
                    blobs: Vec::new(),
                    here: true,
                }));
            }
        }
        out.extend(self.flush(now_ms));
        // The long calls cancelled while their ask waited: her host, had it had the ask on a
        // link before, is told to stop; had it not, it passes this by.
        if first && self.both("calls") {
            for call_id in std::mem::take(&mut self.cancels) {
                out.extend(self.send("call.cancel", &json!({ "callId": call_id }), None, now_ms));
            }
        }
        out
    }

    /// This end no longer wants the call `call_id`. What comes of that is always told:
    ///
    /// - Its ask waits to go again (not sent on this link, or no link): the ask is taken
    ///   back and never goes, and the call ends here, `CallResult` `failed`
    ///   `call_cancelled` with `here` (when it was already told ended here: `Unsent`
    ///   `call.cancel` `call_ended`). Her host may have had the ask on a link before: a long
    ///   one it is told to stop on the next link where both ends listed `calls`, after its
    ///   word on what it provides there, and a result that still comes is delivered.
    /// - Its ask went on this link, or her host acked it, and the action is long: her host
    ///   is told to stop (`call.cancel`) now, on a link where both ends listed `calls`
    ///   (an ask not acked yet is taken back, so that it cannot go again after the stop,
    ///   and her host is told to stop once more on the next link);
    ///   with no such link the shell is told `Unsent` `call.cancel` `call_unavailable`, to
    ///   cancel again when linked. Its result still comes.
    /// - It went and the action is not long: there is no stopping it (the contract cancels
    ///   long calls only). `Unsent` `call.cancel` `call_sent`; its result still comes.
    /// - Cancelled again after it was taken back, or after it was told ended here, with
    ///   nothing to tell her host now: `Unsent` `call.cancel` `call_ended`.
    /// - Not a call this end asks: `Unsent` `call.cancel` `bad_id`.
    pub fn call_cancel(&mut self, call_id: &str, now_ms: i64) -> Vec<Output> {
        let untold = |code: &str| unsent("call.cancel", code, Some(call_id));
        let Some(call) = self.asking.iter_mut().find(|call| call.call_id == call_id) else {
            return untold("bad_id");
        };
        // One whose action this state does not say is taken for long: a stop too many is
        // passed by, one too few is not.
        let long = call
            .action
            .as_deref()
            .is_none_or(|action| myriad_her_link::call_spec(action).is_none_or(|spec| spec.long));
        let waits = |pending: &Pending| {
            pending.t == "call.ask" && pending.id == call_id && pending.seq.is_none()
        };
        if self.outbox.iter().any(waits) {
            let told = call.late;
            call.late = true;
            self.outbox.retain(|pending| !waits(pending));
            if long && !self.cancels.iter().any(|id| id == call_id) {
                self.cancels.push(call_id.to_string());
            }
            return if told {
                untold("call_ended")
            } else {
                vec![Output::Deliver(Delivery::CallResult {
                    call_id: call_id.to_string(),
                    outcome: "failed".into(),
                    result: None,
                    code: Some("call_cancelled".into()),
                    blobs: Vec::new(),
                    here: true,
                })]
            };
        }
        // Its ask does not wait. One already told ended here has nothing more to be told,
        // unless it is long and her host can be told to stop now.
        let told = call.late;
        if !long {
            return untold(if told { "call_ended" } else { "call_sent" });
        }
        let queued = self.cancels.iter().any(|id| id == call_id);
        // No link to tell her host on, or the stop already waits for her host's word on
        // what it provides on this link (it goes then, not before): nothing goes now.
        let open = self.phase == Phase::Open && self.both("calls");
        if !open || (queued && !self.heard_granted) {
            return untold(if told {
                "call_ended"
            } else {
                "call_unavailable"
            });
        }
        // Its ask went on this link and is not acked: were the link to go now, the ask would
        // go again on the next with nothing to stop it. It does not: the ask is taken back,
        // and her host, which may have it, is told to stop once more on the next link.
        let unacked = |pending: &Pending| pending.t == "call.ask" && pending.id == call_id;
        if self.outbox.iter().any(unacked) {
            self.outbox.retain(|pending| !unacked(pending));
            if !queued {
                self.cancels.push(call_id.to_string());
            }
        }
        // A stop that waits for the next link is not undone by another made on this one: it
        // leaves when it goes, when her host's result comes, or with the call.
        self.send("call.cancel", &json!({ "callId": call_id }), None, now_ms)
    }

    /// `call.accepted`, `call.progress`, `call.event` of a call this end asked.
    fn heard_of_call(&mut self, t: &str, body: &Value) -> Vec<Output> {
        let Some(call_id) = string(body, "callId") else {
            return Vec::new();
        };
        let Some(call) = self.asking.iter_mut().find(|call| call.call_id == call_id) else {
            return Vec::new();
        };
        let delivery = match t {
            "call.accepted" => Delivery::CallAccepted { call_id },
            "call.progress" => match string(body, "phase") {
                Some(phase) => Delivery::CallProgress {
                    call_id,
                    phase,
                    waiting_for_owner: body.get("waiting").is_some_and(|w| w == "owner"),
                },
                None => return Vec::new(),
            },
            _ => {
                // In order, each once: one behind what was taken is old.
                let Some(sequence) = body.get("sequence").and_then(unsigned) else {
                    return Vec::new();
                };
                if sequence <= call.pieces {
                    return Vec::new();
                }
                call.pieces = sequence;
                Delivery::CallPiece {
                    call_id,
                    sequence,
                    piece: body.get("piece").cloned().unwrap_or(Value::Null),
                }
            }
        };
        vec![Output::Deliver(delivery)]
    }

    /// `call.result` of a call this end asked: acked, and delivered once.
    fn answered_to(
        &mut self,
        envelope: &Envelope,
        body: &Value,
        taken: bool,
        now_ms: i64,
    ) -> Vec<Output> {
        if !taken {
            return match envelope.id.as_deref().filter(|id| valid_id(id)) {
                Some(_) => self.send("ack", &json!({}), None, now_ms),
                None => Vec::new(),
            };
        }
        let Some(call_id) = string(body, "callId") else {
            return Vec::new();
        };
        // Only of a call this end asked: another's is acked and passed by.
        let asked = self.asking.iter().any(|call| call.call_id == call_id);
        let result = Delivery::CallResult {
            call_id: call_id.clone(),
            outcome: string(body, "outcome").unwrap_or_default(),
            result: body.get("result").cloned(),
            code: body.get("error").and_then(|error| string(error, "code")),
            blobs: body
                .get("blobs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|blob| blob.as_str().map(str::to_string))
                .collect(),
            here: false,
        };
        let out = self.reliable_in(envelope, Value::Null, now_ms, |_, _| result);
        if !asked {
            return out
                .into_iter()
                .filter(|o| !matches!(o, Output::Deliver(_)))
                .collect();
        }
        self.asking.retain(|call| call.call_id != call_id);
        self.cancels.retain(|id| *id != call_id);
        // It is answered: its ask, if it still waited to go again, does not.
        self.outbox
            .retain(|pending| !(pending.t == "call.ask" && pending.id == call_id));
        out
    }

    /// Every call this end asked that has had no answer a minute past its deadline: told
    /// timed out here, once. It stays asked, so that a result that still comes is delivered.
    fn overdue(&mut self, now_ms: i64) -> Vec<Output> {
        let host_now = self.host_now(now_ms);
        let mut out = Vec::new();
        for call in &mut self.asking {
            if !call.late && host_now >= call.deadline_at.saturating_add(Self::CALL_GRACE_MS) {
                call.late = true;
                out.push(Output::Deliver(Delivery::CallResult {
                    call_id: call.call_id.clone(),
                    outcome: "failed".into(),
                    result: None,
                    code: Some("call_timeout".into()),
                    blobs: Vec::new(),
                    here: true,
                }));
            }
        }
        out
    }

    // Streams

    /// The name, version and time of a `stream.state` or `stream.event` body, when both
    /// ends listed `streams` and the contract registers that name at that version for her
    /// host to send; an end passes any other by.
    fn stream(&self, event: bool, body: &Value) -> Option<(String, u64, i64)> {
        if !self.both("streams") {
            return None;
        }
        let name = string(body, "name")?;
        let version = body.get("version").and_then(unsigned)?;
        let spec = myriad_her_link::stream_spec(event, &name)?;
        if spec.version != version || spec.from == "child" {
            return None;
        }
        let at = body.get("at").and_then(unsigned).map_or(0, |at| at as i64);
        Some((name, version, at))
    }

    fn stream_event_in(
        &mut self,
        envelope: &Envelope,
        body: &Value,
        taken: bool,
        now_ms: i64,
    ) -> Vec<Output> {
        let Some(id) = envelope.id.clone().filter(|id| valid_id(id)) else {
            return Vec::new();
        };
        let known = if taken { self.stream(true, body) } else { None };
        let Some((name, version, at)) = known else {
            // Not one this end reads: acked like any reliable frame, and passed by.
            return self.send("ack", &json!({}), None, now_ms);
        };
        let delivery = Delivery::StreamEvent {
            id,
            name,
            version,
            at,
            body: body["body"].clone(),
        };
        self.reliable_in(envelope, Value::Null, now_ms, |_, _| delivery)
    }

    /// Tell her host the newest value of something that changes (`stream.state`): `name`
    /// and `version` as the contract's `streams.state` registers them, or a name of this
    /// end's own that her host may pass by. Only on an open link where both ends listed
    /// `streams`; not kept otherwise.
    pub fn stream_state(
        &mut self,
        name: &str,
        version: u64,
        value: &Value,
        now_ms: i64,
    ) -> Vec<Output> {
        if self.phase != Phase::Open || !self.both("streams") {
            return Vec::new();
        }
        let body = json!({ "name": name, "version": version, "at": now_ms.max(0), "value": value });
        self.send("stream.state", &body, None, now_ms)
    }

    /// Tell her host something that happened (`stream.event`), kept until it is acked: `id`
    /// is the event's own, made once by the shell. It goes only on a link where both ends
    /// listed `streams`, and waits for one otherwise.
    pub fn stream_event(
        &mut self,
        id: &str,
        name: &str,
        version: u64,
        body: &Value,
        now_ms: i64,
    ) -> Vec<Output> {
        // `link.dropped` is the session's own to tell.
        if name == "link.dropped" {
            return unsent("stream.event", "bad_frame", Some(id));
        }
        let frame = json!({ "name": name, "version": version, "at": now_ms.max(0), "body": body });
        if let Err(refused) =
            frames::text("stream.event", WHOLE, WHOLE, Some(id), WHOLE as i64, &frame)
        {
            return unsent("stream.event", &refused.code, Some(id));
        }
        // The same event again under its id is the same event and takes no room; nor does
        // one whose id is not free, which is only told.
        let fresh = valid_id(id)
            && !id.ends_with(RESULT)
            && !self.turns.contains_key(id)
            && !self.outbox.iter().any(|pending| pending.id == id);
        if !fresh {
            return self.reliable_out(id, "stream.event", &frame, now_ms);
        }
        let Some(mut out) = self.room_for_event() else {
            return unsent("stream.event", "too_many", Some(id));
        };
        out.extend(self.reliable_out(id, "stream.event", &frame, now_ms));
        out
    }

    /// Room for one more `stream.event` to be kept until acked: past
    /// [`Self::KEPT_EVENTS`], the oldest that waits to go is let go and told (`Unsent`
    /// `stream.event` `too_many`, with its id). None when every one kept is on its way on
    /// this link: then the new one is not kept.
    fn room_for_event(&mut self) -> Option<Vec<Output>> {
        let event = |pending: &Pending| pending.t == "stream.event" || pending.pass;
        if self.outbox.iter().filter(|pending| event(pending)).count() < Self::KEPT_EVENTS {
            return Some(Vec::new());
        }
        let oldest = self
            .outbox
            .iter()
            .position(|pending| event(pending) && pending.seq.is_none())?;
        let gone = self.outbox.remove(oldest);
        Some(unsent(&gone.t, "too_many", Some(&gone.id)))
    }

    // Clock

    /// Heartbeat and timeouts; call it often (several times a heartbeat).
    pub fn tick(&mut self, now_ms: i64) -> Vec<Output> {
        // A call's deadline passes whether or not the link is up: the call is stopped here,
        // and its answer waits for the link like any reliable frame.
        let mut out = self.expire(now_ms);
        out.extend(self.expire_held(now_ms));
        out.extend(self.overdue(now_ms));
        out.extend(self.beat(now_ms));
        out
    }

    fn beat(&mut self, now_ms: i64) -> Vec<Output> {
        match self.phase {
            Phase::Closed => Vec::new(),
            Phase::Greeting => {
                if now_ms.saturating_sub(self.opened_at_ms) >= self.timing.hello_timeout_ms {
                    vec![Output::Drop(Down::Silent)]
                } else {
                    Vec::new()
                }
            }
            Phase::Open => {
                if now_ms.saturating_sub(self.last_heard_ms)
                    >= self
                        .heartbeat_ms
                        .saturating_mul(self.timing.missed_heartbeats.max(1))
                {
                    vec![Output::Drop(Down::Silent)]
                } else if now_ms.saturating_sub(self.last_sent_ms) >= self.heartbeat_ms {
                    self.send("ping", &json!({}), None, now_ms)
                } else {
                    Vec::new()
                }
            }
        }
    }

    // Closing

    /// A close frame's code (and the name the host gives as its reason), per the contract.
    pub fn after_close(contract: &Contract, code: Option<u16>, reason: Option<&str>) -> After {
        let Some((name, rule)) = contract.close(code, reason) else {
            return After::Reconnect(Down::Network);
        };
        let down = match name {
            "revoked" => return After::Forget(Removal::Revoked),
            "owner_changed" => return After::Forget(Removal::OwnerChanged),
            "replaced" => Down::Replaced,
            "unsupported_protocol" | "version_unsupported" => Down::UnsupportedProtocol,
            "bad_frame" => Down::RefusedHandshake,
            "timeout" => Down::Silent,
            "unavailable" => Down::HostUnavailable,
            "host_moved" => Down::HostMoved(None),
            "host_frozen" => Down::HostFrozen,
            _ => Down::Network,
        };
        if rule.forget_credentials {
            After::Forget(Removal::Revoked)
        } else if rule.reconnect {
            After::Reconnect(down)
        } else {
            After::Stop(down)
        }
    }

    /// The upgrade answered with HTTP `status` instead of switching protocols; `refusal` is what
    /// the site's `{error, code}` said, when it could be read.
    pub fn after_http(status: u16, refusal: Option<&Refusal>) -> After {
        match status {
            300..=399 => After::Stop(Down::Redirected),
            401 => match refusal {
                Some(Refusal::Removed(why)) => After::Forget(*why),
                _ => After::Forget(Removal::TokenUnknown),
            },
            403 => match refusal {
                Some(Refusal::Removed(why)) => After::Forget(*why),
                Some(Refusal::NoOwner) => After::Reconnect(Down::NoOwner),
                Some(Refusal::HostFrozen) => After::Reconnect(Down::HostFrozen),
                Some(Refusal::HostMoved(name)) => After::Reconnect(Down::HostMoved(name.clone())),
                None => After::Reconnect(Down::Network),
            },
            500..=599 => After::Reconnect(Down::HostUnavailable),
            400 | 404 | 405 | 426 => After::Stop(Down::UnsupportedSite),
            _ => After::Reconnect(Down::Network),
        }
    }

    // Turns

    fn remember(&mut self, turn: Turn) {
        self.order.push(turn.turn_id.clone());
        self.turns.insert(turn.turn_id.clone(), turn);
        while self.order.len() > Self::KEPT_TURNS {
            let Some(oldest) = self
                .order
                .iter()
                .find(|id| self.turns.get(*id).is_none_or(|turn| turn.ended))
                .cloned()
            else {
                break;
            };
            self.order.retain(|id| *id != oldest);
            self.turns.remove(&oldest);
        }
    }

    // Wire

    fn flush(&mut self, now_ms: i64) -> Vec<Output> {
        let mut out = Vec::new();
        let mut refused = BTreeSet::new();
        for index in 0..self.outbox.len() {
            if self.outbox[index].seq.is_some() {
                continue;
            }
            // An event goes only on a link where both ends listed `streams`: on another it
            // waits for one that did.
            if self.outbox[index].t == "stream.event" && !self.both("streams") {
                continue;
            }
            // A call's frames go only on a link where both ends listed `calls`; and an ask
            // only once her host has said, on this link, that it provides the action.
            if self.outbox[index].t == "call.result" && !self.both("calls") {
                continue;
            }
            // What a call saw or heard goes only while it is granted on this link.
            if self.outbox[index].t == "call.result" && self.seen_waits(index) {
                continue;
            }
            if self.outbox[index].t == "call.ask" && !self.provided(&self.outbox[index].body) {
                continue;
            }
            let Pending { id, t, body, .. } = self.outbox[index].clone();
            let frame = self.send(&t, &body, Some(&id), now_ms);
            if frame.iter().any(|o| matches!(o, Output::Send(_))) {
                self.outbox[index].seq = Some(self.sent);
            } else {
                // An input that can never go is a turn that will never be answered: it ends
                // here, with the `Unsent` that tells the shell.
                if t == "input"
                    && let Some(turn) = self.turns.get_mut(&id)
                {
                    turn.ended = true;
                }
                refused.insert(id);
            }
            out.extend(frame);
        }
        // One the core will not send can never go: it leaves the outbox.
        self.outbox
            .retain(|pending| pending.seq.is_some() || !refused.contains(&pending.id));
        out
    }

    /// Whether the result waiting at `index` waits on: a seen action's (`calls.seen`) done
    /// result, before her host has said on this link what it grants. Once it has said, one
    /// it no longer grants is answered `call_not_granted` instead, here and to a call asked
    /// again, and what was seen never goes; one it grants goes. Every other result goes
    /// whatever is granted: it says what already happened.
    fn seen_waits(&mut self, index: usize) -> bool {
        let body = &self.outbox[index].body;
        let (Some(call_id), Some(action), Some(version)) = (
            string(body, "callId"),
            string(body, "action"),
            body.get("version").and_then(unsigned),
        ) else {
            return false;
        };
        if string(body, "outcome").as_deref() != Some("done")
            || !myriad_her_link::call_seen(&action)
        {
            return false;
        }
        if !self.heard_granted {
            return true;
        }
        if !self.granted.contains(&(action.clone(), version)) {
            let gone = failed(&call_id, &action, version, "call_not_granted");
            if let Some(kept) = self.answered.iter_mut().find(|a| a.call_id == call_id) {
                kept.result = gone.clone();
            }
            self.outbox[index].body = gone;
        }
        false
    }

    /// The next frame, numbered, as the shared core writes it; when the core refuses it, no number
    /// is taken and only why is told (`Unsent`).
    fn send(&mut self, t: &str, body: &Value, id: Option<&str>, now_ms: i64) -> Vec<Output> {
        let acks = self.acks();
        match frames::text(t, self.sent + 1, acks, id, now_ms, body) {
            Ok(text) => {
                self.sent += 1;
                self.acked = acks;
                self.last_sent_ms = now_ms;
                vec![Output::Send(text)]
            }
            Err(refused) => unsent(t, &refused.code, id),
        }
    }
}

/// A content id of the site's assets (rig packages, portraits): 64 lowercase hex characters,
/// as the Mac's rig cache takes (`HerLinkRigCache.validID`).
fn asset_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
