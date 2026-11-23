//! What someone asked her not to keep (`chat_remember`'s `notToKeep`), and
//! the small step every end holds what it is about to keep to (`not_to_keep`,
//! and `not_asked` for what a group was read as asking). What comes in is
//! what people said, so a refusal quotes none of it, and an answer that
//! cannot be read is told apart from one that says nothing: the caller
//! takes it as a failure and asks again, never as leave to keep.

use myriad_merope::{chat_remember, not_to_keep};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Nothing, quiet_input, text};
use crate::Failure;

/// `{answer, theirs, askedBefore, saidByThem?}` → `{asked, unread}`: what the
/// answer of `chat_remember.extract_system` asked with `asked` says they ask
/// her not to keep. `answer` is the whole answer, as decoded; `theirs` their
/// words it is held to; `saidByThem`, of each line whether it is theirs,
/// where each names its line.
pub(super) fn remember_read_not_to_keep(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        answer: Value,
        theirs: String,
        asked_before: Vec<String>,
        #[serde(default)]
        said_by_them: Option<Vec<bool>>,
    }
    let req: In = quiet_input(raw)?;
    let by = req.said_by_them.as_deref();
    Ok(json!({
        "asked": chat_remember::read_not_to_keep(&req.answer, &req.theirs, by, &req.asked_before),
        "unread": chat_remember::not_to_keep_unread(&req.answer, &req.theirs, by),
    }))
}

/// `{}` → `{asked, leftAlone}`: the sources of the rows that only hold back.
pub(super) fn sources(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = quiet_input(raw)?;
    Ok(json!({
        "asked": not_to_keep::ASKED_SOURCE,
        "leftAlone": not_to_keep::LEFT_ALONE_SOURCE,
    }))
}

/// `{items}` → `{parts: [{from, to}]}`: where they are parted to be asked
/// about, `to` left out.
pub(super) fn parts(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        items: Vec<String>,
    }
    let req: In = quiet_input(raw)?;
    let parts: Vec<Value> = not_to_keep::parts(&req.items)
        .into_iter()
        .map(|part| json!({ "from": part.start, "to": part.end }))
        .collect();
    Ok(json!({ "parts": parts }))
}

pub(super) fn system(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = quiet_input(raw)?;
    Ok(text(Some(not_to_keep::not_to_keep_system())))
}

/// `{asked, who?, items}` → `{input}`: the user message itself, a JSON text.
pub(super) fn input_of(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        asked: Vec<String>,
        #[serde(default)]
        who: Option<String>,
        items: Vec<String>,
    }
    let req: In = quiet_input(raw)?;
    Ok(json!({
        "input": not_to_keep::not_to_keep_input(&req.asked, req.who.as_deref(), &req.items),
    }))
}

/// `{}` → `{schemaName, schema}`.
pub(super) fn schema(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = quiet_input(raw)?;
    Ok(json!({
        "schemaName": not_to_keep::NOT_TO_KEEP_SCHEMA_NAME,
        "schema": not_to_keep::not_to_keep_schema(),
    }))
}

/// `{raw, items}` → `{drop}`: which of `items` (how many) to leave out, or
/// `null` when the answer cannot be read.
pub(super) fn read(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        raw: String,
        items: usize,
    }
    let req: In = quiet_input(raw)?;
    Ok(json!({ "drop": not_to_keep::parse_not_to_keep(&req.raw, req.items) }))
}

pub(super) fn not_asked_system(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = quiet_input(raw)?;
    Ok(text(Some(not_to_keep::not_asked_system())))
}

/// `{line, asked}` → `{input}`: the user message itself, a JSON text.
pub(super) fn not_asked_input(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        line: String,
        asked: Vec<String>,
    }
    let req: In = quiet_input(raw)?;
    Ok(json!({ "input": not_to_keep::not_asked_input(&req.line, &req.asked) }))
}

/// `{}` → `{schemaName, schema}`.
pub(super) fn not_asked_schema(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = quiet_input(raw)?;
    Ok(json!({
        "schemaName": not_to_keep::NOT_ASKED_SCHEMA_NAME,
        "schema": not_to_keep::not_asked_schema(),
    }))
}

/// `{raw, asked}` → `{notAsked}`: which of `asked` (how many) the line does
/// not ask, or `null` when the answer cannot be read.
pub(super) fn not_asked_read(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        raw: String,
        asked: usize,
    }
    let req: In = quiet_input(raw)?;
    Ok(json!({ "notAsked": not_to_keep::parse_not_asked(&req.raw, req.asked) }))
}
