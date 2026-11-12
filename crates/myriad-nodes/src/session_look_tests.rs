//! How her host looks at the sessions it is told: what is looked at and
//! when, what she is shown, and her answer read and held to itself.

use myriad_her_link::now::{Session, read_session, rules};
use serde_json::{Value, json};

use crate::session_look::*;

const AT: i64 = 1_760_000_000_000;

fn session(change: &str, extra: Value) -> Session {
    let mut body = json!({
        "grant": "g_0123456789", "session": "s-1", "change": change, "occurredAt": AT,
        "tool": "coding-agent", "remote": "github.com/me/repo", "title": "Split the store",
        "askedBy": "owner", "fromOwner": "split the store and commit it",
        "fromAgent": "Q: Push now? (push / wait)",
    });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    read_session(&body).unwrap()
}

fn told(session: Session) -> Told {
    Told {
        device: "dev_mac".into(),
        session,
    }
}

#[test]
fn a_change_told_late_is_only_kept() {
    let asked = session("asked", json!({}));
    let on_time = Arrival {
        linked_at: AT - 1,
        at: AT + 1_000,
    };
    assert_eq!(route(&asked, on_time), Route::Look);
    let held = Arrival {
        linked_at: AT + 1,
        at: AT + 2_000,
    };
    assert!(held.late(AT), "held while the link was away");
    assert_eq!(route(&asked, held), Route::Kept);
    let late = rules().look.late_after_ms;
    let long = Arrival {
        linked_at: 0,
        at: AT + late + 1,
    };
    assert_eq!(route(&asked, long), Route::Kept);
    assert!(
        !Arrival {
            linked_at: 0,
            at: AT + late
        }
        .late(AT)
    );
    for change in ["failed", "finished"] {
        let changed = session(change, json!({"fromAgent": "done", "waitingOn": "checks"}));
        assert_eq!(route(&changed, on_time), Route::Look, "{change}");
    }
    for change in ["started", "quiet"] {
        assert_eq!(route(&session(change, json!({})), on_time), Route::Kept);
    }
}

#[test]
fn what_came_is_looked_at_once_quiet_or_once_held_long_enough() {
    assert_eq!(due(AT, AT), AT + 20_000);
    assert_eq!(due(AT, AT + 5_000), AT + 25_000);
    assert_eq!(due(AT, AT + 50_000), AT + 60_000);
}

#[test]
fn she_is_shown_each_change_whole_and_what_is_around_it() {
    let asking = told(session(
        "asked",
        json!({"touched": ["src/a.rs", "src/b.rs"], "ownerSaidAt": AT - 3 * 60_000}),
    ));
    let mut quiet = told(session("quiet", json!({"touched": ["src/b.rs"]})));
    quiet.session.session = "s-2".into();
    let mut done = told(session("finished", json!({"fromAgent": "Done."})));
    done.session.session = "s-3".into();
    let material = material(
        &[
            Shown {
                told: asking.clone(),
                memories: vec![Recalled {
                    id: "m1".into(),
                    text: "pushes need their own word".into(),
                }],
                before: Some("你点头就行：上次也问过".into()),
            },
            Shown {
                told: done,
                memories: Vec::new(),
                before: None,
            },
        ],
        &[asking, quiet],
        &[Said {
            id: "7".into(),
            at: AT - 2 * 60_000,
            text: "测试都过了".into(),
        }],
        AT,
    );
    let numbered = material.numbered();
    for line in [
        "Change 1 of 2: stopped and asks the owner",
        "[1] the session: Split the store (coding-agent in repo)",
        "[2] who gave it this work: the owner, in person",
        "[3] what the owner last told it: split the store and commit it",
        "[4] when the owner last spoke to it themselves: 3 minutes ago; it asked this question 3 minutes after that",
        "[5] what it last said or asked: Q: Push now? (push / wait)",
        "[6] what its choices hold: only its question and the names of its choices come up",
        "[7] files it changed: src/a.rs, src/b.rs",
        "[8] another session in this repository: Split the store (coding-agent in repo): quiet for long; also changed src/b.rs",
        "[9] what you know of the owner (how they decided before): pushes need their own word",
        "[10] what you made of this session last time: 你点头就行：上次也问过",
        "Change 2 of 2: finished a turn, nothing to answer",
        "Around them:",
        "what you already told the owner of their sessions: 2 minutes ago: 测试都过了",
    ] {
        assert!(numbered.contains(line), "{line}\n{numbered}");
    }
    // A finished one has no question, and no word of its choices.
    assert_eq!(numbered.matches("what its choices hold").count(), 1);
    let last = material.pieces.len();
    let evidence = material.evidence(&[5, 9, last]);
    assert!(evidence.contains(&"memory:m1".to_string()));
    assert!(evidence.contains(&"proactive:7".to_string()));
}

