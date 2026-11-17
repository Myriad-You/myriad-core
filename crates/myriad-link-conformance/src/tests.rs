//! Every script in `crates/conformance/v1/child`, the stand-in child against
//! the stand-in host.

use serde_json::json;

use super::*;

const SCRIPTS: &[(&str, &str)] = &[
    (
        "called",
        include_str!("../../conformance/v1/child/called.json"),
    ),
    (
        "called-clock",
        include_str!("../../conformance/v1/child/called-clock.json"),
    ),
    (
        "called-granted",
        include_str!("../../conformance/v1/child/called-granted.json"),
    ),
    (
        "called-long",
        include_str!("../../conformance/v1/child/called-long.json"),
    ),
    (
        "called-lost",
        include_str!("../../conformance/v1/child/called-lost.json"),
    ),
    (
        "called-restart",
        include_str!("../../conformance/v1/child/called-restart.json"),
    ),
    (
        "called-settled",
        include_str!("../../conformance/v1/child/called-settled.json"),
    ),
    (
        "calling",
        include_str!("../../conformance/v1/child/calling.json"),
    ),
    (
        "capabilities",
        include_str!("../../conformance/v1/child/capabilities.json"),
    ),
    (
        "close",
        include_str!("../../conformance/v1/child/close.json"),
    ),
    (
        "dropped",
        include_str!("../../conformance/v1/child/dropped.json"),
    ),
    (
        "handshake",
        include_str!("../../conformance/v1/child/handshake.json"),
    ),
    (
        "heartbeat",
        include_str!("../../conformance/v1/child/heartbeat.json"),
    ),
    (
        "offer",
        include_str!("../../conformance/v1/child/offer.json"),
    ),
    (
        "pass-in",
        include_str!("../../conformance/v1/child/pass-in.json"),
    ),
    (
        "pass-in-full",
        include_str!("../../conformance/v1/child/pass-in-full.json"),
    ),
    (
        "pass-in-relink",
        include_str!("../../conformance/v1/child/pass-in-relink.json"),
    ),
    (
        "pass-off",
        include_str!("../../conformance/v1/child/pass-off.json"),
    ),
    (
        "pass-out",
        include_str!("../../conformance/v1/child/pass-out.json"),
    ),
    (
        "pass-out-owned",
        include_str!("../../conformance/v1/child/pass-out-owned.json"),
    ),
    (
        "reliable",
        include_str!("../../conformance/v1/child/reliable.json"),
    ),
    (
        "streams",
        include_str!("../../conformance/v1/child/streams.json"),
    ),
    ("turn", include_str!("../../conformance/v1/child/turn.json")),
    (
        "unreadable",
        include_str!("../../conformance/v1/child/unreadable.json"),
    ),
    (
        "versions",
        include_str!("../../conformance/v1/child/versions.json"),
    ),
];

fn script(name: &str) -> Script {
    let text = SCRIPTS
        .iter()
        .find(|(known, _)| *known == name)
        .unwrap_or_else(|| panic!("no script {name}"))
        .1;
    serde_json::from_str(text).unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[test]
fn every_script_plays_child_against_host() {
    for (name, _) in SCRIPTS {
        let script = script(name);
        assert_eq!(&script.name, name);
        if let Err(fault) = run(&script) {
            panic!("{name}: {fault}");
        }
    }
}

#[test]
fn every_script_file_is_listed() {
    let folder = concat!(env!("CARGO_MANIFEST_DIR"), "/../conformance/v1/child");
    let mut found: Vec<String> = std::fs::read_dir(folder)
        .expect("the scripts' folder")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .into_string()
                .expect("a name")
        })
        .filter_map(|name| name.strip_suffix(".json").map(str::to_string))
        .collect();
    found.sort();
    let listed: Vec<&str> = SCRIPTS.iter().map(|(name, _)| *name).collect();
    assert_eq!(found, listed);
}

#[test]
fn the_scripts_cover_every_close_code_and_refusal_of_the_contract() {
    let contract: Value = serde_json::from_str(myriad_her_link::CONTRACT_JSON).unwrap();
    let close = script("close");
    for (name, rule) in contract["closeCodes"].as_object().unwrap() {
        let step = close
            .steps
            .iter()
            .find(|step| {
                step.close
                    .as_ref()
                    .is_some_and(|close| close["code"] == rule["code"])
            })
            .unwrap_or_else(|| panic!("no step for {name}"));
        // What the script says the child does is what the contract's flags say.
        let then = match (&rule["forgetCredentials"], &rule["reconnect"]) {
            (Value::Bool(true), _) => "forget",
            (_, Value::Bool(true)) => "reconnect",
            _ => "stop",
        };
        assert_eq!(step.after.as_ref().unwrap()["then"], then, "{name}");
    }
    for (code, status) in contract["refusals"].as_object().unwrap() {
        assert!(
            close.steps.iter().any(|step| step
                .http
                .as_ref()
                .is_some_and(|http| http["body"]["code"] == *code && http["status"] == *status)),
            "{code}"
        );
    }
}

