//! The prompts she writes with: through the C entry point and straight from
//! the crate.

use chrono::{NaiveDate, TimeZone, Utc};
use myriad_merope::{
    chat_days, chat_history, chat_remember, curiosity, doing, explore, heard, inferring, inner,
    life, pace, reading, remembering, sharing, speaking, views, wants,
};
use serde_json::json;

use super::{micros, ok, text};

pub(super) const TESTED: &[&str] = &[
    "chat.reply_instruction",
    "chat_days.day_system",
    "chat_days.pick_system",
    "chat_remember.already_system",
    "chat_remember.extract_system",
    "chat_remember.settle_system",
    "curiosity.digest_system",
    "curiosity.wonder_system",
    "doing.choice_system",
    "doing.digest_system",
    "explore.compare_system",
    "explore.step_system",
    "explore.think_system",
    "explore.wonder_system",
    "heard.system",
    "inferring.input",
    "inferring.read",
    "inferring.schema",
    "inferring.system",
    "inner.system",
    "inner.system_for",
    "inner.system_for_group",
    "life.concepts_system",
    "life.own_day_system",
    "pace.lazing",
    "pace.section",
    "remembering.read",
    "remembering.schema",
    "remembering.system",
    "sharing.come_back_reason",
    "sharing.come_back_system",
    "sharing.reason",
    "sharing.system",
    "speaking.found_out_section",
    "speaking.proactive_system",
    "speaking.proactive_user",
    "speaking.said_lately_line",
    "speaking.share_links_section",
    "speaking.share_section",
    "views.system",
    "wants.section",
    "wants.system",
];

const SOUL: &str = "You are 星见.\nThe speaking contract.\n\n温柔，爱听歌。";

fn some(text: String) -> Option<String> {
    Some(text)
}

#[test]
fn the_texts_without_input_are_the_crates() {
    for (name, expected) in [
        (
            "chat.reply_instruction",
            chat_history::CHAT_REPLY_INSTRUCTION.to_string(),
        ),
        ("chat_days.pick_system", chat_days::pick_system()),
        (
            "chat_remember.already_system",
            chat_remember::already_system(),
        ),
        (
            "explore.compare_system",
            explore::compare_system().to_string(),
        ),
        ("heard.system", heard::system().to_string()),
        ("life.concepts_system", life::CONCEPTS_SYSTEM.to_string()),
        ("remembering.system", remembering::system()),
    ] {
        assert_eq!(text(name, json!({})), some(expected), "{name}");
    }
}

