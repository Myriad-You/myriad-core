//! Her acting, directed by one director: the judgment model answering typed
//! questions about the moment (`questions`). Its answers become a bounded
//! expression/posture baseline, cues, grounded `phrases` and a `score`; the
//! floor (`floor`) shows her state until it lands or when it has nothing.
//! Anime2.5DRig driver values, lip sync, blinking, breathing and secondary
//! motion remain deterministic on the client.
//!
//! Only the judgments are here: which questions are asked, how the answers
//! are read and checked, and how they land on her body, while speaking or
//! standing. Asking the model, its timeouts, the persona read and the events
//! a direction goes out on are each platform's.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::affect::MoodTransition;
use crate::{
    ChatPerformancePlan, RIG_STATE_MOTION_STYLES, RigStateSummary, ScoreBeat, SpeechPhrase,
    cue_is_playable, grounded_score, grounded_speech_phrases, parse_performance_plan,
    plan_is_empty, refine_performance_plan,
};

pub mod control;
pub mod floor;
mod offered;
pub mod presence;
pub mod questions;
pub mod standing;
pub mod way;

pub use floor::local_performance_plan;
pub use offered::{offered_body_controls, offered_score_moves, semantic_valid};

/// A standing score's period (control::STANDING_LOOP_MS): long enough not
/// to read as a loop, short enough to come round.
pub const STANDING_LOOP_MS: u32 = control::STANDING_LOOP_MS as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionPhase {
    Reaction,
    Delivery,
    Outcome,
    Proactive,
    Mood,
    /// Nobody is talking with her: how she acts what she is doing on her own.
    Presence,
}

impl MotionPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reaction => "reaction",
            Self::Delivery => "delivery",
            Self::Outcome => "outcome",
            Self::Proactive => "proactive",
            Self::Mood => "mood",
            Self::Presence => "presence",
        }
    }

    pub fn activity(self) -> &'static str {
        match self {
            Self::Reaction => "thinking",
            Self::Delivery | Self::Outcome | Self::Proactive => "talking",
            Self::Mood | Self::Presence => "idle",
        }
    }
}

/// The moment the director is asked about: what was said, her reply so far,
/// her mood and what her face can do this round.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Moment {
    pub phase: MotionPhase,
    pub mood: MoodTransition,
    pub activity: String,
    #[serde(default)]
    pub user_text: String,
    #[serde(default)]
    pub response_text: Option<String>,
    /// Recently issued intentions, not proof that the body has played them.
    #[serde(default)]
    pub previous_phrases: Vec<SpeechPhrase>,
    #[serde(default)]
    pub task_success: Option<bool>,
    #[serde(default)]
    pub rig_state: Option<RigStateSummary>,
    /// Resolved once per Chat/Work round. Client style is only a fallback.
    pub motion_style: String,
}

impl Moment {
    /// Her face's state with this round's way of moving: what the questions
    /// are asked of and the plan is filtered by.
    pub fn round_rig_state(&self) -> Option<RigStateSummary> {
        apply_round_motion_style(self.rig_state.clone(), &self.motion_style)
    }

    /// Nobody sees her face: no acting at all, only ambient motion.
    pub fn face_hidden(&self) -> bool {
        face_is_hidden(self.round_rig_state().as_ref())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MotionDecision {
    Continue,
    Perform(ChatPerformancePlan),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceDirective {
    pub phase: MotionPhase,
    pub mood_revision: i64,
    pub motion_style: String,
    pub plan: ChatPerformancePlan,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phrases: Vec<SpeechPhrase>,
    /// Beats on the reply's words or on time, so she can act while speaking,
    /// listening, thinking or idle alike.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub score: Vec<ScoreBeat>,
    /// Standing acting only: the score plays again each this many milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loop_ms: Option<u32>,
}

/// What came of the director's answer (`direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nobody sees her face: nothing is acted.
    Hidden,
    /// The director chose to go on with what plays: nothing, and no floor
    /// replayed over it.
    Continued,
    /// The director's own plan.
    Director,
    /// The floor, as the director had nothing.
    Floor,
    /// The plan had nothing this face can play.
    Unplayable,
    /// No answer and no floor wanted (it is already playing).
    Nothing,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Continued => "continued",
            Self::Director => "director",
            Self::Floor => "floor",
            Self::Unplayable => "unplayable",
            Self::Nothing => "none",
        }
    }
}