/// `script` with step `index` replaced.
fn with(name: &str, index: usize, step: Value) -> Script {
    let mut script = script(name);
    script.steps[index] = serde_json::from_value(step).unwrap();
    script
}

#[test]
fn a_script_that_does_not_hold_is_told_where() {
    // The child sends something else than the step says.
    let fault = run(&with("heartbeat", 6, json!({ "child": { "t": "pong" } }))).unwrap_err();
    assert_eq!(fault.step, 6);
    // What the child does that no step expects is told at the next thing that happens: a
    // ping one tick early does not pass for the one the script expects a tick later.
    let early = with("heartbeat", 4, json!({ "at": 15000, "do": "tick" }));
    assert_eq!(run(&early).unwrap_err().step, 5);
    // And at the end.
    let mut more = script("capabilities");
    more.steps.truncate(5);
    assert_eq!(run(&more).unwrap_err().step, 5);
    // A step that expects a frame the child never sends.
    let mut extra = script("heartbeat");
    extra
        .steps
        .push(serde_json::from_value(json!({ "child": { "t": "ping" } })).unwrap());
    assert_eq!(run(&extra).unwrap_err().step, extra.steps.len() - 1);
    // A key that must not be there.
    let shown = json!({ "child": { "t": "input", "body": { "images": ABSENT } } });
    let last = script("capabilities").steps.len() - 1;
    assert_eq!(
        run(&with("capabilities", last, shown)).unwrap_err().step,
        last
    );
    // What the child does after a close is matched whole.
    let wrong = json!({ "close": { "code": 4001 }, "after": { "then": "reconnect" } });
    assert_eq!(run(&with("close", 0, wrong)).unwrap_err().step, 0);
    // Time does not go back.
    let back = json!({ "at": 0, "do": "tick" });
    assert!(
        run(&with("heartbeat", 4, back))
            .unwrap_err()
            .why
            .contains("back")
    );
}

#[test]
fn a_script_cannot_send_a_broken_frame_by_accident() {
    let why = |step: Value| run(&with("heartbeat", 2, step)).unwrap_err().why;
    // A host frame the contract refuses is the script's mistake, unless it says so.
    let broken = json!({ "at": 50, "host": { "t": "welcome", "body": {} } });
    assert!(why(broken).contains("no frame"));
    // One that says so must really be broken, and only then may it override the envelope.
    let good = json!({ "at": 50, "host": { "t": "ping", "unchecked": true } });
    assert!(why(good).contains("good frame"));
    let sly = json!({ "at": 50, "host": { "t": "ping", "envelope": { "v": 2 } } });
    assert!(why(sly).contains("unchecked"));
    // And a frame only a child sends is not a host's.
    let up = json!({ "at": 50, "host": { "t": "run.resume", "body": { "turnId": "t", "afterSeq": 0 } } });
    assert!(why(up).contains("not a frame a host sends"));
}

fn frame(t: &str, seq: u64, ack: u64, body: Value) -> String {
    json!({ "v": 1, "t": t, "seq": seq, "ack": ack, "at": T0, "body": body }).to_string()
}

