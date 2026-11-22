//! Her link's frames through the C entry point: the shared conformance
//! cases (`crates/conformance/v1/link`) and every protocol-2 example in
//! `shared/her_link_fixtures.json`.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::{ffi, ok};
use crate::STATUS_BAD_INPUT;

pub(super) const TESTED: &[&str] = &[
    "conformance.host",
    "link.call_receive",
    "link.check",
    "link.child_after",
    "link.child_claims",
    "link.child_new",
    "link.child_step",
    "link.encode",
    "link.host_call_receive",
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
fn every_host_call_receive_case_answers_through_the_c_entry_point() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../conformance/v1/link/host_call_receive.json");
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(file["call"], "link.host_call_receive");
    let mut said = std::collections::BTreeSet::new();
    for case in file["cases"].as_array().unwrap() {
        let got = ok("link.host_call_receive", case["input"].clone());
        assert_eq!(got, case["output"], "{}", case["name"]);
        // A case is named after what it answers.
        let what = got["code"].as_str().or(got["verdict"].as_str()).unwrap();
        assert!(
            case["name"]
                .as_str()
                .unwrap()
                .starts_with(&format!("{what}:")),
            "{}",
            case["name"]
        );
        said.insert(what.to_string());
    }
    // Every way the host answers has a case; it never holds.
    let every = [
        "call_not_granted",
        "call_timeout",
        "call_unknown",
        "do",
        "version_unsupported",
        "wrong_audience",
    ];
    assert_eq!(said.into_iter().collect::<Vec<_>>(), every);
    // What is not the call's input is bad input, and says nothing of it.
    let mut bad = file["cases"][0]["input"].clone();
    bad["other"] = json!("a value");
    let (status, answer) = ffi("link.host_call_receive", &bad);
    assert_eq!(status, STATUS_BAD_INPUT);
    assert!(!answer.to_string().contains("a value"));
    // providing is said every time, an empty list when the host provides nothing.
    let mut none = file["cases"][0]["input"].clone();
    none.as_object_mut().unwrap().remove("providing");
    assert_eq!(ffi("link.host_call_receive", &none).0, STATUS_BAD_INPUT);
}

#[test]
fn every_call_receive_case_answers_through_the_c_entry_point() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/link/call_receive.json");
    let file: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(file["call"], "link.call_receive");
    let mut said = std::collections::BTreeSet::new();
    for case in file["cases"].as_array().unwrap() {
        let got = ok("link.call_receive", case["input"].clone());
        assert_eq!(got, case["output"], "{}", case["name"]);
        // A case is named after what it answers.
        let what = got["code"].as_str().or(got["verdict"].as_str()).unwrap();
        assert!(
            case["name"]
                .as_str()
                .unwrap()
                .starts_with(&format!("{what}:")),
            "{}",
            case["name"]
        );
        said.insert(what.to_string());
    }
    // Every way it can answer has a case.
    let every = [
        "call_not_granted",
        "call_timeout",
        "call_unknown",
        "do",
        "hold",
        "version_unsupported",
        "wrong_audience",
    ];
    assert_eq!(said.into_iter().collect::<Vec<_>>(), every);
    // What is not the call's input is bad input, and says nothing of it.
    let mut bad = file["cases"][0]["input"].clone();
    bad["other"] = json!("a value");
    let (status, answer) = ffi("link.call_receive", &bad);
    assert_eq!(status, STATUS_BAD_INPUT);
    assert!(!answer.to_string().contains("a value"));
    // granted is said every time, null when her host has not said: left out, the call
    // would hold every ask for good, so it is bad input.
    let mut none = file["cases"][0]["input"].clone();
    none.as_object_mut().unwrap().remove("granted");
    assert_eq!(ffi("link.call_receive", &none).0, STATUS_BAD_INPUT);
    // No number past what every JSON reader keeps exactly.
    let mut far = file["cases"][0]["input"].clone();
    far["deadlineAt"] = json!(u64::MAX);
    assert_eq!(ffi("link.call_receive", &far).0, STATUS_BAD_INPUT);
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
/// hands back, as an end in another language keeps it. With `doubles` it is an end whose
/// JSON library keeps every number as a double: it asks for the state as text, never
/// decodes it, and writes its own whole numbers the way such a library does (`3.0`).
struct AcrossTheEntry {
    /// What `link.child_new` was given: a restart makes the session anew from it.
    new: Value,
    state: Value,
    out: std::collections::VecDeque<Value>,
    doubles: bool,
}

