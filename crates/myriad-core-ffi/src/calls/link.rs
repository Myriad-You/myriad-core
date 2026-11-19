//! Her link's frames (`myriad_her_link`): whether a frame may be sent or
//! taken under protocol 2, and its one byte form. The rules read the
//! contract the core was built with.

use myriad_her_link::check;
use myriad_link_child::whole;
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

/// `{action, version, for, deadlineAt, takenAt, offers, refused?, granted}` →
/// `{verdict, code, version, deadlineAt}`: what the receiver of a `call.ask` does with it
/// (`myriad_her_link::call_receive`), the one place it is decided, for an end that keeps
/// its own link and for the child core's session alike.
///
/// `for` is the ask's (`{kind, groupId?}`). `deadlineAt` is the ask's and `takenAt` when
/// this end took it, both on the host's clock, milliseconds. `offers` is what this end
/// can do, `[{name, version}]`; `refused` what it can do but its own grants do not allow
/// now (none when left out); `granted` what the sender said is granted on this link
/// (`call.granted.actions`), null before it has said.
///
/// `verdict` is `hold` (keep the ask, neither do nor answer it, until `call.granted` has
/// come on this link, then ask again), `do`, or `fail`, with `code` one of the contract's
/// `calls.failCodes` (null otherwise). `version` is the version the answer says: the
/// ask's, or this end's own when the code is `version_unsupported`. `deadlineAt` is the
/// deadline the ask is held to, cut short to the most its action may be given.
pub(super) fn call_receive(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Named {
        name: String,
        #[serde(deserialize_with = "whole::u64")]
        version: u64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        action: String,
        #[serde(deserialize_with = "whole::u64")]
        version: u64,
        #[serde(rename = "for")]
        of: Value,
        #[serde(deserialize_with = "whole::u64")]
        deadline_at: u64,
        #[serde(deserialize_with = "whole::u64")]
        taken_at: u64,
        offers: Vec<Named>,
        #[serde(default)]
        refused: Vec<Named>,
        granted: Option<Vec<Named>>,
    }
    let input: In = super::quiet_input(raw)?;
    let pairs = |list: Vec<Named>| -> Vec<(String, u64)> {
        list.into_iter()
            .map(|one| (one.name, one.version))
            .collect()
    };
    let (offers, refused) = (pairs(input.offers), pairs(input.refused));
    let granted = input.granted.map(pairs);
    let received = myriad_her_link::call_receive(
        &input.action,
        input.version,
        &input.of,
        input.deadline_at,
        input.taken_at,
        &offers,
        &refused,
        granted.as_deref(),
    );
    let (verdict, code) = match received.verdict {
        myriad_her_link::CallVerdict::Hold => ("hold", None),
        myriad_her_link::CallVerdict::Do => ("do", None),
        myriad_her_link::CallVerdict::Fail(code) => ("fail", Some(code)),
    };
    Ok(json!({
        "verdict": verdict,
        "code": code,
        "version": received.version,
        "deadlineAt": received.deadline_at,
    }))
}

/// The largest whole number every JSON reader keeps exactly: no count or time of her link
/// is past it (`myriad_link_child`'s sessions hold to the same).
const WHOLE: i64 = 1 << 53;

/// Milliseconds, as her link counts time, from the microseconds every call takes: a time
/// from the epoch on, and one whose milliseconds a session keeps. Anything else is bad
/// input, so no step leaves a state the next would refuse.
fn link_ms(now: i64) -> Result<i64, Failure> {
    let ms = now / 1000;
    if now < 0 || ms > WHOLE {
        return Err(Failure::BadInput(
            "now is microseconds since the epoch".into(),
        ));
    }
    Ok(ms)
}

/// A session from the state a caller kept: the object `link.child_new` and
/// `link.child_step` hand back, or that object as a JSON text (what they hand back to a
/// caller that asked for `text`).
fn child(state: &Value) -> Result<myriad_link_child::Session, Failure> {
    let read;
    let state = match state {
        Value::String(text) => {
            read = serde_json::from_str::<Value>(text)
                .map_err(|_| Failure::BadInput("not a session's state".into()))?;
            &read
        }
        other => other,
    };
    let mut session = myriad_link_child::Session::restore(myriad_link_child::contract(), state)
        .map_err(Failure::BadInput)?;
    // The ABI it says it runs is this library's, whichever one the state was kept under.
    if let Some(build) = session.running().map(|(_, build)| build.to_string()) {
        session.runs(u64::from(crate::ABI_VERSION), &build);
    }
    Ok(session)
}

/// The state as the caller asked for it: the object, or its JSON text.
fn kept(session: &myriad_link_child::Session, text: bool) -> Value {
    let state = session.state();
    if text {
        Value::String(state.to_string())
    } else {
        state
    }
}

