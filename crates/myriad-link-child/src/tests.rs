//! The child side against frames as her host sends them: the contract's own
//! fixtures (`shared/her_link_fixtures.json`) and frames built from them.

use serde_json::{Value, json};

use crate::*;

const FIXTURES: &str = include_str!("../../../shared/her_link_fixtures.json");
const T0: i64 = 1_760_140_800_000;

/// What a child with her figure, her voice and her conversations says it can do.
const CAPABILITIES: &[&str] = &[
    "rig",
    "voice.out",
    "voice.in",
    "notify",
    "panel",
    "say",
    "playback",
    "touch",
    "wardrobe",
    "conversations",
    "conversations.live",
    "images",
    "reminders",
    "wardrobe.list",
];

fn fixture(t: &str) -> Value {
    let all: Value = serde_json::from_str(FIXTURES).unwrap();
    all["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| frame["t"] == t)
        .unwrap_or_else(|| panic!("no fixture {t}"))
        .clone()
}

/// A host frame: the fixture of type `t`, renumbered, its body changed by `edit`.
fn host(t: &str, seq: u64, ack: u64, edit: impl FnOnce(&mut Value)) -> String {
    let mut frame = fixture(t);
    frame["seq"] = json!(seq);
    frame["ack"] = json!(ack);
    edit(&mut frame["body"]);
    frame.to_string()
}

fn sent(out: &[Output]) -> Vec<Value> {
    out.iter()
        .filter_map(|o| match o {
            Output::Send(text) => Some(serde_json::from_str(text).unwrap()),
            _ => None,
        })
        .collect()
}

fn delivered(out: &[Output]) -> Vec<&Delivery> {
    out.iter()
        .filter_map(|o| match o {
            Output::Deliver(d) => Some(d),
            _ => None,
        })
        .collect()
}

fn session() -> Session {
    Session::new(contract(), CAPABILITIES, ("Child", "0.1.0"), None, None)
}

/// Connected and welcomed; the host has heard the hello.
fn open() -> Session {
    let mut s = session();
    s.connected(T0);
    let out = s.received(&host("welcome", 1, 1, |_| {}), T0 + 50);
    assert!(matches!(delivered(&out)[0], Delivery::Welcome { .. }));
    assert_eq!(s.phase(), Phase::Open);
    s
}

#[test]
fn a_welcome_without_the_frame_version_is_not_taken() {
    let mut s = session();
    s.connected(T0);
    let mut welcome: Value = serde_json::from_str(&host("welcome", 1, 1, |_| {})).unwrap();
    welcome.as_object_mut().unwrap().remove("v");
    let out = s.received(&welcome.to_string(), T0 + 50);
    assert!(matches!(
        &out[..],
        [
            Output::Deliver(Delivery::Unread { t: Some(t), code }),
            Output::Drop(Down::RefusedHandshake)
        ] if t == "welcome" && code == "bad_frame"
    ));
}

#[test]
fn a_welcome_without_sync_is_not_taken() {
    let mut s = session();
    s.connected(T0);
    let welcome = host("welcome", 1, 1, |body| {
        body.as_object_mut().unwrap().remove("sync");
    });
    let out = s.received(&welcome, T0 + 50);
    assert!(matches!(
        &out[..],
        [
            Output::Deliver(Delivery::Unread { t: Some(t), code }),
            Output::Drop(Down::RefusedHandshake)
        ] if t == "welcome" && code == "bad_frame"
    ));
}

#[test]
fn a_frame_of_the_sync_families_passes_her_by() {
    let mut s = open();
    let out = s.received(&host("lease.epoch", 2, 1, |_| {}), T0 + 60);
    assert!(out.is_empty());
    assert_eq!(s.phase(), Phase::Open);
}

#[test]
fn another_protocol_or_frame_version_stops_and_asks_for_an_update() {
    for reason in ["unsupported_protocol", "version_unsupported"] {
        let after = Session::after_close(contract(), None, Some(reason));
        assert!(matches!(after, After::Stop(Down::UnsupportedProtocol)));
    }
}

#[test]
fn hello_goes_first_with_this_ends_capabilities() {
    let mut s = session();
    let out = sent(&s.connected(T0));
    assert_eq!(out.len(), 1);
    let hello = &out[0];
    assert_eq!(hello["t"], "hello");
    assert_eq!(hello["seq"], 1);
    assert_eq!(hello["ack"], 0);
    assert_eq!(hello["v"], 1);
    assert_eq!(hello["body"]["protocol"], 2);
    assert_eq!(hello["body"]["role"], "child");
    assert_eq!(hello["body"]["surface"], "app");
    let caps: Vec<&str> = hello["body"]["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    assert_eq!(
        caps, CAPABILITIES,
        "the contract knows every capability this end says"
    );
}

#[test]
fn every_host_fixture_the_child_reads_is_taken_by_the_core() {
    for t in [
        "welcome",
        "run.accepted",
        "run.rejected",
        "run.event",
        "run.end",
        "perform",
        "mood",
        "say",
        "touch.reaction",
        "outfit",
        "conversations",
        "conversation",
        "notice",
        "ping",
        "error",
    ] {
        assert!(crate::frames::takes(&fixture(t).to_string()), "{t}");
    }
}

#[test]
fn a_turn_goes_out_once_and_streams_in_order() {
    let mut s = open();
    let out = sent(&s.say(
        "t_02",
        "今天过得怎么样？",
        Some("sess_1"),
        None,
        &[],
        T0 + 1000,
    ));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0]["t"], "input");
    assert_eq!(out[0]["id"], "t_02");
    assert_eq!(out[0]["seq"], 2);
    let again = s.say("t_02", "again", None, None, &[], T0 + 1001);
    assert!(sent(&again).is_empty(), "one turn per id");
    assert!(matches!(
        delivered(&again)[..],
        [Delivery::Unsent { code, .. }] if code == "bad_id"
    ));

    let accepted = s.received(&host("run.accepted", 2, 2, |_| {}), T0 + 1020);
    assert!(matches!(
        delivered(&accepted)[..],
        [Delivery::Accepted {
            duplicate: false,
            ..
        }]
    ));

    let event = |seq, sequence| {
        host("run.event", seq, 2, move |b| {
            b["sequence"] = json!(sequence)
        })
    };
    assert_eq!(delivered(&s.received(&event(3, 1), T0 + 1100)).len(), 1);
    assert_eq!(delivered(&s.received(&event(4, 2), T0 + 1200)).len(), 1);
    assert!(
        delivered(&s.received(&event(5, 2), T0 + 1300)).is_empty(),
        "seen"
    );
    assert!(
        delivered(&s.received(&event(6, 1), T0 + 1400)).is_empty(),
        "older"
    );

    let end = s.received(&host("run.end", 7, 2, |_| {}), T0 + 3000);
    assert!(matches!(delivered(&end)[..], [Delivery::End { .. }]));
    assert!(s.turn("t_02").unwrap().ended);
    assert!(s.unfinished().is_empty());
}

