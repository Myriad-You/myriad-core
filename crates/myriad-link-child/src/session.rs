//! The link's protocol as a value: numbering, acks, the reliable outbox,
//! which turns to resume, which run events and directives are new, the
//! heartbeat and what a close means. It began as the Android XR end's port of
//! the Mac's `HerLinkSession` and came here whole, so every end keeps one
//! copy; what only a computer does there (Work, experiences, moments, nodes,
//! proposals, acts, her host lease's moves) it does not read yet.

use std::collections::{BTreeMap, BTreeSet};

use myriad_her_link::valid_id;
use serde_json::{Map, Value, json};

use crate::contract::{Contract, Timing};
use crate::frames;

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
    /// A frame of this end's the shared core would not send: its type and why, never its body.
    /// A reliable one has left the outbox.
    Unsent {
        t: String,
        code: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub turn_id: String,
    pub run_id: Option<String>,
    pub session_id: Option<String>,
    /// The highest run sequence taken; 0 before any.
    pub last_sequence: u64,
    saw_start: bool,
    /// The highest sequence a directive of this turn was played at.
    pub performed: Option<u64>,
    /// The highest version of her director's refinement played while the reply is said; once
    /// there is one, the run's own directives no longer replace it.
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
            performed: None,
            live: None,
            answered: false,
            ended: false,
        }
    }
}

#[derive(Debug, Clone)]
struct Pending {
    id: String,
    t: String,
    body: Value,
    seq: Option<u64>,
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

/// A whole number JSON can carry exactly.
fn unsigned(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        let n = value.as_f64()?;
        (n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n <= 9_007_199_254_740_992.0)
            .then_some(n as u64)
    })
}

fn string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

pub struct Session {
    timing: Timing,
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
}

impl Session {
    pub const KEPT_TURNS: usize = 64;
    pub const KEPT_TAKEN: usize = 256;

