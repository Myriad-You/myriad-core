//! Handing work off from her chat (`myriad_merope::handoff`): her lines,
//! how each piece stands and reads, and where their words go. Times are
//! microseconds like every call; the crate counts whole seconds.

use myriad_agent_rules::channel::PendingOption;
use myriad_merope::handoff::{self, Facts, HandOff, Host, Piece, Standing};
use serde::Deserialize;
use serde_json::{Value, json};

use super::quiet_input as input;
use crate::Failure;

fn secs(micros: i64) -> i64 {
    micros.div_euclid(1_000_000)
}

/// A piece as a call takes it: `startedAt` and `endedAt` in microseconds.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PieceIn {
    number: u32,
    asked: String,
    #[serde(default)]
    started_at: Option<i64>,
    #[serde(default)]
    ended_at: Option<i64>,
    standing: Standing,
}

impl PieceIn {
    fn into_crate(self) -> Piece {
        Piece {
            number: self.number,
            asked: self.asked,
            started_at: self.started_at.map(secs),
            ended_at: self.ended_at.map(secs),
            standing: self.standing,
        }
    }
}

fn pieces(raw: Vec<PieceIn>) -> Vec<Piece> {
    raw.into_iter().map(PieceIn::into_crate).collect()
}

pub(super) fn split(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        reply: String,
    }
    let req: In = input(raw)?;
    let (spoken, hand_offs) = handoff::split(&req.reply);
    Ok(json!({ "spoken": spoken, "handOffs": hand_offs }))
}

pub(super) fn stands(raw: &[u8]) -> Result<Value, Failure> {
    let facts: Facts = input(raw)?;
    Ok(json!({ "standing": handoff::stands(&facts) }))
}

pub(super) fn line(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        piece: PieceIn,
        now: i64,
    }
    let req: In = input(raw)?;
    Ok(json!({ "line": handoff::line(&req.piece.into_crate(), secs(req.now)) }))
}

pub(super) fn shown(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        pieces: Vec<PieceIn>,
        now: i64,
    }
    let req: In = input(raw)?;
    let numbers: Vec<u32> = handoff::shown(pieces(req.pieces), secs(req.now))
        .iter()
        .map(|piece| piece.number)
        .collect();
    Ok(json!({ "numbers": numbers }))
}

pub(super) fn route(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        pieces: Vec<PieceIn>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "route": handoff::route(&pieces(req.pieces)) }))
}

pub(super) fn act(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        hand_off: HandOff,
        pieces: Vec<PieceIn>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "act": handoff::act(&req.hand_off, &pieces(req.pieces)) }))
}

pub(super) fn labelled(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        number: u32,
        asked: String,
        others_unfinished: bool,
        content: String,
    }
    let req: In = input(raw)?;
    let text = handoff::labelled(req.number, &req.asked, req.others_unfinished, &req.content);
    Ok(json!({ "text": text }))
}

pub(super) fn her_choice(raw: &[u8]) -> Result<Value, Failure> {
    /// A choice as a call takes it: nothing but these two.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct OptionIn {
        value: String,
        label: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        options: Vec<OptionIn>,
        text: String,
    }
    let req: In = input(raw)?;
    let options: Vec<PendingOption> = req
        .options
        .into_iter()
        .map(|option| PendingOption {
            value: option.value,
            label: option.label,
        })
        .collect();
    Ok(json!({ "choice": handoff::her_choice(&options, &req.text) }))
}

pub(super) fn next_number(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        numbers: Vec<u32>,
    }
    let req: In = input(raw)?;
    let number = handoff::next_number(req.numbers)
        .ok_or_else(|| Failure::BadInput("piece numbers have run out".into()))?;
    Ok(json!({ "number": number }))
}

pub(super) fn work_input(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        instruction: String,
        their_words: String,
    }
    let req: In = input(raw)?;
    Ok(json!({ "input": handoff::work_input(&req.instruction, &req.their_words) }))
}

pub(super) fn ended_line(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        text: String,
        failed: bool,
    }
    let req: In = input(raw)?;
    Ok(json!({ "line": handoff::ended_line(&req.text, req.failed) }))
}

pub(super) fn section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        host: Host,
        lines: Vec<String>,
        waiting: usize,
        #[serde(default)]
        answered: Option<u32>,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "section": handoff::section(req.host, &req.lines, req.waiting, req.answered)
    }))
}
