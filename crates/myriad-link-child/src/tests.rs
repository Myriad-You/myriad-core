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
    assert!(
        s.say("t_02", "again", None, None, &[], T0 + 1001)
            .is_empty(),
        "one turn per id"
    );

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
        [Delivery::Unsent { t, code }] if t == "speech.delivered" && code == "bad_frame"
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
