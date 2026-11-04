use chrono::FixedOffset;
use serde_json::json;

use super::choice::{self, Hand};
use super::markers::{self, MARKERS};
use super::*;

const NOW: i64 = 1_793_685_600_000_000; // 2026-11-03 06:00 UTC, 14:00 at +08:00

fn zone() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).unwrap()
}

fn private(id: &str) -> Audience<String> {
    serde_json::from_value(json!({"venue": "private", "members": [id]})).unwrap()
}

fn group(id: &str) -> Audience<String> {
    serde_json::from_value(json!({"venue": "group", "group": id, "members": ["1", "2"]})).unwrap()
}

fn own() -> Audience<String> {
    serde_json::from_value(json!({"venue": "own"})).unwrap()
}

fn wish(source: Source) -> Wish {
    Wish {
        to: private("1"),
        source,
        seed: "她刚办完一件事".into(),
        seed_from: own(),
        thing: Some("event:e-1".into()),
        in_person: false,
        keeps_until: None,
        called: false,
        needs: Vec::new(),
        at: NOW - MINUTE,
        text: None,
    }
}

fn said(to: Audience<String>, minutes_ago: i64, text: &str, thing: Option<&str>) -> Said {
    Said {
        at: NOW - minutes_ago * MINUTE,
        to,
        text: text.into(),
        source: Some(Source::Event),
        called: false,
        thing: thing.map(str::to_string),
        place: "web".into(),
        exit: "face".into(),
        extent: Extent::Full,
        heard: None,
        interrupted: false,
        line: None,
        group_name: None,
    }
}

fn input(wish: Wish) -> Admit {
    Admit {
        wish,
        ledger: Vec::new(),
        granted: Vec::new(),
        dnd: false,
        they_opened: false,
        present: true,
        now: NOW,
        last_talk_at: None,
        they_wrote_at: None,
        doing: None,
        touched_at: None,
    }
}

#[test]
fn called_on_passes_only_audience_and_grants() {
    let mut called = input(wish(Source::Reply));
    called.wish.called = true;
    called.dnd = true;
    called.wish.text = Some("早".into());
    called.ledger = vec![said(private("1"), 1, "早", Some("event:e-1"))];
    assert_eq!(admit(&called, &zone()), Admission::Say);

    called.wish.to = group("g");
    called.wish.seed_from = private("1");
    assert_eq!(
        admit(&called, &zone()),
        Admission::Refused(Refusal::Audience)
    );

    called.wish.seed_from = own();
    called.wish.needs = vec!["work.run".into(), "memory.read".into()];
    called.granted = vec!["memory.read".into()];
    assert_eq!(
        admit(&called, &zone()),
        Admission::Refused(Refusal::NotGranted {
            missing: vec!["work.run".into()]
        })
    );
}

#[test]
fn the_four_rules_hold_and_the_gates_are_facts() {
    let mut quiet = input(wish(Source::Event));
    quiet.dnd = true;
    assert_eq!(
        admit(&quiet, &zone()),
        Admission::Refused(Refusal::DoNotDisturb)
    );
    quiet.they_opened = true;
    assert!(matches!(admit(&quiet, &zone()), Admission::Ask { .. }));

    let mut again = input(wish(Source::Event));
    again.ledger = vec![said(
        private("1"),
        3 * 24 * 60,
        "那件事办好了",
        Some("event:e-1"),
    )];
    assert_eq!(
        admit(&again, &zone()),
        Admission::Refused(Refusal::SameThing {
            at: NOW - 3 * 24 * 60 * MINUTE
        })
    );
    // Not reached is not said.
    again.ledger[0].extent = Extent::Marker;
    assert!(matches!(admit(&again, &zone()), Admission::Ask { .. }));

    let mut word_for_word = input(wish(Source::Event));
    word_for_word.wish.thing = None;
    word_for_word.wish.text = Some(" 早呀 ".into());
    word_for_word.ledger = vec![
        said(private("1"), 30, "早呀", None),
        said(private("1"), 5, "在吗", None),
    ];
    // Only the last line counts.
    assert!(matches!(
        admit(&word_for_word, &zone()),
        Admission::Ask { .. }
    ));
    word_for_word.ledger[0].at = NOW - 2 * MINUTE;
    assert_eq!(
        admit(&word_for_word, &zone()),
        Admission::Refused(Refusal::Verbatim)
    );

    // Two minutes after the last one, under the old 180 seconds: a fact now.
    let mut soon = input(wish(Source::Wander));
    soon.wish.thing = Some("thought:t-2".into());
    soon.ledger = vec![said(private("1"), 2, "刚想到你", Some("thought:t-1"))];
    soon.they_wrote_at = Some(NOW - 60 * MINUTE);
    let Admission::Ask { facts } = admit(&soon, &zone()) else {
        panic!("a gate is a fact now");
    };
    assert_eq!(
        facts[0],
        "Why you want to speak: your thoughts wandered to them."
    );
    assert!(facts.iter().any(|fact| {
        fact.contains("- 2 minutes ago, they have not written to you since: 刚想到你")
    }));
}

