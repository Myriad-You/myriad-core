//! Her link's frames through the C entry point: the shared conformance
//! cases (`crates/conformance/v1/link`) and every protocol-2 example in
//! `shared/her_link_fixtures.json`.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::{ffi, ok};
use crate::STATUS_BAD_INPUT;

pub(super) const TESTED: &[&str] = &[
    "conformance.host",
    "link.check",
    "link.child_after",
    "link.child_claims",
    "link.child_new",
    "link.child_step",
    "link.encode",
    "link.now_read",
];

#[test]
fn every_link_case_answers_through_the_c_entry_point() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/link/check.json");
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    for case in file["cases"].as_array().unwrap() {
        let mut got = ok("link.check", case["input"].clone());
        // The files keep the code; the detail is for people.
        if let Some(refused) = got.get_mut("refused").and_then(Value::as_object_mut) {
            refused.remove("detail");
        }
        assert_eq!(got, case["output"], "{}", case["name"]);
    }
}

#[test]
fn every_example_encodes_to_its_canonical_text() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../../shared/her_link_fixtures.json")).unwrap();
    for frame in fixtures["frames"].as_array().unwrap() {
        let text = ok("link.encode", json!({ "frame": frame }))["text"]
            .as_str()
            .unwrap()
            .to_string();
        let back: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(&back, frame);
    }
    let mut bad = fixtures["frames"][0].clone();
    bad["v"] = json!(2);
    let (status, out) = ffi("link.encode", &json!({ "frame": bad }));
    assert_eq!(status, STATUS_BAD_INPUT, "{out}");
}

/// The `now` family's bodies, read by the contract's `sessions` rules: the
/// contract's examples pass, a body against the rules is a refusal (an
/// answer, the frame came from the other end), another frame is bad input.
#[test]
fn now_bodies_are_read_by_the_sessions_rules() {
    let fixtures: Value =
        serde_json::from_str(include_str!("../../../../shared/her_link_fixtures.json")).unwrap();
    let body = |t: &str| {
        fixtures["frames"]
            .as_array()
            .unwrap()
            .iter()
            .find(|frame| frame["t"] == t)
            .unwrap()["body"]
            .clone()
    };
    let session = ok(
        "link.now_read",
        json!({"t": "now.session", "body": body("now.session")}),
    );
    assert_eq!(session["session"]["change"], "asked");
    assert_eq!(session["session"]["waitingOn"], "owner");
    let drop = ok(
        "link.now_read",
        json!({"t": "now.drop", "body": body("now.drop")}),
    );
    assert_eq!(drop["drop"], body("now.drop"));
    let mut bad = body("now.session");
    bad["remote"] = json!("https://token@github.com/me/repo");
    let refused = ok("link.now_read", json!({"t": "now.session", "body": bad}));
    assert_eq!(refused["refused"]["code"], "bad_frame");
    // ownerSaidAt, errors and overwrote only where the welcome listed
    // now.sessions2.
    let mut newer = body("now.session");
    newer["ownerSaidAt"] = json!(1_760_000_000_000_i64);
    let refused = ok(
        "link.now_read",
        json!({"t": "now.session", "body": newer.clone()}),
    );
    assert_eq!(refused["refused"]["code"], "bad_frame");
    let taken = ok(
        "link.now_read",
        json!({"t": "now.session", "body": newer, "sessions2": true}),
    );
    assert_eq!(taken["session"]["ownerSaidAt"], 1_760_000_000_000_i64);
    // choices only where it listed now.choices.
    let mut asked = body("now.session");
    asked["choices"] = json!(["push", "wait"]);
    let refused = ok(
        "link.now_read",
        json!({"t": "now.session", "body": asked.clone()}),
    );
    assert_eq!(refused["refused"]["code"], "bad_frame");
    let taken = ok(
        "link.now_read",
        json!({"t": "now.session", "body": asked, "choices": true}),
    );
    assert_eq!(taken["session"]["choices"], json!(["push", "wait"]));
    let (status, _) = ffi("link.now_read", &json!({"t": "moment", "body": {}}));
    assert_eq!(status, STATUS_BAD_INPUT);
}