#[test]
fn whether_a_question_came_after_the_owner_spoke_is_said_from_the_times() {
    let asked = session("asked", json!({}));
    assert_eq!(
        owner_said(AT - 30_000, &asked, AT + 20_000),
        "just now; it asked this question less than a minute after that"
    );
    assert_eq!(
        owner_said(AT - 60_000, &asked, AT),
        "a minute ago; it asked this question a minute after that"
    );
    assert_eq!(
        owner_said(AT, &asked, AT),
        "just now; it asked this question at that same moment"
    );
    assert_eq!(
        owner_said(AT + 5_000, &asked, AT + 65_000),
        "a minute ago; it had asked this question before that, and no later question has been told"
    );
    let finished = session("finished", json!({"fromAgent": "done"}));
    assert_eq!(owner_said(AT - 120_000, &finished, AT), "2 minutes ago");
}

#[test]
fn words_as_long_as_a_device_tells_say_they_go_on() {
    let most = rules().words_chars;
    assert_eq!(quoted("短"), "短");
    let full = "字".repeat(most);
    assert!(quoted(&full).ends_with(&format!("only its first {most} characters are here]")));
}

fn two() -> Material {
    let piece = |change: Option<usize>, text: &str| Piece {
        change,
        what: "the session".into(),
        text: text.into(),
        evidence: "session:s".into(),
    };
    Material {
        changes: vec!["asks".into(), "finished".into()],
        pieces: vec![
            piece(Some(1), "Split the store"),
            piece(Some(1), "Push now?"),
            piece(Some(2), "Write the docs"),
        ],
        choices: vec![vec!["push".into(), "wait".into()], Vec::new()],
    }
}

