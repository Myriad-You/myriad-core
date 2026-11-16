//! Her link's frames (`myriad_her_link`): whether a frame may be sent or
//! taken under protocol 2, and its one byte form. The rules read the
//! contract the core was built with.

use myriad_her_link::check;
use serde::Deserialize;
use serde_json::{Value, json};

use super::input;
use crate::Failure;

/// `{frame, roles?}` → `{accepted: {t, family, delivery, canonical}}` or
/// `{refused: {code, detail}}`. A refusal is an answer, not bad input: the
/// frame came from the other end.
pub(super) fn check_frame(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        frame: Value,
        #[serde(default)]
        roles: Vec<String>,
    }
    let In { frame, roles } = input(raw)?;
    Ok(match check(&frame, &roles) {
        Ok(checked) => json!({ "accepted": checked }),
        Err(refusal) => json!({ "refused": refusal }),
    })
}

/// `{frame}` → `{text}`: the frame this end is about to send, checked, in
/// its one byte form. A frame that does not pass is bad input.
pub(super) fn encode(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        frame: Value,
    }
    let In { frame } = input(raw)?;
    let checked = check(&frame, &[])
        .map_err(|refusal| Failure::BadInput(format!("{}: {}", refusal.code, refusal.detail)))?;
    Ok(json!({ "text": checked.canonical }))
}

/// `{t, body, sessions2?, choices?}` → `{session}` (`now.session`), `{drop}`
/// (`now.drop`) or `{refused: {code, detail}}`: a `now` family body read by
/// the contract's `sessions` rules. `sessions2` says the welcome listed
/// `now.sessions2` (false when left out): without it a body with
/// `ownerSaidAt`, `errors` or `overwrote` is refused. `choices` says it
/// listed `now.choices`: without it a body with `choices` is refused.
/// Another `t` is bad input.
pub(super) fn now_read(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        t: String,
        body: Value,
        #[serde(default)]
        sessions2: bool,
        #[serde(default)]
        choices: bool,
    }
    let In {
        t,
        body,
        sessions2,
        choices,
    } = input(raw)?;
    let listed = myriad_her_link::now::Listed { sessions2, choices };
    let answer = match t.as_str() {
        "now.session" => myriad_her_link::now::read_listed(&body, listed)
            .map(|session| json!({ "session": session })),
        "now.drop" => myriad_her_link::now::read_drop(&body).map(|drop| json!({ "drop": drop })),
        _ => return Err(Failure::BadInput("t is not now.session or now.drop".into())),
    };
    Ok(answer.unwrap_or_else(|refusal| json!({ "refused": refusal })))
}

/// Milliseconds, as her link counts time, from the microseconds every call takes.
fn link_ms(now: i64) -> i64 {
    now / 1000
}

fn child(state: &Value) -> Result<myriad_link_child::Session, Failure> {
    myriad_link_child::Session::restore(myriad_link_child::contract(), state)
        .map_err(Failure::BadInput)
}

/// `{capabilities, app: {name, version}, lineage?, hostEpoch?}` → `{state}`: a child's
/// session of her link, not connected yet (`myriad_link_child::Session`). `capabilities` is
/// what this end can do; names the contract does not know are left out of its hello.
/// `lineage` and `hostEpoch` are what its last welcome said, when it kept them.
pub(super) fn child_new(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct App {
        name: String,
        version: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        capabilities: Vec<String>,
        app: App,
        #[serde(default)]
        lineage: Option<String>,
        #[serde(default)]
        host_epoch: Option<i64>,
    }
    let input: In = input(raw)?;
    if input.host_epoch.is_some_and(|epoch| epoch < 0) {
        return Err(Failure::BadInput("hostEpoch counts from 0".into()));
    }
    let capabilities: Vec<&str> = input.capabilities.iter().map(String::as_str).collect();
    let session = myriad_link_child::Session::new(
        myriad_link_child::contract(),
        &capabilities,
        (&input.app.name, &input.app.version),
        input.lineage.as_deref(),
        input.host_epoch,
    );
    Ok(json!({ "state": session.state() }))
}