/// A child end that is nothing but the C entry: its session is the state `link.child_step`
/// hands back, as an end in another language keeps it.
struct AcrossTheEntry {
    state: Value,
    out: std::collections::VecDeque<Value>,
}

impl AcrossTheEntry {
    fn new(capabilities: &[String]) -> Self {
        let made = ok(
            "link.child_new",
            json!({ "capabilities": capabilities, "app": { "name": "Child", "version": "1" } }),
        );
        Self {
            state: made["state"].clone(),
            out: Default::default(),
        }
    }

    fn step(&mut self, event: Value, now_ms: i64) {
        let stepped = ok(
            "link.child_step",
            json!({ "state": self.state, "now": now_ms * 1000, "event": event }),
        );
        self.state = stepped["state"].clone();
        self.out
            .extend(stepped["outputs"].as_array().unwrap().iter().cloned());
    }
}

impl myriad_link_conformance::ChildEnd for AcrossTheEntry {
    fn act(&mut self, what: &Value, now_ms: i64) -> Result<(), String> {
        let event = match what.as_str() {
            Some("connect") => json!("connected"),
            Some("disconnect") => json!("disconnected"),
            Some("tick") => json!("tick"),
            // `say` and `delivered` are the call's own events.
            _ => what.clone(),
        };
        self.step(event, now_ms);
        Ok(())
    }

    fn received(&mut self, text: &str, now_ms: i64) {
        self.step(json!({ "received": text }), now_ms);
    }

    fn next(&mut self) -> Option<Value> {
        self.out.pop_front()
    }
}

fn scripts() -> Vec<myriad_link_conformance::Script> {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/child");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 8);
    paths
        .iter()
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .collect()
}

#[test]
fn every_child_script_plays_through_the_c_entry_point() {
    for script in scripts() {
        let mut child = AcrossTheEntry::new(&script.capabilities);
        if let Err(fault) = myriad_link_conformance::play(&script, &mut child) {
            panic!("{}: {fault}", script.name);
        }
    }
}