fn good(raw: &str) -> Answer {
    match read(raw, &two()) {
        Read::Good(answer) => answer,
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_ground_must_be_shown_and_every_change_there() {
    let material = two();
    for raw in [
        r#"{"changes":[{"change":1,"verdict":"nod","about":"push","suggestion":"push","because":[9],"missing":null,"ownerMustSay":false}],"say":null}"#,
        r#"{"changes":[{"change":3,"verdict":"no_need","about":"x","suggestion":null,"because":[],"missing":null,"ownerMustSay":false}],"say":null}"#,
        r#"{"changes":[{"change":1,"verdict":"nod","about":"push","suggestion":"push","because":[],"missing":null,"ownerMustSay":false}],"say":null}"#,
        r#"{"changes":[],"say":{"change":1,"text":"推吧","because":[]}}"#,
    ] {
        assert_eq!(read(raw, &material), Read::BadGrounds, "{raw}");
    }
    assert_eq!(read("not json", &material), Read::Unreadable);
    let answer = good(
        "```json\n{\"changes\":[{\"change\":\"1\",\"verdict\":\"nod\",\"about\":\"push\",\"suggestion\":\"推\",\"because\":[\"[2]\",1],\"missing\":null,\"ownerMustSay\":false}],\"say\":null}\n```",
    );
    assert_eq!(answer.judgments[0].as_ref().unwrap().because, vec![1, 2]);
    assert_eq!(answer.judgments[1], None);
}

#[test]
fn what_she_says_goes_whatever_card_it_is_of() {
    let raw = r#"{"changes":[
        {"change":1,"verdict":"decide","about":"asks whether to push","suggestion":"推","because":[2],"missing":null,"ownerMustSay":true},
        {"change":2,"verdict":"no_need","about":"docs done","suggestion":null,"because":[],"missing":null,"ownerMustSay":false}
    ],"say":{"change":CHANGE,"text":"要推吗","because":[2]}}"#;
    let said = settle(good(&raw.replace("CHANGE", "1")), &[true, false]);
    assert_eq!(said.line.unwrap().text, "要推吗");
    // Of a change that has no card, or of a card with no suggestion: it
    // goes all the same.
    assert_eq!(
        settle(good(&raw.replace("CHANGE", "2")), &[true, false])
            .line
            .unwrap()
            .text,
        "要推吗"
    );
    let bare = settle(
        good(&raw.replace("CHANGE", "1").replace("\"推\"", "null")),
        &[true, false],
    );
    assert_eq!(bare.line.unwrap().text, "要推吗");
    // Never left to itself when it waits on them.
    let left = settle(
        good(
            &raw.replace("CHANGE", "1")
                .replace("\"decide\"", "\"no_need\""),
        ),
        &[true, false],
    );
    let kept = left.judgments[0].as_ref().unwrap();
    assert_eq!(kept.verdict, Verdict::Nod);
    assert!(card_text(kept).starts_with("你点头就行：asks whether to push"));
}

#[test]
fn the_contract_names_her_verdicts() {
    for verdict in &rules().look.verdicts {
        serde_json::from_value::<Verdict>(json!(verdict)).unwrap();
    }
    assert_eq!(
        schema()["properties"]["changes"]["items"]["properties"]["verdict"]["enum"],
        json!(rules().look.verdicts)
    );
}

#[test]
fn failures_and_undone_lines_are_said_by_counts_and_ids_alone() {
    let sig = "0123456789abcdef";
    let failed = told(session(
        "failed",
        json!({
            "fromAgent": "the build keeps failing",
            "errors": [{"sig": sig, "count": 3, "firstAt": AT - 10 * 60_000, "lastAt": AT - 60_000},
                       {"sig": "fedcba9876543210", "count": 2, "firstAt": AT - 60_000, "lastAt": AT - 60_000}],
            "overwrote": [{"session": "s-3", "files": 2, "undid": true, "at": AT - 2 * 60_000}],
        }),
    ));
    let mut shares = told(session(
        "finished",
        json!({"fromAgent": "OTHERS-OWN-WORDS", "title": "Rename the types",
               "errors": [{"sig": sig, "count": 1, "firstAt": AT - 5 * 60_000, "lastAt": AT - 5 * 60_000}]}),
    ));
    shares.session.session = "s-3".into();
    let rewrote = told(session(
        "quiet",
        json!({"title": "Tidy the docs", "session": "s-4",
               "overwrote": [{"session": "s-1", "files": 1, "undid": false, "at": AT - 3 * 60_000}]}),
    ));
    // Under another grant, or on another computer: never compared, from
    // either side, though their ids and failures are the same.
    let mut elsewhere = shares.clone();
    elsewhere.device = "dev_pc".into();
    elsewhere.session.title = Some("ELSEWHERE".into());
    elsewhere.session.overwrote = rewrote.session.overwrote.clone();
    let mut regranted = elsewhere.clone();
    regranted.device = "dev_mac".into();
    regranted.session.grant = "g_9876543210".into();
    let numbered = material(
        &[Shown {
            told: failed.clone(),
            memories: Vec::new(),
            before: None,
        }],
        &[elsewhere, regranted, failed, shares, rewrote],
        &[],
        AT,
    )
    .numbered();
    for line in [
        "a failure it keeps hitting: the same failure 3 times within 9 minutes, the latest a minute ago",
        "a failure another session hits too: «Rename the types (coding-agent in repo)» failed the same way (once, the latest 5 minutes ago)",
        "what it did to another session's work: it put back what «Rename the types (coding-agent in repo)» had just replaced, in 2 file(s), 2 minutes ago",
        "what another session did to its work: «Tidy the docs (coding-agent in repo)» rewrote lines this one had just written, in 1 file(s), 3 minutes ago",
    ] {
        assert!(numbered.contains(line), "{line}\n{numbered}");
    }
    assert_eq!(numbered.matches("keeps hitting").count(), 1, "{numbered}");
    assert_eq!(numbered.matches("hits too").count(), 1, "{numbered}");
    assert_eq!(
        numbered.matches("another session did").count(),
        1,
        "{numbered}"
    );
    // Of the others: their titles, counts and times, never their words.
    assert!(!numbered.contains("OTHERS-OWN-WORDS"), "{numbered}");
    assert!(
        !numbered.contains("ELSEWHERE («") && !numbered.contains("«ELSEWHERE"),
        "{numbered}"
    );
}

