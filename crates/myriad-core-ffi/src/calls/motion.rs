//! Her acting (`myriad_merope::motion`): the questions put to the director
//! about a moment, its answers as a plan, the plan as what she plays, and
//! her standing acting. Asking the model is the platform's.

use myriad_merope::motion::{
    self, Moment, PerformanceDirective, questions,
    standing::{self, Doing},
    way,
};
use myriad_merope::sources::Thing;
use serde::Deserialize;
use serde_json::{Value, json};

use super::input;
use crate::Failure;

/// The questions about the moment and the moment as the model reads it; a
/// face nobody sees is not acted, so nothing is asked.
pub(super) fn asked(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        moment: Moment,
        #[serde(default)]
        persona: Option<String>,
        direct: bool,
    }
    let req: In = input(raw)?;
    let state = req.moment.round_rig_state();
    if motion::face_is_hidden(state.as_ref()) {
        return Ok(json!({ "hidden": true }));
    }
    let (asked_state, asked) = questions::asked(
        &req.moment,
        state.as_ref(),
        req.persona.as_deref(),
        req.direct,
    );
    Ok(json!({ "hidden": false, "state": asked_state, "questions": asked }))
}

/// The same questions for a chat model: its system prompt and the schema of
/// its answers.
pub(super) fn chat_prompt(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        questions: Value,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "system": questions::chat_system_prompt(&req.questions),
        "schema": questions::chat_schema(&req.questions),
    }))
}

/// The answers as the plan, as the director would have written it.
pub(super) fn plan(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        moment: Moment,
        answers: Value,
        direct: bool,
    }
    let req: In = input(raw)?;
    let state = req.moment.round_rig_state();
    Ok(json!({ "plan": questions::plan(&req.answers, &req.moment, state.as_ref(), req.direct) }))
}

/// The director's answer (`motion.plan`'s plan as text, or none when it had
/// no answer) as what she plays.
pub(super) fn direction(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        moment: Moment,
        #[serde(default)]
        answer: Option<String>,
        fallback_to_local: bool,
    }
    let req: In = input(raw)?;
    let direction = motion::direction(&req.moment, req.answer.as_deref(), req.fallback_to_local);
    Ok(json!({
        "outcome": direction.outcome.as_str(),
        "dropped": direction.dropped,
        "directive": direction.directive,
    }))
}

/// The floor as a directive: her state, played before the director answers.
pub(super) fn floor(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        moment: Moment,
    }
    let req: In = input(raw)?;
    Ok(json!({ "directive": motion::local_directive(&req.moment) }))
}

/// Her standing acting by the kind of what she is doing, when the director
/// cannot be reached.
pub(super) fn standing_floor(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        moment: Moment,
        way: String,
    }
    let req: In = input(raw)?;
    Ok(json!({ "directive": motion::standing_floor(&req.moment, &req.way) }))
}

/// What she is doing on her own, as the director hears it, and the way it
/// shows (`presence::WAYS`) when the director cannot be reached.
pub(super) fn standing_doing(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        #[serde(default)]
        thing: Option<Thing>,
        #[serde(default)]
        lazing: Option<String>,
        together: bool,
    }
    let req: In = input(raw)?;
    let doing = match (&req.thing, &req.lazing) {
        (Some(thing), None) => Doing::Thing(thing),
        (None, Some(kind)) => Doing::Lazing(kind),
        _ => {
            return Err(Failure::BadInput("give one of thing and lazing".into()));
        }
    };
    let (way, activity) = standing::doing(doing, req.together);
    Ok(json!({ "way": way, "activity": activity }))
}

/// Whether a standing direction the director gave is any acting.
pub(super) fn standing_acting(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        directive: PerformanceDirective,
    }
    let req: In = input(raw)?;
    Ok(json!({ "acting": standing::is_acting(&req.directive) }))
}

/// A new look's way there: the change the answers make and the two
/// questions about it, or none when the look stays.
pub(super) fn way(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        moment: Moment,
        answers: Value,
    }
    let req: In = input(raw)?;
    let state = req.moment.round_rig_state();
    let way = way::change(&req.answers, state.as_ref()).map(|(from, to)| {
        let (way_state, way_questions) = way::questions(&from, &to, &req.moment, state.as_ref());
        json!({ "from": from, "to": to, "state": way_state, "questions": way_questions })
    });
    Ok(json!({ "way": way }))
}

/// The plan with the looks the answers about the way there pass through.
pub(super) fn lay_way(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        plan: Value,
        answers: Value,
    }
    let mut req: In = input(raw)?;
    way::lay(&mut req.plan, &req.answers);
    Ok(json!({ "plan": req.plan }))
}