/// `{capabilities, app: {name, version}, lineage?, hostEpoch?, offers?, coreCommit?, text?}`
/// → `{state}`: a child's
/// session of her link, not connected yet (`myriad_link_child::Session`). `capabilities` is
/// what this end can do; names the contract does not know are left out of its hello.
/// `lineage` and `hostEpoch` are what its last welcome said, when it kept them. `offers`
/// is what this end may be asked to do for her, `[{name, version}]` of the contract's
/// `calls` (said in `call.offer` on a link where both ends listed `calls`; one the contract
/// does not name at that version, or that her host does, is left out). `coreCommit` is the
/// myriad-core commit this end was built from (40 hex digits, its pin): with it, `link.versions` is sent on a link
/// where both ends listed `versions` (the ABI it says is this library's). With
/// `text: true` the state comes back as a JSON text in a string (see `link.child_step`).
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
        #[serde(default, deserialize_with = "whole::maybe_i64")]
        host_epoch: Option<i64>,
        #[serde(default)]
        offers: Vec<Offer>,
        #[serde(default)]
        core_commit: Option<String>,
        #[serde(default)]
        passthrough: bool,
        #[serde(default)]
        text: bool,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Offer {
        name: String,
        #[serde(deserialize_with = "whole::u64")]
        version: u64,
    }
    let input: In = super::quiet_input(raw)?;
    if input
        .host_epoch
        .is_some_and(|epoch| !(0..=WHOLE).contains(&epoch))
    {
        return Err(Failure::BadInput("hostEpoch counts from 0".into()));
    }
    let capabilities: Vec<&str> = input.capabilities.iter().map(String::as_str).collect();
    let mut session = myriad_link_child::Session::new(
        myriad_link_child::contract(),
        &capabilities,
        (&input.app.name, &input.app.version),
        input.lineage.as_deref(),
        input.host_epoch,
    );
    let offers: Vec<(&str, u64)> = input
        .offers
        .iter()
        .map(|offer| (offer.name.as_str(), offer.version))
        .collect();
    session.offer(&offers);
    session.passes(input.passthrough);
    // The ABI is this library's own; what the end calls its build of the core is its to say.
    if let Some(build) = &input.core_commit {
        session.runs(u64::from(crate::ABI_VERSION), build);
    }
    Ok(json!({ "state": kept(&session, input.text) }))
}