#[test]
fn what_got_no_answer_keeps_its_card_if_it_waits_on_them() {
    let material = two();
    let raw = r#"{"changes":[{"change":2,"verdict":"no_need","about":"docs done","suggestion":null,"because":[3],"missing":null,"ownerMustSay":false}],"say":null}"#;
    for (raw, status) in [(raw, "good"), ("not json", "unreadable")] {
        let read = look_read(ReadInput {
            material: material.clone(),
            waits: vec![true, false],
            decides: Vec::new(),
            failed: Vec::new(),
            raw: raw.into(),
        })
        .unwrap();
        assert_eq!(read.status, status);
        let card = read.judgments[0].as_ref().unwrap();
        assert_eq!(card.judgment.verdict, Verdict::Nod);
        assert_eq!(card.card, "你点头就行：asks");
        assert_eq!(read.judgments[1].is_some(), status == "good");
    }
    // A failure keeps its card, answered or not.
    for (raw, verdict) in [(raw, Verdict::NoNeed), ("not json", Verdict::NoNeed)] {
        let read = look_read(ReadInput {
            material: material.clone(),
            waits: vec![true, false],
            decides: Vec::new(),
            failed: vec![false, true],
            raw: raw.into(),
        })
        .unwrap();
        let card = read.judgments[1].as_ref().unwrap();
        assert!(card.has_card);
        assert_eq!(card.judgment.verdict, verdict);
    }
    assert!(
        look_read(ReadInput {
            material,
            waits: vec![true, false],
            decides: Vec::new(),
            failed: vec![true],
            raw: raw.into(),
        })
        .is_err()
    );
}

#[test]
fn a_refused_body_is_never_quoted_back() {
    let mut frame = serde_json::to_value(session("asked", json!({}))).unwrap();
    frame["change"] = json!("token-sk-123");
    let refused = route_call(RouteInput {
        frame,
        linked_at: 0,
        at: AT,
    })
    .unwrap_err();
    assert!(!format!("{refused:?}").contains("sk-123"), "{refused:?}");
}

#[test]
fn times_at_the_edges_never_overflow() {
    assert_eq!(due(i64::MAX, i64::MAX), i64::MAX);
    assert!(
        !Arrival {
            linked_at: i64::MIN,
            at: i64::MIN
        }
        .late(i64::MAX)
    );
}

#[test]
fn what_waits_on_the_owners_answer_is_theirs_to_decide_without_her() {
    assert!(decides(&session("asked", json!({"waitingOn": "owner"}))));
    // Whoever gave the work: another session's, waiting on the owner.
    let sent_on = session(
        "asked",
        json!({"askedBy": "session", "askedByTitle": "Plan", "waitingOn": "owner"}),
    );
    assert!(decides(&sent_on));
    // Waiting on the session that sent it on is not theirs to decide (an
    // asked session always waits on the owner: the contract says so).
    let reports_back = session(
        "quiet",
        json!({"askedBy": "session", "askedByTitle": "Plan", "waitingOn": "session"}),
    );
    assert!(!decides(&reports_back));
    assert!(!decides(&session("finished", json!({"fromAgent": "done"}))));
    assert_eq!(card_only("asks", true, true).verdict, Verdict::Decide);
    assert_eq!(card_only("asks", true, false).verdict, Verdict::Nod);
    assert_eq!(card_only("failed", false, false).verdict, Verdict::NoNeed);
}