#[test]
fn a_child_stepped_through_the_c_entry_point_is_the_crates_session() {
    use myriad_link_child::{Session, contract};
    let capabilities = ["say", "playback", "made.up"];
    let made = ok(
        "link.child_new",
        json!({
            "capabilities": capabilities,
            "app": { "name": "Child", "version": "1" },
            "lineage": "ln_01",
            "hostEpoch": 3,
        }),
    );
    let mut session = Session::new(
        contract(),
        &capabilities,
        ("Child", "1"),
        Some("ln_01"),
        Some(3),
    );
    assert_eq!(made["state"], session.state());
    let now = 1_760_140_800_000_i64;
    let stepped = ok(
        "link.child_step",
        json!({ "state": made["state"], "now": now * 1000 + 999, "event": "connected" }),
    );
    let outputs: Vec<Value> = session.connected(now).iter().map(|o| o.json()).collect();
    assert_eq!(stepped["outputs"], json!(outputs));
    assert_eq!(stepped["state"], session.state());
    assert_eq!(stepped["phase"], "greeting");
    // Every event the shell has reaches the same method of the same session: on an open
    // link, what comes back and the state after are the crate's, step by step.
    let welcome = json!({
        "v": 1, "t": "welcome", "seq": 1, "ack": 1, "at": now,
        "body": {
            "protocol": 2, "role": "host", "host": "site", "deviceId": "dev_1",
            "heartbeatMs": 15000, "capabilities": ["say", "playback", "images"], "turns": [],
            "serverTime": now, "sync": "off",
        },
    })
    .to_string();
    let mut state = stepped["state"].clone();
    let rig = json!({ "expression": "steady" });
    let touch = json!({ "zone": "head" });
    let body = json!({ "turnId": "t_02", "spokenChars": 1, "totalChars": 2, "interrupted": false });
    let photo = "data:image/jpeg;base64,/9j/4AAQ".to_string();
    type Did = Box<dyn Fn(&mut Session, i64) -> Vec<myriad_link_child::Output>>;
    let (r1, r2, r3, t1, t2, b1, p1, w1) = (
        rig.clone(),
        rig.clone(),
        rig.clone(),
        touch.clone(),
        touch.clone(),
        body.clone(),
        photo.clone(),
        welcome.clone(),
    );
    let events: Vec<(Value, Did)> = vec![
        (
            json!({ "received": welcome }),
            Box::new(move |s, at| s.received(&w1, at)),
        ),
        (
            json!({ "say": { "turnId": "t_02", "text": "你看", "sessionId": "sess_1", "rigState": rig, "images": [photo] } }),
            Box::new(move |s, at| {
                s.say(
                    "t_02",
                    "你看",
                    Some("sess_1"),
                    Some(&r1),
                    std::slice::from_ref(&p1),
                    at,
                )
            }),
        ),
        (
            json!({ "presence": { "presence": { "pageVisible": true } } }),
            Box::new(|s, at| s.presence(&json!({ "presence": { "pageVisible": true } }), at)),
        ),
        (
            json!({ "observePlayback": { "runId": "run_1", "upcomingText": "还不错", "rig": rig } }),
            Box::new(move |s, at| s.observe_playback("run_1", "还不错", &r2, at)),
        ),
        (
            json!({ "endPlayback": { "runId": "run_1" } }),
            Box::new(|s, at| s.end_playback("run_1", at)),
        ),
        (
            json!({ "delivered": { "id": "sd_1", "body": body } }),
            Box::new(move |s, at| s.delivered("sd_1", &b1, at)),
        ),
        (
            json!({ "appraiseTouch": { "touchId": "tc_1", "touch": touch, "rigState": rig } }),
            Box::new(move |s, at| s.appraise_touch("tc_1", &t1, Some(&r3), at)),
        ),
        (
            json!({ "touched": { "touchId": "tc_2", "touch": touch } }),
            Box::new(move |s, at| s.touched("tc_2", &t2, at)),
        ),
        (
            json!({ "wear": { "outfitId": "coat" } }),
            Box::new(|s, at| s.wear("coat", at)),
        ),
        (json!("tick"), Box::new(|s, at| s.tick(at + 15_000))),
        (
            json!({ "setCapabilities": ["say", "made.up"] }),
            Box::new(|s, _| {
                s.set_capabilities(&["say", "made.up"]);
                Vec::new()
            }),
        ),
        (
            json!("disconnected"),
            Box::new(|s, _| {
                s.disconnected();
                Vec::new()
            }),
        ),
    ];
    let mut sent_something = 0;
    for (index, (event, did)) in events.iter().enumerate() {
        let at = now + 100 * (index as i64 + 1);
        // `tick` is given the later time its closure uses.
        let call_at = if event == "tick" { at + 15_000 } else { at };
        let stepped = ok(
            "link.child_step",
            json!({ "state": state, "now": call_at * 1000, "event": event }),
        );
        let outputs: Vec<Value> = did(&mut session, at).iter().map(|o| o.json()).collect();
        sent_something += usize::from(!outputs.is_empty());
        assert_eq!(stepped["outputs"], json!(outputs), "{event}");
        assert_eq!(stepped["state"], session.state(), "{event}");
        state = stepped["state"].clone();
    }
    assert!(
        sent_something >= 10,
        "the events did something: {sent_something}"
    );
    assert_eq!(session.phase().name(), "closed");
    // A state that is not one, an event that is not one: bad input, with nothing of either.
    let (status, out) = ffi(
        "link.child_step",
        &json!({ "state": { "secretish": 1 }, "now": 0, "event": "tick" }),
    );
    assert_eq!(status, STATUS_BAD_INPUT);
    assert!(!out.to_string().contains("secretish"), "{out}");
    // Numbers no link reaches: bad input, not a step that overflows.
    for (field, value) in [
        ("sent", json!(u64::MAX)),
        ("openedAtMs", json!(i64::MIN)),
        ("heartbeatMs", json!(i64::MAX)),
    ] {
        let mut broken = made["state"].clone();
        broken["phase"] = json!("open");
        broken[field] = value;
        let step = json!({ "state": broken, "now": 0, "event": "tick" });
        assert_eq!(ffi("link.child_step", &step).0, STATUS_BAD_INPUT, "{field}");
    }
    let before = json!({ "state": made["state"], "now": -1, "event": "tick" });
    assert_eq!(ffi("link.child_step", &before).0, STATUS_BAD_INPUT);
    let epoch =
        json!({ "capabilities": [], "app": { "name": "c", "version": "1" }, "hostEpoch": -5 });
    assert_eq!(ffi("link.child_new", &epoch).0, STATUS_BAD_INPUT);
    let (status, out) = ffi(
        "link.child_step",
        &json!({ "state": made["state"], "now": 0, "event": { "say": { "turnId": "t", "text": "私话", "extra": 1 } } }),
    );
    assert_eq!(status, STATUS_BAD_INPUT);
    assert!(!out.to_string().contains("私话"), "{out}");
}

