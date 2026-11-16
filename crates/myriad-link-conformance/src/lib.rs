//! Her link's conformance scripts (`crates/conformance/v1/child`), played in
//! memory.
//!
//! A script is one conversation over the link seen from outside: what the
//! child's shell does (`do`), each frame the child must send (`child`), each
//! frame her host sends (`host`, or `raw` for a text that is no frame), and
//! what the child's shell is handed (`deliver`, `drop`).
//!
//! - A child end tests itself with [`play`]: it implements [`ChildEnd`] over
//!   its own link code and every script is played to it by [`Host`], the
//!   stand-in host, which numbers and writes the host's frames and holds each
//!   of the child's to the script, the contract and the link's numbering.
//!   [`run`] is [`play`] for [`Child`], the child core itself; this
//!   repository's tests do that for every script.
//! - A host tests itself with [`Child`], the stand-in child, in scenarios of
//!   its own: what a real host answers (its ids, how many events a reply
//!   takes) is its own, so the scripts are not replayed to it. Every frame it
//!   sends goes through [`Child::hears`], which holds it to the contract and
//!   the numbering before the child core reads it.
//!
//! Neither stand-in is her: the host answers only what the script says.
//!
//! Pure: no socket, no clock. Time is the script's (`at`, milliseconds from
//! [`T0`]).

use std::collections::{BTreeMap, VecDeque};

use myriad_link_child::{Output, Refusal, Session, contract};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// The scripts' time zero, milliseconds since the Unix epoch.
pub const T0: i64 = 1_760_140_800_000;

/// The script version this crate plays.
pub const VERSION: u64 = 1;

/// One script file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub version: u64,
    pub name: String,
    /// What it shows, for a reader.
    pub about: String,
    /// What the child says it can do in hello.
    pub capabilities: Vec<String>,
    pub steps: Vec<Step>,
}

/// One step. Exactly one of `do`, `child`, `host`, `raw`, `deliver`, `drop`, `close`, `http`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Milliseconds from [`T0`]; left out, the step before's.
    #[serde(default)]
    pub at: Option<i64>,
    /// The child's shell does something: `"connect"`, `"disconnect"`, `"tick"`,
    /// `{say: {turnId, text, sessionId?, rigState?, images?}}` or `{delivered: {id, body}}`.
    #[serde(default, rename = "do")]
    pub act: Option<Value>,
    /// The child sends this frame: `{t, id?, seq?, ack?, body?}`, each given part matched.
    #[serde(default)]
    pub child: Option<Value>,
    /// Her host sends this frame: `{t, id?, body}`. It must be one the contract takes from a
    /// host, unless it says `unchecked: true`: a frame made for the child not to read, by its
    /// body or by `envelope`, which overrides envelope fields (`{"v": 2}`; null takes one out).
    #[serde(default)]
    pub host: Option<Value>,
    /// Her host sends this text, which is no frame.
    #[serde(default)]
    pub raw: Option<String>,
    /// The child's shell is handed this delivery (`Delivery::json`), each given field matched.
    #[serde(default)]
    pub deliver: Option<Value>,
    /// The child's shell is told to drop the link (`Down::json`), each given field matched.
    #[serde(default)]
    pub drop: Option<Value>,
    /// The socket closed with `{code?, reason?}`; with `after`.
    #[serde(default)]
    pub close: Option<Value>,
    /// The upgrade was refused with `{status, body?}`, the HTTP status and the JSON body
    /// her host answers with (contract `auth`: `{error, code, host?}`); with `after`.
    #[serde(default)]
    pub http: Option<Value>,
    /// What the child does after a `close` or `http` (`After::json`), matched whole.
    #[serde(default)]
    pub after: Option<Value>,
}

/// Why a script did not play: the step (from 0) and what was wrong there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    pub step: usize,
    pub why: String,
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "step {}: {}", self.step, self.why)
    }
}

/// Values a script names (`"$runId"`): bound by the first frame that carries one, the same
/// from then on. A script played against a real end learns the ids that end makes this way.
pub type Vars = BTreeMap<String, Value>;

