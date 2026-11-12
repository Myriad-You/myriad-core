//! The prompts she is given to write with: how she closes a chat reply
//! (`chat_history::CHAT_REPLY_INSTRUCTION`), how she speaks up first
//! (`speaking::compose_proactive_*`), and what she writes for herself at
//! night or after a talk (views, wants, what is inside her, what she did,
//! what she wonders and finds out, what she means to share, what she keeps
//! of them, her diary, her pace). Each is the crate's own text; a platform
//! keeps no copy of it.

use myriad_merope::{
    chat_days, chat_history, chat_remember, curiosity, doing, explore, heard, inner, life, pace,
    reading, remembering, sharing, speaking, views, wants,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Nothing, at, input, text};
use crate::Failure;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Soul {
    soul: String,
}

fn soul(raw: &[u8], write: fn(&str) -> String) -> Result<Value, Failure> {
    let req: Soul = input(raw)?;
    Ok(text(Some(write(&req.soul))))
}

fn plain(raw: &[u8], said: String) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(text(Some(said)))
}

pub(super) fn reply_instruction(raw: &[u8]) -> Result<Value, Failure> {
    plain(raw, chat_history::CHAT_REPLY_INSTRUCTION.to_string())
}

/// `mind`: the speaking sections a chat turn wears; `recent`: what she
/// already said to them unasked, newest first.
pub(super) fn proactive_system(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        soul: String,
        mind: String,
        recent: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(speaking::compose_proactive_system(
        &req.soul,
        &req.mind,
        &req.recent,
    ))))
}

/// `gist`: what she means to say; `happened`: the event as it was told.
pub(super) fn proactive_user(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        gist: String,
        #[serde(default)]
        happened: Option<String>,
    }
    let req: In = input(raw)?;
    Ok(text(Some(speaking::compose_proactive_user(
        &req.gist,
        req.happened.as_deref(),
    ))))
}

/// One line of what she said unasked lately, as her decision and her line
/// see it: how long ago, whether they wrote since.
pub(super) fn said_lately_line(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        minutes_ago: i64,
        they_wrote_since: bool,
        line: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(speaking::said_lately_line(
        req.minutes_ago,
        req.they_wrote_since,
        &req.line,
    ))))
}

pub(super) fn found_out_section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        notes: Vec<String>,
    }
    let req: In = input(raw)?;
    Ok(text(speaking::format_found_out_section(&req.notes)))
}

/// Songs she could put on their player: each line as it is numbered.
pub(super) fn share_section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        songs: Vec<String>,
    }
    let req: In = input(raw)?;
    Ok(text(speaking::format_share_section(&req.songs)))
}

/// Songs she could send as links: `[line, link]` each.
pub(super) fn share_links_section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        songs: Vec<(String, String)>,
    }
    let req: In = input(raw)?;
    Ok(text(speaking::format_share_links_section(&req.songs)))
}

pub(super) fn views_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, views::system)
}

pub(super) fn wants_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, wants::system)
}

/// A want as kept: `reach` is `on_your_own`, `with_others` or `beyond_you`;
/// `since` and each note's time in Unix microseconds, notes oldest first.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Want {
    id: String,
    want: String,
    why: String,
    reach: String,
    longing: bool,
    since: i64,
    notes: Vec<(i64, String)>,
}

pub(super) fn wants_section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        wants: Vec<Want>,
        now: i64,
    }
    let req: In = input(raw)?;
    let wants = req
        .wants
        .into_iter()
        .map(|want| {
            Ok(wants::Want {
                id: want.id,
                want: want.want,
                why: want.why,
                reach: wants::Reach::parse(&want.reach)
                    .ok_or_else(|| Failure::BadInput(format!("unknown reach {:?}", want.reach)))?,
                longing: want.longing,
                since: at(want.since)?,
                notes: want
                    .notes
                    .into_iter()
                    .map(|(when, note)| Ok((at(when)?, note)))
                    .collect::<Result<_, Failure>>()?,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    Ok(text(wants::section(&wants, at(req.now)?)))
}

pub(super) fn inner_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, inner::system)
}

pub(super) fn inner_system_for(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        soul: String,
        private: bool,
    }
    let req: In = input(raw)?;
    Ok(text(Some(inner::system_for(&req.soul, req.private))))
}

pub(super) fn inner_system_for_group(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, inner::system_for_group)
}

pub(super) fn doing_choice_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, doing::choice_system)
}

/// `what` she just finished, `why` she picked it, `how` to write of it.
pub(super) fn doing_digest_system(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        soul: String,
        what: String,
        why: String,
        how: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(doing::digest_system(
        &req.soul, &req.what, &req.why, &req.how,
    ))))
}

pub(super) fn explore_wonder_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, explore::wonder_system)
}

pub(super) fn explore_think_system(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        soul: String,
        question: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(explore::think_system(&req.soul, &req.question))))
}

/// What her senses can do right now.
pub(super) fn explore_step_system(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        search: bool,
        search_is_wikipedia: bool,
        read: bool,
        video: bool,
    }
    let req: In = input(raw)?;
    Ok(text(Some(explore::step_system(reading::Senses {
        search: req.search,
        search_is_wikipedia: req.search_is_wikipedia,
        read: req.read,
        video: req.video,
    }))))
}

