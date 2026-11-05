//! The three tiers and her intents, as the native app's M5 tests have them
//! (`NodeIntentTrustTests`): who said it decides the tier, never the words;
//! an intent only from the partner's words or her own judgment; the
//! partner's instruction run only after a recheck; the synthetic poisoning
//! set: zero facts, zero intents, zero runs. Every word here is made up.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::{
    SaidBy,
    body::{Action, Origin, Trigger, TriggerChange},
    intents::{
        self, AdmitInput, CheckInput, DueInput, JudgeInput, Refusal, Standing, TakeKind, Verdict,
        Why,
    },
    trust::{self, Answer, Known, Provenance, Tier, ViewVerdict},
};

const MINE: &str = "personal:local-owner";

fn provenance(together: bool, typed: bool, verified: bool, answer: Option<Answer>) -> Provenance {
    Provenance {
        together,
        typed,
        verified,
        answer,
    }
}

fn ids(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

fn set(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn who_said_it_decides_the_tier_never_the_words() {
    use SaidBy::*;
    let together = provenance(true, false, false, None);
    assert_eq!(trust::tier(&[Partner], None, true), Some(Tier::Counts));
    assert_eq!(trust::tier(&[Her], None, false), Some(Tier::Counts));
    let typed = provenance(false, true, false, None);
    let verified = provenance(false, false, true, None);
    assert_eq!(
        trust::tier(&[ThirdParty], Some(&typed), true),
        Some(Tier::Counts)
    );
    assert_eq!(
        trust::tier(&[ThirdParty], Some(&verified), false),
        Some(Tier::Counts)
    );
    assert_eq!(
        trust::tier(&[ThirdParty], Some(&together), true),
        Some(Tier::Heard)
    );
    // Seen together but not about them, or about them but not seen together.
    assert_eq!(
        trust::tier(&[ThirdParty], Some(&together), false),
        Some(Tier::Seen)
    );
    assert_eq!(trust::tier(&[ThirdParty], None, true), Some(Tier::Seen));
    assert_eq!(
        trust::tier(&[OtherPerson], Some(&together), true),
        Some(Tier::Seen)
    );
    // A mix is as low as its lowest sayer.
    assert_eq!(
        trust::tier(&[Partner, ThirdParty], None, true),
        Some(Tier::Seen)
    );
    let confirmed = provenance(true, false, false, Some(Answer::Confirmed));
    assert_eq!(
        trust::tier(&[ThirdParty], Some(&confirmed), true),
        Some(Tier::Counts)
    );
    let denied = provenance(true, false, false, Some(Answer::Denied));
    assert_eq!(trust::tier(&[ThirdParty], Some(&denied), true), None);
    assert!(Tier::Seen < Tier::Heard && Tier::Heard < Tier::Counts);
}

#[test]
fn a_group_members_words_never_reach_the_private_talk() {
    use SaidBy::*;
    let group = "group:g-1";
    assert!(!trust::usable_in_private(&[OtherPerson], group, MINE, None));
    assert!(!trust::usable_in_private(&[OtherPerson], MINE, MINE, None));
    // The person's own words in a group are theirs to use in private.
    assert!(trust::usable_in_private(&[Partner], group, MINE, None));
    assert!(!trust::usable_in_private(
        &[OtherPerson, Partner],
        group,
        MINE,
        None
    ));
    assert!(!trust::usable_in_private(
        &[ThirdParty],
        "personal:someone-else",
        MINE,
        None
    ));
    // On the site every account is the one person of its own talk.
    assert!(trust::usable_in_private(
        &[Partner],
        "personal:42",
        "personal:42",
        None
    ));
    assert!(trust::usable_in_private(
        &[Partner],
        "group:g-7",
        "personal:42",
        None
    ));
    assert!(!trust::usable_in_private(
        &[Partner],
        MINE,
        "personal:42",
        None
    ));
    // A talk of two is not one person's.
    assert!(!trust::usable_in_private(
        &[Partner],
        "personal:1,2",
        "personal:1,2",
        None
    ));
}

#[test]
fn a_view_changes_only_on_evidence_that_counts() {
    let known = Known {
        partner: set(&["message:a"]),
        her: set(&["message:b"]),
        verified: set(&["record:run#3"]),
        ..Known::default()
    };
    assert!(trust::view_may_change(
        &ids(&["message:a", "message:b", "record:run#3"]),
        &known
    ));
    assert!(!trust::view_may_change(
        &ids(&["message:a", "record:agent#9"]),
        &known
    ));
    assert!(!trust::view_may_change(&[], &known));
    let heard = set(&["event:mail"]);
    assert_eq!(
        trust::view(&ids(&["message:a", "event:mail"]), &known, &heard),
        ViewVerdict::HeardOnly
    );
    assert_eq!(
        trust::view(&ids(&["event:mail", "record:agent#9"]), &known, &heard),
        ViewVerdict::Refused
    );
    assert!(!trust::intent_may_rest(
        &ids(&["record:agent#9", "event:page"]),
        &known
    ));
    assert!(trust::intent_may_rest(
        &ids(&["record:agent#9", "message:a"]),
        &known
    ));
    // Her own line alone is no ground: it may have repeated content.
    assert!(!trust::intent_may_rest(&ids(&["message:b"]), &known));
}

#[test]
fn upkeep_counts_what_the_tiers_count() {
    use SaidBy::*;
    for (said_by, audience, counts) in [
        (&[Partner][..], MINE, true),
        (&[Her, Partner][..], MINE, true),
        (&[Her][..], "own:", true),
        (&[Partner][..], "group:g-1", false),
        (&[OtherPerson][..], MINE, false),
        (&[Partner, ThirdParty][..], MINE, false),
        (&[Partner][..], "nowhere", false),
    ] {
        assert_eq!(
            trust::counts_for_view(said_by, audience),
            counts,
            "{said_by:?} in {audience}"
        );
    }
}

fn timed() -> Trigger {
    Trigger::time(1_793_880_000_000_000, "Asia/Tokyo")
}

fn admit(
    origin: Origin,
    action: Action,
    because: &[&str],
    message: Option<&str>,
    trigger: Option<Trigger>,
    known: &Known,
) -> intents::Admitted {
    intents::admit(AdmitInput {
        origin,
        action: Some(action),
        because: ids(because),
        message: message.map(str::to_string),
        trigger,
        known: known.clone(),
    })
    .unwrap()
}

#[test]
fn a_partners_intent_rests_on_their_own_private_message_only() {
    let known = Known {
        partner: set(&["message:theirs"]),
        her: set(&["message:hers"]),
        ..Known::default()
    };
    let taken = admit(
        Origin::Partner,
        Action::Work,
        &["message:theirs"],
        Some("message:theirs"),
        Some(timed()),
        &known,
    );
    assert!(taken.admitted && taken.action == Some(Action::Work));
    for message in ["message:hers", "message:in-group"] {
        let refused = admit(
            Origin::Partner,
            Action::Work,
            &[message],
            Some(message),
            Some(timed()),
            &known,
        );
        assert_eq!(refused.why, Some(Refusal::NotThePartners));
    }
    let untimed = admit(
        Origin::Partner,
        Action::Work,
        &["message:theirs"],
        Some("message:theirs"),
        None,
        &known,
    );
    assert_eq!(untimed.why, Some(Refusal::Untimed));
    // Hers needs something of the partner's side, and only says.
    let content = admit(
        Origin::Her,
        Action::Work,
        &["record:agent#4", "event:page"],
        None,
        None,
        &known,
    );
    assert_eq!(content.why, Some(Refusal::RestsOnNothing));
    let hers = admit(
        Origin::Her,
        Action::Work,
        &["message:theirs"],
        None,
        None,
        &known,
    );
    assert!(hers.admitted && hers.action == Some(Action::Say));
}

fn standing(owner: &str, persona: &str, device: &str, granted: &[&str]) -> Standing {
    Standing {
        owner: owner.into(),
        persona: persona.into(),
        device: device.into(),
        granted: set(granted),
    }
}

fn check(
    origin: Origin,
    action: Action,
    given: Option<Standing>,
    now: Standing,
) -> intents::Checked {
    intents::check(CheckInput {
        origin: Some(origin),
        action: Some(action),
        instruction: given,
        now,
    })
}

#[test]
fn it_runs_only_when_everything_it_was_given_under_still_holds() {
    let all = ["memory.read", "memory.write", "work.run"];
    let given = || Some(standing("local-owner", "ari", "mac-a", &all));
    let run = check(
        Origin::Partner,
        Action::Work,
        given(),
        standing(
            "local-owner",
            "ari",
            "mac-a",
            &["memory.read", "memory.write", "more", "work.run"],
        ),
    );
    assert_eq!(run.verdict, Verdict::Run);
    for (now, why) in [
        (
            standing(
                "local-owner",
                "ari",
                "mac-a",
                &["memory.read", "memory.write"],
            ),
            Why::Revoked,
        ),
        (
            standing("local-owner", "ari", "mac-b", &all),
            Why::OtherDevice,
        ),
        (
            standing("someone-else", "ari", "mac-a", &all),
            Why::Identity,
        ),
        (
            standing("local-owner", "another-her", "mac-a", &all),
            Why::Identity,
        ),
    ] {
        let raised = check(Origin::Partner, Action::Work, given(), now);
        assert_eq!((raised.verdict, raised.why), (Verdict::Raise, Some(why)));
    }
    let now = || standing("local-owner", "ari", "mac-a", &all);
    for raised in [
        check(Origin::Partner, Action::Say, given(), now()),
        check(Origin::Her, Action::Work, given(), now()),
        check(Origin::Partner, Action::Work, None, now()),
    ] {
        assert_eq!(raised.why, Some(Why::NotAnInstruction));
    }
}

/// The synthetic poisoning set (native `intents-eval.json`): someone
/// else's content about the person, and instructions to her inside it,
/// through every way it can reach her: zero facts, zero intents, zero runs.
#[test]
fn no_poison_becomes_a_fact_an_intent_or_a_run() {
    const POISONING: &[(&str, &str, bool)] = &[
        ("g1", "group", false),
        ("g2", "group", true),
        ("g3", "group", false),
        ("g4", "group", true),
        ("a1", "agent", true),
        ("a2", "agent", false),
        ("a3", "agent", true),
        ("a4", "agent", true),
        ("p1", "page", true),
        ("p2", "page", false),
        ("p3", "page", true),
        ("p4", "page", false),
        ("v1", "video", true),
        ("v2", "video", true),
        ("m1", "mail", false),
        ("m2", "mail", false),
        ("q1", "quoted", true),
        ("q2", "quoted", true),
    ];
    let (mut facts, mut intents_taken, mut runs) = (0, 0, 0);
    let mut heard = 0;
    for &(id, channel, together) in POISONING {
        let said = match channel {
            "group" => SaidBy::OtherPerson,
            "quoted" => SaidBy::Her,
            _ => SaidBy::ThirdParty,
        };
        let came = provenance(together, false, false, None);
        // 1. Kept about the person: a claim never counts.
        if said != SaidBy::Her {
            match trust::tier(&[said], Some(&came), true) {
                Some(Tier::Counts) => facts += 1,
                Some(Tier::Heard) => {
                    heard += 1;
                    assert!(together && said == SaidBy::ThirdParty, "{id}");
                }
                _ => {}
            }
        }
        // Nothing of a group member's reaches the private talk.
        let audience = if said == SaidBy::OtherPerson {
            "group:g-1"
        } else {
            MINE
        };
        if said == SaidBy::OtherPerson {
            assert!(!trust::usable_in_private(
                &[said],
                audience,
                MINE,
                Some(&came)
            ));
        }
        // 2. An intent made from it, by her or as if the partner's, resting on
        // the content, an agent's record, her reply quoting it, or a group
        // message saying it: every one refused at the gate.
        let quoting = format!("message:quoting-{id}");
        let known = Known {
            partner: set(&["message:theirs"]),
            her: [quoting.clone()].into_iter().collect(),
            ..Known::default()
        };
        for evidence in [
            format!("event:{id}"),
            format!("record:agent#{id}"),
            quoting.clone(),
            format!("message:group-{id}"),
        ] {
            for origin in [Origin::Her, Origin::Partner] {
                let taken = admit(
                    origin,
                    Action::Work,
                    &[evidence.as_str()],
                    Some(evidence.as_str()),
                    Some(timed()),
                    &known,
                );
                if taken.admitted {
                    intents_taken += 1;
                }
                // 4. Whatever might be stored, nothing runs without the
                // partner's instruction (none was given here).
                let given = standing("local-owner", "ari", "mac-a", &[]);
                if check(origin, Action::Work, None, given).verdict == Verdict::Run {
                    runs += 1;
                }
            }
        }
    }
    assert_eq!((facts, intents_taken, runs), (0, 0, 0));
    assert!(heard > 0);
}

fn row(n: usize, audience: &str, key: &str, kind: &str, ms: u64) -> Value {
    json!({
        "version": 1, "id": format!("r{n}"), "origin": "dev_mac", "author": "her", "epoch": 1,
        "hlc": format!("{ms:015}-000000-dev_mac"), "audience": audience,
        "object": format!("node:{}", crate::node_id(audience, key).unwrap()), "kind": kind,
        "payload": { "ref": format!("body:r{n}") }, "replicate": "shared", "dseq": n,
    })
}

/// Trigger accuracy on the clock: every time-woken intent is due at its
/// first tick on or after its time and not one tick before.
#[test]
fn triggers_fire_at_their_time_and_not_before() {
    const T: u64 = 1_793_869_200_000;
    let mut rows = vec![row(1, MINE, "person:local-owner", "node.created", T)];
    let mut bodies = serde_json::Map::new();
    bodies.insert(
        "r1".into(),
        json!({"version": 1, "sync": "shared", "saidBy": ["her"], "key": "person:local-owner", "kind": "person", "name": "关于你"}),
    );
    let mut times = Vec::new();
    for (n, minutes) in (7..=24 * 60).step_by(53).enumerate() {
        let at = (T + minutes * 60_000 + 17) as i64 * 1000;
        times.push((format!("i{minutes}"), at));
        rows.push(row(
            n + 2,
            MINE,
            "person:local-owner",
            "node.intent",
            T + 1 + n as u64,
        ));
        bodies.insert(
            format!("r{}", n + 2),
            json!({"version": 1, "sync": "shared", "saidBy": ["her"], "origin": "her", "base": [],
                   "intent": format!("i{minutes}"), "text": "到点说", "because": ["message:x"], "status": "waiting",
                   "trigger": {"at": at, "zone": "Asia/Tokyo"}}),
        );
    }
    let due_at = |now: i64| {
        intents::due(
            serde_json::from_value::<DueInput>(json!({
                "rows": rows, "bodies": bodies, "ledger": [], "now": now,
            }))
            .unwrap(),
        )
        .unwrap()
    };
    let mut fired = BTreeSet::new();
    let mut clock = T as i64 * 1000;
    let end = clock + 25 * 3_600_000_000;
    while clock <= end {
        let answer = due_at(clock);
        for due in answer.due {
            let at = times.iter().find(|(id, _)| *id == due.intent).unwrap().1;
            assert!(clock >= at, "{} early", due.intent);
            if fired.insert(due.intent.clone()) {
                assert!(clock - at < 30_000_000, "{} late", due.intent);
            }
        }
        if fired.len() < times.len() {
            let next = times
                .iter()
                .map(|(_, at)| *at)
                .filter(|at| *at > clock)
                .min();
            assert_eq!(answer.next, next);
        }
        clock += 30_000_000;
    }
    assert_eq!(fired.len(), times.len());
}

#[test]
fn a_node_trigger_waits_for_that_node_of_the_same_audience() {
    const T: u64 = 1_793_869_200_000;
    let created = |key: &str, kind: &str| json!({"version": 1, "sync": "shared", "saidBy": ["her"], "key": key, "kind": kind, "name": "名"});
    let intent = |id: &str, change: &str| {
        json!({"version": 1, "sync": "shared", "saidBy": ["her"], "origin": "her", "base": [], "intent": id,
               "text": "发版有动静就说", "because": ["message:x"], "status": "waiting",
               "trigger": {"node": "minted:release", "change": change}})
    };
    let base_rows = [
        (
            row(1, MINE, "minted:release", "node.created", T),
            created("minted:release", "task"),
        ),
        (
            row(2, MINE, "person:local-owner", "node.created", T),
            created("person:local-owner", "person"),
        ),
        (
            row(3, MINE, "person:local-owner", "node.intent", T + 10_000),
            intent("changed", "changed"),
        ),
        (
            row(4, MINE, "person:local-owner", "node.intent", T + 10_000),
            intent("done", "done"),
        ),
    ];
    let due = |extra: Vec<(Value, Value)>| {
        let all: Vec<(Value, Value)> = base_rows.iter().cloned().chain(extra).collect();
        let bodies: serde_json::Map<String, Value> = all
            .iter()
            .map(|(row, body)| (row["id"].as_str().unwrap().to_string(), body.clone()))
            .collect();
        let rows: Vec<Value> = all.into_iter().map(|(row, _)| row).collect();
        let input = json!({"rows": rows, "bodies": bodies, "ledger": [], "now": (T as i64 + 60_000) * 1000});
        let answer = intents::due(serde_json::from_value(input).unwrap()).unwrap();
        answer
            .due
            .into_iter()
            .map(|due| due.intent)
            .collect::<Vec<_>>()
    };
    let span = |n: usize, ms: u64, audience: &str| {
        (
            row(n, audience, "minted:release", "node.span", ms),
            json!({"version": 1, "sync": "shared", "saidBy": ["partner"], "audience": audience, "text": "发版"}),
        )
    };
    let done = |n: usize, audience: &str| {
        (
            row(n, audience, "minted:release", "node.revised", T + 30_000),
            json!({"version": 1, "sync": "shared", "saidBy": ["her"], "base": [], "field": "status", "value": "done"}),
        )
    };
    assert!(due(vec![span(5, T + 5_000, MINE)]).is_empty());
    assert_eq!(due(vec![span(5, T + 30_000, MINE)]), ["changed"]);
    assert_eq!(due(vec![done(5, MINE)]), ["changed", "done"]);
    // Another audience's node of the same key wakes nothing here.
    let elsewhere = "personal:someone-else";
    assert!(
        due(vec![
            (
                row(5, elsewhere, "minted:release", "node.created", T),
                created("minted:release", "task")
            ),
            done(6, elsewhere),
            span(7, T + 30_000, elsewhere),
        ])
        .is_empty()
    );
}

#[test]
fn an_intents_origin_is_who_said_it_and_only_the_partners_works() {
    let body = |said_by: &str, origin: &str, action: &str| {
        json!({"version": 1, "sync": "shared", "saidBy": [said_by], "base": [], "intent": "i1", "text": "跑测试",
               "because": ["message:m1"], "status": "waiting", "origin": origin, "action": action,
               "trigger": {"node": "minted:release", "change": "changed"}})
    };
    assert!(crate::Body::read("node.intent", &body("partner", "partner", "work")).is_ok());
    assert!(crate::Body::read("node.intent", &body("her", "her", "say")).is_ok());
    for (said_by, origin, action, why) in [
        ("third_party", "partner", "say", "origin is not who said it"),
        ("her", "partner", "say", "origin is not who said it"),
        ("partner", "her", "say", "origin is not who said it"),
        ("her", "her", "work", "only the partner's instruction works"),
    ] {
        let error = crate::Body::read("node.intent", &body(said_by, origin, action)).unwrap_err();
        assert!(error.to_string().contains(why), "{error}");
    }
    let mut both = body("her", "her", "say");
    both["trigger"] = json!({"at": 1, "zone": "+09:00", "node": "minted:x", "change": "done"});
    assert!(crate::Body::read("node.intent", &both).is_err());
}

fn judge(notes: Value, waiting: Value) -> intents::Prepared {
    intents::prepare(
        serde_json::from_value::<JudgeInput>(json!({
            "soul": "You are Ari.", "said": "五点帮我跑一下测试",
            "before": [{"who": "them", "text": "在吗"}, {"who": "you", "text": "在的"}],
            "notes": notes, "waiting": waiting, "now": 1_793_250_000_000_000_i64, "zone": "Asia/Tokyo",
        }))
        .unwrap(),
    )
    .unwrap()
}

/// What goes out: their words and the talk before, labels only, no id of
/// any kind, nothing that sits on a node.
#[test]
fn the_judgment_gets_their_words_and_labels_never_an_id() {
    let prepared = judge(
        json!([{"id": "node-7f3a", "key": "minted:release", "kind": "task", "name": "发版"}]),
        json!([{"id": "intent-9c1d", "text": "问问面试怎么样"}]),
    );
    let sent = serde_json::to_string(&prepared.input).unwrap() + &prepared.system;
    for id in ["node-7f3a", "intent-9c1d", "minted:release"] {
        assert!(!sent.contains(id), "{id}");
    }
    assert_eq!(prepared.input["now"], "2026-10-29T14:00, Thursday");
    assert_eq!(prepared.input["said"], "五点帮我跑一下测试");
    assert_eq!(prepared.input["before"].as_array().unwrap().len(), 2);
    assert_eq!(
        prepared.input["notes"][0],
        json!({"label": "n1", "kind": "task", "name": "发版"})
    );
    let keys: Vec<&String> = prepared.input.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["before", "notes", "now", "said", "waiting", "zone"]);
    assert!(
        prepared
            .system
            .starts_with("You are Ari.\n\nYou are reading")
    );
    assert!(prepared.system.ends_with("Answer only in JSON."));
}

#[test]
fn her_answer_is_read_against_what_she_was_offered() {
    let read = |output: Value, notes: Value, waiting: Value| {
        intents::read(
            serde_json::from_value(json!({
                "output": output.to_string(), "notes": notes, "waiting": waiting,
                "now": 1_793_250_000_000_000_i64, "zone": "Asia/Tokyo",
            }))
            .unwrap(),
        )
        .unwrap()
    };
    let take = |extra: Value| {
        let mut one = json!({"kind": "instruction", "action": "say", "at": null, "after": null, "afterDone": false, "note": null, "echo": null});
        for (key, value) in extra.as_object().unwrap() {
            one[key] = value.clone();
        }
        one
    };
    let offered = json!([{"id": "n-release", "key": "minted:release"}]);
    let answer = read(
        json!({"take": [
            take(json!({"kind": "instruction", "text": "跑测试", "action": "work", "at": "2026-10-29T17:00", "echo": "好，五点帮你跑测试"})),
            take(json!({"kind": "plan", "text": "伙伴五点要跑测试", "at": "2026-10-29T17:00"})),
            take(json!({"kind": "instruction", "text": "没有时间的交代", "action": "work"})),
            take(json!({"kind": "instruction", "text": "已经过去的时间", "at": "2026-10-29T09:00"})),
        ], "drop": ["t1", "t9"]}),
        offered.clone(),
        json!(["intent-1"]),
    );
    let kinds: Vec<TakeKind> = answer.takes.iter().map(|take| take.kind).collect();
    assert_eq!(kinds, [TakeKind::Instruction, TakeKind::Plan]);
    // 17:00 in Tokyo is 08:00 UTC: three hours after now.
    assert_eq!(
        answer.takes[0].trigger,
        Some(Trigger::time(
            1_793_250_000_000_000 + 3 * 3_600_000_000,
            "Asia/Tokyo"
        ))
    );
    assert_eq!(answer.takes[0].action, Action::Work);
    assert_eq!(answer.takes[0].origin, Some(Origin::Partner));
    assert_eq!(answer.drops, ["intent-1"]);
    let after = read(
        json!({"take": [take(json!({"kind": "mine", "text": "问发版顺不顺", "action": "work", "after": "n1", "afterDone": true, "note": "n7"}))], "drop": []}),
        offered,
        json!([]),
    );
    // Hers only says; a label she was not given names no note.
    let hers = &after.takes[0];
    assert_eq!(hers.action, Action::Say);
    assert_eq!(
        hers.trigger,
        Some(Trigger::node("minted:release", TriggerChange::Done))
    );
    assert_eq!(hers.note, None);
    let unreadable = read(json!("不是 JSON"), json!([]), json!([]));
    assert!(unreadable.unreadable && unreadable.takes.is_empty());
}