/// In an expected object, the value of a key that must not be there. It is for frames: a
/// delivery leaves no field out (one with nothing to say is null).
pub const ABSENT: &str = "$absent";

/// Whether `actual` is what `expected` asks: an object's given keys match (others are free;
/// one given as [`ABSENT`] must not be there), a list matches item by item, `"$name"` binds
/// or equals the value of that name, anything else is equal.
pub fn matches(expected: &Value, actual: &Value, vars: &mut Vars) -> bool {
    match (expected, actual) {
        (Value::String(name), _) if name.starts_with('$') && name != ABSENT => match vars.get(name)
        {
            Some(bound) => bound == actual,
            None => {
                vars.insert(name.clone(), actual.clone());
                true
            }
        },
        (Value::Object(expected), Value::Object(actual)) => {
            expected.iter().all(|(key, want)| match actual.get(key) {
                _ if want == ABSENT => !actual.contains_key(key),
                Some(have) => matches(want, have, vars),
                None => false,
            })
        }
        (Value::Array(expected), Value::Array(actual)) => {
            expected.len() == actual.len()
                && expected
                    .iter()
                    .zip(actual)
                    .all(|(want, have)| matches(want, have, vars))
        }
        _ => expected == actual,
    }
}

/// `value` with every `"$name"` replaced by what it is bound to; the name of one that is not.
pub fn filled(value: &Value, vars: &Vars) -> Result<Value, String> {
    Ok(match value {
        Value::String(name) if name.starts_with('$') => {
            vars.get(name).cloned().ok_or_else(|| name.clone())?
        }
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| Ok((key.clone(), filled(value, vars)?)))
                .collect::<Result<Map<_, _>, String>>()?,
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| filled(item, vars))
                .collect::<Result<_, _>>()?,
        ),
        other => other.clone(),
    })
}

/// Who sends frame type `t`, by the contract; none for a type it does not name.
fn sender(t: &str) -> Option<&'static str> {
    contract().frames.get(t).map(|rule| rule.from.as_str())
}

/// The stand-in host's side of one link: its numbering and what the script has named.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Host {
    sent: u64,
    heard: u64,
    acked: u64,
    pub vars: Vars,
}

impl Host {
    /// A socket came up: numbering starts again. What the script named stays.
    pub fn connected(&mut self) {
        self.sent = 0;
        self.heard = 0;
        self.acked = 0;
    }

    /// The text of the script's `host` frame `spec`, numbered, at `now_ms`. Unless the step
    /// says `unchecked`, it is a frame the contract takes and a host sends, in its one byte
    /// form: a script cannot send a broken frame by accident.
    pub fn frame(&mut self, spec: &Value, now_ms: i64) -> Result<String, String> {
        let spec = filled(spec, &self.vars).map_err(|name| format!("{name} is not bound yet"))?;
        let t = spec["t"].as_str().ok_or("a host frame needs t")?;
        let unchecked = match spec.get("unchecked") {
            None => false,
            Some(Value::Bool(unchecked)) => *unchecked,
            Some(_) => return Err("unchecked is true or false".into()),
        };
        let mut frame = Map::new();
        frame.insert("v".into(), json!(contract().encoding.frame_version));
        frame.insert("t".into(), json!(t));
        frame.insert("seq".into(), json!(self.sent + 1));
        frame.insert("ack".into(), json!(self.heard));
        frame.insert("at".into(), json!(now_ms));
        frame.insert(
            "body".into(),
            spec.get("body").cloned().unwrap_or(json!({})),
        );
        if let Some(id) = spec.get("id") {
            frame.insert("id".into(), id.clone());
        }
        match spec.get("envelope") {
            None => {}
            Some(Value::Object(fields)) if unchecked => {
                for (key, value) in fields {
                    if value.is_null() {
                        frame.remove(key);
                    } else {
                        frame.insert(key.clone(), value.clone());
                    }
                }
            }
            Some(Value::Object(_)) => return Err("envelope goes with unchecked: true".into()),
            Some(_) => return Err("envelope is an object of envelope fields".into()),
        }
        let frame = Value::Object(frame);
        let text = if unchecked {
            if myriad_her_link::check(&frame, &[]).is_ok() {
                return Err(format!(
                    "the script's {t} says unchecked and is a good frame"
                ));
            }
            frame.to_string()
        } else {
            if !matches!(sender(t), Some("host" | "server" | "either")) {
                return Err(format!("the script's {t} is not a frame a host sends"));
            }
            myriad_her_link::check(&frame, &[])
                .map_err(|refusal| format!("the script's {t} is no frame: {}", refusal.detail))?
                .canonical
        };
        self.sent += 1;
        Ok(text)
    }