/// The director's answer, read: what to play, if anything.
#[derive(Debug, Clone)]
pub struct Direction {
    pub outcome: Outcome,
    /// An answer came but was not a valid plan.
    pub dropped: bool,
    pub directive: Option<PerformanceDirective>,
}

impl Direction {
    fn none(outcome: Outcome, dropped: bool) -> Self {
        Self {
            outcome,
            dropped,
            directive: None,
        }
    }
}

/// The director's `answer` for this moment as what she plays: `None` when it
/// had no answer (an error or a timeout). With `fallback_to_local` the floor
/// plays when the director has nothing; without, a floor already published
/// stands and the same local beat is never replayed.
pub fn direction(moment: &Moment, answer: Option<&str>, fallback_to_local: bool) -> Direction {
    let rig_state = moment.round_rig_state();
    if face_is_hidden(rig_state.as_ref()) {
        return Direction::none(Outcome::Hidden, false);
    }
    let mut phrases = Vec::new();
    let mut score = Vec::new();
    let mut loop_ms = None;
    let mut asked_for = None;
    let mut dropped = false;
    let directed = match answer {
        Some(raw) => match parse_motion_decision(raw) {
            // Deliberate continuation is not a failed call: direct and
            // proactive callers must not replay a floor over it.
            Some(MotionDecision::Continue) => return Direction::none(Outcome::Continued, false),
            Some(MotionDecision::Perform(parsed)) => {
                if let Ok(value) = serde_json::from_str::<Value>(strip_motion_json(raw)) {
                    asked_for = value["askedFor"].as_str().map(str::to_owned);
                    phrases =
                        grounded_speech_phrases(&value["phrases"], moment.response_text.as_deref());
                    score = grounded_score(
                        &value["score"],
                        moment.response_text.as_deref(),
                        rig_state
                            .as_ref()
                            .map(|state| state.capabilities.as_slice()),
                    );
                    if moment.phase == MotionPhase::Presence && !score.is_empty() {
                        loop_ms = Some(STANDING_LOOP_MS);
                    }
                }
                Some(parsed)
            }
            None => {
                dropped = true;
                None
            }
        },
        None => None,
    };
    let directed_here = directed.is_some();
    let mut plan = if let Some(plan) = directed {
        plan
    } else if fallback_to_local {
        local_performance_plan(
            moment.phase,
            &moment.mood,
            moment.task_success,
            &moment.motion_style,
        )
    } else {
        return Direction::none(Outcome::Nothing, dropped);
    };
    let asked =
        asked_for.and_then(|intent| plan.cues.iter().find(|cue| cue.intent == intent).cloned());
    if let Some(state) = rig_state.as_ref() {
        plan = keep_agreed_face(refine_performance_plan(plan, state), asked, state);
    }
    if plan_is_empty(&plan) && phrases.is_empty() && score.is_empty() {
        return Direction::none(Outcome::Unplayable, dropped);
    }
    Direction {
        outcome: if directed_here {
            Outcome::Director
        } else {
            Outcome::Floor
        },
        dropped,
        directive: Some(PerformanceDirective {
            phase: moment.phase,
            mood_revision: moment.mood.revision,
            motion_style: moment.motion_style.clone(),
            plan,
            phrases,
            score,
            loop_ms,
        }),
    }
}

/// A face she agreed to make is made even over her words: it survives the
/// state filter whenever the face can make it.
fn keep_agreed_face(
    mut plan: ChatPerformancePlan,
    agreed: Option<crate::ChatPerformanceCue>,
    state: &RigStateSummary,
) -> ChatPerformancePlan {
    if let Some(cue) = agreed.filter(|cue| {
        cue_is_playable(&state.capabilities, &cue.intent)
            && !plan.cues.iter().any(|kept| kept.intent == cue.intent)
    }) {
        plan.cues.insert(0, cue);
        plan.cues.truncate(3);
    }
    plan
}

/// Her standing acting when the director cannot be reached (offline): shown
/// the `way` what she is doing shows by its kind (`presence::WAYS`), with no
/// network call.
pub fn standing_floor(context: &Moment, way: &str) -> Option<PerformanceDirective> {
    let rig_state = context.round_rig_state();
    if face_is_hidden(rig_state.as_ref()) {
        return None;
    }
    let value = presence::standing_floor(way, context, rig_state.as_ref());
    let Some(MotionDecision::Perform(mut plan)) = parse_motion_decision(&value.to_string()) else {
        return None;
    };
    if let Some(state) = rig_state.as_ref() {
        plan = refine_performance_plan(plan, state);
    }
    let score = grounded_score(
        &value["score"],
        None,
        rig_state
            .as_ref()
            .map(|state| state.capabilities.as_slice()),
    );
    Some(PerformanceDirective {
        phase: MotionPhase::Presence,
        mood_revision: context.mood.revision,
        motion_style: context.motion_style.clone(),
        plan,
        phrases: Vec::new(),
        score,
        loop_ms: Some(STANDING_LOOP_MS),
    })
}