impl AcrossTheEntry {
    fn new(script: &myriad_link_conformance::Script, doubles: bool) -> Self {
        let mut new = json!({
            "capabilities": script.capabilities,
            "app": { "name": "Child", "version": "1" },
            "offers": script.offers,
            "passthrough": script.passthrough,
            "text": doubles,
        });
        if let Some(core) = &script.core {
            new["coreCommit"] = json!(core.commit);
        }
        let made = ok("link.child_new", new.clone());
        assert_eq!(made["state"].is_string(), doubles);
        Self {
            new,
            state: made["state"].clone(),
            out: Default::default(),
            doubles,
        }
    }

    fn step(&mut self, event: Value, now_ms: i64) {
        // Every number this end writes, the time and the counts in what it hands in.
        fn doubled(value: &mut Value) {
            match value {
                Value::Number(n) => *value = json!(n.as_f64().unwrap()),
                Value::Array(items) => items.iter_mut().for_each(doubled),
                Value::Object(fields) => fields.values_mut().for_each(doubled),
                _ => {}
            }
        }
        let (mut now, mut event) = (json!(now_ms * 1000), event);
        if self.doubles {
            doubled(&mut now);
            doubled(&mut event);
        }
        let stepped = ok(
            "link.child_step",
            json!({ "state": self.state, "now": now, "event": event, "text": self.doubles }),
        );
        assert_eq!(stepped["state"].is_string(), self.doubles);
        self.state = stepped["state"].clone();
        self.out
            .extend(stepped["outputs"].as_array().unwrap().iter().cloned());
    }
}

