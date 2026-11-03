//! How she types in a chat: the way people write there
//! (`speaking::chat_app_section`), how the people she talks with type
//! (`talk_shape`), her reply as the messages it goes out as
//! (`channel::as_messages`, `talk_shape::messages_this_turn`,
//! `goes_out_as`) and how long each takes to type (`timing::typing`); and
//! what she has learned there: the words and memes she looked up (`memes`)
//! and the stickers she can send (`stickers`).

use myriad_agent_rules::channel;
use myriad_merope::{memes, speaking, stickers, talk_shape, timing};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Nothing, input, text};
use crate::Failure;

pub(super) fn chat_app_section(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(text(Some(speaking::chat_app_section().to_string())))
}

/// How people type in a place, from their lines there (speaker, seconds,
/// text; oldest first, hers left out); null while too few
/// (`talk_shape::ROOM_AT_LEAST` messages).
pub(super) fn room_of(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        lines: Vec<(String, i64, String)>,
    }
    let req: In = input(raw)?;
    let lines: Vec<(&str, i64, &str)> = req
        .lines
        .iter()
        .map(|(by, at, said)| (by.as_str(), *at, said.as_str()))
        .collect();
    Ok(json!({ "room": talk_shape::room_of(&lines) }))
}

/// How people in a chat app usually type, until a place or a person has
/// said enough to go by.
pub(super) fn usual(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(json!({ "room": talk_shape::CHAT_APP_USUAL }))
}

pub(super) fn describe(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        room: talk_shape::Shape,
        title: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(talk_shape::describe(&req.room, &req.title))))
}

/// Her reply as the messages it goes out as, each line its own.
pub(super) fn as_messages(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        text: String,
    }
    let req: In = input(raw)?;
    Ok(json!({ "messages": channel::as_messages(&req.text) }))
}

/// `roll` is uniform in [0, 1).
pub(super) fn messages_this_turn(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        written: usize,
        roll: f64,
    }
    let req: In = input(raw)?;
    Ok(json!({ "most": talk_shape::messages_this_turn(req.written, req.roll) }))
}

/// `room` absent: how people in a chat app usually type.
pub(super) fn goes_out_as(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        lines: Vec<String>,
        most: usize,
        #[serde(default)]
        room: Option<talk_shape::Shape>,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "messages": talk_shape::goes_out_as(&req.lines, req.most, req.room.as_ref()),
    }))
}

/// Seconds to type a message on a phone; `roll` is uniform in [0, 1).
pub(super) fn typing(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        chars: usize,
        roll: f64,
    }
    let req: In = input(raw)?;
    Ok(json!({ "seconds": timing::typing(req.chars, req.roll) }))
}

/// A word or meme she knows.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Known {
    term: String,
    meaning: String,
}

/// One of her kept lines (`memes::kept_line`) read back; null when it is
/// not one.
pub(super) fn read_kept(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        line: String,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "known": memes::read_kept(&req.line).map(|known| Known {
            term: known.term,
            meaning: known.meaning,
        }),
    }))
}

/// What she knows of the words and memes in `talk`, from her kept lines
/// (her own rows of source `memes::SOURCE`), as the section a turn wears:
/// `memes::read_kept`, `known_in`, `section`, as the site and her group mind
/// put them together.
pub(super) fn meme_section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        talk: String,
        kept: Vec<String>,
    }
    let req: In = input(raw)?;
    let known: Vec<memes::Known> = req
        .kept
        .iter()
        .filter_map(|line| memes::read_kept(line))
        .collect();
    Ok(text(memes::section(&memes::known_in(&req.talk, &known))))
}

pub(super) fn sticker_section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        meanings: Vec<String>,
        left_this_month: usize,
    }
    let req: In = input(raw)?;
    Ok(text(stickers::format_sticker_section(
        &req.meanings,
        req.left_this_month,
    )))
}