pub(super) fn explore_compare_system(raw: &[u8]) -> Result<Value, Failure> {
    plain(raw, explore::compare_system().to_string())
}

pub(super) fn curiosity_wonder_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, curiosity::wonder_system)
}

pub(super) fn curiosity_digest_system(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        soul: String,
        why: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(curiosity::digest_system(&req.soul, &req.why))))
}

pub(super) fn sharing_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, sharing::system)
}

pub(super) fn sharing_come_back_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, sharing::come_back_system)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WhatWhy {
    what: String,
    why: String,
}

pub(super) fn sharing_reason(raw: &[u8]) -> Result<Value, Failure> {
    let req: WhatWhy = input(raw)?;
    Ok(text(Some(sharing::reason(&req.what, &req.why))))
}

pub(super) fn sharing_come_back_reason(raw: &[u8]) -> Result<Value, Failure> {
    let req: WhatWhy = input(raw)?;
    Ok(text(Some(sharing::come_back_reason(&req.what, &req.why))))
}

/// The examples' language for `theirs` (what they said); without it, the
/// Chinese ones every host had before it was asked.
fn examples(theirs: Option<&str>) -> chat_remember::Examples {
    theirs.map_or(
        chat_remember::Examples::Chinese,
        chat_remember::Examples::for_words,
    )
}

/// `existing`: what she already keeps of them; `theirs`: their message.
pub(super) fn remember_extract_system(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        existing: Vec<String>,
        #[serde(default)]
        theirs: Option<String>,
        /// Ask also when each fact happened ([`remember_extract_schema`]).
        #[serde(default)]
        dated: bool,
    }
    let req: In = input(raw)?;
    let examples = examples(req.theirs.as_deref());
    Ok(text(Some(if req.dated {
        chat_remember::extract_system_prompt_dated(&req.existing, examples)
    } else {
        chat_remember::extract_system_prompt(&req.existing, examples)
    })))
}

/// `{dated?}` → `{schema}`: what keeping what they said answers in; dated,
/// with when each fact happened.
pub(super) fn remember_extract_schema(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        #[serde(default)]
        dated: bool,
    }
    let req: In = input(raw)?;
    Ok(serde_json::json!({ "schema": if req.dated {
        chat_remember::extract_schema_dated()
    } else {
        chat_remember::extract_schema()
    } }))
}

/// `known`: what she knows of them for good; `theirs`: their lines in the
/// stretch.
pub(super) fn remember_settle_system(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        known: Vec<String>,
        #[serde(default)]
        theirs: Option<String>,
    }
    let req: In = input(raw)?;
    Ok(text(Some(chat_remember::settle_system_prompt(
        &req.known,
        examples(req.theirs.as_deref()),
    ))))
}

pub(super) fn remember_already_system(raw: &[u8]) -> Result<Value, Failure> {
    plain(raw, chat_remember::already_system())
}

pub(super) fn remembering_system(raw: &[u8]) -> Result<Value, Failure> {
    plain(raw, remembering::system())
}

/// `{}` → `{schema}`: the recall judgment's answer.
pub(super) fn remembering_schema(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(serde_json::json!({ "schema": remembering::schema() }))
}

/// `{output}` → `{cues, thorough, lookBack, aboutYou, range?, open?}`, or
/// `null` when the answer cannot be read.
pub(super) fn remembering_read(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        output: String,
    }
    let req: In = input(raw)?;
    Ok(match remembering::parse(&req.output) {
        None => Value::Null,
        Some(cues) => {
            let mut out = serde_json::json!({
                "cues": cues.cues,
                "thorough": cues.thorough,
                "lookBack": cues.look_back,
                "aboutYou": cues.about_you,
            });
            if let Some(range) = &cues.range {
                out["range"] = serde_json::to_value(range)
                    .map_err(|_| Failure::BadInput("answer does not serialize".into()))?;
            }
            if let Some(open) = cues.open {
                out["open"] = Value::Bool(open);
            }
            out
        }
    })
}

pub(super) fn chat_days_day_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, chat_days::day_system)
}

pub(super) fn chat_days_pick_system(raw: &[u8]) -> Result<Value, Failure> {
    plain(raw, chat_days::pick_system())
}

pub(super) fn heard_system(raw: &[u8]) -> Result<Value, Failure> {
    plain(raw, heard::system().to_string())
}

pub(super) fn own_day_system(raw: &[u8]) -> Result<Value, Failure> {
    soul(raw, life::own_day_prompt)
}

pub(super) fn concepts_system(raw: &[u8]) -> Result<Value, Failure> {
    plain(raw, life::CONCEPTS_SYSTEM.to_string())
}

/// Her pace as of a day: `day` (`YYYY-MM-DD`), `usualMinutes`,
/// `daysPastUsual`, `tone` (`flat`, `even` or `keen`); minutes spent on
/// things of her own and lazed today.
pub(super) fn pace_section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        pace: pace::Pace,
        spent_minutes: f64,
        lazed_minutes: f64,
    }
    let req: In = input(raw)?;
    Ok(text(pace::section(
        &req.pace,
        req.spent_minutes,
        req.lazed_minutes,
    )))
}

/// Ways of lazing about she can pick: `[kind, what it is]` each.
pub(super) fn pace_lazing(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(json!({ "lazing": pace::LAZING }))
}