#[test]
fn an_unanswered_input_goes_again_after_a_reconnect() {
    let mut s = open();
    s.say("t_02", "在吗", None, None, &[], T0 + 1000);
    s.disconnected();
    s.connected(T0 + 5000);
    let out = s.received(&host("welcome", 1, 1, |_| {}), T0 + 5050);
    let frames = sent(&out);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["t"], "input");
    assert_eq!(frames[0]["id"], "t_02", "same id");
    assert_eq!(frames[0]["seq"], 2, "numbering restarts per connection");
}

#[test]
fn an_unfinished_turn_resumes_after_its_last_event() {
    let mut s = open();
    s.say("t_02", "在吗", None, None, &[], T0 + 1000);
    s.received(&host("run.accepted", 2, 2, |_| {}), T0 + 1020);
    s.received(
        &host("run.event", 3, 2, |b| b["sequence"] = json!(4)),
        T0 + 1100,
    );
    s.disconnected();
    s.connected(T0 + 5000);
    let frames = sent(&s.received(&host("welcome", 1, 1, |_| {}), T0 + 5050));
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["t"], "run.resume");
    assert_eq!(frames[0]["body"]["afterSeq"], 4);
}

#[test]
fn a_turn_her_host_acked_and_never_answered_is_asked_for_by_its_turn() {
    let mut s = open();
    s.say("t_02", "在吗", None, None, &[], T0 + 1000);
    // Her host heard the input (its ping acks seq 2) and the link went before it answered.
    s.received(&host("ping", 2, 2, |_| {}), T0 + 1010);
    s.disconnected();
    s.connected(T0 + 5000);
    let frames = sent(&s.received(&host("welcome", 1, 1, |_| {}), T0 + 5050));
    assert_eq!(frames.len(), 1, "not sent again: her host has it");
    assert_eq!(frames[0]["t"], "run.resume");
    assert_eq!(
        frames[0]["body"],
        json!({ "turnId": "t_02", "afterSeq": 0 })
    );
    // A host that never started it says so, and the turn ends unsent.
    let out = s.received(
        &host("run.rejected", 2, 2, |b| {
            b["turnId"] = json!("t_02");
            b["code"] = json!("turn_unknown");
        }),
        T0 + 5100,
    );
    assert!(matches!(
        delivered(&out)[..],
        [Delivery::Rejected { code, .. }] if code == "turn_unknown"
    ));
    assert!(s.unfinished().is_empty());
}

#[test]
fn a_turn_asked_for_by_its_turn_is_answered_by_its_runs_events() {
    let mut s = open();
    s.say("t_02", "在吗", None, None, &[], T0 + 1000);
    s.received(&host("ping", 2, 2, |_| {}), T0 + 1010);
    s.disconnected();
    s.connected(T0 + 5000);
    s.received(&host("welcome", 1, 1, |_| {}), T0 + 5050);
    assert!(!s.turn("t_02").unwrap().answered);
    // Her host has the turn: it sends the run's events and its end, and no run.accepted.
    let out = s.received(&host("run.event", 2, 2, |_| {}), T0 + 5100);
    assert!(matches!(
        delivered(&out)[..],
        [Delivery::Event { run_id, .. }] if run_id == "run_fedcba9876543210fedcba9876543210"
    ));
    let turn = s.turn("t_02").unwrap();
    assert!(turn.answered);
    assert_eq!(
        turn.run_id.as_deref(),
        Some("run_fedcba9876543210fedcba9876543210")
    );
    let out = s.received(&host("run.end", 3, 2, |_| {}), T0 + 5200);
    assert!(matches!(delivered(&out)[..], [Delivery::End { .. }]));
    assert!(s.unfinished().is_empty());
}

#[test]
fn an_input_the_core_will_not_send_ends_its_turn() {
    let mut s = open();
    let long = "长".repeat(300_000);
    let out = s.say("t_09", &long, None, None, &[], T0 + 1000);
    assert!(matches!(
        delivered(&out)[..],
        [Delivery::Unsent { t, code, .. }] if t == "input" && code == "too_large"
    ));
    assert!(s.unfinished().is_empty());
    // So nothing is asked for it after a reconnect.
    s.disconnected();
    s.connected(T0 + 5000);
    assert!(sent(&s.received(&host("welcome", 1, 1, |_| {}), T0 + 5050)).is_empty());
}