#[test]
fn the_fuse_stops_a_loop() {
    let mut runaway = input(wish(Source::Event));
    runaway.wish.thing = None;
    runaway.ledger = (0..FUSE_PER_HOUR as i64)
        .map(|n| said(private("1"), n % 59, &format!("第{n}句"), None))
        .collect();
    assert_eq!(
        admit(&runaway, &zone()),
        Admission::Refused(Refusal::Fuse {
            count: FUSE_PER_HOUR
        })
    );
    runaway.ledger.pop();
    assert!(matches!(admit(&runaway, &zone()), Admission::Ask { .. }));
}

#[test]
fn waiting_and_face_to_face() {
    let mut away = input(wish(Source::Screen));
    away.wish.in_person = true;
    away.present = false;
    assert_eq!(admit(&away, &zone()), Admission::Refused(Refusal::NotThere));
    away.wish.keeps_until = Some(NOW + MINUTE);
    assert_eq!(
        admit(&away, &zone()),
        Admission::Wait {
            until: NOW + MINUTE
        }
    );
    away.wish.keeps_until = Some(NOW);
    assert_eq!(admit(&away, &zone()), Admission::Refused(Refusal::Stale));
}

#[test]
fn her_night_and_where_else_are_told() {
    let mut late = input(wish(Source::Reach));
    late.now = NOW + 12 * HOUR + 40 * MINUTE; // 02:40 at +08:00
    late.ledger = vec![Said {
        group_name: Some("猫猫群".into()),
        ..said(group("telegram:-1"), 0, "群里说过", Some("event:e-1"))
    }];
    let Admission::Ask { facts } = admit(&late, &zone()) else {
        panic!("her night is a fact");
    };
    assert!(
        facts
            .iter()
            .any(|fact| fact.starts_with("By your usual hours it is your night now (02:40)"))
    );
    let told = told(
        &late.ledger,
        &private("1"),
        Some("event:e-1"),
        None,
        late.now,
    );
    assert_eq!(
        told.elsewhere,
        vec![Elsewhere::Group {
            at: NOW,
            group: "telegram:-1".into(),
            name: Some("猫猫群".into())
        }]
    );
    assert!(
        facts
            .iter()
            .any(|fact| fact.contains("in the group 猫猫群"))
    );
    assert!(!facts.iter().any(|fact| fact.contains("群里说过")));
}

#[test]
fn marks_are_one_table_and_all_come_out() {
    assert!(MARKERS.windows(2).all(|pair| pair[0].name < pair[1].name));
    assert_eq!(markers::strip("好\n[[work: 查天气]]\n[[sticker:2]]"), "好");
    assert_eq!(markers::strip("[[workers]] 不是"), "[[workers]] 不是");
    assert_eq!(
        markers::strip("换上⟦wear:睡衣⟧啦\n\n下一段"),
        "换上啦\n\n下一段"
    );
    assert_eq!(markers::shown("好呀\n[[mus"), ("好呀".into(), true));
    assert_eq!(markers::shown("好呀 [[music:next"), ("好呀".into(), true));
    assert_eq!(markers::shown("[[见过]]"), ("[[见过]]".into(), false));
    assert_eq!(markers::shown("a ["), ("a".into(), true));
}