/// The deterministic floor as a directive, with no model call.
///
/// Chat plays this the moment the round starts so the character reacts before
/// it speaks; the director's answer replaces it.
pub fn local_directive(context: &Moment) -> Option<PerformanceDirective> {
    let rig_state = context.round_rig_state();
    if face_is_hidden(rig_state.as_ref()) {
        return None;
    }
    let mut plan = local_performance_plan(
        context.phase,
        &context.mood,
        context.task_success,
        &context.motion_style,
    );
    if let Some(state) = rig_state.as_ref() {
        plan = refine_performance_plan(plan, state);
    }
    if plan_is_empty(&plan) {
        return None;
    }
    Some(PerformanceDirective {
        phase: context.phase,
        mood_revision: context.mood.revision,
        motion_style: context.motion_style.clone(),
        plan,
        phrases: Vec::new(),
        score: Vec::new(),
        loop_ms: None,
    })
}

pub fn apply_round_motion_style(
    summary: Option<RigStateSummary>,
    motion_style: &str,
) -> Option<RigStateSummary> {
    let mut summary = summary?;
    summary.motion_style = if RIG_STATE_MOTION_STYLES.contains(&motion_style) {
        motion_style.to_string()
    } else {
        summary.motion_style
    };
    Some(summary)
}

pub fn face_is_hidden(state: Option<&RigStateSummary>) -> bool {
    state.is_some_and(|summary| !summary.page_visible || !summary.face_visible)
}

pub fn parse_motion_decision(raw: &str) -> Option<MotionDecision> {
    let stripped = strip_motion_json(raw);
    let value: serde_json::Value = serde_json::from_str(stripped).ok()?;
    let object = value.as_object()?;
    match object.get("continue") {
        Some(flag) if !flag.is_boolean() => return None,
        Some(flag) if flag.as_bool() == Some(true) => {
            if motion_payload_present(object) {
                return None;
            }
            return Some(MotionDecision::Continue);
        }
        _ => {}
    }
    if let Some(plan) = parse_performance_plan(stripped) {
        return Some(MotionDecision::Perform(plan));
    }
    // A grounded phrase refinement does not need to replace the standing face.
    // Do not turn an invalid baseline/cue payload into a phrase-only success.
    let empty_plan = object.get("baseline").is_none_or(Value::is_null)
        && object
            .get("cues")
            .is_none_or(|value| value.as_array().is_some_and(Vec::is_empty));
    let valid_score = !grounded_score(
        object.get("score").unwrap_or(&Value::Null),
        // Words are checked against the reply later; here only the beat's shape.
        Some(&scored_words(object)),
        None,
    )
    .is_empty();
    let valid_phrase = object
        .get("phrases")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().take(6).any(|item| {
                !grounded_speech_phrases(
                    &serde_json::json!([item]),
                    item.get("text").and_then(Value::as_str),
                )
                .is_empty()
            })
        });
    (empty_plan && (valid_phrase || valid_score))
        .then(|| MotionDecision::Perform(ChatPerformancePlan::default()))
}

/// The words a score's beats name, so its shape can be checked without the reply.
fn scored_words(object: &serde_json::Map<String, Value>) -> String {
    object
        .get("score")
        .and_then(Value::as_array)
        .map(|beats| {
            beats
                .iter()
                .filter_map(|beat| beat.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn motion_payload_present(object: &serde_json::Map<String, serde_json::Value>) -> bool {
    let has_baseline = object.get("baseline").is_some_and(|value| !value.is_null());
    let has_cues = object
        .get("cues")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|cues| !cues.is_empty());
    has_baseline
        || has_cues
        || object
            .get("score")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
        || object
            .get("phrases")
            .and_then(Value::as_array)
            .is_some_and(|items| !items.is_empty())
}

pub fn strip_motion_json(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|inner| inner.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim()
}

#[cfg(test)]
mod tests;