/// The recall judgment's answer, its schema and how it is read, with the
/// stretch of time it names.
#[test]
fn the_recall_judgment_reads_as_the_crate_reads_it() {
    assert_eq!(
        ok("remembering.schema", json!({}))["schema"],
        remembering::schema()
    );
    let read = ok(
        "remembering.read",
        json!({"output": r#"{"cues":["干洗"],"thorough":true,"range":{"how":"previous","unit":"month","count":1,"from":"","to":""},"open":true,"lookBack":false,"aboutYou":false}"#}),
    );
    assert_eq!(
        read,
        json!({"cues": ["干洗"], "thorough": true, "lookBack": false, "aboutYou": false,
            "range": {"how": "previous", "unit": "month", "count": 1}, "open": true})
    );
    let plain = ok(
        "remembering.read",
        json!({"output": r#"{"cues":[],"thorough":false}"#}),
    );
    assert!(plain.get("range").is_none() && plain.get("open").is_none());
    assert_eq!(
        ok("remembering.read", json!({"output": "嗯"})),
        serde_json::Value::Null
    );
}

/// Inferring what a turn brings, through the calls as through the crate:
/// the instructions as judged or not, the schema, what she is shown, and
/// the answer read back with the turn always among what it rests on.
#[test]
fn an_inference_reads_as_the_crate_reads_it() {
    for judged in [true, false] {
        assert_eq!(
            text("inferring.system", json!({ "judged": judged })),
            some(inferring::infer_system_prompt(judged))
        );
    }
    assert_eq!(
        ok("inferring.schema", json!({}))["schema"],
        inferring::infer_schema()
    );
    let known = [
        inferring::Known {
            on: "2026-08-01",
            fact: "每天骑车上班",
        },
        inferring::Known {
            on: "2026-10-02",
            fact: "打球把腿摔断了",
        },
    ];
    assert_eq!(
        text(
            "inferring.input",
            json!({"changes": [1], "known": [
                {"on": "2026-08-01", "fact": "每天骑车上班"},
                {"on": "2026-10-02", "fact": "打球把腿摔断了"},
            ], "already": ["腿断了"], "today": "2026-10-03 (Saturday)"})
        ),
        some(inferring::infer_input(
            &[1],
            &known,
            &["腿断了"],
            "2026-10-03 (Saturday)"
        ))
    );
    assert_eq!(
        ok(
            "inferring.read",
            json!({"output": r#"{"inference":"腿断了，这阵子大概骑不了车上班","because":[0,9],"whenFor":["聊到上班怎么去"]}"#,
                "changes": [1], "known": 2})
        ),
        json!({"inference": "腿断了，这阵子大概骑不了车上班", "because": [0, 1], "whenFor": ["聊到上班怎么去"]})
    );
    assert_eq!(
        ok(
            "inferring.read",
            json!({"output": r#"{"inference":null,"because":[],"whenFor":[]}"#, "changes": [1], "known": 2})
        ),
        serde_json::Value::Null
    );
}

#[test]
fn the_texts_on_her_soul_are_the_crates() {
    for (name, write) in [
        (
            "chat_days.day_system",
            chat_days::day_system as fn(&str) -> String,
        ),
        ("curiosity.wonder_system", curiosity::wonder_system),
        ("doing.choice_system", doing::choice_system),
        ("explore.wonder_system", explore::wonder_system),
        ("inner.system", inner::system),
        ("inner.system_for_group", inner::system_for_group),
        ("life.own_day_system", life::own_day_prompt),
        ("sharing.come_back_system", sharing::come_back_system),
        ("sharing.system", sharing::system),
        ("views.system", views::system),
        ("wants.system", wants::system),
    ] {
        assert_eq!(
            text(name, json!({ "soul": SOUL })),
            some(write(SOUL)),
            "{name}"
        );
    }
    for private in [true, false] {
        assert_eq!(
            text(
                "inner.system_for",
                json!({ "soul": SOUL, "private": private })
            ),
            some(inner::system_for(SOUL, private))
        );
    }
}

#[test]
fn the_texts_with_their_input_are_the_crates() {
    assert_eq!(
        text(
            "doing.digest_system",
            json!({ "soul": SOUL, "what": "listening to 晴天", "why": "", "how": "" })
        ),
        some(doing::digest_system(SOUL, "listening to 晴天", "", ""))
    );
    assert_eq!(
        text(
            "doing.digest_system",
            json!({ "soul": SOUL, "what": "reading", "why": "想看结局", "how": "Say how it ended. " })
        ),
        some(doing::digest_system(
            SOUL,
            "reading",
            "想看结局",
            "Say how it ended. "
        ))
    );
    assert_eq!(
        text(
            "explore.think_system",
            json!({ "soul": SOUL, "question": "鲸鱼为什么唱歌" })
        ),
        some(explore::think_system(SOUL, "鲸鱼为什么唱歌"))
    );
    for (search, wiki, read, video) in [
        (true, true, true, true),
        (true, false, false, false),
        (false, false, true, false),
        (false, false, false, false),
    ] {
        let senses = reading::Senses {
            search,
            search_is_wikipedia: wiki,
            read,
            video,
        };
        assert_eq!(
            text(
                "explore.step_system",
                json!({ "search": search, "searchIsWikipedia": wiki, "read": read, "video": video })
            ),
            some(explore::step_system(senses))
        );
    }
    assert_eq!(
        text(
            "curiosity.digest_system",
            json!({ "soul": SOUL, "why": "他们提到了" })
        ),
        some(curiosity::digest_system(SOUL, "他们提到了"))
    );
    for (what, why) in [("一首新歌", "，因为想起他们"), ("", "")] {
        assert_eq!(
            text("sharing.reason", json!({ "what": what, "why": why })),
            some(sharing::reason(what, why))
        );
        assert_eq!(
            text(
                "sharing.come_back_reason",
                json!({ "what": what, "why": why })
            ),
            some(sharing::come_back_reason(what, why))
        );
    }
    let known = vec!["养了一只猫".to_string(), "在上海工作".to_string()];
    use chat_remember::Examples;
    for kept in [known.clone(), vec![]] {
        assert_eq!(
            text("chat_remember.extract_system", json!({ "existing": kept })),
            some(chat_remember::extract_system_prompt(
                &kept,
                Examples::Chinese
            ))
        );
        assert_eq!(
            text("chat_remember.settle_system", json!({ "known": kept })),
            some(chat_remember::settle_system_prompt(
                &kept,
                Examples::Chinese
            ))
        );
        for (theirs, examples) in [
            ("I got a new bike", Examples::English),
            ("新买了一辆自行车", Examples::Chinese),
            ("新しい自転車を買った", Examples::English),
        ] {
            assert_eq!(
                text(
                    "chat_remember.extract_system",
                    json!({ "existing": kept, "theirs": theirs })
                ),
                some(chat_remember::extract_system_prompt(&kept, examples))
            );
            assert_eq!(
                text(
                    "chat_remember.settle_system",
                    json!({ "known": kept, "theirs": theirs })
                ),
                some(chat_remember::settle_system_prompt(&kept, examples))
            );
        }
    }
}

#[test]
fn speaking_up_first_is_the_crates() {
    let mind = "## Addressee\nYou are speaking to 阿离.";
    let recent = "- 早上好";
    assert_eq!(
        text(
            "speaking.proactive_system",
            json!({ "soul": SOUL, "mind": mind, "recent": recent })
        ),
        some(speaking::compose_proactive_system(SOUL, mind, recent))
    );
    for happened in [None, Some("pi 说：「搞定了」"), Some("  ")] {
        let mut input = json!({ "gist": "跟他们说查好了" });
        if let Some(happened) = happened {
            input["happened"] = json!(happened);
        }
        assert_eq!(
            text("speaking.proactive_user", input),
            some(speaking::compose_proactive_user("跟他们说查好了", happened))
        );
    }
}

#[test]
fn what_she_said_lately_is_the_crates() {
    for (minutes, since) in [
        (0, false),
        (1, true),
        (45, false),
        (200, true),
        (5000, false),
        (-3, true),
    ] {
        assert_eq!(
            text(
                "speaking.said_lately_line",
                json!({ "minutesAgo": minutes, "theyWroteSince": since, "line": "在吗" })
            ),
            some(speaking::said_lately_line(minutes, since, "在吗"))
        );
    }
}

#[test]
fn the_sections_are_the_crates() {
    for notes in [vec![], vec![" 鲸鱼会唱歌 ".to_string(), "".to_string()]] {
        assert_eq!(
            text("speaking.found_out_section", json!({ "notes": notes })),
            speaking::format_found_out_section(&notes)
        );
        assert_eq!(
            text("speaking.share_section", json!({ "songs": notes })),
            speaking::format_share_section(&notes)
        );
    }
    let links = vec![(
        "晴天：好听".to_string(),
        "https://example.com/1".to_string(),
    )];
    for songs in [links, vec![]] {
        assert_eq!(
            text("speaking.share_links_section", json!({ "songs": songs })),
            speaking::format_share_links_section(&songs)
        );
    }
    let now = Utc.with_ymd_and_hms(2026, 11, 3, 12, 0, 0).unwrap();
    let since = Utc.with_ymd_and_hms(2026, 10, 20, 9, 0, 0).unwrap();
    let noted = Utc.with_ymd_and_hms(2026, 11, 1, 22, 0, 0).unwrap();
    let kept = vec![
        wants::Want {
            id: "w1".into(),
            want: "听完这位歌手的所有专辑".into(),
            why: "一首歌打动了我".into(),
            reach: wants::Reach::OnYourOwn,
            longing: false,
            since,
            notes: vec![(noted, "听了第二张".into())],
        },
        wants::Want {
            id: "w2".into(),
            want: "看一次海".into(),
            why: "书里总写".into(),
            reach: wants::Reach::BeyondYou,
            longing: true,
            since,
            notes: vec![],
        },
    ];
    let shown = json!([
        { "id": "w1", "want": "听完这位歌手的所有专辑", "why": "一首歌打动了我", "reach": "on_your_own",
          "longing": false, "since": micros(since), "notes": [[micros(noted), "听了第二张"]] },
        { "id": "w2", "want": "看一次海", "why": "书里总写", "reach": "beyond_you",
          "longing": true, "since": micros(since), "notes": [] },
    ]);
    assert_eq!(
        text(
            "wants.section",
            json!({ "wants": shown, "now": micros(now) })
        ),
        wants::section(&kept, now)
    );
    assert!(
        text(
            "wants.section",
            json!({ "wants": shown, "now": micros(now) })
        )
        .is_some()
    );
    let day = NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
    for (tone, past, spent, lazed) in [
        (pace::Tone::Flat, 3, 300.0, 40.0),
        (pace::Tone::Even, 0, 10.0, 200.0),
        (pace::Tone::Keen, 0, 60.0, 0.0),
    ] {
        let mine = pace::Pace {
            day,
            usual_minutes: 120.0,
            days_past_usual: past,
            tone,
        };
        assert_eq!(
            text(
                "pace.section",
                json!({ "pace": mine, "spentMinutes": spent, "lazedMinutes": lazed })
            ),
            pace::section(&mine, spent, lazed)
        );
    }
    assert_eq!(
        super::ok("pace.lazing", json!({}))["lazing"],
        json!(pace::LAZING)
    );
}