#[test]
fn what_a_close_means_and_what_an_end_claims_through_the_c_entry_point() {
    for script in scripts() {
        for step in &script.steps {
            let input = match (&step.close, &step.http) {
                (Some(close), _) => json!({ "close": close }),
                (_, Some(http)) => json!({ "http": http }),
                _ => continue,
            };
            assert_eq!(Some(&ok("link.child_after", input)), step.after.as_ref());
        }
    }
    let after = ok(
        "link.child_after",
        json!({ "close": { "code": 4408 }, "attempt": 2 }),
    );
    assert_eq!(after["then"], "reconnect");
    assert_eq!(after["backoffMs"], 4000);
    // Only a reconnect waits; and what is not a close as the call reads one is bad input,
    // never taken for a close it does not know.
    let gone = ok(
        "link.child_after",
        json!({ "close": { "code": 4001 }, "attempt": 2 }),
    );
    assert_eq!(gone, json!({ "then": "forget", "removal": "revoked" }));
    assert_eq!(
        ok("link.child_after", json!({ "close": {} }))["then"],
        "reconnect"
    );
    for wrong in [
        json!({}),
        json!({ "close": { "code": "4001" } }),
        json!({ "close": { "cod": 4001 } }),
        json!({ "close": 4001 }),
        json!({ "close": null }),
        json!({ "http": { "status": "403" } }),
        json!({ "close": {}, "http": { "status": 403 } }),
    ] {
        assert_eq!(
            ffi("link.child_after", &wrong).0,
            STATUS_BAD_INPUT,
            "{wrong}"
        );
    }
    // An end that says it can take her lines and does not read `say`.
    let claims = ok(
        "link.child_claims",
        json!({
            "capabilities": ["say", "rig", "reminders"],
            "reads": myriad_link_child::capabilities::BASE_TAKES,
        }),
    );
    assert_eq!(
        claims["unread"],
        json!([{ "capability": "say", "t": "say" }])
    );
    assert_eq!(claims["idle"], json!(["rig", "reminders"]));
    assert!(claims["takes"].as_array().unwrap().contains(&json!("say")));
    assert!(
        claims["sends"]
            .as_array()
            .unwrap()
            .contains(&json!("input"))
    );
}

#[test]
fn the_stand_in_host_moves_through_the_c_entry_point() {
    let host = ok("conformance.host", json!({ "event": "connected" }))["host"].clone();
    let hello = json!({
        "v": 1, "t": "hello", "seq": 1, "ack": 0, "at": 0,
        "body": { "protocol": 2, "role": "child", "surface": "app" },
    })
    .to_string();
    let took = ok(
        "conformance.host",
        json!({ "host": host, "event": { "takes": { "expected": { "t": "hello" }, "text": hello } } }),
    );
    assert!(took.get("fault").is_none(), "{took}");
    let sent = ok(
        "conformance.host",
        json!({ "host": took["host"], "event": { "frame": { "spec": { "t": "ping" }, "now": 5 } } }),
    );
    let frame: Value = serde_json::from_str(sent["text"].as_str().unwrap()).unwrap();
    assert_eq!((&frame["seq"], &frame["ack"]), (&json!(1), &json!(1)));
    // A move that does not hold says why and leaves the host where it was.
    let again = ok(
        "conformance.host",
        json!({ "host": sent["host"], "event": { "takes": { "expected": {}, "text": hello } } }),
    );
    assert!(again["fault"].as_str().unwrap().contains("seq"));
    assert_eq!(again["host"], sent["host"]);
    let far = json!({ "host": { "sent": u64::MAX }, "event": { "frame": { "spec": { "t": "ping" }, "now": 5 } } });
    assert_eq!(ffi("conformance.host", &far).0, STATUS_BAD_INPUT);
}