impl myriad_link_conformance::ChildEnd for AcrossTheEntry {
    fn act(&mut self, what: &Value, now_ms: i64) -> Result<(), String> {
        let event = match what.as_str() {
            // The process is gone: nothing of the state it held is kept.
            Some("restart") => {
                self.state = ok("link.child_new", self.new.clone())["state"].clone();
                return Ok(());
            }
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
    assert_eq!(paths.len(), 26);
    paths
        .iter()
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap())
        .collect()
}

#[test]
fn every_child_script_plays_through_the_c_entry_point() {
    for doubles in [false, true] {
        for script in scripts() {
            let mut child = AcrossTheEntry::new(&script, doubles);
            if let Err(fault) = myriad_link_conformance::play(&script, &mut child) {
                panic!("{} (doubles: {doubles}): {fault}", script.name);
            }
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
        let accepted: Vec<&String> = session.host_capabilities().iter().collect();
        assert_eq!(stepped["hostCapabilities"], json!(accepted), "{event}");
        assert_eq!(
            stepped["mayPut"],
            json!(session.may_put(call_at)),
            "{event}"
        );
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
    // A time before the epoch, a time past what a session keeps (milliseconds past 2^53: a
    // step with it would leave a state the next step refuses), a time with a fraction.
    for now in [
        json!(-1),
        json!(i64::MAX),
        json!((1i64 << 53) * 1000 + 1000),
        json!(1.5),
    ] {
        let step = json!({ "state": made["state"], "now": now, "event": "connected" });
        assert_eq!(ffi("link.child_step", &step).0, STATUS_BAD_INPUT, "{now}");
    }
    // The latest time it takes leaves a state the next step takes.
    let last = (1i64 << 53) * 1000;
    let far = ok(
        "link.child_step",
        json!({ "state": made["state"], "now": last, "event": "connected" }),
    );
    ok(
        "link.child_step",
        json!({ "state": far["state"], "now": last, "event": "tick" }),
    );
    for epoch in [
        json!(-5),
        json!((1u64 << 53) + 1),
        json!(u64::MAX),
        json!(2.5),
        json!("3"),
    ] {
        let new = json!({ "capabilities": [], "app": { "name": "c", "version": "1" }, "hostEpoch": epoch });
        assert_eq!(ffi("link.child_new", &new).0, STATUS_BAD_INPUT, "{epoch}");
    }
    // A state as text that is not a state's text.
    let step = json!({ "state": "{\"secretish\": 1}", "now": 0, "event": "tick" });
    let (status, out) = ffi("link.child_step", &step);
    assert_eq!(status, STATUS_BAD_INPUT);
    assert!(!out.to_string().contains("secretish"), "{out}");
    // What the owner said in a place of the wrong type: bad input that quotes none of it.
    for event in [
        json!({ "say": { "turnId": "t", "text": ["私话"] } }),
        json!({ "say": { "turnId": "t", "text": "x", "sessionId": { "私话": 1 } } }),
        json!({ "say": { "turnId": "t", "text": "x", "images": "私话" } }),
        json!({ "received": { "私话": true } }),
        json!("私话"),
    ] {
        let step = json!({ "state": made["state"], "now": 0, "event": event });
        let (status, out) = ffi("link.child_step", &step);
        assert_eq!(status, STATUS_BAD_INPUT, "{event}");
        assert!(!out.to_string().contains("私话"), "{out}");
    }
    // An end of doubles hands in counts as doubles: with `text` they go out as counts.
    let open_text = ok(
        "link.child_step",
        json!({ "state": made["state"], "now": (now * 1000) as f64, "event": "connected", "text": true }),
    );
    let spoken =
        json!({ "turnId": "t_02", "spokenChars": 4.0, "totalChars": 11.0, "interrupted": false });
    let kept = ok(
        "link.child_step",
        json!({ "state": open_text["state"], "now": (now * 1000) as f64, "event": { "delivered": { "id": "sd_1", "body": spoken } }, "text": true }),
    );
    let text = kept["state"].as_str().unwrap();
    assert!(
        text.contains("\"spokenChars\":4,") && !text.contains("4.0"),
        "{text}"
    );
    // Words under an id that is not one are not lost in silence: the step says so.
    let lost = ok(
        "link.child_step",
        json!({ "state": made["state"], "now": now * 1000, "event": { "say": { "turnId": "not an id", "text": "在吗" } } }),
    );
    assert_eq!(
        lost["outputs"],
        json!([{ "deliver": { "kind": "unsent", "t": "input", "code": "bad_id", "id": null } }])
    );
    let (status, out) = ffi(
        "link.child_step",
        &json!({ "state": made["state"], "now": 0, "event": { "say": { "turnId": "t", "text": "私话", "extra": 1 } } }),
    );
    assert_eq!(status, STATUS_BAD_INPUT);
    assert!(!out.to_string().contains("私话"), "{out}");
}

#[test]
fn the_abi_a_child_says_it_runs_is_this_librarys_whatever_its_state_says() {
    let made = ok(
        "link.child_new",
        json!({
            "capabilities": ["versions"],
            "app": { "name": "Child", "version": "1" },
            "coreCommit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        }),
    );
    assert_eq!(
        made["state"]["core"],
        json!({ "abi": crate::ABI_VERSION, "commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" })
    );
    // A state kept under an older library.
    let mut state = made["state"].clone();
    state["core"]["abi"] = json!(1);
    let now = 1_760_140_800_000_000_i64;
    let open = ok(
        "link.child_step",
        json!({ "state": state, "now": now, "event": "connected" }),
    );
    let welcome = json!({
        "v": 1, "t": "welcome", "seq": 1, "ack": 1, "at": now / 1000,
        "body": {
            "protocol": 2, "role": "host", "host": "site", "deviceId": "dev_1",
            "heartbeatMs": 15000, "capabilities": ["versions"], "turns": [],
            "serverTime": now / 1000, "sync": "off",
        },
    })
    .to_string();
    let said = ok(
        "link.child_step",
        json!({ "state": open["state"], "now": now, "event": { "received": welcome } }),
    );
    let versions: Value =
        serde_json::from_str(said["outputs"][1]["send"].as_str().unwrap()).unwrap();
    assert_eq!(versions["t"], "link.versions");
    assert_eq!(
        versions["body"]["core"],
        json!({ "abi": crate::ABI_VERSION, "commit": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" })
    );
    assert_eq!(said["state"]["core"]["abi"], crate::ABI_VERSION);
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
    // Whole numbers as an encoder of doubles writes them are the same numbers.
    let doubled = ok(
        "link.child_after",
        json!({ "close": { "code": 4001.0 }, "attempt": 2.0 }),
    );
    assert_eq!(doubled, gone);
    let refused = ok(
        "link.child_after",
        json!({ "http": { "status": 500.0 }, "attempt": 0.0 }),
    );
    assert_eq!(
        (&refused["then"], &refused["backoffMs"]),
        (&json!("reconnect"), &json!(1000))
    );
    for wrong in [
        json!({}),
        json!({ "close": { "code": "4001" } }),
        json!({ "close": { "cod": 4001 } }),
        json!({ "close": { "code": 4001.5 } }),
        json!({ "close": { "code": 70000 } }),
        json!({ "close": { "code": -1 } }),
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