/// `{state, now, event}` → `{state, outputs, phase}`: one thing happened to the session.
/// `now` is the time in microseconds. `event` is `"connected"` (the socket is up),
/// `"disconnected"`, `"tick"` (call it several times a heartbeat while the socket is up),
/// `{received: text}` (a text frame from her host), or something the shell does:
/// `{say: {turnId, text, sessionId?, rigState?, images?}}`, `{presence: body}`,
/// `{observePlayback: {runId, upcomingText, rig}}`, `{endPlayback: {runId}}`,
/// `{delivered: {id, body}}`, `{appraiseTouch: {touchId, touch, rigState?}}`,
/// `{touched: {touchId, touch}}`, `{wear: {outfitId}}`, `{setCapabilities: [..]}` (for the
/// next hello). `outputs` is what the shell does now, in order: `{send: text}`,
/// `{deliver: {kind, …}}`, `{drop: {why, …}}` (close the socket). `phase` is `closed`,
/// `greeting` or `open`. The state is the caller's to keep and hand back whole.
pub(super) fn child_step(raw: &[u8]) -> Result<Value, Failure> {
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
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Observe {
        run_id: String,
        upcoming_text: String,
        rig: Value,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Run {
        run_id: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Delivered {
        id: String,
        body: Value,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Touch {
        touch_id: String,
        touch: Value,
        #[serde(default)]
        rig_state: Option<Value>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Wear {
        outfit_id: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    enum Event {
        Connected,
        Disconnected,
        Tick,
        Received(String),
        Say(Say),
        Presence(Value),
        ObservePlayback(Observe),
        EndPlayback(Run),
        Delivered(Delivered),
        AppraiseTouch(Touch),
        Touched(Touch),
        Wear(Wear),
        SetCapabilities(Vec<String>),
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        state: Value,
        now: i64,
        event: Event,
    }
    // What the owner said and showed her is in the input: a failure names no value of it.
    let In { state, now, event } = super::quiet_input(raw)?;
    if now < 0 {
        return Err(Failure::BadInput(
            "now is microseconds since the epoch".into(),
        ));
    }
    let mut session = child(&state)?;
    let now = link_ms(now);
    let outputs = match event {
        Event::Connected => session.connected(now),
        Event::Disconnected => {
            session.disconnected();
            Vec::new()
        }
        Event::Tick => session.tick(now),
        Event::Received(text) => session.received(&text, now),
        Event::Say(say) => session.say(
            &say.turn_id,
            &say.text,
            say.session_id.as_deref(),
            say.rig_state.as_ref(),
            &say.images,
            now,
        ),
        Event::Presence(body) => session.presence(&body, now),
        Event::ObservePlayback(observe) => {
            session.observe_playback(&observe.run_id, &observe.upcoming_text, &observe.rig, now)
        }
        Event::EndPlayback(run) => session.end_playback(&run.run_id, now),
        Event::Delivered(delivered) => session.delivered(&delivered.id, &delivered.body, now),
        Event::AppraiseTouch(touch) => {
            session.appraise_touch(&touch.touch_id, &touch.touch, touch.rig_state.as_ref(), now)
        }
        Event::Touched(touch) => session.touched(&touch.touch_id, &touch.touch, now),
        Event::Wear(wear) => session.wear(&wear.outfit_id, now),
        Event::SetCapabilities(capabilities) => {
            let capabilities: Vec<&str> = capabilities.iter().map(String::as_str).collect();
            session.set_capabilities(&capabilities);
            Vec::new()
        }
    };
    Ok(json!({
        "state": session.state(),
        "outputs": outputs.iter().map(myriad_link_child::Output::json).collect::<Vec<_>>(),
        "phase": session.phase().name(),
    }))
}

/// `{close: {code?, reason?}}` or `{http: {status, body?}}` → `{then: "reconnect" | "stop",
/// down}` or `{then: "forget", removal}`: what a child does once its socket closed with
/// that code (and the name the host gave as its reason; `{}` when it gave neither), or once
/// the upgrade was refused with that HTTP status and JSON body (her host's
/// `{error, code, host?}`). With `attempt` (0 for the first retry) a `reconnect` also says
/// `backoffMs`, the wait before it.
pub(super) fn child_after(raw: &[u8]) -> Result<Value, Failure> {
    use myriad_link_child::{After, Refusal, Session, contract};
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Close {
        #[serde(default)]
        code: Option<u16>,
        #[serde(default)]
        reason: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Http {
        status: u16,
        #[serde(default)]
        body: Option<Value>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        #[serde(default)]
        close: Option<Close>,
        #[serde(default)]
        http: Option<Http>,
        #[serde(default)]
        attempt: Option<u32>,
    }
    let In {
        close,
        http,
        attempt,
    } = input(raw)?;
    let after = match (close, http) {
        (Some(close), None) => {
            Session::after_close(contract(), close.code, close.reason.as_deref())
        }
        (None, Some(http)) => {
            let refusal = http.body.as_ref().and_then(Refusal::read);
            Session::after_http(http.status, refusal.as_ref())
        }
        _ => return Err(Failure::BadInput("exactly one of close and http".into())),
    };
    let mut answer = after.json();
    if let (After::Reconnect(_), Some(attempt)) = (&after, attempt) {
        answer["backoffMs"] = json!(contract().timing.backoff_ms(attempt));
    }
    Ok(answer)
}

/// `{capabilities, reads}` → `{unread: [{capability, t}], idle, sends, takes}`: what an end
/// that says `capabilities` in hello claims. `reads` is every host frame type its own code
/// handles. `unread` is each frame her host may then send that it does not read (under the
/// capability that brings it, `""` for one every child gets): an end tests that this is
/// empty. `idle` is what it says to no effect; `sends` and `takes` are every frame type it
/// then sends and may be sent.
pub(super) fn child_claims(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        capabilities: Vec<String>,
        reads: Vec<String>,
    }
    let In {
        capabilities,
        reads,
    } = input(raw)?;
    let said: Vec<&str> = capabilities.iter().map(String::as_str).collect();
    let unread: Vec<Value> =
        myriad_link_child::capabilities::unread(&said, |t| reads.iter().any(|read| read == t))
            .into_iter()
            .map(|(capability, t)| json!({ "capability": capability, "t": t }))
            .collect();
    Ok(json!({
        "unread": unread,
        "idle": myriad_link_child::capabilities::idle(&said),
        "sends": myriad_link_child::capabilities::sends(&said),
        "takes": myriad_link_child::capabilities::takes(&said),
    }))
}

/// `{host?, event}` → `{host, text?, fault?}`: the stand-in host of her link's conformance
/// scripts (`crates/conformance/v1/child`), one move at a time, for an end that plays them
/// to its own child. `host` is its state, kept by the caller (left out at first). `event`
/// is `"connected"` (a socket came up: numbering starts again), `{frame: {spec, now}}`
/// (a script's `host` step, `now` in the script's milliseconds: `text` is the frame to hand
/// the child) or `{takes: {expected, text}}` (the child sent `text` where a `child` step
/// expects `expected`). `fault` says why the move does not hold; the state is then as before.
/// For tests: a fault quotes the frame it is about, which no call a product makes does.
pub(super) fn conformance_host(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Frame {
        spec: Value,
        now: i64,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Takes {
        expected: Value,
        text: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    enum Event {
        Connected,
        Frame(Frame),
        Takes(Takes),
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        #[serde(default)]
        host: myriad_link_conformance::Host,
        event: Event,
    }
    let In { host, event } = input(raw)?;
    if !host.sound() {
        return Err(Failure::BadInput("not a stand-in host's state".into()));
    }
    let mut moved = host.clone();
    let done = match event {
        Event::Connected => {
            moved.connected();
            Ok(None)
        }
        Event::Frame(frame) => moved.frame(&frame.spec, frame.now).map(Some),
        Event::Takes(takes) => moved.takes(&takes.expected, &takes.text).map(|()| None),
    };
    Ok(match done {
        Ok(Some(text)) => json!({ "host": moved, "text": text }),
        Ok(None) => json!({ "host": moved }),
        Err(fault) => json!({ "host": host, "fault": fault }),
    })
}