#[test]
fn nothing_the_owner_said_is_let_fall_in_silence() {
    // Told at once, with which words (the id), and nothing goes.
    let unsent = |out: &[Output], t: &str, code: &str, id: Option<&str>| {
        matches!(
            delivered(out)[..],
            [Delivery::Unsent { t: got, code: why, id: which }]
                if got == t && why == code && which.as_deref() == id
        ) && sent(out).is_empty()
    };
    let mut s = open();
    // An id that is not one: the words are not kept, and the shell is told. What is not an
    // id is not repeated back: it may be anything the shell had in hand.
    let bad = s.say("bad id", "在吗", None, None, &[], T0 + 1000);
    assert!(unsent(&bad, "input", "bad_id", None));
    assert!(unsent(
        &s.say("", "在吗", None, None, &[], T0 + 1000),
        "input",
        "bad_id",
        None
    ));
    assert!(s.unfinished().is_empty());
    // An id already used for other words, or the same words to another session.
    assert_eq!(
        sent(&s.say("t_02", "在吗", None, None, &[], T0 + 1100)).len(),
        1
    );
    let other = s.say("t_02", "别的话", None, None, &[], T0 + 1200);
    assert!(unsent(&other, "input", "bad_id", Some("t_02")));
    let elsewhere = s.say("t_02", "在吗", Some("sess_9"), None, &[], T0 + 1200);
    assert!(unsent(&elsewhere, "input", "bad_id", Some("t_02")));
    // The same words under their id are the same turn, said once: while it waits, once her
    // host has acked it and not answered, and after it is answered. Nothing, and nothing lost.
    assert!(s.say("t_02", "在吗", None, None, &[], T0 + 1300).is_empty());
    s.received(&host("ping", 2, 2, |_| {}), T0 + 1350);
    assert!(s.say("t_02", "在吗", None, None, &[], T0 + 1360).is_empty());
    s.received(&host("run.accepted", 3, 3, |_| {}), T0 + 1400);
    assert!(s.say("t_02", "在吗", None, None, &[], T0 + 1500).is_empty());
    assert_eq!(s.unfinished().len(), 1, "one turn, not ended by the tries");
    // The other reliable frames of the shell's alike.
    let spoken = fixture("speech.delivered")["body"].clone();
    let no_id = s.delivered("no good", &spoken, T0 + 1600);
    assert!(unsent(&no_id, "speech.delivered", "bad_id", None));
    let touch = json!({ "zone": "head" });
    assert!(unsent(
        &s.touched("tc 1", &touch, T0 + 1600),
        "touch",
        "bad_id",
        None
    ));
    let flat = s.touched("tc_1", &json!("head"), T0 + 1600);
    assert!(unsent(&flat, "touch", "bad_frame", Some("tc_1")));
    // A turn's id is not free for another frame, nor a waiting frame's for a turn.
    assert!(unsent(
        &s.touched("t_02", &touch, T0 + 1650),
        "touch",
        "bad_id",
        Some("t_02")
    ));
    s.disconnected();
    assert!(
        s.delivered("sd_1", &spoken, T0 + 1700).is_empty(),
        "kept for the next link"
    );
    assert!(
        s.delivered("sd_1", &spoken, T0 + 1800).is_empty(),
        "the same again is the same"
    );
    let mut changed = spoken.clone();
    changed["spokenChars"] = json!(0);
    let again = s.delivered("sd_1", &changed, T0 + 1900);
    assert!(unsent(&again, "speech.delivered", "bad_id", Some("sd_1")));
    let taken = s.say("sd_1", "在吗", None, None, &[], T0 + 1950);
    assert!(unsent(&taken, "input", "bad_id", Some("sd_1")));
    // And while the link is down the owner's words are kept, never dropped.
    assert!(
        s.say("t_03", "还在吗", None, None, &[], T0 + 2000)
            .is_empty()
    );
    assert_eq!(s.unfinished().len(), 2);
    // Both waiting frames go on the next link, each once.
    s.connected(T0 + 3000);
    let out = sent(&s.received(&host("welcome", 1, 1, |_| {}), T0 + 3050));
    let types: Vec<&str> = out
        .iter()
        .map(|frame| frame["t"].as_str().unwrap())
        .collect();
    assert_eq!(types, ["speech.delivered", "input", "run.resume"]);
}

#[test]
fn what_the_owner_shows_her_is_part_of_what_was_said() {
    let photo = String::from("data:image/jpeg;base64,/9j/4AAQ");
    let takes_images = |s: &mut Session, at: i64, images: bool| {
        s.connected(at);
        let caps = if images { json!(["images"]) } else { json!([]) };
        s.received(
            &host("welcome", 1, 1, |body| body["capabilities"] = caps),
            at + 50,
        )
    };
    let again = |s: &mut Session, at: i64| {
        s.say("t_02", "你看", None, None, std::slice::from_ref(&photo), at)
    };
    let mut s = session();
    takes_images(&mut s, T0, true);
    assert_eq!(
        sent(&again(&mut s, T0 + 1000))[0]["body"]["images"],
        json!([photo])
    );
    // The same words without the picture, under the same id, are other words.
    let without = s.say("t_02", "你看", None, None, &[], T0 + 1100);
    assert!(matches!(
        delivered(&without)[..],
        [Delivery::Unsent { code, id: Some(id), .. }] if code == "bad_id" && id == "t_02"
    ));
    // The same call again is the same turn on whatever link it is made: while the next
    // link is still greeting, and on one whose host takes no pictures.
    assert!(again(&mut s, T0 + 1200).is_empty());
    s.disconnected();
    s.connected(T0 + 5000);
    assert!(again(&mut s, T0 + 5001).is_empty(), "before the welcome");
    s.disconnected();
    let resent = sent(&takes_images(&mut s, T0 + 9000, false));
    assert_eq!(resent.len(), 1, "the turn itself goes again, once");
    assert!(
        again(&mut s, T0 + 9100).is_empty(),
        "to a host that takes no pictures"
    );
    // And one first said with no link, then again once a host that takes them is there.
    let mut offline = session();
    assert!(
        offline
            .say("t_02", "你看", None, None, std::slice::from_ref(&photo), T0)
            .is_empty()
    );
    takes_images(&mut offline, T0 + 100, true);
    assert!(again(&mut offline, T0 + 200).is_empty());
    assert_eq!(offline.unfinished().len(), 1);
}

