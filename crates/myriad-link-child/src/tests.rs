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
fn a_line_the_core_refuses_is_acked_so_her_host_counts_it_delivered() {
    let mut s = open();
    let bad = host("say", 2, 1, |b| b["later"] = json!(true));
    let out = s.received(&bad, T0 + 2000);
    assert!(matches!(
        delivered(&out)[..],
        [Delivery::Unread { t: Some(t), code }] if t == "say" && code == "bad_frame"
    ));
    // The ack that goes back covers the frame it could not read (seq 2): by the link's
    // rules her host has delivered it, and will not send it again.
    let ack = &sent(&out)[0];
    assert_eq!((&ack["t"], &ack["ack"]), (&json!("ack"), &json!(2)));
    // Should her host send the line anew under the same id, it is taken: the id was not kept.
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
            "call.granted",
            "conversation",
            "conversations",
            "error",
            "link.verdict",
            "link.versions",
            "mood",
            "notice",
            "outfit",
            "say",
            "touch.reaction",
            "wardrobe",
        ];
        let elsewhere = [
            "call.accepted",
            "call.ask",
            "call.cancel",
            "call.event",
            "call.progress",
            "call.result",
            "perform",
            "ping",
            "pong",
            "run.accepted",
            "run.end",
            "run.event",
            "run.rejected",
            "stream.event",
            "stream.state",
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

mod calls {
    use super::*;

    fn offering(capabilities: &[&str]) -> Session {
        let mut s = Session::new(contract(), capabilities, ("Child", "0.1.0"), None, None);
        // In the contract's words only: not another version, not her host's own, not one
        // the contract does not name; each once.
        s.offer(&[
            ("screen.look", 1),
            ("notify.post", 1),
            ("screen.look", 1),
            ("screen.look", 2),
            ("model.chat", 1),
            ("made.up", 1),
        ]);
        s.runs(4, "1111111111111111111111111111111111111111");
        s
    }

    fn welcomed(s: &mut Session, accepted: &[&str]) -> Vec<Value> {
        s.connected(T0);
        sent(&s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(accepted)
            }),
            T0 + 50,
        ))
    }

    #[test]
    fn a_new_link_is_told_what_this_end_runs_and_may_be_asked() {
        let mut s = offering(&["calls", "versions"]);
        let out = welcomed(&mut s, &["calls", "versions"]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["t"], "link.versions");
        assert_eq!(
            out[0]["body"],
            json!({
                "contract": contract().schema_version,
                "core": { "abi": 4, "commit": "1111111111111111111111111111111111111111" },
                "app": { "name": "Child", "version": "0.1.0" },
                "actions": { "notify.post": 1, "screen.look": 1 },
            })
        );
        assert_eq!(out[1]["t"], "call.offer");
        assert_eq!(
            out[1]["body"]["actions"],
            json!([{ "name": "notify.post", "version": 1 }, { "name": "screen.look", "version": 1 }])
        );
        // Only what both ends listed; and nothing of versions from an end that does not say
        // what it runs.
        let mut s = offering(&["calls", "versions"]);
        assert!(welcomed(&mut s, &[]).is_empty());
        let mut s = offering(&["calls"]);
        let only: Vec<Value> = welcomed(&mut s, &["calls", "versions"]);
        assert_eq!(only.len(), 1);
        assert_eq!(only[0]["t"], "call.offer");
        let mut quiet = Session::new(contract(), &["versions"], ("Child", "1"), None, None);
        assert!(welcomed(&mut quiet, &["versions"]).is_empty());
    }

    #[test]
    fn what_this_end_offers_and_runs_is_kept_in_its_state() {
        let mut s = offering(&["calls", "versions"]);
        let state = s.state();
        let mut back = Session::restore(contract(), &state).unwrap();
        assert_eq!(back.state(), state);
        assert_eq!(
            welcomed(&mut back, &["calls", "versions"]),
            welcomed(&mut s, &["calls", "versions"])
        );
        // Written as doubles, and from before a session kept them.
        let mut doubled = state.clone();
        doubled["offers"][0]["version"] = json!(1.0);
        doubled["core"]["abi"] = json!(4.0);
        assert_eq!(
            Session::restore(contract(), &doubled).unwrap().state(),
            state
        );
        let mut before = state.clone();
        before.as_object_mut().unwrap().remove("offers");
        before.as_object_mut().unwrap().remove("core");
        let mut old = Session::restore(contract(), &before).unwrap();
        assert!(welcomed(&mut old, &["versions"]).is_empty());
        let mut far = state.clone();
        far["core"]["abi"] = json!(u64::MAX);
        assert!(Session::restore(contract(), &far).is_err());
    }

    #[test]
    fn a_state_kept_across_an_update_offers_by_the_contract_it_is_restored_under() {
        let mut state = offering(&["calls", "versions"]).state();
        // As a state kept under an older contract might hold them: an action since dropped,
        // one at a version no longer done, one her host does, one twice.
        state["offers"] = json!([
            { "name": "gone.action", "version": 9 },
            { "name": "screen.look", "version": 7 },
            { "name": "model.chat", "version": 1 },
            { "name": "screen.look", "version": 1 },
            { "name": "screen.look", "version": 1 },
        ]);
        let mut back = Session::restore(contract(), &state).unwrap();
        let out = welcomed(&mut back, &["calls"]);
        assert_eq!(
            out[0]["body"]["actions"],
            json!([{ "name": "screen.look", "version": 1 }])
        );
        // And the end says again what it runs now: the state's own word is the old one.
        assert_eq!(
            back.running(),
            Some((4, "1111111111111111111111111111111111111111"))
        );
        back.runs(5, "2222222222222222222222222222222222222222");
        back.disconnected();
        let out = welcomed(&mut back, &["versions"]);
        assert_eq!(
            out[0]["body"]["core"],
            json!({ "abi": 5, "commit": "2222222222222222222222222222222222222222" })
        );
    }

    /// Open on a link where both listed calls, offering three actions; her host's clock is
    /// `ahead` of the shell's.
    fn called(ahead: i64) -> Session {
        let mut s = Session::new(contract(), &["calls"], ("Child", "1"), None, None);
        s.offer(&[("screen.look", 1), ("notify.post", 1), ("work.run", 1)]);
        s.connected(T0);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls"]);
                body["serverTime"] = json!(T0 + 50 + ahead);
            }),
            T0 + 50,
        );
        // Her host says what is granted on this link: all three.
        let granted = host("call.granted", 2, 2, |body| {
            body["actions"] = json!([
                { "name": "screen.look", "version": 1 },
                { "name": "notify.post", "version": 1 },
                { "name": "work.run", "version": 1 },
            ]);
        });
        s.received(&granted, T0 + 60);
        s
    }

    /// `call.ask` for `screen.look`, sent by her host at `at` by its own clock.
    fn ask(call_id: &str, seq: u64, at: i64, deadline_at: i64) -> String {
        let mut frame = fixture("call.ask");
        frame["seq"] = json!(seq);
        frame["id"] = json!(call_id);
        frame["at"] = json!(at);
        frame["body"]["callId"] = json!(call_id);
        frame["body"]["deadlineAt"] = json!(deadline_at);
        frame.to_string()
    }

    #[test]
    fn a_deadline_is_read_by_her_hosts_clock() {
        // Her host's clock runs an hour ahead of the shell's.
        let hour = 3_600_000;
        let mut s = called(hour);
        let out = s.received(
            &ask("c_1", 2, T0 + hour + 1000, T0 + hour + 9000),
            T0 + 1000,
        );
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Call { deadline_at, .. }] if *deadline_at == T0 + hour + 9000
        ));
        assert!(delivered(&s.tick(T0 + 8999)).is_empty());
        let out = s.tick(T0 + 9000);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::CallStop { why, .. }] if why == "timeout"
        ));
        // One whose deadline is past by her clock, though not by the shell's, is not done.
        let out = s.received(
            &ask("c_2", 3, T0 + hour + 9500, T0 + hour + 9400),
            T0 + 9500,
        );
        assert!(delivered(&out).is_empty());
        assert_eq!(sent(&out)[1]["body"]["error"]["code"], "call_timeout");
    }

    #[test]
    fn a_deadline_further_than_its_action_may_be_given_is_cut_short_not_refused() {
        let hour = 3_600_000;
        let mut s = called(hour);
        // screen.look, given two minutes: held to the most a call that is not long may be.
        let out = s.received(
            &ask("c_1", 2, T0 + hour + 1000, T0 + hour + 1000 + 120_000),
            T0 + 1000,
        );
        let most = myriad_her_link::call_most_ms("screen.look") as i64;
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Call { deadline_at, .. }] if *deadline_at == T0 + hour + 1000 + most
        ));
        // As far as a number every end reads exactly (2^53 - 1): cut short all the same.
        let mut s = called(hour);
        let far = (1_i64 << 53) - 1;
        let out = s.received(&ask("c_2", 2, T0 + hour + 2000, far), T0 + 2000);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Call { deadline_at, .. }] if *deadline_at == T0 + hour + 2000 + most
        ));
    }

    #[test]
    fn a_deadline_passes_with_the_link_down_and_the_answer_waits_for_it() {
        let mut s = called(0);
        s.received(&ask("c_1", 2, T0 + 1000, T0 + 9000), T0 + 1000);
        s.disconnected();
        let out = s.tick(T0 + 9000);
        assert!(sent(&out).is_empty());
        assert!(matches!(delivered(&out)[..], [Delivery::CallStop { .. }]));
        s.connected(T0 + 20_000);
        let out = sent(&s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls"])
            }),
            T0 + 20_050,
        ));
        let result = out
            .iter()
            .find(|frame| frame["t"] == "call.result")
            .unwrap();
        assert_eq!(result["id"], "c_1.result");
        assert_eq!(result["body"]["error"]["code"], "call_timeout");
    }

    #[test]
    fn a_call_in_hand_and_its_answer_are_kept_in_the_state() {
        let mut s = called(0);
        s.received(&ask("c_1", 2, T0 + 1000, T0 + 9000), T0 + 1000);
        s.received(&ask("c_2", 3, T0 + 1100, T0 + 9000), T0 + 1100);
        let facts = json!({ "facts": { "app": "Xcode" } });
        s.call_done("c_1", &facts, &[], T0 + 1500);
        let state = s.state();
        let mut back = Session::restore(contract(), &state).unwrap();
        assert_eq!(back.state(), state);
        // The one in hand is still answered once; the one answered is answered the same.
        let again = ask("c_1", 4, T0 + 2000, T0 + 9000);
        assert_eq!(
            back.received(&again, T0 + 2000),
            s.received(&again, T0 + 2000)
        );
        assert_eq!(
            back.call_done("c_2", &facts, &[], T0 + 2100),
            s.call_done("c_2", &facts, &[], T0 + 2100)
        );
        assert_eq!(back.state(), s.state());
        // Numbers no call has are not a state.
        let mut far = state.clone();
        far["doing"][0]["deadlineAt"] = json!(-1);
        assert!(Session::restore(contract(), &far).is_err());
        let mut far = state.clone();
        far["hostAheadMs"] = json!(i64::MIN);
        assert!(Session::restore(contract(), &far).is_err());
        // A state from before a session kept calls.
        let mut before = state.clone();
        for key in ["doing", "answered", "hostAheadMs"] {
            before.as_object_mut().unwrap().remove(key);
        }
        assert!(Session::restore(contract(), &before).is_ok());
    }

    #[test]
    fn an_ask_this_end_cannot_read_is_acked_and_not_done() {
        let mut s = called(0);
        let mut later: Value = serde_json::from_str(&ask("c_1", 2, T0 + 1000, T0 + 9000)).unwrap();
        later["body"]["later"] = json!(true);
        let out = s.received(&later.to_string(), T0 + 1000);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Unread { t: Some(t), .. }] if t == "call.ask"
        ));
        let frames = sent(&out);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["t"], "ack");
        // Only the answers kept are answered again: once her host has acked them, the
        // oldest of many is forgotten, and asked again is a call like any other.
        let mut s = called(0);
        for n in 0..=Session::KEPT_ANSWERS as u64 {
            let id = format!("c_{n}");
            s.received(&ask(&id, 3 + n, T0 + 1000, T0 + 9000), T0 + 1000);
            s.call_failed(&id, "not_now", T0 + 1001);
        }
        s.received(&host("ping", 400, 10_000, |_| {}), T0 + 1001);
        s.received(&ask("c_last", 401, T0 + 1000, T0 + 9000), T0 + 1001);
        s.call_failed("c_last", "not_now", T0 + 1001);
        let out = s.received(&ask("c_0", 500, T0 + 1000, T0 + 9000), T0 + 1002);
        assert!(matches!(delivered(&out)[..], [Delivery::Call { .. }]));
        let out = s.received(&ask("c_last", 501, T0 + 1000, T0 + 9000), T0 + 1003);
        assert!(delivered(&out).is_empty());
    }

    #[test]
    fn calls_in_hand_when_it_was_restarted_are_answered_unknown_whatever_the_session_kept() {
        // The end kept its session's state across the restart: the call is still in it.
        let mut before = called(0);
        before.received(&ask("c_1", 3, T0 + 1000, T0 + 9000), T0 + 1000);
        let mut s = Session::restore(contract(), &before.state()).unwrap();
        let out = s.lost(&[("c_1", "screen.look", 1)], T0 + 2000);
        let frames = sent(&out);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["id"], "c_1.result");
        assert_eq!(frames[0]["body"]["outcome"], "unknown");
        // It is not in hand any more: no deadline stops it later, and asked again it is
        // answered unknown, not handed to the shell.
        assert!(
            s.tick(T0 + 9000)
                .iter()
                .all(|o| !matches!(o, Output::Deliver(_)))
        );
        let out = s.received(&ask("c_1", 4, T0 + 3000, T0 + 9000), T0 + 3000);
        assert!(delivered(&out).is_empty());
        // One already answered stands as it was answered.
        let out = s.lost(&[("c_1", "screen.look", 1)], T0 + 4000);
        assert!(matches!(delivered(&out)[..], [Delivery::Unsent { code, .. }] if code == "bad_id"));
    }

    #[test]
    fn a_call_handed_back_as_lost_is_not_done_again_though_a_kept_state_still_held_it() {
        let grant = |s: &mut Session, seq: u64, at: i64| {
            let all = json!([{ "name": "screen.look", "version": 1 }]);
            s.received(
                &host("call.granted", seq, 2, |body| body["actions"] = all),
                at,
            )
        };
        let welcome = |s: &mut Session, at: i64| {
            s.connected(at);
            s.received(
                &host("welcome", 1, 1, |body| {
                    body["capabilities"] = json!(["calls"]);
                    body["serverTime"] = json!(at + 50);
                }),
                at + 50,
            )
        };
        // The call waits for her host's word; the state is kept here.
        let mut s = called(0);
        s.disconnected();
        welcome(&mut s, T0 + 5000);
        s.received(&ask("c_x", 2, T0 + 6000, T0 + 60_000), T0 + 6000);
        let kept = s.state();
        // Then the word comes, the shell is handed the call and writes it down, and the
        // process dies before that step's state is kept.
        let out = grant(&mut s, 3, T0 + 6100);
        assert!(matches!(delivered(&out)[..], [_, Delivery::Call { .. }]));
        // Restarted from the kept state: the shell hands the call back before it connects.
        let mut s = Session::restore(contract(), &kept).unwrap();
        s.disconnected();
        assert!(
            s.lost(&[("c_x", "screen.look", 1)], T0 + 9000).is_empty(),
            "queued for the link"
        );
        let out = welcome(&mut s, T0 + 10_000);
        let results: Vec<Value> = sent(&out)
            .into_iter()
            .filter(|f| f["t"] == "call.result")
            .collect();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["body"]["outcome"], "unknown");
        // Her host's word on the new link hands nothing over, and nothing more is answered.
        let out = grant(&mut s, 2, T0 + 10_100);
        assert!(matches!(delivered(&out)[..], [Delivery::Granted { .. }]));
        assert!(sent(&out).iter().all(|f| f["t"] != "call.result"));
        let done = s.call_done("c_x", &json!({ "facts": {} }), &[], T0 + 10_200);
        assert!(sent(&done).is_empty());
    }

    #[test]
    fn an_answer_her_host_has_not_acked_is_never_forgotten() {
        // More calls answered than answers are kept, and her host has acked none of them.
        let mut s = called(0);
        let many = Session::KEPT_ANSWERS as u64 + 10;
        for n in 0..many {
            let id = format!("c_{n}");
            s.received(&ask(&id, 3 + n, T0 + 1000, T0 + 50_000), T0 + 1000);
            s.call_failed(&id, "not_now", T0 + 1001);
        }
        // (The asks themselves ack the first few results; the rest wait.)
        let state = s.state();
        let waiting: Vec<String> = state["outbox"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|pending| pending["t"] == "call.result")
            .map(|pending| pending["body"]["callId"].as_str().unwrap().to_string())
            .collect();
        assert!(
            waiting.len() > Session::KEPT_ANSWERS / 2,
            "{}",
            waiting.len()
        );
        let kept: Vec<&str> = state["answered"]
            .as_array()
            .unwrap()
            .iter()
            .map(|answer| answer["callId"].as_str().unwrap())
            .collect();
        assert!(waiting.iter().all(|id| kept.contains(&id.as_str())));
        // The oldest of those asked again is answered the same, not done again.
        let out = s.received(&ask(&waiting[0], 500, T0 + 1002, T0 + 50_000), T0 + 1002);
        assert!(delivered(&out).is_empty());
        // Once her host has acked them, only so many are kept.
        let acked = host("ping", 501, 10_000, |_| {});
        s.received(&acked, T0 + 1003);
        s.received(&ask("c_new", 502, T0 + 1004, T0 + 50_000), T0 + 1004);
        s.call_failed("c_new", "not_now", T0 + 1005);
        let kept = s.state()["answered"].as_array().unwrap().len();
        assert!(kept <= Session::KEPT_ANSWERS + 1, "{kept}");
    }

    #[test]
    fn a_grant_decides_only_where_calls_was_agreed_and_only_for_what_is_offered_now() {
        // A call waits; the next link's host did not list calls, and says granted anyway.
        let mut s = called(0);
        s.disconnected();
        s.connected(T0 + 5000);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls"])
            }),
            T0 + 5050,
        );
        s.received(&ask("c_1", 2, T0 + 5100, T0 + 60_000), T0 + 5100);
        s.disconnected();
        s.connected(T0 + 6000);
        s.received(&host("welcome", 1, 1, |_| {}), T0 + 6050);
        let all = json!([{ "name": "screen.look", "version": 1 }]);
        let out = s.received(
            &host("call.granted", 2, 1, |body| body["actions"] = all.clone()),
            T0 + 6100,
        );
        assert!(
            matches!(delivered(&out)[..], [Delivery::Granted { .. }]),
            "{out:?}"
        );
        // What this end no longer offers is not done, though her host grants it.
        let mut s = called(0);
        s.disconnected();
        s.connected(T0 + 5000);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls"])
            }),
            T0 + 5050,
        );
        s.received(&ask("c_2", 2, T0 + 5100, T0 + 60_000), T0 + 5100);
        s.offer(&[("notify.post", 1)]);
        let out = s.received(
            &host("call.granted", 3, 2, |body| body["actions"] = all),
            T0 + 5200,
        );
        assert!(matches!(delivered(&out)[..], [Delivery::Granted { .. }]));
        let result = sent(&out)
            .into_iter()
            .find(|f| f["t"] == "call.result")
            .unwrap();
        assert_eq!(result["body"]["error"]["code"], "call_unknown");
    }

    #[test]
    fn a_long_call_asked_again_while_in_hand_is_said_to_be_in_hand_again() {
        let mut s = called(0);
        let mut work: Value =
            serde_json::from_str(&ask("w_1", 3, T0 + 1000, T0 + 600_000)).unwrap();
        work["body"]["action"] = json!("work.run");
        work["body"]["args"] = json!({ "proposalId": "p", "title": "t", "instruction": "i" });
        let first = s.received(&work.to_string(), T0 + 1000);
        assert_eq!(delivered(&first).len(), 1);
        let types = |out: &[Output]| -> Vec<String> {
            sent(out)
                .iter()
                .map(|f| f["t"].as_str().unwrap().to_string())
                .collect()
        };
        assert_eq!(types(&first), ["ack", "call.accepted"]);
        work["seq"] = json!(4);
        let again = s.received(&work.to_string(), T0 + 2000);
        assert!(delivered(&again).is_empty(), "not done again");
        assert_eq!(types(&again), ["ack", "call.accepted"]);
        // The pieces of its result are numbered whether or not they went.
        assert_eq!(
            sent(&s.call_piece("w_1", &json!("a"), T0 + 2100))[0]["body"]["sequence"],
            1
        );
        s.disconnected();
        assert!(s.call_piece("w_1", &json!("b"), T0 + 2200).is_empty());
        s.connected(T0 + 3000);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls"])
            }),
            T0 + 3050,
        );
        assert_eq!(
            sent(&s.call_piece("w_1", &json!("c"), T0 + 3100))[0]["body"]["sequence"],
            3
        );
    }

    #[test]
    fn a_call_is_taken_only_where_calls_was_agreed_and_granted() {
        // A link where her host did not accept calls: an ask passes by, as before.
        let mut s = Session::new(contract(), &["calls"], ("Child", "1"), None, None);
        s.offer(&[("screen.look", 1)]);
        s.connected(T0);
        s.received(&host("welcome", 1, 1, |_| {}), T0 + 50);
        assert!(
            s.received(&ask("c_1", 2, T0 + 1000, T0 + 9000), T0 + 1000)
                .is_empty()
        );
        // A link where it did, and nothing is granted yet; then granted; then a new link.
        let mut s = called(0);
        s.received(
            &host("call.granted", 3, 2, |body| body["actions"] = json!([])),
            T0 + 100,
        );
        let out = s.received(&ask("c_1", 4, T0 + 1000, T0 + 9000), T0 + 1000);
        assert!(delivered(&out).is_empty());
        assert_eq!(sent(&out)[1]["body"]["error"]["code"], "call_not_granted");
        // Only this link's own word on what is granted decides. Her host sent an ask into a
        // link that was already gone, and sends it again on the new one before it has said
        // what is granted there: the call waits, neither done by the old grant nor refused.
        let reconnect = |s: &mut Session, at: i64| {
            s.disconnected();
            s.connected(at);
            s.received(
                &host("welcome", 1, 1, |body| {
                    body["capabilities"] = json!(["calls"]);
                    body["serverTime"] = json!(at + 50);
                }),
                at + 50,
            );
        };
        let grant = |s: &mut Session, seq: u64, actions: Value, at: i64| {
            s.received(
                &host("call.granted", seq, 2, |body| body["actions"] = actions),
                at,
            )
        };
        let look = json!([{ "name": "screen.look", "version": 1 }]);
        let mut s = called(0);
        reconnect(&mut s, T0 + 5000);
        let out = s.received(&ask("c_2", 2, T0 + 6000, T0 + 9000), T0 + 6000);
        assert!(delivered(&out).is_empty(), "not handed to the shell");
        let frames = sent(&out);
        assert_eq!(frames.len(), 1, "and not answered");
        assert_eq!(frames[0]["t"], "ack");
        // Asked again while it waits: it waits on.
        let out = s.received(&ask("c_2", 3, T0 + 6050, T0 + 9000), T0 + 6050);
        assert!(delivered(&out).is_empty() && sent(&out).len() == 1);
        // Her host says it is granted here too: now it is the shell's, once.
        let out = grant(&mut s, 4, look.clone(), T0 + 6100);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Granted { .. }, Delivery::Call { call_id, .. }] if call_id == "c_2"
        ));
        // The owner took the action back while the link was down: the call her host sends
        // again is refused when the new link says what is granted, and the shell never hears.
        let mut s = called(0);
        reconnect(&mut s, T0 + 5000);
        s.received(&ask("c_3", 2, T0 + 6000, T0 + 9000), T0 + 6000);
        let out = grant(&mut s, 3, json!([]), T0 + 6100);
        assert!(matches!(delivered(&out)[..], [Delivery::Granted { .. }]));
        let result = sent(&out)
            .into_iter()
            .find(|f| f["t"] == "call.result")
            .unwrap();
        assert_eq!(result["id"], "c_3.result");
        assert_eq!(result["body"]["error"]["code"], "call_not_granted");
        // One that waits is still held to its deadline, and can be cancelled; the shell
        // was never asked, so it is not told to stop.
        let mut s = called(0);
        reconnect(&mut s, T0 + 5000);
        s.received(&ask("c_4", 2, T0 + 6000, T0 + 9000), T0 + 6000);
        s.received(&ask("c_5", 3, T0 + 6000, T0 + 9000), T0 + 6000);
        let cancel = host("call.cancel", 4, 2, |body| body["callId"] = json!("c_5"));
        let out = s.received(&cancel, T0 + 6500);
        assert!(delivered(&out).is_empty());
        assert_eq!(sent(&out)[0]["body"]["error"]["code"], "call_cancelled");
        assert!(
            sent(&s.tick(T0 + 8999))
                .iter()
                .all(|f| f["t"] != "call.result")
        );
        let out = s.tick(T0 + 9000);
        assert!(delivered(&out).is_empty());
        let result = sent(&out)
            .into_iter()
            .find(|f| f["t"] == "call.result")
            .unwrap();
        assert_eq!(result["body"]["error"]["code"], "call_timeout");
        // A granted that comes after says nothing more of them.
        let out = grant(&mut s, 5, look.clone(), T0 + 9100);
        assert!(matches!(delivered(&out)[..], [Delivery::Granted { .. }]));
        // One that waits when the link goes waits for the next link's word, and is judged
        // by that; kept with the state meanwhile.
        let mut s = called(0);
        reconnect(&mut s, T0 + 5000);
        s.received(&ask("c_6", 2, T0 + 6000, T0 + 30_000), T0 + 6000);
        let mut s = Session::restore(contract(), &s.state()).unwrap();
        reconnect(&mut s, T0 + 10_000);
        let out = grant(&mut s, 2, look, T0 + 10_100);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Granted { .. }, Delivery::Call { call_id, .. }] if call_id == "c_6"
        ));
        // A session that has heard nothing granted at all holds too, and does nothing.
        let mut fresh = Session::new(contract(), &["calls"], ("Child", "1"), None, None);
        fresh.offer(&[("screen.look", 1)]);
        fresh.connected(T0);
        fresh.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls"])
            }),
            T0 + 50,
        );
        let out = fresh.received(&ask("c_7", 2, T0 + 1000, T0 + 9000), T0 + 1000);
        assert!(delivered(&out).is_empty() && sent(&out).len() == 1);
    }

    #[test]
    fn an_answer_as_large_as_a_frame_holds_is_refused_before_it_is_kept() {
        let mut s = called(0);
        s.received(&ask("c_1", 3, T0 + 1000, T0 + 9000), T0 + 1000);
        // Near the frame's limit: it would pass as a short frame and not as the one it is.
        let summary = "x".repeat(261_958);
        let out = s.call_done(
            "c_1",
            &json!({ "facts": {}, "summary": summary }),
            &[],
            T0 + 2000,
        );
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Unsent { code, .. }] if code == "too_large"
        ));
        let frames = sent(&out);
        assert_eq!(frames.len(), 1, "her host is answered all the same");
        assert_eq!(frames[0]["body"]["error"]["code"], "internal_error");
        // An id that ends as a call's answer does is not a turn's or another frame's.
        let unsent = |out: Vec<Output>| matches!(delivered(&out)[..], [Delivery::Unsent { code, .. }] if code == "bad_id");
        assert!(unsent(s.say(
            "c_9.result",
            "在吗",
            None,
            None,
            &[],
            T0 + 3000
        )));
        assert!(unsent(s.touched("c_9.result", &json!({}), T0 + 3000)));
    }

    #[test]
    fn what_the_shell_says_of_a_call_is_held_to_the_contract() {
        let mut s = called(0);
        s.received(&ask("c_1", 2, T0 + 1000, T0 + 9000), T0 + 1000);
        // A code that is not one of the contract's: told, and her host hears internal_error.
        let out = s.call_failed("c_1", "because", T0 + 1100);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Unsent { t, code, id: Some(id) }]
                if t == "call.result" && code == "bad_frame" && id == "c_1"
        ));
        assert_eq!(sent(&out)[0]["body"]["error"]["code"], "internal_error");
        // Of a call it does not have, nothing goes.
        for out in [
            s.call_progress("c_9", "running", false, T0 + 1200),
            s.call_piece("c_9", &json!("x"), T0 + 1200),
            s.call_failed("c_9", "not_now", T0 + 1200),
            s.call_done("c_9", &json!({}), &[], T0 + 1200),
        ] {
            assert!(sent(&out).is_empty());
            assert!(
                matches!(delivered(&out)[..], [Delivery::Unsent { code, .. }] if code == "bad_id")
            );
        }
    }

    #[test]
    fn her_host_is_told_what_this_end_drops_once_a_kind_and_never_what_it_carried() {
        let mut s = Session::new(contract(), &["streams"], ("Child", "1"), None, None);
        welcomed(&mut s, &["streams"]);
        let later = |seq: u64, secret: &str| {
            let mut frame: Value = serde_json::from_str(&host("mood", seq, 1, |_| {})).unwrap();
            frame["t"] = json!("call.later");
            frame["body"]["secret"] = json!(secret);
            frame.to_string()
        };
        let out = s.received(&later(2, "私话"), T0 + 100);
        let frames = sent(&out);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["t"], "stream.event");
        assert_eq!(
            frames[0]["body"]["body"],
            json!({ "t": "call.later", "reason": "unknown_frame" })
        );
        assert!(!frames[0].to_string().contains("私话"));
        assert_eq!(frames[0]["id"], format!("ld_{T0}_1"));
        // The same kind again: the shell is told, her host is not.
        let out = s.received(&later(3, "x"), T0 + 200);
        assert!(sent(&out).is_empty());
        assert_eq!(delivered(&out).len(), 1);
        // What is told is kept with the session, so a restored one does not tell it twice.
        let mut back = Session::restore(contract(), &s.state()).unwrap();
        assert!(sent(&back.received(&later(4, "x"), T0 + 300)).is_empty());
        // Every refusal of the core's that is not one of the contract's reasons is bad_frame.
        let mut odd: Value = serde_json::from_str(&host("say", 5, 1, |_| {})).unwrap();
        odd["id"] = json!("not an id");
        let out = s.received(&odd.to_string(), T0 + 400);
        let told = sent(&out);
        let event = told.iter().find(|f| f["t"] == "stream.event").unwrap();
        assert_eq!(
            event["body"]["body"],
            json!({ "t": "say", "reason": "bad_frame" })
        );
        // A link where her host did not list streams: only the shell.
        let mut quiet = Session::new(contract(), &["streams"], ("Child", "1"), None, None);
        welcomed(&mut quiet, &[]);
        let out = quiet.received(&later(2, "x"), T0 + 100);
        assert!(sent(&out).is_empty());
        assert_eq!(delivered(&out).len(), 1);
    }

    #[test]
    fn a_report_of_a_dropped_frame_has_an_id_of_its_own_and_is_told_once() {
        let unknown = |seq: u64, t: &str| {
            let mut frame: Value = serde_json::from_str(&host("mood", seq, 0, |_| {})).unwrap();
            frame["t"] = json!(t);
            frame.to_string()
        };
        let ids = |out: &[Output]| -> Vec<String> {
            sent(out)
                .iter()
                .filter(|frame| frame["t"] == "stream.event")
                .map(|frame| frame["id"].as_str().unwrap().to_string())
                .collect()
        };
        let mut s = Session::new(contract(), &["streams"], ("Child", "1"), None, None);
        welcomed(&mut s, &["streams"]);
        // Her host never acks: both reports wait.
        let first = ids(&s.received(&unknown(2, "a.later"), T0 + 100));
        let second = ids(&s.received(&unknown(3, "b.later"), T0 + 200));
        // A second link that came up at the very same time by the shell's clock.
        s.disconnected();
        let again: Vec<String> = welcomed(&mut s, &["streams"])
            .iter()
            .filter(|frame| frame["t"] == "stream.event")
            .map(|frame| frame["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            again,
            [first.clone(), second.clone()].concat(),
            "they go again"
        );
        // Told again on this link, the two that wait are not told twice; a new kind has an
        // id no other report has.
        assert!(ids(&s.received(&unknown(2, "a.later"), T0 + 300)).is_empty());
        let third = ids(&s.received(&unknown(3, "c.later"), T0 + 400));
        assert_eq!(third.len(), 1);
        assert!(!first.contains(&third[0]) && !second.contains(&third[0]));
        // Nor one a turn of the shell's has.
        let mut s = Session::new(contract(), &["streams"], ("Child", "1"), None, None);
        welcomed(&mut s, &["streams"]);
        let taken = format!("ld_{T0}_1");
        s.say(&taken, "在吗", None, None, &[], T0 + 50);
        assert_eq!(
            ids(&s.received(&unknown(2, "a.later"), T0 + 100)),
            [format!("ld_{T0}_2")]
        );
    }

    #[test]
    fn a_frame_that_is_no_envelope_is_told_by_its_type_when_it_came_with_one() {
        let mut s = Session::new(contract(), &["streams"], ("Child", "1"), None, None);
        welcomed(&mut s, &["streams"]);
        let told = |out: &[Output]| -> Vec<Value> {
            sent(out)
                .iter()
                .filter(|frame| frame["t"] == "stream.event")
                .map(|frame| frame["body"]["body"].clone())
                .collect()
        };
        // No `at`: not an envelope, and its type came.
        let no_at = r#"{"v":1,"t":"peer.offer","seq":2,"ack":1,"body":{}}"#;
        let out = s.received(no_at, T0 + 100);
        assert_eq!(
            told(&out),
            [json!({ "t": "peer.offer", "reason": "bad_frame" })]
        );
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Unread { t: None, .. }]
        ));
        // No type, or none that is written as one.
        let out = s.received(r#"{"v":1,"seq":3,"ack":1,"body":{}}"#, T0 + 200);
        assert_eq!(
            told(&out),
            [json!({ "t": "unnamed", "reason": "bad_frame" })]
        );
        assert!(
            told(&s.received("[1, 2]", T0 + 300)).is_empty(),
            "told already"
        );
        // Past so many kinds on one link a new type is told as unnamed, so her host cannot
        // make this end keep a list as long as it likes.
        let mut s = Session::new(contract(), &["streams"], ("Child", "1"), None, None);
        welcomed(&mut s, &["streams"]);
        let mut kinds = Vec::new();
        for n in 0..(Session::KEPT_DROPPED as u64 + 20) {
            let mut frame: Value =
                serde_json::from_str(&host("mood", 2 + n, 1 + n, |_| {})).unwrap();
            frame["t"] = json!(format!("later.{n}"));
            kinds.extend(told(&s.received(&frame.to_string(), T0 + 100)));
        }
        assert_eq!(kinds.len(), Session::KEPT_DROPPED + 1);
        assert_eq!(kinds.last().unwrap()["t"], "unnamed");
    }

    #[test]
    fn her_hosts_reading_and_what_is_granted_are_handed_to_the_shell() {
        let mut s = open();
        let out = s.received(&host("link.verdict", 2, 1, |_| {}), T0 + 100);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Verdict { usable, stopped }] if usable == "partial" && stopped == &["work.run"]
        ));
        let out = s.received(&host("call.granted", 3, 1, |_| {}), T0 + 200);
        let [Delivery::Granted { actions, providing }] = delivered(&out)[..] else {
            panic!("no granted: {out:?}")
        };
        assert_eq!(actions.len(), 2);
        assert_eq!(providing, &[("model.chat".to_string(), 1)]);
        // And what her host says it runs itself.
        let out = s.received(&host("link.versions", 4, 1, |_| {}), T0 + 300);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::HostVersions(body)] if body["contract"] == 3
        ));
    }
}