#[test]
fn her_choice_keeps_only_hands_that_are_up() {
    let hands = vec![
        Hand {
            id: "6F9619FF-8B86-D011-B42D-00C04FC964FF".into(),
            source: Source::Screen,
            line: "你在看这个呀".into(),
            raised_at: NOW - 90 * SECOND,
        },
        Hand {
            id: "b".into(),
            source: Source::Share,
            line: "这首歌".into(),
            raised_at: NOW - 2 * HOUR,
        },
    ];
    let choice = choice::parse(
        r#" {"say": "6f9619ff-8b86-d011-b42d-00c04fc964ff", "let_go": ["b", "6F9619FF-8B86-D011-B42D-00C04FC964FF", "x", "b"]} "#,
        &hands,
    )
    .unwrap();
    assert_eq!(
        choice.say.as_deref(),
        Some("6F9619FF-8B86-D011-B42D-00C04FC964FF")
    );
    assert_eq!(choice.let_go, vec!["b".to_string()]);
    assert_eq!(choice::parse(r#"{"say": null}"#, &hands), None);
    assert_eq!(choice::parse("say b", &hands), None);
    let input = choice::input(
        &hands,
        &[choice::Held::TheyTyping],
        &Default::default(),
        None,
        None,
        NOW,
    );
    assert_eq!(input["hands"][0]["raised"], "2 minutes ago");
    assert_eq!(input["hands"][1]["raised"], "2 hours ago");
    assert_eq!(input["held_by"], json!(["they were typing to you"]));
}

#[test]
fn a_row_of_her_log_is_a_line_of_the_ledger() {
    let payload = json!({
        "at": NOW / 1000, "text": "群里说一声", "place": "group", "source": "group_share",
        "called": false, "thing": "share:s-1", "exit": "im", "extent": "full",
        "heard": 3, "interrupted": true, "line": "m_9", "groupName": "猫猫群",
    });
    let said = Said::from_row("group:telegram:-1", None, &payload).unwrap();
    assert_eq!(said.at, NOW);
    assert!(same_audience(&said.to, &group("telegram:-1")));
    assert_eq!(said.group_name.as_deref(), Some("猫猫群"));
    assert_eq!(said.source, Some(Source::GroupShare));

    let partner = Said::from_row("personal:1", None, &payload).unwrap();
    assert_eq!(partner.to, private("1"));
    let theirs = Said::from_row("personal:1", Some("2"), &payload).unwrap();
    assert_eq!(theirs.to, private("2"));
    assert_eq!(Said::from_row("own:", None, &payload), None);
    assert_eq!(Said::from_row("personal:1", None, &json!({"at": 1})), None);

    // A newer side's words: kept, read as what this side knows.
    let mut newer = payload.clone();
    newer["source"] = json!("daydream");
    newer["extent"] = json!("hologram");
    newer["mood"] = json!("warm");
    let newer = Said::from_row("personal:1", None, &newer).unwrap();
    assert_eq!(newer.source, None);
    assert!(!newer.reached());
}

#[test]
fn a_greeting_is_told_when_she_last_greeted_them_by_the_date() {
    let greeted = |minutes_ago: i64| {
        let mut said = said(private("1"), minutes_ago, "早呀，今天来得挺早。", None);
        said.source = Some(Source::Greeting);
        said
    };
    let greeting = |ledger: Vec<Said>| {
        let mut asked = input(wish(Source::Greeting));
        asked.ledger = ledger;
        let Admission::Ask { facts } = admit(&asked, &zone()) else {
            panic!("hers to judge");
        };
        facts
            .into_iter()
            .find(|fact| fact.contains(" greeted them "))
    };
    // 14:00 at +08:00; greeted at 09:00 today.
    assert_eq!(
        greeting(vec![
            said(private("1"), 30, "看到你订的票了", None),
            greeted(300)
        ])
        .as_deref(),
        Some("You already greeted them today at 09:00: \"早呀，今天来得挺早。\"")
    );
    // Yesterday evening's is yesterday's, whenever she got up.
    assert_eq!(
        greeting(vec![greeted(20 * 60 + 30)]).as_deref(),
        Some("You last greeted them yesterday at 17:30: \"早呀，今天来得挺早。\"")
    );
    // The latest one is told.
    assert_eq!(
        greeting(vec![greeted(300), greeted(20 * 60 + 30)]).as_deref(),
        Some("You already greeted them today at 09:00: \"早呀，今天来得挺早。\"")
    );
    // Before yesterday, another person's, or one that never reached them
    // is not told.
    assert_eq!(greeting(vec![greeted(40 * 60)]), None);
    let mut elsewhere = greeted(300);
    elsewhere.to = private("2");
    assert_eq!(greeting(vec![elsewhere]), None);
    let mut unsent = greeted(300);
    unsent.extent = Extent::None;
    assert_eq!(greeting(vec![unsent]), None);
    // Only when she wants to greet them.
    let mut event = input(wish(Source::Event));
    event.ledger = vec![greeted(300)];
    let Admission::Ask { facts } = admit(&event, &zone()) else {
        panic!("hers to judge");
    };
    assert!(!facts.iter().any(|fact| fact.contains(" greeted them ")));
}