#[test]
fn a_request_that_cannot_be_a_frame_is_told_and_one_made_while_down_is_let_go() {
    let told = |out: Vec<Output>, t: &str, code: &str| {
        matches!(
            delivered(&out)[..],
            [Delivery::Unsent { t: got, code: why, id: None }] if got == t && why == code
        )
    };
    let mut s = open();
    let rig = json!({});
    assert!(told(
        s.presence(&json!({}), T0 + 100),
        "presence",
        "bad_frame"
    ));
    assert!(told(
        s.observe_playback("", "x", &rig, T0 + 100),
        "playback.observe",
        "bad_frame"
    ));
    assert!(told(
        s.end_playback("", T0 + 100),
        "playback.end",
        "bad_frame"
    ));
    assert!(told(
        s.appraise_touch("tc 1", &rig, None, T0 + 100),
        "touch.appraise",
        "bad_id"
    ));
    assert!(told(
        s.appraise_touch("tc_1", &json!(1), None, T0 + 100),
        "touch.appraise",
        "bad_frame"
    ));
    assert!(told(s.wear("", T0 + 100), "outfit.wear", "bad_frame"));
    // With no link they are of the moment and let go, as the contract's deliveries say.
    s.disconnected();
    assert!(s.presence(&json!({ "presence": {} }), T0 + 200).is_empty());
    assert!(s.end_playback("run_1", T0 + 200).is_empty());
    assert!(s.wear("coat", T0 + 200).is_empty());
}

#[test]
fn a_line_she_says_is_acked_every_time_and_taken_once() {
    let mut s = open();
    let say = host("say", 2, 1, |_| {});
    let first = s.received(&say, T0 + 2000);
    assert!(matches!(delivered(&first)[..], [Delivery::Say { .. }]));
    assert_eq!(sent(&first)[0]["t"], "ack");
    let again = s.received(&say, T0 + 3000);
    assert!(delivered(&again).is_empty());
    assert_eq!(sent(&again)[0]["t"], "ack");
}

#[test]
fn a_frame_the_core_refuses_carries_nothing() {
    let mut s = open();
    let bad = host("mood", 2, 1, |b| b["unknownField"] = json!(1));
    assert_eq!(
        s.received(&bad, T0 + 2000),
        [Output::Deliver(Delivery::Unread {
            t: Some("mood".into()),
            code: "bad_frame".into()
        })]
    );
    let good = host("mood", 3, 1, |_| {});
    assert!(matches!(
        delivered(&s.received(&good, T0 + 2100))[..],
        [Delivery::Mood(_)]
    ));
}

#[test]
fn a_line_the_core_refuses_is_acked_and_its_id_stays_free() {
    let mut s = open();
    let bad = host("say", 2, 1, |b| b["later"] = json!(true));
    let out = s.received(&bad, T0 + 2000);
    assert!(matches!(
        delivered(&out)[..],
        [Delivery::Unread { t: Some(t), code }] if t == "say" && code == "bad_frame"
    ));
    assert_eq!(sent(&out)[0]["t"], "ack");
    // The same id in a frame this end reads is still taken.
    let good = host("say", 3, 2, |_| {});
    assert!(matches!(
        delivered(&s.received(&good, T0 + 2100))[..],
        [Delivery::Say { .. }]
    ));
}

#[test]
fn a_frame_the_core_will_not_send_leaves_the_outbox() {
    let mut s = open();
    // speech.delivered without the fields the contract requires.
    let out = s.delivered("sd_1", &json!({ "turnId": "t_02" }), T0 + 1000);
    assert!(matches!(
        delivered(&out)[..],
        [Delivery::Unsent { t, code, .. }] if t == "speech.delivered" && code == "bad_frame"
    ));
    s.disconnected();
    s.connected(T0 + 2000);
    assert!(sent(&s.received(&host("welcome", 1, 1, |_| {}), T0 + 2050)).is_empty());
}

#[test]
fn the_heartbeat_pings_and_silence_drops() {
    let mut s = open();
    let beat = contract().timing.heartbeat_ms;
    assert!(s.tick(T0 + 100).is_empty());
    let ping = sent(&s.tick(T0 + 50 + beat));
    assert_eq!(ping[0]["t"], "ping");
    let silence = contract().timing.silence_ms();
    assert_eq!(s.tick(T0 + 50 + silence), vec![Output::Drop(Down::Silent)]);
}

#[test]
fn no_welcome_in_time_is_silence() {
    let mut s = session();
    s.connected(T0);
    let wait = contract().timing.hello_timeout_ms;
    assert!(s.tick(T0 + wait - 1).is_empty());
    assert_eq!(s.tick(T0 + wait), vec![Output::Drop(Down::Silent)]);
}

#[test]
fn what_a_close_means() {
    let c = contract();
    assert_eq!(
        Session::after_close(c, Some(4001), None),
        After::Forget(Removal::Revoked)
    );
    assert_eq!(
        Session::after_close(c, Some(4002), None),
        After::Forget(Removal::OwnerChanged)
    );
    assert_eq!(
        Session::after_close(c, Some(4003), None),
        After::Stop(Down::Replaced)
    );
    assert_eq!(
        Session::after_close(c, Some(4005), None),
        After::Reconnect(Down::HostFrozen)
    );
    assert_eq!(
        Session::after_close(c, None, Some("host_moved")),
        After::Reconnect(Down::HostMoved(None))
    );
    assert_eq!(
        Session::after_close(c, Some(1006), None),
        After::Reconnect(Down::Network)
    );
}

#[test]
fn what_a_refused_upgrade_means() {
    let refusal = |code| Refusal::of(code, Some("MacBook"));
    assert_eq!(
        Session::after_http(401, None),
        After::Forget(Removal::TokenUnknown)
    );
    assert_eq!(
        Session::after_http(401, refusal("device_revoked").as_ref()),
        After::Forget(Removal::Revoked)
    );
    assert_eq!(
        Session::after_http(403, refusal("site_owner_required").as_ref()),
        After::Reconnect(Down::NoOwner)
    );
    assert_eq!(
        Session::after_http(403, refusal("host_moved").as_ref()),
        After::Reconnect(Down::HostMoved(Some("MacBook".into())))
    );
    assert_eq!(
        Session::after_http(302, None),
        After::Stop(Down::Redirected)
    );
    assert_eq!(
        Session::after_http(426, None),
        After::Stop(Down::UnsupportedSite)
    );
    assert_eq!(
        Session::after_http(503, None),
        After::Reconnect(Down::HostUnavailable)
    );
}

