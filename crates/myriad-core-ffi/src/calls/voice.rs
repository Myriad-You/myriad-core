//! How she says it, the same on every end: her soul and where she lives
//! (`myriad_merope::speaking::soul`), her personality from its fields
//! (`personality`), what goes out of a reply (`sending`), when she sees a
//! message and how long typing takes (`timing`), the look before it goes
//! (`draft`), and the prompts she writes her bits, her story and a new
//! persona with (`bits`, `self_story`, `onboarding_prompts`).

use myriad_merope::{
    bits, draft, onboarding_prompts, personality, self_story, sending,
    speaking::{self, Home},
    talk_shape::Shape,
    timing::{self, Where},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Nothing, input, text};
use crate::Failure;

pub(super) fn home(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(tag = "at", rename_all = "camelCase", deny_unknown_fields)]
    enum In {
        /// The site, by its name as people say it (`speaking::site_name`).
        Site {
            name: String,
        },
        Mac,
    }
    let home = match input(raw)? {
        In::Site { name } => Home::site(&name),
        In::Mac => Home::mac(),
    };
    Ok(json!({ "home": home }))
}

pub(super) fn soul(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        name: String,
        personality: String,
        home: Home,
    }
    let req: In = input(raw)?;
    Ok(text(speaking::soul(&req.name, &req.personality, &req.home)))
}

pub(super) fn contract_at(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        place: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(speaking::speaking_contract_at(&req.place))))
}

pub(super) fn flatten(raw: &[u8]) -> Result<Value, Failure> {
    /// [`personality::Fields`], refusing a field it does not know.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields, default)]
    #[derive(Default)]
    struct Fields {
        summary: String,
        temperament: Vec<String>,
        likes: Vec<String>,
        drives: Vec<String>,
        social_style: String,
        speech_style: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        fields: Fields,
        language: String,
    }
    let req: In = input(raw)?;
    let fields = personality::Fields {
        summary: req.fields.summary,
        temperament: req.fields.temperament,
        likes: req.fields.likes,
        drives: req.fields.drives,
        social_style: req.fields.social_style,
        speech_style: req.fields.speech_style,
    };
    Ok(text(Some(personality::flatten(&fields, &req.language))))
}

pub(super) fn goes_out(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        reply: String,
        /// How people there type (`talk_shape::Shape`); how people in a
        /// chat app usually do when absent.
        #[serde(default)]
        room: Option<Shape>,
        rolls: sending::Rolls,
    }
    let req: In = input(raw)?;
    serde_json::to_value(sending::goes_out(&req.reply, req.room.as_ref(), &req.rolls))
        .map_err(|error| Failure::BadInput(error.to_string()))
}

pub(super) fn limit_take(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        /// Tokens said before `piece`, as the last answer gave them.
        used: f64,
        piece: String,
        /// All of the reply so far, `piece` with it.
        raw: String,
    }
    let req: In = input(raw)?;
    let mut limit = sending::Limit { used: req.used };
    Ok(match limit.take(&req.piece, &req.raw) {
        Some((stop, kept)) => json!({ "used": limit.used, "stop": stop, "kept": kept }),
        None => json!({ "used": limit.used, "stop": null, "kept": null }),
    })
}

/// [`Where`] with its fields in camelCase.
#[derive(Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum At {
    InTalk,
    Free,
    Busy { free_in: f64 },
    Asleep { wakes_in: f64 },
}

impl From<Where> for At {
    fn from(at: Where) -> Self {
        match at {
            Where::InTalk => Self::InTalk,
            Where::Free => Self::Free,
            Where::Busy { free_in } => Self::Busy { free_in },
            Where::Asleep { wakes_in } => Self::Asleep { wakes_in },
        }
    }
}

impl From<At> for Where {
    fn from(at: At) -> Self {
        match at {
            At::InTalk => Self::InTalk,
            At::Free => Self::Free,
            At::Busy { free_in } => Self::Busy { free_in },
            At::Asleep { wakes_in } => Self::Asleep { wakes_in },
        }
    }
}

pub(super) fn where_she_is(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        in_talk: bool,
        may_sleep: bool,
        /// Seconds until she wakes, when she is asleep now
        /// (`timing.asleep_at`).
        #[serde(default)]
        wakes_in: Option<f64>,
        /// Seconds until what she is doing is done.
        #[serde(default)]
        free_in: Option<f64>,
    }
    let req: In = input(raw)?;
    let at = timing::where_she_is(req.in_talk, req.may_sleep, req.wakes_in, req.free_in);
    Ok(json!({ "at": At::from(at) }))
}