/// `{state, now, event, text?}` → `{state, outputs, phase, hostCapabilities}`: one thing
/// happened to the
/// session. `state` is what the last call handed back, whole: the object, or its JSON text
/// in a string, which is what comes back when `text` is true. A caller whose JSON library
/// keeps every number as a double asks for `text` and never decodes it: the counts in a
/// state are read back either way, but the frames waiting in it are kept as they were
/// written, and such a library would rewrite the numbers in them. For the same caller,
/// every whole number written as a double in `event` (`4.0`) is read as the whole number
/// (`4`) before a frame is made of it. `now` is the time in
/// microseconds since the epoch (her link's own frames, and `myriad_link_child` used from
/// Rust, count milliseconds: a caller that passes those here is not told, and the heartbeat
/// goes wrong). `event` is `"connected"` (the socket is up),
/// `"disconnected"`, `"tick"` (call it several times a heartbeat; also while the socket is down when the end
/// may have calls in hand, whose deadlines pass all the same),
/// `{received: text}` (a text frame from her host), or something the shell does:
/// `{say: {turnId, text, sessionId?, rigState?, images?}}`, `{presence: body}`,
/// `{observePlayback: {runId, upcomingText, rig}}`, `{endPlayback: {runId}}`,
/// `{delivered: {id, body}}`, `{appraiseTouch: {touchId, touch, rigState?}}`,
/// `{touched: {touchId, touch}}`, `{wear: {outfitId}}`, `{setCapabilities: [..]}` (for the
/// next hello); and of a call her host asked this end to do (a `call` delivery):
/// `{callDone: {callId, result, blobs?}}`, `{callFailed: {callId, code}}`,
/// `{callProgress: {callId, phase, waiting?}}` (`waiting` is true while it waits for the
/// owner on the device), `{callPiece: {callId, piece}}`, and
/// `{callsLost: [{callId, action, version}]}` for calls it had in hand when it was
/// restarted, handed back before `"connected"`; of a call this end asks of her host:
/// `{call: {callId, action, version, args, for, withinMs}}` (answered within `withinMs`
/// milliseconds from now) and `{callCancel: {callId}}` (what comes of it is always told:
/// a `call.cancel` sent, a `call_result` said here, or an `unsent` of `call.cancel`); and what it tells her host as it
/// changes or happens: `{streamState: {name, version, value}}`,
/// `{streamEvent: {id, name, version, body}}`. `outputs` is what the shell does now, in order: `{send: text}`,
/// `{deliver: {kind, …}}`, `{drop: {why, …}}` (close the socket). `phase` is `closed`,
/// `greeting` or `open`. `hostCapabilities` is what her host accepted of this end's
/// capabilities in its last welcome (empty before a first, and from a new connection's
/// start until its welcome; kept while the link is down): pictures go with what the owner says only
/// when `images` is in it. The state is the caller's to keep and hand back whole.
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
    struct CallDone {
        call_id: String,
        result: Value,
        #[serde(default)]
        blobs: Vec<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct CallFailed {
        call_id: String,
        code: String,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct CallProgress {
        call_id: String,
        phase: String,
        #[serde(default)]
        waiting: bool,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct CallPiece {
        call_id: String,
        piece: Value,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Ask {
        call_id: String,
        action: String,
        #[serde(deserialize_with = "whole::u64")]
        version: u64,
        args: Value,
        #[serde(rename = "for")]
        of: Value,
        #[serde(deserialize_with = "whole::i64")]
        within_ms: i64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Cancel {
        call_id: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StreamState {
        name: String,
        #[serde(deserialize_with = "whole::u64")]
        version: u64,
        value: Value,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StreamEvent {
        id: String,
        name: String,
        #[serde(deserialize_with = "whole::u64")]
        version: u64,
        body: Value,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Took {
        #[serde(deserialize_with = "whole::u64")]
        link: u64,
        #[serde(deserialize_with = "whole::u64")]
        seq: u64,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Pass {
        t: String,
        #[serde(default)]
        id: Option<String>,
        body: Value,
        #[serde(default)]
        keep: Option<bool>,
        #[serde(default)]
        needs: Option<String>,
    }
    /// One frame, or several to go in the order given.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Passed {
        One(Pass),
        Many(Vec<Pass>),
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Lost {
        call_id: String,
        action: String,
        #[serde(deserialize_with = "whole::u64")]
        version: u64,
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
        CallDone(CallDone),
        CallFailed(CallFailed),
        CallProgress(CallProgress),
        CallPiece(CallPiece),
        CallsLost(Vec<Lost>),
        Call(Ask),
        CallCancel(Cancel),
        StreamState(StreamState),
        StreamEvent(StreamEvent),
        Took(Vec<Took>),
        Send(Passed),
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        state: Value,
        #[serde(deserialize_with = "whole::i64")]
        now: i64,
        event: Value,
        #[serde(default)]
        text: bool,
    }
    // What the owner said and showed her is in the input: a failure names no value of it.
    let In {
        state,
        now,
        event,
        text,
    } = super::quiet_input(raw)?;
    // An end that keeps every number as a double (it asked for `text`) writes the counts in
    // what it hands in as doubles too; a frame's body goes out as given, so they are read
    // here as the whole numbers they are.
    let mut event = event;
    if text {
        whole::settle(&mut event);
    }
    let event = Event::deserialize(event)
        .map_err(|_| Failure::BadInput("input does not fit the call".into()))?;
    let now = link_ms(now)?;
    let mut session = child(&state)?;
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
        Event::CallDone(done) => session.call_done(&done.call_id, &done.result, &done.blobs, now),
        Event::CallFailed(failed) => session.call_failed(&failed.call_id, &failed.code, now),
        Event::CallProgress(progress) => {
            session.call_progress(&progress.call_id, &progress.phase, progress.waiting, now)
        }
        Event::CallPiece(piece) => session.call_piece(&piece.call_id, &piece.piece, now),
        Event::Call(ask) => session.call(
            &ask.call_id,
            &ask.action,
            ask.version,
            &ask.args,
            &ask.of,
            ask.within_ms,
            now,
        ),
        Event::CallCancel(cancel) => session.call_cancel(&cancel.call_id, now),
        Event::StreamState(state) => {
            session.stream_state(&state.name, state.version, &state.value, now)
        }
        Event::StreamEvent(event) => {
            session.stream_event(&event.id, &event.name, event.version, &event.body, now)
        }
        Event::Took(took) => {
            let took: Vec<(u64, u64)> = took.iter().map(|one| (one.link, one.seq)).collect();
            session.took(&took, now)
        }
        Event::Send(passed) => {
            let frames = match passed {
                Passed::One(one) => vec![one],
                Passed::Many(many) => many,
            };
            frames
                .iter()
                .flat_map(|frame| {
                    session.pass(
                        &frame.t,
                        frame.id.as_deref(),
                        &frame.body,
                        frame.keep,
                        frame.needs.as_deref(),
                        now,
                    )
                })
                .collect()
        }
        Event::CallsLost(lost) => {
            let lost: Vec<(&str, &str, u64)> = lost
                .iter()
                .map(|call| (call.call_id.as_str(), call.action.as_str(), call.version))
                .collect();
            session.lost(&lost, now)
        }
    };
    Ok(json!({
        "state": kept(&session, text),
        "outputs": outputs.iter().map(myriad_link_child::Output::json).collect::<Vec<_>>(),
        "phase": session.phase().name(),
        "hostCapabilities": session.host_capabilities(),
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
        #[serde(default, deserialize_with = "whole::maybe_u16")]
        code: Option<u16>,
        #[serde(default)]
        reason: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Http {
        #[serde(deserialize_with = "whole::u16")]
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
        #[serde(default, deserialize_with = "whole::maybe_u32")]
        attempt: Option<u32>,
    }
    let In {
        close,
        http,
        attempt,
    } = super::quiet_input(raw)?;
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
    } = super::quiet_input(raw)?;
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
        #[serde(deserialize_with = "whole::i64")]
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