    /// The child sent `text` where the script's `child` step expects `expected`: it must be a
    /// frame the contract takes and a child sends, numbered one after its last, acking no
    /// less than before and no more than the host sent, and what the step asks.
    pub fn takes(&mut self, expected: &Value, text: &str) -> Result<(), String> {
        let checked = myriad_her_link::check_bytes(text.as_bytes(), &[]).map_err(|refusal| {
            format!(
                "the child sent what the contract refuses ({}): {}",
                refusal.code, refusal.detail
            )
        })?;
        if text != checked.canonical {
            return Err(format!(
                "the child's {} is not in its one byte form (canonical JSON)",
                checked.t
            ));
        }
        let frame: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if !matches!(sender(&checked.t), Some("child" | "device" | "either")) {
            return Err(format!("{} is not a frame a child sends", checked.t));
        }
        let (seq, ack) = (
            frame["seq"].as_u64().unwrap_or(0),
            frame["ack"].as_u64().unwrap_or(0),
        );
        if seq != self.heard + 1 {
            return Err(format!(
                "the child's seq is {seq}, one after {} was due",
                self.heard
            ));
        }
        if ack > self.sent || ack < self.acked {
            return Err(format!(
                "the child acks {ack}; it acked {} before and the host has sent {}",
                self.acked, self.sent
            ));
        }
        // Nothing of the host moves unless the frame is what the step asks.
        let mut vars = self.vars.clone();
        if !matches(expected, &frame, &mut vars) {
            return Err(format!(
                "the child sent {frame}, the script expects {expected}"
            ));
        }
        self.vars = vars;
        self.heard = seq;
        self.acked = ack;
        Ok(())
    }
}

/// A child end under test: its own link code, driven as a script says. [`Child`] is one.
pub trait ChildEnd {
    /// A `do` step: the shell does this at `now_ms`.
    fn act(&mut self, what: &Value, now_ms: i64) -> Result<(), String>;
    /// A text from her host arrived at `now_ms`.
    fn received(&mut self, text: &str, now_ms: i64);
    /// What the end did next that no step has taken yet, in the order it did it, as
    /// `Output::json`: `{send: text}` for a frame it sent, `{deliver: {kind, …}}` for what its
    /// shell was handed, `{drop: {why, …}}` when it dropped the link. None when there is no more.
    fn next(&mut self) -> Option<Value>;
}

/// The stand-in child: the child core doing what a script's `do` steps say, and what it gave
/// back that no step has taken yet, in order.
pub struct Child {
    session: Session,
    pub out: VecDeque<Output>,
    /// Frames this child has sent and the host's numbering on this connection, for [`Child::hears`].
    sent: u64,
    heard: u64,
    acked: u64,
}

impl Child {
    pub fn new(capabilities: &[String]) -> Self {
        let capabilities: Vec<&str> = capabilities.iter().map(String::as_str).collect();
        Self {
            session: Session::new(contract(), &capabilities, ("Child", "1"), None, None),
            out: VecDeque::new(),
            sent: 0,
            heard: 0,
            acked: 0,
        }
    }

    fn took(&mut self, out: Vec<Output>) {
        self.sent += out
            .iter()
            .filter(|output| matches!(output, Output::Send(_)))
            .count() as u64;
        self.out.extend(out);
    }