#[test]
fn backoff_doubles_to_the_cap() {
    let t = contract().timing;
    assert_eq!(t.backoff_ms(0), t.reconnect_initial_ms);
    assert_eq!(t.backoff_ms(1), t.reconnect_initial_ms * t.reconnect_factor);
    assert_eq!(t.backoff_ms(40), t.reconnect_max_ms);
}

#[test]
fn what_the_owner_shows_her_goes_with_what_they_say_only_to_a_host_that_takes_it() {
    let photo = String::from("data:image/jpeg;base64,/9j/4AAQ");
    // A host that accepted images: the picture goes with the words.
    let mut s = session();
    s.connected(T0);
    s.received(
        &host("welcome", 1, 1, |body| {
            body["capabilities"] = json!(["images"])
        }),
        T0 + 50,
    );
    assert!(s.shows());
    let out = sent(&s.say(
        "t_02",
        "你看",
        None,
        None,
        std::slice::from_ref(&photo),
        T0 + 1000,
    ));
    assert_eq!(out[0]["body"]["images"], json!([photo]));
    // One that did not: the words go alone.
    let mut s = open();
    assert!(!s.shows());
    let out = sent(&s.say("t_02", "你看", None, None, &[photo], T0 + 1000));
    assert_eq!(out[0]["t"], "input");
    assert!(out[0]["body"].get("images").is_none());
}

#[test]
fn her_wardrobe_comes_down_to_choose_what_she_wears_here() {
    let mut s = open();
    let out = s.received(
        &host("wardrobe", 2, 1, |body| {
            body["outfits"].as_array_mut().unwrap().extend([
                json!({ "outfitId": "sketch", "name": "", "profile": "bust", "wearable": true }),
                json!({ "outfitId": "", "name": "无名" }),
            ]);
        }),
        T0 + 100,
    );
    let Delivery::Wardrobe(outfits) = delivered(&out)[0] else {
        panic!("no wardrobe")
    };
    assert_eq!(outfits[0].outfit_id, "default");
    assert_eq!(
        (
            outfits[0].name.as_str(),
            outfits[0].full_body,
            outfits[0].wearable
        ),
        ("日常", false, true)
    );
    assert!(outfits[1].full_body);
    // Wearable only with a rig; named by its id when it has no name; none without an id.
    let sketch = outfits.last().unwrap();
    assert_eq!((sketch.name.as_str(), sketch.wearable), ("sketch", false));
    assert!(outfits.iter().all(|o| !o.outfit_id.is_empty()));
    let wear = sent(&s.wear("coat", T0 + 200));
    assert_eq!(
        (&wear[0]["t"], &wear[0]["body"]["outfitId"]),
        (&json!("outfit.wear"), &json!("coat"))
    );
}

#[test]
fn a_frame_this_end_cannot_read_is_told_and_never_quoted() {
    let mut s = open();
    // A type from a newer contract: told without its name, which is the sender's text.
    let mut newer: Value = serde_json::from_str(&host("mood", 2, 1, |_| {})).unwrap();
    newer["t"] = json!("call.later");
    assert_eq!(
        s.received(&newer.to_string(), T0 + 100),
        [Output::Deliver(Delivery::Unread {
            t: None,
            code: "unknown_frame".into()
        })]
    );
    // Another frame format.
    let mut binary: Value = serde_json::from_str(&host("mood", 3, 1, |_| {})).unwrap();
    binary["v"] = json!(2);
    assert_eq!(
        s.received(&binary.to_string(), T0 + 200),
        [Output::Deliver(Delivery::Unread {
            t: Some("mood".into()),
            code: "version_unsupported".into()
        })]
    );
    // Not an envelope at all.
    assert_eq!(
        s.received("not a frame", T0 + 300),
        [Output::Deliver(Delivery::Unread {
            t: None,
            code: "bad_frame".into()
        })]
    );
    // The link is still up and still counts what it heard.
    assert_eq!(s.phase(), Phase::Open);
    let ping = sent(&s.tick(T0 + 300 + contract().timing.heartbeat_ms));
    assert_eq!(
        (&ping[0]["t"], &ping[0]["ack"]),
        (&json!("ping"), &json!(3))
    );
    // With no socket nothing is told.
    s.disconnected();
    assert!(s.received("not a frame", T0 + 400).is_empty());
}

#[test]
fn a_welcome_this_end_cannot_read_is_told_before_the_link_drops() {
    let mut s = session();
    s.connected(T0);
    let out = s.received(
        &host("welcome", 1, 1, |b| b["later"] = json!(true)),
        T0 + 50,
    );
    assert_eq!(
        out,
        [
            Output::Deliver(Delivery::Unread {
                t: Some("welcome".into()),
                code: "bad_frame".into()
            }),
            Output::Drop(Down::RefusedHandshake)
        ]
    );
}

mod capabilities {
    use std::collections::BTreeSet;

    use super::*;
    use crate::capabilities::{
        BASE_SENDS, BASE_TAKES, Kind, RULES, idle, rule, sends, takes, unread,
    };

    #[test]
    fn every_capability_of_the_contract_has_its_row_and_no_other() {
        let rows: Vec<&str> = RULES.iter().map(|rule| rule.capability).collect();
        let mut named: Vec<&str> = contract().capabilities.iter().map(String::as_str).collect();
        named.sort_unstable();
        assert_eq!(rows, named);
        for rule in RULES {
            assert_eq!(
                rule.kind == Kind::Frames,
                !(rule.sends.is_empty() && rule.takes.is_empty()),
                "{}",
                rule.capability
            );
            for needed in rule.with {
                let needed = crate::capabilities::rule(needed).expect(needed);
                assert!(
                    needed.with.is_empty(),
                    "{} needs a row that needs another",
                    rule.capability
                );
            }
        }
    }