pub(super) fn until_read(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        at: At,
        text: String,
        u1: f64,
        u2: f64,
    }
    let req: In = input(raw)?;
    let seconds = timing::until_read(req.at.into(), req.text.chars().count(), req.u1, req.u2);
    Ok(json!({ "seconds": seconds }))
}

pub(super) fn typing(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        text: String,
        u: f64,
    }
    let req: In = input(raw)?;
    Ok(json!({ "seconds": timing::typing(req.text.chars().count(), req.u) }))
}

pub(super) fn draft_ask(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Came {
        who: String,
        text: String,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        said: String,
        draft: String,
        /// What came while she wrote it, oldest first.
        came: Vec<Came>,
    }
    let req: In = input(raw)?;
    if !draft::asks(&req.came) {
        return Ok(json!({ "ask": null }));
    }
    let came: Vec<(String, String)> = req.came.into_iter().map(|c| (c.who, c.text)).collect();
    Ok(json!({ "ask": {
        "system": draft::system(),
        "input": draft::input(&req.said, &req.draft, &came),
        "schemaName": draft::SCHEMA_NAME,
        "schema": draft::schema(),
        "withinSeconds": draft::LOOK_WITHIN_SECONDS,
    } }))
}

pub(super) fn draft_send(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        /// Her raw answer; absent when none came in time.
        #[serde(default)]
        answer: Option<String>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "send": draft::send(req.answer.as_deref()) }))
}

pub(super) fn bits_ask(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        soul: String,
        group: bool,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "system": bits::system(&req.soul, req.group),
        "schemaName": bits::SCHEMA_NAME,
        "schema": bits::schema(req.group),
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    id: String,
    row: String,
    line: String,
    missed: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Claim {
    text: String,
    rests_on: Vec<String>,
}

fn records(records: &[Record]) -> Vec<self_story::Record> {
    records
        .iter()
        .map(|record| self_story::Record {
            id: record.id.clone(),
            row: record.row.clone(),
            line: record.line.clone(),
            missed: record.missed,
        })
        .collect()
}

fn claims(before: &[Claim]) -> Vec<self_story::Claim> {
    before
        .iter()
        .map(|claim| self_story::Claim {
            text: claim.text.clone(),
            rests_on: claim.rests_on.clone(),
        })
        .collect()
}

pub(super) fn self_story_ask(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        soul: String,
        records: Vec<Record>,
        tally: Value,
        /// Last time's claims.
        before: Vec<Claim>,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "system": self_story::system(&req.soul),
        "input": self_story::input(&records(&req.records), &req.tally, &claims(&req.before)),
        "schemaName": self_story::SCHEMA_NAME,
        "schema": self_story::schema(),
    }))
}

pub(super) fn self_story_checked(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        /// Her raw answer.
        answer: String,
        records: Vec<Record>,
        tally: Value,
        before: Vec<Claim>,
    }
    let req: In = input(raw)?;
    let kept = self_story::parse(&req.answer).and_then(|written| {
        self_story::checked(
            written,
            &records(&req.records),
            &req.tally,
            &claims(&req.before),
        )
    });
    let kept: Option<Vec<Claim>> = kept.map(|claims| {
        claims
            .into_iter()
            .map(|claim| Claim {
                text: claim.text,
                rests_on: claim.rests_on,
            })
            .collect()
    });
    Ok(json!({ "claims": kept }))
}

pub(super) fn onboarding_tags(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(text(Some(
        onboarding_prompts::TAGS_SYSTEM_PROMPT.to_string(),
    )))
}

pub(super) fn onboarding_name(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        /// `chinese`, `japanese`, `european` or `mythic`; any other is
        /// `chinese`.
        name_style: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(onboarding_prompts::name_system_prompt(
        &req.name_style,
    ))))
}

pub(super) fn onboarding_persona(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(text(Some(
        onboarding_prompts::PERSONA_SYSTEM_PROMPT.to_string(),
    )))
}

pub(super) fn onboarding_import(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(text(Some(
        onboarding_prompts::IMPORT_PERSONA_SYSTEM_PROMPT.to_string(),
    )))
}

pub(super) fn onboarding_visual_design(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(text(
        Some(onboarding_prompts::visual_design_system_prompt()),
    ))
}

pub(super) fn onboarding_observe_portrait(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        language: String,
        gender: String,
    }
    let req: In = input(raw)?;
    Ok(text(Some(
        onboarding_prompts::observe_portrait_visual_prompt(&req.language, &req.gender),
    )))
}