    /// A text from a host under test: before the child core reads it, it must be a frame the
    /// contract takes and a host sends, numbered one after the host's last on this
    /// connection, acking no less than before and no more than this child sent.
    pub fn hears(&mut self, text: &str, now_ms: i64) -> Result<(), String> {
        let checked = myriad_her_link::check_bytes(text.as_bytes(), &[]).map_err(|refusal| {
            format!(
                "the host sent what the contract refuses ({}): {}",
                refusal.code, refusal.detail
            )
        })?;
        if !matches!(sender(&checked.t), Some("host" | "server" | "either")) {
            return Err(format!("{} is not a frame a host sends", checked.t));
        }
        let frame: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
        let (seq, ack) = (
            frame["seq"].as_u64().unwrap_or(0),
            frame["ack"].as_u64().unwrap_or(0),
        );
        if seq != self.heard + 1 {
            return Err(format!(
                "the host's seq is {seq}, one after {} was due",
                self.heard
            ));
        }
        if ack > self.sent || ack < self.acked {
            return Err(format!(
                "the host acks {ack}; it acked {} before and the child has sent {}",
                self.acked, self.sent
            ));
        }
        self.heard = seq;
        self.acked = ack;
        self.received(text, now_ms);
        Ok(())
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// A `do` step.
    pub fn act(&mut self, what: &Value, now_ms: i64) -> Result<(), String> {
        let out = match what {
            Value::String(name) => match name.as_str() {
                "connect" => {
                    (self.sent, self.heard, self.acked) = (0, 0, 0);
                    self.session.connected(now_ms)
                }
                "disconnect" => {
                    self.session.disconnected();
                    Vec::new()
                }
                "tick" => self.session.tick(now_ms),
                other => return Err(format!("no such thing to do: {other}")),
            },
            Value::Object(object) if object.len() == 1 => {
                let (name, with) = object.iter().next().expect("one key");
                let text = |key: &str| with.get(key).and_then(Value::as_str);
                match name.as_str() {
                    "say" => {
                        #[derive(Deserialize)]
                        #[serde(rename_all = "camelCase", deny_unknown_fields)]
                        struct Say {
                            turn_id: String,
                            text: String,
                            #[serde(default)]
                            session_id: Option<String>,
                            #[serde(default)]
                            rig_state: Option<Value>,
                            #[serde(default)]
                            images: Vec<String>,
                        }
                        let say = Say::deserialize(with).map_err(|error| error.to_string())?;
                        self.session.say(
                            &say.turn_id,
                            &say.text,
                            say.session_id.as_deref(),
                            say.rig_state.as_ref(),
                            &say.images,
                            now_ms,
                        )
                    }
                    "delivered" => self.session.delivered(
                        text("id").ok_or("delivered needs id")?,
                        with.get("body").ok_or("delivered needs body")?,
                        now_ms,
                    ),
                    other => return Err(format!("no such thing to do: {other}")),
                }
            }
            other => return Err(format!("no such thing to do: {other}")),
        };
        self.took(out);
        Ok(())
    }

    /// A text from her host, read as it is.
    pub fn received(&mut self, text: &str, now_ms: i64) {
        let out = self.session.received(text, now_ms);
        self.took(out);
    }
}

impl ChildEnd for Child {
    fn act(&mut self, what: &Value, now_ms: i64) -> Result<(), String> {
        Child::act(self, what, now_ms)
    }

    fn received(&mut self, text: &str, now_ms: i64) {
        Child::received(self, text, now_ms);
    }

    fn next(&mut self) -> Option<Value> {
        self.out.pop_front().map(|output| output.json())
    }
}

/// What the child does after the socket closed with `close` (`{code?, reason?}`).
pub fn after_close(close: &Value) -> Value {
    let code = close
        .get("code")
        .and_then(Value::as_u64)
        .and_then(|code| u16::try_from(code).ok());
    let reason = close.get("reason").and_then(Value::as_str);
    Session::after_close(contract(), code, reason).json()
}

/// What the child does after the upgrade was refused with `http` (`{status, body?}`).
pub fn after_http(http: &Value) -> Result<Value, String> {
    let status = http
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|status| u16::try_from(status).ok())
        .ok_or("http needs status")?;
    let refusal = http.get("body").and_then(Refusal::read);
    Ok(Session::after_http(status, refusal.as_ref()).json())
}

/// Plays `script` to the stand-in child, the child core itself.
pub fn run(script: &Script) -> Result<(), Fault> {
    play(script, &mut Child::new(&script.capabilities))
}