mod calling {
    use super::*;

    /// Open on a link where both listed calls and streams; her host provides `model.chat`
    /// and its clock is `ahead` of the shell's.
    fn linked(ahead: i64) -> Session {
        let mut s = Session::new(
            contract(),
            &["calls", "streams"],
            ("Child", "1"),
            None,
            None,
        );
        s.connected(T0);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls", "streams"]);
                body["serverTime"] = json!(T0 + 50 + ahead);
            }),
            T0 + 50,
        );
        s.received(
            &host("call.granted", 2, 2, |body| {
                body["actions"] = json!([]);
                body["providing"] = json!([{ "name": "model.chat", "version": 1 }]);
            }),
            T0 + 60,
        );
        s
    }

    fn ask(s: &mut Session, call_id: &str, within: i64, at: i64) -> Vec<Output> {
        let args = json!({ "messages": [] });
        s.call(
            call_id,
            "model.chat",
            1,
            &args,
            &json!({ "kind": "owner" }),
            within,
            at,
        )
    }

    #[test]
    fn an_ask_carries_her_hosts_time_and_goes_again_with_the_same_deadline() {
        // Her host's clock runs an hour ahead: the deadline is by it, the frame's own time by
        // this end's.
        let hour = 3_600_000;
        let mut s = linked(hour);
        let out = sent(&ask(&mut s, "ask_1", 30_000, T0 + 1000));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["t"], "call.ask");
        assert_eq!(out[0]["at"], T0 + 1000);
        assert_eq!(out[0]["body"]["deadlineAt"], T0 + hour + 31_000);
        // Not acked when the link goes: on the next it waits for her host's word on what it
        // provides there, then goes again, the same id and the same deadline.
        s.disconnected();
        s.connected(T0 + 5000);
        let welcomed = sent(&s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls", "streams"]);
                body["serverTime"] = json!(T0 + 5050 + hour);
            }),
            T0 + 5050,
        ));
        assert!(welcomed.iter().all(|frame| frame["t"] != "call.ask"));
        let again = sent(&s.received(
            &host("call.granted", 2, 2, |body| {
                body["actions"] = json!([]);
                body["providing"] = json!([{ "name": "model.chat", "version": 1 }]);
            }),
            T0 + 5060,
        ));
        let resent = again.iter().find(|frame| frame["t"] == "call.ask").unwrap();
        assert_eq!(resent["id"], "ask_1");
        assert_eq!(resent["body"]["deadlineAt"], T0 + hour + 31_000);
        // Asked for longer than a call of the action may be given: cut short to that from
        // now by her host's clock, as her host would hold it, and not refused.
        let far = sent(&ask(&mut s, "ask_far", 3_600_000, T0 + 6000));
        let most = myriad_her_link::call_most_ms("model.chat") as i64;
        assert_eq!(far.len(), 1);
        assert_eq!(far[0]["at"], T0 + 6000);
        assert_eq!(far[0]["body"]["deadlineAt"], T0 + hour + 6000 + most);
        // Nor twice under one id.
        let twice = ask(&mut s, "ask_1", 30_000, T0 + 6100);
        assert!(sent(&twice).is_empty());
    }

    #[test]
    fn only_what_her_host_provides_on_this_link_is_asked() {
        // Before it has said, and on a link where calls was not agreed.
        let mut s = Session::new(contract(), &["calls"], ("Child", "1"), None, None);
        s.connected(T0);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls"])
            }),
            T0 + 50,
        );
        let refused = |out: Vec<Output>| {
            matches!(
                delivered(&out)[..],
                [Delivery::Unsent { code, .. }] if code == "call_not_granted"
            ) && sent(&out).is_empty()
        };
        assert!(refused(ask(&mut s, "ask_1", 30_000, T0 + 100)));
        // What it provided on the link before is not provided on this one until it says so.
        let mut s = linked(0);
        s.disconnected();
        s.connected(T0 + 5000);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls", "streams"])
            }),
            T0 + 5050,
        );
        assert!(refused(ask(&mut s, "ask_2", 30_000, T0 + 5100)));
    }

    #[test]
    fn an_ask_that_waits_goes_only_where_her_host_says_it_provides_the_action() {
        let reconnect = |s: &mut Session, at: i64, caps: Value| {
            s.disconnected();
            s.connected(at);
            sent(&s.received(
                &host("welcome", 1, 1, |body| body["capabilities"] = caps),
                at + 50,
            ))
        };
        let mut s = linked(0);
        ask(&mut s, "ask_1", 30_000, T0 + 1000);
        // With no link nothing is asked.
        s.disconnected();
        let down = ask(&mut s, "ask_down", 30_000, T0 + 1500);
        assert!(matches!(
            delivered(&down)[..],
            [Delivery::Unsent { code, .. }] if code == "call_unavailable"
        ));
        // A link where calls was not agreed: the ask that waits is not sent.
        let out = reconnect(&mut s, T0 + 2000, json!([]));
        assert!(out.iter().all(|frame| frame["t"] != "call.ask"));
        // A link where it was, and her host says it provides nothing: not granted, told here.
        reconnect(&mut s, T0 + 3000, json!(["calls"]));
        let out = s.received(
            &host("call.granted", 2, 2, |body| {
                body["actions"] = json!([]);
                body["providing"] = json!([]);
            }),
            T0 + 3100,
        );
        assert!(sent(&out).iter().all(|frame| frame["t"] != "call.ask"));
        assert!(matches!(
            delivered(&out)[..],
            [
                Delivery::Granted { .. },
                Delivery::CallResult { call_id, code: Some(code), here: true, .. }
            ] if call_id == "ask_1" && code == "call_not_granted"
        ));
        // Told once here: no timeout is told of it later.
        assert!(
            delivered(&s.tick(T0 + 200_000))
                .iter()
                .all(|d| !matches!(d, Delivery::CallResult { .. }))
        );
        // An ask sent on this link is her host's to answer, though it narrows what it
        // provides before it has acked it.
        let mut s = linked(0);
        ask(&mut s, "ask_2", 30_000, T0 + 1000);
        let out = s.received(
            &host("call.granted", 3, 2, |body| {
                body["actions"] = json!([]);
                body["providing"] = json!([]);
            }),
            T0 + 1100,
        );
        assert!(matches!(delivered(&out)[..], [Delivery::Granted { .. }]));
        // Where calls is not agreed, nothing of a call in hand goes either.
        let mut s = Session::new(contract(), &["calls"], ("Child", "1"), None, None);
        s.offer(&[("work.run", 1)]);
        s.connected(T0);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls"])
            }),
            T0 + 50,
        );
        s.received(
            &host("call.granted", 2, 2, |body| {
                body["actions"] = json!([{ "name": "work.run", "version": 1 }]);
            }),
            T0 + 60,
        );
        let mut work = fixture("call.ask");
        work["seq"] = json!(3);
        work["id"] = json!("w_1");
        work["at"] = json!(T0 + 100);
        work["body"]["callId"] = json!("w_1");
        work["body"]["action"] = json!("work.run");
        work["body"]["args"] = json!({ "proposalId": "p", "title": "t", "instruction": "i" });
        work["body"]["deadlineAt"] = json!(T0 + 600_000);
        assert!(!delivered(&s.received(&work.to_string(), T0 + 100)).is_empty());
        reconnect(&mut s, T0 + 5000, json!([]));
        assert!(
            s.call_progress("w_1", "running", false, T0 + 5100)
                .is_empty()
        );
        assert!(s.call_piece("w_1", &json!("x"), T0 + 5200).is_empty());
    }

    #[test]
    fn what_the_shell_sets_for_the_next_hello_opens_nothing_on_this_link() {
        // The hello of this link said nothing of calls or streams.
        let mut s = Session::new(contract(), &[], ("Child", "1"), None, None);
        s.offer(&[("screen.look", 1)]);
        s.connected(T0);
        // Mid-link the shell adds them for its next hello, and her host, against the
        // contract, lists them in its welcome and says everything is granted and provided.
        s.set_capabilities(&["calls", "streams"]);
        let welcomed = s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls", "streams"])
            }),
            T0 + 50,
        );
        assert!(
            sent(&welcomed)
                .iter()
                .all(|frame| frame["t"] != "call.offer")
        );
        s.received(
            &host("call.granted", 2, 1, |body| {
                body["actions"] = json!([{ "name": "screen.look", "version": 1 }]);
                body["providing"] = json!([{ "name": "model.chat", "version": 1 }]);
            }),
            T0 + 60,
        );
        // No call is taken, none is asked, nothing of streams goes, nothing dropped is told.
        let mut look = fixture("call.ask");
        look["seq"] = json!(3);
        look["at"] = json!(T0 + 100);
        look["body"]["deadlineAt"] = json!(T0 + 9000);
        assert!(s.received(&look.to_string(), T0 + 100).is_empty());
        let asked = ask(&mut s, "ask_1", 30_000, T0 + 200);
        assert!(sent(&asked).is_empty());
        assert!(
            s.stream_state("example.now", 1, &json!({}), T0 + 300)
                .is_empty()
        );
        let mut later: Value = serde_json::from_str(&host("mood", 4, 1, |_| {})).unwrap();
        later["t"] = json!("call.later");
        assert!(sent(&s.received(&later.to_string(), T0 + 400)).is_empty());
        // The next link's hello says them, and there they are agreed.
        s.disconnected();
        let hello = sent(&s.connected(T0 + 1000)).remove(0);
        assert_eq!(hello["body"]["capabilities"], json!(["calls", "streams"]));
        let welcomed = s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls", "streams"])
            }),
            T0 + 1050,
        );
        assert!(
            sent(&welcomed)
                .iter()
                .any(|frame| frame["t"] == "call.offer")
        );
        // Kept with the state, and a state from before it was kept goes by its hello.
        let state = s.state();
        assert_eq!(state["said"], json!(["calls", "streams"]));
        let mut before = state.clone();
        before.as_object_mut().unwrap().remove("said");
        assert_eq!(
            Session::restore(contract(), &before).unwrap().state(),
            state
        );
    }

    /// Many random lives of a link: reconnects to hosts that list what they like, clocks
    /// that differ, grants, calls, results that come again, cancels, ticks, the session
    /// kept as its state between every step. Seeded, so a failure is the same every run.
    #[test]
    fn whatever_happens_a_call_ends_once_and_goes_only_where_it_may() {
        struct Dice(u64);
        impl Dice {
            fn roll(&mut self, sides: u64) -> u64 {
                self.0 = self
                    .0
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (self.0 >> 33) % sides
            }
        }
        for seed in 0..300u64 {
            let mut dice = Dice(seed * 7919 + 1);
            let says: Vec<&str> = match dice.roll(8) {
                0 => vec!["streams"],
                1 => vec![],
                2 => vec!["calls"],
                _ => vec!["calls", "streams"],
            };
            let mut s = Session::new(contract(), &says, ("Child", "1"), None, None);
            let (mut now, mut host_seq, mut child_sent) = (T0, 0u64, 0u64);
            let (mut open, mut calls, mut streams, mut provided) = (false, false, false, false);
            let (mut asked, mut next) = (Vec::<String>::new(), 0u64);
            let mut here = std::collections::BTreeMap::<String, u32>::new();
            let mut there = std::collections::BTreeMap::<String, u32>::new();
            let mut answered = std::collections::BTreeSet::<String>::new();
            let mut withdrawn = std::collections::BTreeSet::<String>::new();
            let mut asked_here = std::collections::BTreeSet::<String>::new();
            let mut log = Vec::<String>::new();
            for _ in 0..60 {
                now += [1, 50, 1000, 20_000, 70_000][dice.roll(5) as usize];
                let mut result_of = None;
                let out = match dice.roll(14) {
                    0 => {
                        s.disconnected();
                        (host_seq, child_sent, provided) = (1, 0, false);
                        asked_here.clear();
                        let mut out = s.connected(now);
                        let lists: Vec<&str> = match dice.roll(4) {
                            0 => vec![],
                            1 => vec!["streams"],
                            2 => vec!["calls"],
                            _ => vec!["calls", "streams"],
                        };
                        calls = lists.contains(&"calls") && says.contains(&"calls");
                        streams = lists.contains(&"streams") && says.contains(&"streams");
                        let ahead = [0, 3_600_000, -3_600_000][dice.roll(3) as usize];
                        let welcome = host("welcome", 1, 1, |body| {
                            body["capabilities"] = json!(lists);
                            body["serverTime"] = json!(now + ahead);
                        });
                        out.extend(s.received(&welcome, now));
                        open = true;
                        log.push(format!("reconnect, host lists {lists:?}"));
                        out
                    }
                    1 => {
                        s.disconnected();
                        open = false;
                        log.push("disconnect".into());
                        Vec::new()
                    }
                    2 | 3 if open => {
                        let provides = dice.roll(2) == 0;
                        host_seq += 1;
                        let ack = if dice.roll(2) == 0 { child_sent } else { 0 };
                        if calls {
                            provided = provides;
                        }
                        log.push(format!("granted, provides {provides}, acks {ack}"));
                        let granted = host("call.granted", host_seq, ack, |body| {
                            body["actions"] = json!([]);
                            body["providing"] = if provides {
                                json!([{ "name": "model.chat", "version": 1 }])
                            } else {
                                json!([])
                            };
                        });
                        s.received(&granted, now)
                    }
                    4..=6 => {
                        let id = format!("c{next}");
                        next += 1;
                        log.push(format!("call {id}"));
                        let within = [1000, 30_000, 60_000][dice.roll(3) as usize];
                        let out = ask(&mut s, &id, within, now);
                        if sent(&out).len() + delivered(&out).len() == sent(&out).len() {
                            asked.push(id);
                        }
                        out
                    }
                    7..=9 if open && !asked.is_empty() => {
                        let id = asked[dice.roll(asked.len() as u64) as usize].clone();
                        host_seq += 1;
                        let mut frame = fixture("call.result");
                        frame["seq"] = json!(host_seq);
                        frame["ack"] = json!(0);
                        frame["id"] = json!(format!("{id}.result"));
                        frame["body"] = match dice.roll(3) {
                            0 => json!({ "callId": id, "action": "model.chat", "version": 1,
                                "outcome": "done", "result": { "text": "x" } }),
                            1 => json!({ "callId": id, "action": "model.chat", "version": 1,
                                "outcome": "failed", "error": { "code": "call_timeout" } }),
                            _ => json!({ "callId": id, "action": "model.chat", "version": 1,
                                "outcome": "unknown" }),
                        };
                        log.push(format!("her host's result of {id}"));
                        result_of = Some(id);
                        s.received(&frame.to_string(), now)
                    }
                    10 if !asked.is_empty() => {
                        let id = asked[dice.roll(asked.len() as u64) as usize].clone();
                        log.push(format!("cancel {id}"));
                        s.call_cancel(&id, now)
                    }
                    11 => {
                        log.push("stream".into());
                        next += 1;
                        let mut out = s.stream_state("example.now", 1, &json!({}), now);
                        let id = format!("e{next}");
                        out.extend(s.stream_event(&id, "example.happened", 1, &json!({}), now));
                        out
                    }
                    12 if open => {
                        host_seq += 1;
                        log.push("ping, acking all".into());
                        s.received(&host("ping", host_seq, child_sent, |_| {}), now)
                    }
                    _ => {
                        log.push("tick".into());
                        let out = s.tick(now);
                        if out.iter().any(|o| matches!(o, Output::Drop(_))) {
                            s.disconnected();
                            open = false;
                        }
                        out
                    }
                };
                let fail = |why: &str| panic!("seed {seed}: {why}\n{}", log.join("\n"));
                let mut delivered_hers = false;
                for frame in sent(&out) {
                    child_sent += 1;
                    let t = frame["t"].as_str().unwrap();
                    if t.starts_with("call.") && !(open && calls) {
                        fail(&format!("{t} on a link where calls was not agreed"));
                    }
                    if t.starts_with("stream.") && !(open && streams) {
                        fail(&format!("{t} on a link where streams was not agreed"));
                    }
                    if t == "call.ask" {
                        let id = frame["id"].as_str().unwrap().to_string();
                        if !provided {
                            fail("an ask her host does not provide on this link");
                        }
                        if !asked_here.insert(id.clone()) {
                            fail("an ask twice on one link");
                        }
                        if answered.contains(&id) {
                            fail("an ask after its result came");
                        }
                        if withdrawn.contains(&id) {
                            fail("an ask sent after it was cancelled while it waited");
                        }
                    }
                }
                for delivery in delivered(&out) {
                    match delivery {
                        Delivery::CallResult {
                            call_id,
                            here: true,
                            ..
                        } => {
                            let count = here.entry(call_id.clone()).or_default();
                            *count += 1;
                            if *count > 1 {
                                fail("two results told here of one call");
                            }
                            if answered.contains(call_id) {
                                fail("a result told here after her host's");
                            }
                            if matches!(delivery, Delivery::CallResult { code: Some(code), .. } if code == "call_cancelled")
                            {
                                withdrawn.insert(call_id.clone());
                            }
                        }
                        Delivery::CallResult { call_id, .. } => {
                            let count = there.entry(call_id.clone()).or_default();
                            *count += 1;
                            delivered_hers = true;
                            if *count > 1 {
                                fail("her host's result delivered twice");
                            }
                        }
                        Delivery::Unread { .. } => fail("a frame of this test's it could not read"),
                        _ => {}
                    }
                }
                if let Some(id) = result_of {
                    let first = answered.insert(id.clone());
                    if first != delivered_hers {
                        fail(&format!(
                            "the result of {id}: first {first}, delivered {delivered_hers}"
                        ));
                    }
                }
                // Kept as its state between every step, as an end across the C entry keeps it.
                let state = s.state();
                let back = Session::restore(contract(), &state)
                    .unwrap_or_else(|why| panic!("seed {seed}: {why}\n{}", log.join("\n")));
                if back.state() != state {
                    fail("the state does not come back the same");
                }
                let asking = state["asking"].as_array().unwrap();
                if asking.len() > Session::KEPT_ANSWERS + 8 {
                    fail("calls asked are kept without end");
                }
                for pending in state["outbox"].as_array().unwrap() {
                    let waits = asking.iter().any(|call| call["callId"] == pending["id"]);
                    if pending["t"] == "call.ask" && !waits {
                        fail("an ask waits to go that is not asked any more");
                    }
                }
                s = back;
            }
        }
    }

    /// The link goes and comes back with `calls`; her host has not yet said what it provides.
    fn relinked(s: &mut Session, at: i64) {
        s.disconnected();
        s.connected(at);
        s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["calls", "streams"]);
                body["serverTime"] = json!(at + 50);
            }),
            at + 50,
        );
    }

    fn provides(s: &mut Session, at: i64) -> Vec<Output> {
        s.received(
            &host("call.granted", 2, 2, |body| {
                body["actions"] = json!([]);
                body["providing"] = json!([{ "name": "model.chat", "version": 1 }]);
            }),
            at,
        )
    }

    /// Her host's result of the call `id`, done.
    fn done(id: &str, seq: u64) -> String {
        let mut frame: Value = serde_json::from_str(&host("call.result", seq, 2, |body| {
            *body = json!({ "callId": id, "action": "model.chat", "version": 1,
                "outcome": "done", "result": { "text": "x" } });
        }))
        .unwrap();
        frame["id"] = json!(format!("{id}.result"));
        frame.to_string()
    }

    fn told_here(out: &[Output], id: &str, why: &str) -> bool {
        matches!(
            delivered(out)[..],
            [Delivery::CallResult { call_id, outcome, code: Some(code), here: true, .. }]
                if call_id == id && outcome == "failed" && code == why
        )
    }

    fn untold(out: &[Output], id: &str, why: &str) -> bool {
        sent(out).is_empty()
            && matches!(
                delivered(out)[..],
                [Delivery::Unsent { t, code, id: Some(of) }]
                    if t == "call.cancel" && code == why && of == id
            )
    }

    #[test]
    fn a_call_cancelled_while_its_ask_waits_never_goes() {
        // Not acked when the link goes; cancelled with no link: taken back, ended here.
        let mut s = linked(0);
        ask(&mut s, "ask_1", 30_000, T0 + 1000);
        s.disconnected();
        let out = s.call_cancel("ask_1", T0 + 1500);
        assert!(sent(&out).is_empty());
        assert!(told_here(&out, "ask_1", "call_cancelled"));
        // Told ended here once: cancelled again, only that it has ended.
        assert!(untold(
            &s.call_cancel("ask_1", T0 + 1600),
            "ask_1",
            "call_ended"
        ));
        // On the next link it does not go, and the action is not long: no cancel either.
        relinked(&mut s, T0 + 2000);
        let out = provides(&mut s, T0 + 2060);
        assert!(sent(&out).iter().all(|frame| frame["t"] != "call.ask"));
        assert!(sent(&out).iter().all(|frame| frame["t"] != "call.cancel"));
        // Had her host had the ask after all, its result is what happened, delivered.
        let out = s.received(&done("ask_1", 3), T0 + 2100);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::CallResult { here: false, outcome, .. }] if outcome == "done"
        ));
        // Cancelled on the next link before her host's word on what it provides: the same.
        ask(&mut s, "ask_2", 30_000, T0 + 3000);
        relinked(&mut s, T0 + 4000);
        let out = s.call_cancel("ask_2", T0 + 4055);
        assert!(sent(&out).is_empty());
        assert!(told_here(&out, "ask_2", "call_cancelled"));
        let out = provides(&mut s, T0 + 4060);
        assert!(sent(&out).iter().all(|frame| frame["t"] != "call.ask"));
        // A minute past its deadline nothing more is told of it here.
        assert!(delivered(&s.tick(T0 + 200_000)).is_empty());
    }

    #[test]
    fn a_call_that_went_is_cancelled_only_when_it_is_long_and_always_told() {
        let mut s = linked(0);
        ask(&mut s, "ask_1", 30_000, T0 + 1000);
        // Sent on this link, and not long: there is no stopping it, and that is told.
        assert!(untold(
            &s.call_cancel("ask_1", T0 + 1100),
            "ask_1",
            "call_sent"
        ));
        // Not a call this end asks.
        assert!(untold(
            &s.call_cancel("ask_none", T0 + 1200),
            "ask_none",
            "bad_id"
        ));
        // A long one. No action her host does is long today, so this is a call kept in a
        // state from before the action was kept, which is taken for long.
        let long = |s: &Session| {
            let mut state = s.state();
            for call in state["asking"].as_array_mut().unwrap() {
                call.as_object_mut().unwrap().remove("action");
            }
            Session::restore(contract(), &state).unwrap()
        };
        let mut s = long(&s);
        // Sent on this link: her host is told to stop, now.
        let out = s.call_cancel("ask_1", T0 + 1300);
        assert!(delivered(&out).is_empty());
        assert!(matches!(&sent(&out)[..], [frame]
            if frame["t"] == "call.cancel" && frame["body"]["callId"] == "ask_1"));
        // Acked, then the link goes: the shell is told to cancel again when linked. The ask
        // was not acked when it was stopped, so her host is told once more on the next link.
        let seq = sent(&out)[0]["seq"].as_u64().unwrap();
        s.received(&host("ping", 3, seq, |_| {}), T0 + 1400);
        s.disconnected();
        assert!(untold(
            &s.call_cancel("ask_1", T0 + 1500),
            "ask_1",
            "call_unavailable"
        ));
        relinked(&mut s, T0 + 2000);
        assert!(matches!(&sent(&provides(&mut s, T0 + 2060))[..], [frame]
            if frame["t"] == "call.cancel" && frame["body"]["callId"] == "ask_1"));
        // Its ask waits to go again when it is cancelled: taken back, ended here, and on the
        // next link with calls her host is told to stop after its word on what it provides.
        ask(&mut s, "ask_2", 30_000, T0 + 3000);
        let mut s = long(&s);
        s.disconnected();
        assert!(told_here(
            &s.call_cancel("ask_2", T0 + 3500),
            "ask_2",
            "call_cancelled"
        ));
        // Kept across a state, and not sent where calls was not agreed.
        let mut s = Session::restore(contract(), &s.state()).unwrap();
        s.connected(T0 + 4000);
        let out = sent(&s.received(&host("welcome", 1, 1, |_| {}), T0 + 4050));
        assert!(out.iter().all(|frame| frame["t"] != "call.cancel"));
        relinked(&mut s, T0 + 5000);
        let out = sent(&provides(&mut s, T0 + 5060));
        let told: Vec<&Value> = out.iter().filter(|frame| frame["t"] != "ack").collect();
        assert!(matches!(&told[..], [frame]
            if frame["t"] == "call.cancel" && frame["body"]["callId"] == "ask_2"));
        // Once.
        relinked(&mut s, T0 + 6000);
        assert!(
            sent(&provides(&mut s, T0 + 6060))
                .iter()
                .all(|frame| frame["t"] != "call.cancel")
        );
    }

    #[test]
    fn a_stop_that_waits_goes_after_her_hosts_word_and_a_stopped_ask_never_goes_again() {
        let long = |s: &Session| {
            let mut state = s.state();
            for call in state["asking"].as_array_mut().unwrap() {
                call.as_object_mut().unwrap().remove("action");
            }
            Session::restore(contract(), &state).unwrap()
        };
        // Cancelled while its ask waited; cancelled again on the next link before her host's
        // word on what it provides: the stop still waits for that word, and goes once.
        let mut s = linked(0);
        ask(&mut s, "ask_1", 30_000, T0 + 1000);
        let mut s = long(&s);
        s.disconnected();
        assert!(told_here(
            &s.call_cancel("ask_1", T0 + 1500),
            "ask_1",
            "call_cancelled"
        ));
        relinked(&mut s, T0 + 2000);
        assert!(untold(
            &s.call_cancel("ask_1", T0 + 2055),
            "ask_1",
            "call_ended"
        ));
        let stops = |out: &[Output]| {
            sent(out)
                .iter()
                .filter(|frame| frame["t"] == "call.cancel")
                .count()
        };
        let out = provides(&mut s, T0 + 2060);
        assert_eq!(stops(&out), 1);
        assert!(sent(&out).iter().all(|frame| frame["t"] != "call.ask"));
        // Stops are kept in the order made, across a state.
        let mut s = linked(0);
        ask(&mut s, "ask_b", 30_000, T0 + 1000);
        ask(&mut s, "ask_a", 30_000, T0 + 1100);
        let mut s = long(&s);
        s.disconnected();
        s.call_cancel("ask_b", T0 + 1500);
        s.call_cancel("ask_a", T0 + 1600);
        let state = s.state();
        assert_eq!(state["cancels"], json!(["ask_b", "ask_a"]));
        assert_eq!(Session::restore(contract(), &state).unwrap().state(), state);
        // Sent on this link, not acked, and stopped: the link goes, and on the next the ask
        // does not go again; her host, which may have had it, is told to stop once more,
        // after its word on what it provides and not before, though the shell asks again.
        let mut s = linked(0);
        ask(&mut s, "ask_2", 30_000, T0 + 1000);
        let mut s = long(&s);
        assert_eq!(stops(&s.call_cancel("ask_2", T0 + 1100)), 1);
        relinked(&mut s, T0 + 2000);
        assert!(untold(
            &s.call_cancel("ask_2", T0 + 2055),
            "ask_2",
            "call_unavailable"
        ));
        let out = provides(&mut s, T0 + 2060);
        assert!(sent(&out).iter().all(|frame| frame["t"] != "call.ask"));
        assert_eq!(stops(&out), 1);
        relinked(&mut s, T0 + 3000);
        assert_eq!(stops(&provides(&mut s, T0 + 3060)), 0);
        // With no answer it is told timed out here, once, like any call.
        assert!(told_here(&s.tick(T0 + 200_000), "ask_2", "call_timeout"));
    }

    #[test]
    fn an_action_the_contract_does_not_register_is_not_asked() {
        let mut s = linked(0);
        s.received(
            &host("call.granted", 3, 2, |body| {
                body["actions"] = json!([]);
                body["providing"] = json!([{ "name": "future.do", "version": 1 }]);
            }),
            T0 + 100,
        );
        let out = s.call(
            "ask_1",
            "future.do",
            1,
            &json!({}),
            &json!({ "kind": "owner" }),
            1000,
            T0 + 200,
        );
        assert!(sent(&out).is_empty());
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::Unsent { t, code, .. }] if t == "call.ask" && code == "call_unknown"
        ));
    }

    #[test]
    fn calls_ended_here_with_no_result_are_kept_to_a_bound_and_the_shell_is_told_which_go() {
        let mut s = linked(0);
        let kept = Session::KEPT_ANSWERS;
        // Each asked, then told timed out here a minute past its deadline.
        let mut now = T0 + 1000;
        let mut forgotten = Vec::new();
        for n in 0..kept + 3 {
            let out = ask(&mut s, &format!("ask_{n}"), 1000, now);
            for delivery in delivered(&out) {
                if let Delivery::CallsForgotten { call_ids } = delivery {
                    forgotten.extend(call_ids.iter().cloned());
                }
            }
            now += 100_000;
            s.received(&host("ping", 3 + n as u64, 2, |_| {}), now);
            let late = s.tick(now);
            assert!(told_here(&late, &format!("ask_{n}"), "call_timeout"), "{n}");
        }
        // The asks before the last found one more than is kept each time, from the second
        // past the bound on: the oldest went, and were told.
        assert_eq!(forgotten, ["ask_0", "ask_1"]);
        let state = s.state();
        assert_eq!(state["asking"].as_array().unwrap().len(), kept + 1);
        // A result of her host's for one let go is acked and not delivered.
        let out = s.received(&done("ask_0", 3 + kept as u64 + 3), now + 10);
        assert!(delivered(&out).is_empty());
        assert!(sent(&out).iter().any(|frame| frame["t"] == "ack"));
    }

    #[test]
    fn what_comes_of_a_call_is_delivered_in_order_and_its_result_once() {
        let mut s = linked(0);
        ask(&mut s, "ask_1", 30_000, T0 + 1000);
        let piece = |seq: u64, n: u64| {
            host("call.event", seq, 3, |body| {
                body["callId"] = json!("ask_1");
                body["sequence"] = json!(n);
            })
        };
        assert_eq!(delivered(&s.received(&piece(3, 1), T0 + 1100)).len(), 1);
        assert_eq!(
            delivered(&s.received(&piece(4, 3), T0 + 1200)).len(),
            1,
            "a gap is a gap"
        );
        assert!(
            delivered(&s.received(&piece(5, 2), T0 + 1300)).is_empty(),
            "behind is old"
        );
        let result = |seq: u64| {
            let mut frame = fixture("call.result");
            frame["seq"] = json!(seq);
            frame["id"] = json!("ask_1.result");
            frame["body"]["callId"] = json!("ask_1");
            frame["body"]["action"] = json!("model.chat");
            frame["body"]["result"] = json!({ "text": "好啊" });
            frame.to_string()
        };
        let out = s.received(&result(6), T0 + 1400);
        assert!(matches!(
            delivered(&out)[..],
            [Delivery::CallResult { outcome, here: false, .. }] if outcome == "done"
        ));
        assert_eq!(sent(&out)[0]["t"], "ack");
        let again = s.received(&result(7), T0 + 1500);
        assert!(delivered(&again).is_empty());
        assert_eq!(sent(&again)[0]["t"], "ack");
        // Kept with the state while it is asked.
        let mut s = linked(0);
        ask(&mut s, "ask_2", 10_000, T0 + 1000);
        let mut back = Session::restore(contract(), &s.state()).unwrap();
        assert_eq!(back.state(), s.state());
        // No answer a minute past the deadline: told here, once; the result that still comes
        // is what happened.
        assert!(delivered(&back.tick(T0 + 70_999)).is_empty());
        let late = back.tick(T0 + 71_000);
        assert!(matches!(
            delivered(&late)[..],
            [Delivery::CallResult { code: Some(code), here: true, .. }] if code == "call_timeout"
        ));
        assert!(delivered(&back.tick(T0 + 72_000)).is_empty());
    }

    #[test]
    fn an_event_waits_for_a_link_with_streams_and_a_state_does_not() {
        let mut s = linked(0);
        assert_eq!(
            sent(&s.stream_state("example.now", 1, &json!({}), T0 + 100)).len(),
            1
        );
        s.disconnected();
        assert!(
            s.stream_state("example.now", 1, &json!({}), T0 + 200)
                .is_empty()
        );
        assert!(
            s.stream_event("evt_1", "example.happened", 1, &json!({}), T0 + 300)
                .is_empty()
        );
        // A link whose host did not list streams: the event waits on.
        s.connected(T0 + 1000);
        let out = sent(&s.received(&host("welcome", 1, 1, |_| {}), T0 + 1050));
        assert!(out.iter().all(|frame| frame["t"] != "stream.event"));
        s.disconnected();
        s.connected(T0 + 2000);
        let out = sent(&s.received(
            &host("welcome", 1, 1, |body| {
                body["capabilities"] = json!(["streams"])
            }),
            T0 + 2050,
        ));
        let event = out
            .iter()
            .find(|frame| frame["t"] == "stream.event")
            .unwrap();
        assert_eq!(event["id"], "evt_1");
        // The report of a dropped frame is not the shell's to make.
        let own = s.stream_event("evt_2", "link.dropped", 1, &json!({}), T0 + 2100);
        assert!(matches!(delivered(&own)[..], [Delivery::Unsent { .. }]));
    }

    #[test]
    fn events_that_wait_are_kept_to_a_bound_and_the_oldest_let_go_is_told() {
        let mut s = linked(0);
        s.disconnected();
        let event = |s: &mut Session, id: &str| {
            s.stream_event(id, "example.happened", 1, &json!({}), T0 + 300)
        };
        for n in 0..Session::KEPT_EVENTS {
            assert!(event(&mut s, &format!("evt_{n}")).is_empty());
        }
        let gone = |out: &[Output], id: &str| {
            matches!(
                delivered(out)[..],
                [Delivery::Unsent { t, code, id: Some(of) }]
                    if t == "stream.event" && code == "too_many" && of == id
            )
        };
        // One more: the oldest that waits is let go, and told by its id.
        assert!(gone(&event(&mut s, "evt_more"), "evt_0"));
        // The same event again, or one whose id is not free, lets nothing go.
        assert!(event(&mut s, "evt_more").is_empty());
        let bad = event(&mut s, "not an id");
        assert!(matches!(
            delivered(&bad)[..],
            [Delivery::Unsent { t, .. }] if t == "stream.event"
        ));
        let kept = |s: &Session| {
            let state = s.state();
            let outbox = state["outbox"].as_array().unwrap();
            (outbox.len(), outbox[0]["id"].clone())
        };
        assert_eq!(kept(&s), (Session::KEPT_EVENTS, json!("evt_1")));
        // All on their way on a link: the new one is the one not kept.
        relinked(&mut s, T0 + 1000);
        let out = event(&mut s, "evt_last");
        assert!(sent(&out).is_empty());
        assert!(gone(&out, "evt_last"));
        assert_eq!(kept(&s), (Session::KEPT_EVENTS, json!("evt_1")));
    }
}
