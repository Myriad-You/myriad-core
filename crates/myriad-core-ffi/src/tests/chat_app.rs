//! How she types in a chat and what she has learned there: through the C
//! entry point and straight from the crate.

use myriad_agent_rules::channel;
use myriad_merope::{memes, speaking, stickers, talk_shape, timing};
use serde_json::{Value, json};

use super::{bits, ok, text};

pub(super) const TESTED: &[&str] = &[
    "channel.as_messages",
    "memes.read_kept",
    "memes.section",
    "speaking.chat_app_section",
    "stickers.section",
    "talk_shape.describe",
    "talk_shape.goes_out_as",
    "talk_shape.messages_this_turn",
    "talk_shape.room_of",
    "talk_shape.usual",
    "timing.typing",
];

#[test]
fn the_chat_app_section_is_the_crate_text() {
    assert_eq!(
        text("speaking.chat_app_section", json!({})).as_deref(),
        Some(speaking::chat_app_section())
    );
}

fn their_lines() -> Vec<(String, i64, String)> {
    [
        "在吗",
        "今天好累",
        "哈哈哈",
        "你吃了没",
        "我去买奶茶了",
        "回来了",
        "好冷",
        "明天见",
        "晚安啦",
        "嗯嗯",
    ]
    .iter()
    .enumerate()
    .map(|(index, said)| ("them".to_string(), index as i64 * 600, said.to_string()))
    .collect()
}

#[test]
fn a_place_is_measured_as_the_crate_measures_it() {
    let lines = their_lines();
    let borrowed: Vec<(&str, i64, &str)> = lines
        .iter()
        .map(|(by, at, said)| (by.as_str(), *at, said.as_str()))
        .collect();
    let room = talk_shape::room_of(&borrowed).expect("ten lines are enough");
    assert_eq!(
        ok("talk_shape.room_of", json!({ "lines": lines }))["room"],
        json!(room)
    );
    let few = &lines[..talk_shape::ROOM_AT_LEAST - 1];
    assert_eq!(
        ok("talk_shape.room_of", json!({ "lines": few }))["room"],
        Value::Null,
        "too few to say"
    );
    assert_eq!(
        ok("talk_shape.usual", json!({}))["room"],
        json!(talk_shape::CHAT_APP_USUAL)
    );
    for title in ["How they type", "How people type here"] {
        for shape in [&room, &talk_shape::CHAT_APP_USUAL] {
            assert_eq!(
                text(
                    "talk_shape.describe",
                    json!({ "room": shape, "title": title })
                ),
                Some(talk_shape::describe(shape, title))
            );
        }
    }
}

#[test]
fn her_reply_goes_out_as_the_crate_sends_it() {
    for reply in [
        "",
        "好呀。",
        "在呢！\n\n你呢？",
        "一\n二\n三\n四\n五\n六\n七",
        "真的吗！太好了。\n我也想去！",
    ] {
        let lines = channel::as_messages(reply);
        assert_eq!(
            ok("channel.as_messages", json!({ "text": reply }))["messages"],
            json!(lines)
        );
        for roll in [0.0, 0.3, 0.6, 0.85, 0.999] {
            let most = talk_shape::messages_this_turn(lines.len(), roll);
            assert_eq!(
                ok(
                    "talk_shape.messages_this_turn",
                    json!({ "written": lines.len(), "roll": roll })
                )["most"],
                json!(most)
            );
            assert_eq!(
                ok(
                    "talk_shape.goes_out_as",
                    json!({ "lines": lines, "most": most })
                )["messages"],
                json!(talk_shape::goes_out_as(&lines, most, None))
            );
        }
    }
    let shouting = talk_shape::Shape {
        end_mark: 0.9,
        bang: 0.5,
        ..talk_shape::CHAT_APP_USUAL
    };
    let lines = ["真的吗！太好了。", "我也想去！"];
    assert_eq!(
        ok(
            "talk_shape.goes_out_as",
            json!({ "lines": lines, "most": 2, "room": shouting })
        )["messages"],
        json!(talk_shape::goes_out_as(&lines, 2, Some(&shouting)))
    );
}

#[test]
fn typing_takes_what_the_crate_says() {
    for (chars, roll) in [(0, 0.0), (7, 0.5), (12, 0.999), (400, 0.3)] {
        let seconds =
            ok("timing.typing", json!({ "chars": chars, "roll": roll }))["seconds"].clone();
        assert_eq!(bits(&seconds), timing::typing(chars, roll).to_bits());
    }
}

#[test]
fn the_words_she_knows_are_the_crate_section() {
    let kept = vec![
        memes::kept_line("hyw", "「何意味」的拼音缩写，意思是「什么意思」"),
        memes::kept_line("尊嘟假嘟", "「真的假的」的可爱说法"),
        "不是梗的一行".to_string(),
    ];
    assert_eq!(
        ok("memes.read_kept", json!({ "line": kept[0] }))["known"],
        json!({ "term": "hyw", "meaning": "「何意味」的拼音缩写，意思是「什么意思」" })
    );
    assert_eq!(
        ok("memes.read_kept", json!({ "line": kept[2] }))["known"],
        Value::Null
    );
    let known: Vec<memes::Known> = kept
        .iter()
        .filter_map(|line| memes::read_kept(line))
        .collect();
    for talk in ["Hikurn_Xi：hyw", "尊嘟假嘟 hyw", "今天天气不错"] {
        assert_eq!(
            text("memes.section", json!({ "talk": talk, "kept": kept })),
            memes::section(&memes::known_in(talk, &known)),
            "{talk}"
        );
    }
    assert!(text("memes.section", json!({ "talk": "hyw", "kept": kept })).is_some());
    assert_eq!(
        text(
            "memes.section",
            json!({ "talk": "今天天气不错", "kept": kept })
        ),
        None
    );
}

#[test]
fn the_stickers_she_can_send_are_the_crate_section() {
    let meanings = vec!["够格了，认了".to_string(), "不是这个".to_string()];
    for (meanings, left) in [
        (meanings.clone(), 3),
        (meanings, 0),
        (vec![], 2),
        (vec![], 0),
    ] {
        assert_eq!(
            text(
                "stickers.section",
                json!({ "meanings": meanings, "leftThisMonth": left })
            ),
            stickers::format_sticker_section(&meanings, left)
        );
    }
}