    #[test]
    fn every_frame_a_row_names_is_the_contracts_and_goes_that_way() {
        let from = |t: &str| {
            contract()
                .frames
                .get(t)
                .unwrap_or_else(|| panic!("no frame {t}"))
                .from
                .as_str()
        };
        let up = BASE_SENDS
            .iter()
            .chain(RULES.iter().flat_map(|rule| rule.sends));
        for t in up {
            assert!(
                ["child", "device", "mind", "either"].contains(&from(t)),
                "{t} goes up"
            );
        }
        let down = BASE_TAKES
            .iter()
            .chain(RULES.iter().flat_map(|rule| rule.takes));
        for t in down {
            assert!(
                ["host", "server", "either"].contains(&from(t)),
                "{t} comes down"
            );
        }
    }

    #[test]
    fn what_a_child_says_it_can_do_is_what_this_session_reads() {
        let reads = |t: &str| Session::READS.contains(&t);
        assert_eq!(unread(CAPABILITIES, reads), []);
        // One that says more than the session reads is told which frames no code takes.
        assert_eq!(
            unread(&["act", "say", "cards"], reads),
            [("act", "act.cancel"), ("act", "act.request")],
            "cards brings nothing without conversations.live"
        );
        assert_eq!(unread(&[], |_| false).len(), BASE_TAKES.len());
    }

    #[test]
    fn every_frame_the_session_says_it_reads_brings_something() {
        // Those that need no turn of their own: each fixture is delivered. The run's frames
        // and perform are read in the turn tests above; ping and pong by the heartbeat's.
        let mut s = open();
        let mut seq = 1;
        let alone = [
            "conversation",
            "conversations",
            "error",
            "mood",
            "notice",
            "outfit",
            "say",
            "touch.reaction",
            "wardrobe",
        ];
        let elsewhere = [
            "perform",
            "ping",
            "pong",
            "run.accepted",
            "run.end",
            "run.event",
            "run.rejected",
            "welcome",
        ];
        let mut both: Vec<&str> = alone.iter().chain(&elsewhere).copied().collect();
        both.sort_unstable();
        assert_eq!(
            both,
            Session::READS,
            "a type it says it reads has a test that it does"
        );
        for t in alone {
            seq += 1;
            let out = s.received(&host(t, seq, 1, |_| {}), T0 + 100);
            assert!(
                matches!(delivered(&out)[..], [d] if !matches!(d, Delivery::Unread { .. })),
                "{t}: {out:?}"
            );
        }
        // And a host frame it does not read brings nothing, though the core takes it.
        let known: BTreeSet<&str> = Session::READS.iter().copied().collect();
        let all: Value = serde_json::from_str(FIXTURES).unwrap();
        for frame in all["frames"].as_array().unwrap() {
            let t = frame["t"].as_str().unwrap();
            let host_sent = matches!(contract().frames[t].from.as_str(), "host" | "server");
            if !host_sent || known.contains(t) || frame["body"].get("sealed").is_some() {
                continue;
            }
            seq += 1;
            let mut frame = frame.clone();
            frame["seq"] = json!(seq);
            let out = s.received(&frame.to_string(), T0 + 200);
            assert!(delivered(&out).is_empty(), "{t}: {out:?}");
        }
    }

    #[test]
    fn what_flows_and_what_is_said_to_no_effect() {
        assert!(takes(&["reminders"]).iter().all(|t| BASE_TAKES.contains(t)));
        assert!(takes(&["reminders", "notify"]).contains("notice"));
        assert!(!sends(&["cards"]).contains("card.answer"));
        assert!(sends(&["cards", "conversations.live"]).contains("card.answer"));
        assert!(sends(&["playback"]).contains("speech.delivered"));
        assert!(!sends(&["say"]).contains("speech.delivered"));
        assert_eq!(idle(CAPABILITIES), ["rig", "voice.out", "panel"]);
        assert_eq!(
            idle(&["reminders", "cards", "unknown"]),
            ["reminders", "cards"]
        );
        assert_eq!(rule("pi").map(|rule| rule.kind), Some(Kind::Unheeded));
    }
}

mod state {
    use std::time::Instant;

    use super::*;

    /// The state through its text and back, as an end across the C entry keeps it.
    fn kept(s: &Session) -> (Session, usize) {
        let text = s.state().to_string();
        let back = Session::restore(contract(), &serde_json::from_str(&text).unwrap()).unwrap();
        (back, text.len())
    }

    #[test]
    fn a_session_kept_as_its_state_is_the_same_session() {
        let mut s = open();
        s.say(
            "t_02",
            "今天过得怎么样？",
            Some("sess_1"),
            None,
            &[],
            T0 + 1000,
        );
        s.received(&host("run.accepted", 2, 2, |_| {}), T0 + 1020);
        s.received(&host("run.event", 3, 2, |_| {}), T0 + 1100);
        s.received(&host("mood", 4, 2, |_| {}), T0 + 1150);
        s.received(&host("say", 5, 2, |_| {}), T0 + 1200);
        s.delivered("sd_1", &fixture("speech.delivered")["body"], T0 + 1300);
        let (mut back, _) = kept(&s);
        assert_eq!(back.state(), s.state());
        // And goes on as the one it was kept from: the same frames for the same events.
        for step in 0..3 {
            let now = T0 + 20_000 * (step + 1);
            assert_eq!(back.tick(now), s.tick(now));
            let again = host("say", 6 + step as u64, 2, |_| {});
            assert_eq!(back.received(&again, now + 1), s.received(&again, now + 1));
        }
        s.disconnected();
        back.disconnected();
        assert_eq!(back.connected(T0 + 90_000), s.connected(T0 + 90_000));
        let welcome = host("welcome", 1, 1, |_| {});
        assert_eq!(
            back.received(&welcome, T0 + 90_050),
            s.received(&welcome, T0 + 90_050)
        );
        assert_eq!(back.state(), s.state());
    }