#[test]
fn the_form_with_nothing_in_it_is_hollow() {
    let empty = r#"{"changes":[{"change":1,"verdict":"decide","about":" ","suggestion":null,"because":[],"missing":null,"ownerMustSay":false}],"say":null}"#;
    assert!(hollow(empty));
    assert!(hollow(&format!("```json\n{empty}\n```")));
    assert!(!hollow(&empty.replace("\" \"", "\"push?\"")));
    assert!(!hollow("not json"));
    assert!(!hollow(r#"{"changes":[],"say":null}"#));
}

/// What she leans to is one of the change's choices, or none; her
/// suggestion answers only when there is one.
#[test]
fn a_lean_is_one_of_the_choices_and_an_answer_is_a_suggestion() {
    let answer = good(
        r#"{"changes":[{"change":1,"verdict":"decide","about":"push","suggestion":"wait","because":[2],"missing":null,"ownerMustSay":true,"leans":2,"answers":true},{"change":2,"verdict":"nod","about":"docs","suggestion":"","because":[3],"missing":null,"ownerMustSay":false,"leans":1,"answers":true}],"say":null}"#,
    );
    let first = answer.judgments[0].as_ref().unwrap();
    assert_eq!((first.leans, first.answers), (Some(2), true));
    // The second change gave no choices, and its suggestion is empty.
    let second = answer.judgments[1].as_ref().unwrap();
    assert_eq!((second.leans, second.answers), (None, false));
    for leans in ["3", "0", "\"x\"", "null"] {
        let raw = format!(
            r#"{{"changes":[{{"change":1,"verdict":"decide","about":"push","suggestion":"wait","because":[2],"missing":null,"ownerMustSay":true,"leans":{leans},"answers":false}}],"say":null}}"#
        );
        let answer = good(&raw);
        assert_eq!(answer.judgments[0].as_ref().unwrap().leans, None, "{leans}");
    }
    // An answer of the old shape reads as leaning to none.
    let answer = good(
        r#"{"changes":[{"change":1,"verdict":"nod","about":"push","suggestion":"push","because":[2],"missing":null,"ownerMustSay":false}],"say":null}"#,
    );
    let first = answer.judgments[0].as_ref().unwrap();
    assert_eq!((first.leans, first.answers), (None, false));
}

/// A session's choices are shown in one line each, numbered for her lean,
/// and kept with the material for reading her answer.
#[test]
fn the_choices_are_shown_numbered() {
    let asked = session(
        "asked",
        json!({"waitingOn": "owner", "choices": ["push\nnow", "wait"]}),
    );
    let shown = material(
        &[Shown {
            told: told(asked),
            memories: Vec::new(),
            before: None,
        }],
        &[],
        &[],
        AT,
    );
    assert!(
        shown
            .numbered()
            .contains("its choices: choice 1: «push now»; choice 2: «wait»"),
        "{}",
        shown.numbered()
    );
    assert_eq!(shown.choices_of(1), ["push now", "wait"]);
    assert!(shown.choices_of(2).is_empty());
    // A change with none keeps none, and the material says nothing of it.
    let plain = material(
        &[Shown {
            told: told(session("asked", json!({"waitingOn": "owner"}))),
            memories: Vec::new(),
            before: None,
        }],
        &[],
        &[],
        AT,
    );
    assert!(
        serde_json::to_value(&plain)
            .unwrap()
            .get("choices")
            .is_none()
    );
}
