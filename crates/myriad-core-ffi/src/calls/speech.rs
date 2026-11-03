//! What she means to say, the ledger of what she said, and whether it is
//! said (`myriad_merope::speech`). The crate's types cross as they are;
//! times are microseconds like every call.

use std::collections::BTreeMap;

use myriad_agent_rules::memory::audience::Audience;
use myriad_merope::speech::{
    self, Admit, FUSE_PER_HOUR, FUSE_WINDOW, LATELY_SHOWN, Said, Wish,
    choice::{self, Hand, Held},
    markers,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Nothing, Zone, on_zone, quiet_input as input};
use crate::Failure;

fn answer(value: &impl serde::Serialize) -> Result<Value, Failure> {
    serde_json::to_value(value).map_err(|error| Failure::BadInput(error.to_string()))
}

pub(super) fn texts(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(json!({
        "fusePerHour": FUSE_PER_HOUR,
        "fuseWindowSeconds": FUSE_WINDOW / speech::SECOND,
        "latelyShown": LATELY_SHOWN,
        "choiceTimeoutSeconds": choice::TIMEOUT_SECS,
        "choiceLineChars": choice::LINE_CHARS,
        "choiceSchemaName": choice::SCHEMA_NAME,
        "choiceSchema": choice::schema(),
    }))
}

pub(super) fn admit(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        wish: Wish,
        #[serde(default)]
        ledger: Vec<Said>,
        #[serde(default)]
        granted: Vec<String>,
        #[serde(default)]
        dnd: bool,
        #[serde(default)]
        they_opened: bool,
        #[serde(default)]
        present: bool,
        now: i64,
        #[serde(default)]
        last_talk_at: Option<i64>,
        #[serde(default)]
        they_wrote_at: Option<i64>,
        #[serde(default)]
        doing: Option<String>,
        #[serde(default)]
        touched_at: Option<i64>,
        zone: String,
    }
    let req: In = input(raw)?;
    let zone = Zone::parse(&req.zone)?;
    let admit = Admit {
        wish: req.wish,
        ledger: req.ledger,
        granted: req.granted,
        dnd: req.dnd,
        they_opened: req.they_opened,
        present: req.present,
        now: req.now,
        last_talk_at: req.last_talk_at,
        they_wrote_at: req.they_wrote_at,
        doing: req.doing,
        touched_at: req.touched_at,
    };
    answer(&on_zone!(zone, |zone| speech::admit(&admit, &zone)))
}

pub(super) fn told(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        ledger: Vec<Said>,
        to: Audience<String>,
        #[serde(default)]
        thing: Option<String>,
        #[serde(default)]
        text: Option<String>,
        now: i64,
    }
    let req: In = input(raw)?;
    answer(&speech::told(
        &req.ledger,
        &req.to,
        req.thing.as_deref(),
        req.text.as_deref(),
        req.now,
    ))
}

pub(super) fn choice_prompt(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        soul: String,
        lives: String,
        #[serde(default)]
        at_pause: bool,
        hands: Vec<Hand>,
        #[serde(default)]
        held_by: Vec<Held>,
        #[serde(default)]
        scene: BTreeMap<String, String>,
        #[serde(default)]
        pause: Option<String>,
        #[serde(default)]
        reactions: Option<String>,
        now: i64,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "system": choice::system(&req.soul, &req.lives, req.at_pause),
        "input": choice::input(
            &req.hands,
            &req.held_by,
            &req.scene,
            req.pause.as_deref(),
            req.reactions.as_deref(),
            req.now,
        ),
    }))
}

pub(super) fn choice_parse(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        raw: String,
        hands: Vec<Hand>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "choice": choice::parse(&req.raw, &req.hands) }))
}

pub(super) fn markers(raw: &[u8]) -> Result<Value, Failure> {
    let Nothing {} = input(raw)?;
    Ok(json!({ "markers": markers::MARKERS }))
}

pub(super) fn strip_markers(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        text: String,
    }
    let req: In = input(raw)?;
    let (shown, held) = markers::shown(&req.text);
    Ok(json!({ "text": shown, "held": held }))
}