#[test]
fn the_stand_in_host_holds_the_child_to_the_links_numbering() {
    let mut host = Host::default();
    host.connected();
    let hello = |seq, ack| {
        frame(
            "hello",
            seq,
            ack,
            json!({ "protocol": 2, "role": "child", "surface": "app" }),
        )
    };
    let any = json!({});
    assert!(host.takes(&any, &hello(2, 0)).unwrap_err().contains("seq"));
    assert!(host.takes(&any, &hello(1, 1)).unwrap_err().contains("acks"));
    assert!(host.takes(&any, "{}").unwrap_err().contains("refuses"));
    let down = frame("pong", 1, 0, json!({}));
    host.takes(&any, &down).unwrap();
    let mood = host.frame(&json!({ "t": "ping" }), T0).unwrap();
    assert!(mood.starts_with("{\"ack\":1,"), "its one byte form: {mood}");
    // A frame only a host sends, from the child.
    let wrong_way = frame(
        "run.end",
        2,
        1,
        json!({ "turnId": "t", "runId": "r", "lastSequence": 0 }),
    );
    assert!(
        host.takes(&any, &wrong_way)
            .unwrap_err()
            .contains("not a frame a child sends")
    );
    // The same frame not in its one byte form.
    let loose = "{ \"t\":\"ping\", \"v\":1, \"seq\":2, \"ack\":1, \"at\":0, \"body\":{} }";
    assert!(
        host.takes(&any, loose)
            .unwrap_err()
            .contains("one byte form")
    );
    // A frame that is not what the step asks leaves the host where it was.
    let before = host.clone();
    let other = json!({ "t": "pong", "seq": "$n" });
    assert!(host.takes(&other, &frame("ping", 2, 1, json!({}))).is_err());
    assert_eq!(host, before);
    // An ack that goes back.
    host.takes(&any, &frame("ping", 2, 1, json!({}))).unwrap();
    assert!(
        host.takes(&any, &frame("ping", 3, 0, json!({})))
            .unwrap_err()
            .contains("acks")
    );
    // A name is bound by the first frame that carries it and holds from then on.
    let mut vars = Vars::new();
    assert!(matches(
        &json!({ "a": "$x" }),
        &json!({ "a": 1, "b": 2 }),
        &mut vars
    ));
    assert!(!matches(&json!(["$x"]), &json!([2]), &mut vars));
    assert_eq!(
        filled(&json!({ "run": "$x" }), &vars),
        Ok(json!({ "run": 1 }))
    );
    assert_eq!(filled(&json!("$y"), &vars), Err("$y".to_string()));
}

#[test]
fn the_stand_in_child_holds_a_host_to_the_contract_and_the_numbering() {
    let welcome = json!({
        "protocol": 2, "role": "host", "host": "site", "deviceId": "dev_1", "heartbeatMs": 15000,
        "capabilities": [], "turns": [], "serverTime": T0, "sync": "off",
    });
    let mut child = Child::new(&[]);
    assert!(
        child
            .act(
                &json!({ "say": { "turnId": "t", "text": "x", "sesionId": "typo" } }),
                T0
            )
            .is_err()
    );
    child.act(&json!("connect"), T0).unwrap();
    // Acking more than the child sent, a seq that skips, a frame only a child sends, and one
    // the contract refuses.
    assert!(
        child
            .hears(&frame("welcome", 1, 2, welcome.clone()), T0)
            .unwrap_err()
            .contains("acks")
    );
    assert!(
        child
            .hears(&frame("welcome", 2, 1, welcome.clone()), T0)
            .unwrap_err()
            .contains("seq")
    );
    let up = frame("presence", 1, 1, json!({ "presence": {} }));
    assert!(
        child
            .hears(&up, T0)
            .unwrap_err()
            .contains("not a frame a host sends")
    );
    assert!(
        child
            .hears(&frame("welcome", 1, 1, json!({})), T0)
            .unwrap_err()
            .contains("refuses")
    );
    assert!(
        child
            .out
            .iter()
            .all(|output| matches!(output, Output::Send(_))),
        "none was read"
    );
    // A good one is read by the child core.
    child
        .hears(&frame("welcome", 1, 1, welcome), T0 + 50)
        .unwrap();
    assert_eq!(child.session().phase().name(), "open");
    assert!(
        child
            .hears(&frame("ping", 2, 0, json!({})), T0 + 60)
            .unwrap_err()
            .contains("acks")
    );
    child
        .hears(&frame("ping", 2, 1, json!({})), T0 + 60)
        .unwrap();
    // A second connection numbers from 1 again.
    child.act(&json!("disconnect"), T0 + 100).unwrap();
    child.act(&json!("connect"), T0 + 200).unwrap();
    child
        .hears(&frame("ping", 1, 1, json!({})), T0 + 210)
        .unwrap();
}

/// A child end whose shell does one thing wrong, to show [`play`] tells it.
struct Chatty(Child);

impl ChildEnd for Chatty {
    fn act(&mut self, what: &Value, now_ms: i64) -> Result<(), String> {
        self.0.act(what, now_ms)
    }
    fn received(&mut self, text: &str, now_ms: i64) {
        self.0.received(text, now_ms);
    }
    fn next(&mut self) -> Option<Value> {
        // It hides what it could not read from its shell.
        let next = ChildEnd::next(&mut self.0)?;
        if next["deliver"]["kind"] == "unread" {
            return self.next();
        }
        Some(next)
    }
}

#[test]
fn another_child_end_is_played_the_same_scripts() {
    let hides = |name: &str| play(&script(name), &mut Chatty(Child::of(&script(name))));
    assert!(hides("turn").is_ok());
    assert!(hides("unreadable").is_err());
}