/// Plays `script` to `child` against the stand-in host, in memory. The caller makes `child`
/// an end that says the script's `capabilities` in its hello. Every frame the child
/// sends and everything its shell is handed must be a step, in order: nothing may be waiting
/// when the script moves on to the next thing that happens, and nothing left over at the end.
/// `close` and `http` steps ask the child core what it would do, not `child`: an end checks
/// its own handling of them with [`after_close`] and [`after_http`].
pub fn play(script: &Script, child: &mut impl ChildEnd) -> Result<(), Fault> {
    if script.version != VERSION {
        return Err(Fault {
            step: 0,
            why: format!("script version {}, this plays {VERSION}", script.version),
        });
    }
    let mut host = Host::default();
    let mut waiting: VecDeque<Value> = VecDeque::new();
    let mut now = T0;
    for (index, step) in script.steps.iter().enumerate() {
        let fault = |why: String| Fault { step: index, why };
        if let Some(at) = step.at {
            if T0 + at < now {
                return Err(fault("at goes back in time".into()));
            }
            now = T0 + at;
        }
        let kinds = [
            step.act.is_some(),
            step.child.is_some(),
            step.host.is_some(),
            step.raw.is_some(),
            step.deliver.is_some(),
            step.drop.is_some(),
            step.close.is_some(),
            step.http.is_some(),
        ];
        if kinds.iter().filter(|kind| **kind).count() != 1 {
            return Err(fault("a step is exactly one thing".into()));
        }
        if step.after.is_some() != (step.close.is_some() || step.http.is_some()) {
            return Err(fault(
                "after goes with close or http, and only with them".into(),
            ));
        }
        let happens = step.act.is_some() || step.host.is_some() || step.raw.is_some();
        if let (true, Some(left)) = (happens, waiting.front()) {
            return Err(fault(format!(
                "before this, the child did what no step expects: {left}"
            )));
        }
        if let Some(what) = &step.act {
            if what == "connect" {
                host.connected();
            }
            child.act(what, now).map_err(fault)?;
        } else if let Some(spec) = &step.host {
            let text = host.frame(spec, now).map_err(fault)?;
            child.received(&text, now);
        } else if let Some(text) = &step.raw {
            child.received(text, now);
        } else if let Some(expected) = &step.child {
            match waiting.pop_front() {
                Some(output) if output["send"].is_string() => host
                    .takes(expected, output["send"].as_str().unwrap_or_default())
                    .map_err(fault)?,
                other => return Err(fault(unexpected("the child to send", expected, other))),
            }
        } else if let Some(expected) = &step.deliver {
            match waiting.pop_front() {
                Some(output)
                    if output
                        .get("deliver")
                        .is_some_and(|delivery| matches(expected, delivery, &mut host.vars)) => {}
                other => return Err(fault(unexpected("the delivery", expected, other))),
            }
        } else if let Some(expected) = &step.drop {
            match waiting.pop_front() {
                Some(output)
                    if output
                        .get("drop")
                        .is_some_and(|down| matches(expected, down, &mut host.vars)) => {}
                other => return Err(fault(unexpected("the link dropped with", expected, other))),
            }
        } else {
            let after = match (&step.close, &step.http) {
                (Some(close), _) => after_close(close),
                (_, Some(http)) => after_http(http).map_err(fault)?,
                _ => unreachable!("one of the eight, checked above"),
            };
            if Some(&after) != step.after.as_ref() {
                return Err(fault(format!("the child would do {after}")));
            }
        }
        if happens {
            while let Some(output) = child.next() {
                waiting.push_back(output);
            }
        }
    }
    match waiting.front() {
        None => Ok(()),
        Some(left) => Err(Fault {
            step: script.steps.len(),
            why: format!("left over from the child: {left}"),
        }),
    }
}

fn unexpected(what: &str, expected: &Value, got: Option<Value>) -> String {
    let got = got.map_or_else(|| "nothing".to_string(), |output| output.to_string());
    format!("the script expects {what} {expected}; next from the child is {got}")
}

#[cfg(test)]
mod tests;