    /// The host's frame types this session reads; any other is passed by
    /// (`crate::capabilities::unread` compares an end's capabilities with it). A `pong` is
    /// read only as a sign of life.
    pub const READS: &[&str] = &[
        "conversation",
        "conversations",
        "error",
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
        hello.insert("lineage".into(), json!(lineage));
        hello.insert("hostEpoch".into(), json!(host_epoch));
        Self {
            timing: contract.timing,
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
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn host_capabilities(&self) -> &BTreeSet<String> {
        &self.host_capabilities
    }

    /// Whether the host takes what the owner shows her (`images`) with what they say.
    pub fn shows(&self) -> bool {
        self.host_capabilities.contains("images")
    }

    /// What this end says it can do, as its hello says it.
    pub fn capabilities(&self) -> Vec<String> {
        self.hello["capabilities"]
            .as_array()
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
        for pending in &mut self.outbox {
            pending.seq = None;
        }
        let hello = Value::Object(self.hello.clone());
        self.send("hello", &hello, None, now_ms)
    }

    /// The socket is gone; reliable frames wait for the next connection.
    pub fn disconnected(&mut self) {
        self.phase = Phase::Closed;
        for pending in &mut self.outbox {
            pending.seq = None;
        }
    }

    // From the child

    /// Something the owner said here: a reliable `input` whose id is the turn. Sent now when
    /// the link is open, otherwise with the next welcome. `images` is what they showed her with
    /// it (data URLs), sent only to a host that accepted `images` ([`Session::shows`]).
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
        if !valid_id(turn_id) || self.turns.contains_key(turn_id) {
            return Vec::new();
        }
        let mut body = json!({ "surface": "app", "text": text });
        if let Some(session_id) = session_id {
            body["sessionId"] = json!(session_id);
        }
        if !images.is_empty() && self.shows() {
            body["images"] = json!(images);
        }
        if let Some(rig_state) = rig_state.filter(|state| state.is_object()) {
            body["rigState"] = rig_state.clone();
        }
        let mut turn = Turn::new(turn_id);
        turn.session_id = session_id.map(str::to_string);
        self.remember(turn);
        self.current_turn = Some(turn_id.to_string());
        self.outbox.push(Pending {
            id: turn_id.to_string(),
            t: "input".into(),
            body,
            seq: None,
        });
        if self.phase == Phase::Open {
            self.flush(now_ms)
        } else {
            Vec::new()
        }
    }

    /// Whether the owner is with her here; dropped while the link is down (a latest value).
    pub fn presence(&mut self, body: &Value, now_ms: i64) -> Vec<Output> {
        if self.phase != Phase::Open || !body.get("presence").is_some_and(Value::is_object) {
            return Vec::new();
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
        if self.phase != Phase::Open || run_id.is_empty() || !rig.is_object() {
            return Vec::new();
        }
        let body = json!({ "runId": run_id, "upcomingText": upcoming_text, "rig": rig });
        self.send("playback.observe", &body, None, now_ms)
    }

    /// The run's reply has been said, or stopped being said (`playback.end`, a request).
    pub fn end_playback(&mut self, run_id: &str, now_ms: i64) -> Vec<Output> {
        if self.phase != Phase::Open || run_id.is_empty() {
            return Vec::new();
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
        if self.phase != Phase::Open || !valid_id(touch_id) || !touch.is_object() {
            return Vec::new();
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
            return Vec::new();
        }
        self.reliable_out(touch_id, "touch", &json!({ "touch": touch }), now_ms)
    }

    /// The owner put the set `outfit_id` on her here (`outfit.wear`, a request).
    pub fn wear(&mut self, outfit_id: &str, now_ms: i64) -> Vec<Output> {
        if self.phase != Phase::Open || outfit_id.is_empty() {
            return Vec::new();
        }
        self.send(
            "outfit.wear",
            &json!({ "outfitId": outfit_id }),
            None,
            now_ms,
        )
    }

    fn reliable_out(&mut self, id: &str, t: &str, body: &Value, now_ms: i64) -> Vec<Output> {
        if !valid_id(id) || self.outbox.iter().any(|pending| pending.id == id) {
            return Vec::new();
        }
        self.outbox.push(Pending {
            id: id.to_string(),
            t: t.to_string(),
            body: body.clone(),
            seq: None,
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
                Phase::Open => vec![unread],
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
        out
    }

    fn read(&mut self, envelope: &Envelope, taken: bool, now_ms: i64) -> Vec<Output> {
        self.last_heard_ms = now_ms;
        self.heard = self.heard.max(envelope.seq);
        self.outbox
            .retain(|pending| !pending.seq.is_some_and(|seq| seq <= envelope.ack));
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
            // One the core refuses is acked, so her host stops sending it again, but it is not
            // taken: its id stays free for a line this end can read.
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
            _ => Vec::new(),
        }
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
        let mut out = vec![Output::Deliver(Delivery::Welcome {
            device_id,
            host: string(body, "host").unwrap_or_else(|| "site".into()),
            lineage,
            host_epoch,
        })];
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

    // Clock

    /// Heartbeat and timeouts; call it often (several times a heartbeat).
    pub fn tick(&mut self, now_ms: i64) -> Vec<Output> {
        match self.phase {
            Phase::Closed => Vec::new(),
            Phase::Greeting => {
                if now_ms - self.opened_at_ms >= self.timing.hello_timeout_ms {
                    vec![Output::Drop(Down::Silent)]
                } else {
                    Vec::new()
                }
            }
            Phase::Open => {
                if now_ms - self.last_heard_ms
                    >= self.heartbeat_ms * self.timing.missed_heartbeats.max(1)
                {
                    vec![Output::Drop(Down::Silent)]
                } else if now_ms - self.last_sent_ms >= self.heartbeat_ms {
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

    /// The next frame, numbered, as the shared core writes it; when the core refuses it, no number
    /// is taken and only why is told (`Unsent`).
    fn send(&mut self, t: &str, body: &Value, id: Option<&str>, now_ms: i64) -> Vec<Output> {
        match frames::text(t, self.sent + 1, self.heard, id, now_ms, body) {
            Ok(text) => {
                self.sent += 1;
                self.last_sent_ms = now_ms;
                vec![Output::Send(text)]
            }
            Err(unsent) => vec![Output::Deliver(Delivery::Unsent {
                t: t.to_string(),
                code: unsent.code,
            })],
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