    #[test]
    fn what_is_not_a_state_is_told_without_what_it_holds() {
        let mut state = open().state();
        state["hello"]["app"]["name"] = json!("secret-looking");
        state["later"] = json!(1);
        let why = Session::restore(contract(), &state).err().unwrap();
        assert!(!why.contains("secret") && !why.contains("later"), "{why}");
        let mut other = open().state();
        other["version"] = json!(2);
        assert!(Session::restore(contract(), &other).is_err());
        assert!(Session::restore(contract(), &json!(null)).is_err());
        // Numbers no session reaches, a turn twice, a hello with nothing to say.
        let broken = |edit: &dyn Fn(&mut Value)| {
            let mut state = open().state();
            edit(&mut state);
            Session::restore(contract(), &state).is_err()
        };
        assert!(broken(&|s| s["sent"] = json!(u64::MAX)));
        assert!(broken(&|s| s["sent"] = json!(1u64 << 53)));
        assert!(broken(&|s| s["heard"] = json!((1u64 << 53) + 1)));
        assert!(broken(&|s| s["openedAtMs"] = json!(i64::MIN)));
        assert!(broken(&|s| s["lastHeardMs"] = json!(-1)));
        assert!(broken(&|s| s["heartbeatMs"] = json!(i64::MAX)));
        assert!(broken(&|s| s["heartbeatMs"] = json!(0)));
        assert!(broken(&|s| s["hello"] = json!({})));
        let mut twice = open();
        twice.say("t_02", "在吗", None, None, &[], T0 + 1000);
        assert!(broken(&|s| {
            *s = twice.state();
            let turn = s["turns"][0].clone();
            s["turns"].as_array_mut().unwrap().push(turn);
        }));
    }

    #[test]
    fn a_state_whose_numbers_went_through_doubles_is_the_same_state() {
        let mut s = open();
        s.say("t_02", "在吗", None, None, &[], T0 + 1000);
        s.received(&host("run.accepted", 2, 2, |_| {}), T0 + 1020);
        s.received(&host("run.event", 3, 2, |_| {}), T0 + 1100);
        s.received(
            &host("perform", 4, 2, |b| b["turnId"] = json!("t_02")),
            T0 + 1150,
        );
        s.delivered("sd_1", &fixture("speech.delivered")["body"], T0 + 1200);
        // Every count and time written the way an encoder of doubles writes it: 3 as 3.0.
        // (A waiting frame's body is its own: what an encoder does to the numbers in it goes
        // out on the wire, so an end keeps the state as text when its encoder does that.)
        fn doubled(value: &mut Value) {
            match value {
                Value::Number(n) => *value = json!(n.as_f64().unwrap()),
                Value::Array(items) => items.iter_mut().for_each(doubled),
                Value::Object(fields) => fields
                    .iter_mut()
                    .filter(|(key, _)| *key != "body")
                    .for_each(|(_, field)| doubled(field)),
                _ => {}
            }
        }
        let state = s.state();
        let mut floats = state.clone();
        doubled(&mut floats);
        assert_ne!(floats.to_string(), state.to_string());
        let text = floats.to_string();
        assert!(text.contains("\"sent\":3.0"), "{text}");
        let back = Session::restore(contract(), &serde_json::from_str(&text).unwrap()).unwrap();
        assert_eq!(back.state(), state);
        // A number with a fraction, or a count below zero, is not one.
        for (field, value) in [
            ("sent", json!(2.5)),
            ("heard", json!(-1.0)),
            ("version", json!(1.5)),
        ] {
            let mut broken = state.clone();
            broken[field] = value;
            assert!(Session::restore(contract(), &broken).is_err(), "{field}");
        }
    }

    #[test]
    fn a_hello_no_host_would_take_is_not_kept() {
        let broken = |edit: &dyn Fn(&mut Value)| {
            let mut state = open().state();
            edit(&mut state["hello"]);
            Session::restore(contract(), &state).is_err()
        };
        assert!(broken(&|h| h["lineage"] = json!(7)));
        assert!(broken(&|h| h["hostEpoch"] = json!("three")));
        assert!(broken(&|h| h["hostEpoch"] = json!(-1)));
        assert!(broken(&|h| h["hostEpoch"] = json!(u64::MAX)));
        assert!(broken(&|h| h["app"]["name"] = json!(null)));
        assert!(broken(&|h| h["app"]["extra"] = json!("x")));
        assert!(broken(&|h| h["later"] = json!(true)));
        assert!(!broken(&|h| h["hostEpoch"] = json!(4.0)));
        assert!(!broken(&|h| h["lineage"] = json!("ln_01")));
    }

    #[test]
    fn an_epoch_written_as_a_double_is_said_as_the_count_it_is() {
        let mut state = session().state();
        state["hello"]["lineage"] = json!("ln_01");
        state["hello"]["hostEpoch"] = json!(4.0);
        let mut back = Session::restore(contract(), &state).unwrap();
        let hello = sent(&back.connected(T0)).remove(0);
        assert_eq!(hello["body"]["hostEpoch"].to_string(), "4");
        assert_eq!(hello["body"]["lineage"], "ln_01");
        // A new session does not say an epoch that is not a count.
        let mut odd = Session::new(contract(), CAPABILITIES, ("Child", "1"), None, Some(-1));
        assert!(sent(&odd.connected(T0))[0]["body"]["hostEpoch"].is_null());
        assert!(Session::restore(contract(), &odd.state()).is_ok());
    }

    #[test]
    fn a_turn_kept_before_turns_had_a_mark_is_not_said_twice() {
        let mut s = open();
        s.say("t_02", "在吗", None, None, &[], T0 + 1000);
        let mut state = s.state();
        state["turns"][0].as_object_mut().unwrap().remove("said");
        let mut back = Session::restore(contract(), &state).unwrap();
        assert!(
            back.say("t_02", "在吗", None, None, &[], T0 + 1100)
                .is_empty()
        );
        assert_eq!(back.unfinished().len(), 1);
    }

