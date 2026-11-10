//! What a look at the owner's screen may become of hers
//! (`myriad_merope::screen`): her words without the window's, the notes she
//! is offered with it, and the fact of a look she only knew of.

use myriad_merope::screen;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Zone, at, input, on_zone};
use crate::Failure;

pub(super) fn unquoted(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        took_in: Option<String>,
        #[serde(default)]
        mood: Option<String>,
        #[serde(default)]
        note: Option<String>,
        /// The window's text she was given with the look.
        #[serde(default)]
        shown: Option<String>,
    }
    let req: In = input(raw)?;
    let take = screen::unquoted(
        screen::Take {
            took_in: req.took_in,
            mood: req.mood,
            note: req.note,
        },
        req.shown.as_deref(),
    );
    Ok(json!({ "tookIn": take.took_in, "mood": take.mood, "note": take.note }))
}

pub(super) fn notes(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Note {
        kind: String,
        title: String,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
        aliases: Vec<String>,
        #[serde(default)]
        threads: Vec<String>,
        #[serde(default)]
        active: bool,
        #[serde(default)]
        updated_at: i64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        app: String,
        #[serde(default)]
        thing: Option<String>,
        notes: Vec<Note>,
    }
    let req: In = input(raw)?;
    let candidates: Vec<screen::Note> = req
        .notes
        .iter()
        .map(|note| screen::Note {
            kind: &note.kind,
            title: &note.title,
            name: note.name.as_deref(),
            summary: note.summary.as_deref(),
            aliases: &note.aliases,
            threads: &note.threads,
            active: note.active,
            updated_at: note.updated_at,
        })
        .collect();
    let picked: Vec<Value> = screen::notes(&req.app, req.thing.as_deref(), &candidates)
        .into_iter()
        .enumerate()
        .map(|(place, index)| json!({ "label": screen::label(place), "index": index }))
        .collect();
    Ok(json!({ "notes": picked }))
}

pub(super) fn fact(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        app: String,
        #[serde(default)]
        thing: Option<String>,
        /// Microseconds since the epoch.
        at: i64,
        zone: String,
        language: String,
    }
    let req: In = input(raw)?;
    let when = at(req.at)?;
    let zone = Zone::parse(&req.zone)?;
    let text = on_zone!(zone, |zone| screen::fact(
        &req.app,
        req.thing.as_deref(),
        when,
        &req.language,
        &zone
    ));
    Ok(json!({ "text": text, "source": screen::FACT_SOURCE }))
}