    #[test]
    fn her_lineage_and_her_epoch_are_each_said_as_given() {
        let said = |s: &mut Session| {
            let hello = sent(&s.connected(T0)).remove(0);
            (
                hello["body"]["lineage"].clone(),
                hello["body"]["hostEpoch"].clone(),
            )
        };
        // The device she lives on says an epoch and no lineage (the contract's own example).
        let lives: Value = serde_json::from_str(FIXTURES).unwrap();
        assert_eq!(lives["closed"][0]["frame"]["body"]["hostEpoch"], 3);
        let mut epoch_only = Session::new(contract(), CAPABILITIES, ("Child", "1"), None, Some(3));
        assert_eq!(said(&mut epoch_only), (Value::Null, json!(3)));
        let mut both = Session::new(
            contract(),
            CAPABILITIES,
            ("Child", "1"),
            Some("ln_01"),
            Some(3),
        );
        assert_eq!(said(&mut both), (json!("ln_01"), json!(3)));
        // And a kept state says what it held.
        let mut state = session().state();
        state["hello"]["hostEpoch"] = json!(4);
        let mut back = Session::restore(contract(), &state).unwrap();
        assert_eq!(said(&mut back), (Value::Null, json!(4)));
        assert_eq!(
            Session::restore(contract(), &back.state()).unwrap().state(),
            back.state()
        );
    }

    #[test]
    fn a_state_whose_nulls_were_dropped_on_the_way_is_still_that_state() {
        let mut s = open();
        s.say("t_02", "在吗", None, None, &[], T0 + 1000);
        s.disconnected();
        s.say("t_03", "还在吗", None, None, &[], T0 + 2000);
        fn without_nulls(value: &mut Value) {
            match value {
                Value::Array(items) => items.iter_mut().for_each(without_nulls),
                Value::Object(fields) => {
                    fields.retain(|_, field| !field.is_null());
                    fields.values_mut().for_each(without_nulls);
                }
                _ => {}
            }
        }
        let state = s.state();
        let mut bare = state.clone();
        without_nulls(&mut bare);
        assert_ne!(bare, state);
        let back = Session::restore(contract(), &bare).unwrap();
        assert_eq!(back.state(), state);
    }

    #[test]
    fn a_state_kept_under_another_contract_says_this_contracts_hello() {
        let mut state = session().state();
        state["hello"]["protocol"] = json!(1);
        state["hello"]["role"] = json!("host");
        state["hello"]["capabilities"] = json!(["say", "from.an.older.contract", 7]);
        let mut back = Session::restore(contract(), &state).unwrap();
        assert_eq!(back.capabilities(), ["say"]);
        let hello = &sent(&back.connected(T0))[0];
        assert_eq!(hello["body"]["protocol"], contract().protocol);
        assert_eq!(hello["body"]["role"], "child");
    }

    #[test]
    fn a_count_no_reader_keeps_exactly_is_no_envelope() {
        let mut s = open();
        let mut far: Value = serde_json::from_str(&host("ping", 2, 1, |_| {})).unwrap();
        far["seq"] = json!(u64::MAX);
        let out = s.received(&far.to_string(), T0 + 100);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Unread { t: None, code }] if code == "bad_frame"
        ));
        assert_eq!(s.state()["heard"], 1);
        // And a clock far off moves nothing past what a number holds.
        assert!(
            s.tick(i64::MAX)
                .iter()
                .any(|o| matches!(o, Output::Drop(_)))
        );
        assert!(s.tick(i64::MIN).is_empty());
    }

    /// What keeping the session as its state costs per frame. Run in release:
    /// `cargo test -p myriad-link-child --release -- --ignored --nocapture cost`.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn the_cost_of_keeping_a_session_as_its_state() {
        // The quietest of several batches: other work on the machine only ever adds.
        let measure = |name: &str, s: &Session| {
            let (batches, rounds) = (7, 50);
            let mut bytes = 0;
            let mut best = f64::MAX;
            let mut worst = 0.0_f64;
            for _ in 0..batches {
                let started = Instant::now();
                for _ in 0..rounds {
                    bytes = kept(s).1;
                }
                let each = started.elapsed().as_secs_f64() * 1e6 / f64::from(rounds);
                best = best.min(each);
                worst = worst.max(each);
            }
            println!(
                "{name}: {bytes} bytes; {best:.0} to {worst:.0} µs per frame, out and back in"
            );
        };
        // Typical: open, a few turns, nothing waiting.
        let mut typical = open();
        for turn in 0..3 {
            let id = format!("t_{turn}");
            typical.say(&id, "今天过得怎么样？", None, None, &[], T0 + 1000);
            typical.received(
                &host("run.accepted", 2 + turn, 2, |b| b["turnId"] = json!(id)),
                T0 + 1020,
            );
        }
        measure("typical", &typical);
        // Full: every turn and every taken id it keeps, nothing waiting.
        let mut full = open();
        for turn in 0..Session::KEPT_TURNS as u64 {
            let id = format!("turn_{turn:058}");
            full.say(&id, "在吗", None, None, &[], T0 + 1000);
            full.received(
                &host("run.accepted", 2 + turn, 2 + turn, |b| {
                    b["turnId"] = json!(id)
                }),
                T0 + 1020,
            );
        }
        for line in 0..Session::KEPT_TAKEN as u64 {
            let mut say: Value = serde_json::from_str(&host("say", 100 + line, 2, |_| {})).unwrap();
            say["id"] = json!(format!("spk_{line:060}"));
            full.received(&say.to_string(), T0 + 2000);
        }
        measure("full", &full);
        // Worst: that, and the link down with pictures waiting to go (two of 120 KB each in
        // one input is about what a frame holds; four such inputs said while offline).
        let mut worst = Session::restore(contract(), &full.state()).unwrap();
        worst.disconnected();
        worst.connected(T0 + 2900);
        worst.received(
            &host("welcome", 1, 1, |b| b["capabilities"] = json!(["images"])),
            T0 + 3000,
        );
        assert!(worst.shows());
        worst.disconnected();
        let picture = format!("data:image/jpeg;base64,{}", "A".repeat(120_000));
        for turn in 0..4 {
            let id = format!("offline_{turn}");
            worst.say(
                &id,
                "你看",
                None,
                None,
                &[picture.clone(), picture.clone()],
                T0 + 4000,
            );
        }
        measure("worst", &worst);
    }
}
