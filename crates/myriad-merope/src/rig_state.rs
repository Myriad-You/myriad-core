//! Semantic live-face summary for the Lite motion director.
//!
//! No bones, coordinates, angles, raw drivers, frames, or pixels.

use serde::{Deserialize, Serialize};

use crate::ChatPerformancePlan;
use crate::rig_contract::{
    PERFORMANCE_BASELINE_EXPRESSIONS, PERFORMANCE_CUE_INTENTS, PERFORMANCE_POSTURES,
};

pub const RIG_STATE_CHANNEL_OWNERS: &[&str] = &[
    "preview",
    "speech",
    "performance",
    "music",
    "coSpeech",
    "mood",
    "pointer",
    "ambient",
    "autonomy",
    "idle",
];
pub const RIG_STATE_SPECIAL_INTENTS: &[&str] = &[
    "dizzy",
    "cry",
    "angry",
    "speechless",
    "maniac",
    "silly",
    "lovestruck",
];
/// Cue intents that occupy the mouth. Capability names (`cry-mouth`) are not
/// intents; substring matching them would never fire on a parsed plan.
pub const RIG_STATE_MOUTH_INTENTS: &[&str] = &["cry", "maniac", "silly"];
pub const RIG_STATE_HEAD_BODY_INTENTS: &[&str] = &[
    "greet",
    "question",
    "delight",
    "emphasize",
    "notify",
    "angry",
    "speechless",
    "maniac",
    "silly",
    "lovestruck",
];
pub const RIG_STATE_CAPABILITIES: &[&str] = &[
    "blink",
    "independent-eyes",
    "dizzy-eye",
    "squeeze-eye",
    "cry-eye",
    "silly-eye",
    "lovestruck",
    "cry-mouth",
    "maniac-mouth",
    "silly-mouth",
    "mouth-shapes",
    "head-body",
];
pub const RIG_STATE_MUSIC_ENERGIES: &[&str] = &["quiet", "soft", "present", "strong"];
pub const RIG_STATE_BEAT_PHASES: &[&str] = &["rest", "downbeat", "pulse", "hold"];
pub const RIG_STATE_MOTION_STYLES: &[&str] = &["restrained", "even", "open"];
pub const MAX_RECENT_ACTIONS: usize = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigActingSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    pub phase: String,
    pub remaining_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigChannelOwners {
    pub mouth: String,
    pub expression: String,
    pub gaze: String,
    pub head_body: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigMusicSummary {
    pub energy: String,
    pub beat: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RigStateSummary {
    pub expression: String,
    pub posture: String,
    pub acting: RigActingSummary,
    pub owners: RigChannelOwners,
    pub speaking: bool,
    pub singing: bool,
    pub music_playing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub music: Option<RigMusicSummary>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub recent_intents: Vec<String>,
    pub motion_style: String,
    pub page_visible: bool,
    pub face_visible: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSummary {
    #[serde(default)]
    expression: Option<String>,
    #[serde(default)]
    posture: Option<String>,
    #[serde(default)]
    acting: Option<RawActing>,
    #[serde(default)]
    owners: Option<RawOwners>,
    #[serde(default)]
    speaking: bool,
    #[serde(default)]
    singing: bool,
    #[serde(default)]
    music_playing: bool,
    #[serde(default)]
    music: Option<RawMusic>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    recent_intents: Vec<String>,
    #[serde(default)]
    motion_style: Option<String>,
    #[serde(default = "default_true")]
    page_visible: bool,
    #[serde(default = "default_true")]
    face_visible: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawActing {
    #[serde(default)]
    intent: Option<String>,
    #[serde(default)]
    phase: Option<String>,
    #[serde(default)]
    remaining_ms: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOwners {
    #[serde(default)]
    mouth: Option<String>,
    #[serde(default)]
    expression: Option<String>,
    #[serde(default)]
    gaze: Option<String>,
    #[serde(default)]
    head_body: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMusic {
    #[serde(default)]
    energy: Option<String>,
    #[serde(default)]
    beat: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Drops bones, drivers, frames, and unknown vocabulary. Missing fields idle out.
pub fn sanitize_rig_state(value: &serde_json::Value) -> Option<RigStateSummary> {
    let raw: RawSummary = serde_json::from_value(value.clone()).ok()?;
    Some(sanitize_raw(raw))
}

fn sanitize_raw(raw: RawSummary) -> RigStateSummary {
    let expression = allow(
        raw.expression.as_deref(),
        PERFORMANCE_BASELINE_EXPRESSIONS,
        "steady",
    );
    let posture = allow(raw.posture.as_deref(), PERFORMANCE_POSTURES, "neutral");
    let acting_intent = raw
        .acting
        .as_ref()
        .and_then(|acting| acting.intent.as_deref())
        .filter(|intent| PERFORMANCE_CUE_INTENTS.contains(intent))
        .map(str::to_string);
    let phase = allow(
        raw.acting
            .as_ref()
            .and_then(|acting| acting.phase.as_deref()),
        &[
            "reaction",
            "delivery",
            "outcome",
            "proactive",
            "mood",
            "idle",
        ],
        "idle",
    );
    let remaining_ms = raw
        .acting
        .as_ref()
        .map(|acting| acting.remaining_ms.min(12_000))
        .unwrap_or(0);
    let owners = RigChannelOwners {
        mouth: owner(
            raw.owners
                .as_ref()
                .and_then(|owners| owners.mouth.as_deref()),
        ),
        expression: owner(
            raw.owners
                .as_ref()
                .and_then(|owners| owners.expression.as_deref()),
        ),
        gaze: owner(
            raw.owners
                .as_ref()
                .and_then(|owners| owners.gaze.as_deref()),
        ),
        head_body: owner(
            raw.owners
                .as_ref()
                .and_then(|owners| owners.head_body.as_deref()),
        ),
    };
    let music = raw.music.and_then(|music| {
        let energy = allow(music.energy.as_deref(), RIG_STATE_MUSIC_ENERGIES, "");
        let beat = allow(music.beat.as_deref(), RIG_STATE_BEAT_PHASES, "");
        (energy != "quiet" || beat != "rest" || raw.music_playing).then_some(RigMusicSummary {
            energy: if energy.is_empty() {
                "quiet".to_string()
            } else {
                energy
            },
            beat: if beat.is_empty() {
                "rest".to_string()
            } else {
                beat
            },
        })
    });
    RigStateSummary {
        expression,
        posture,
        acting: RigActingSummary {
            intent: acting_intent,
            phase,
            remaining_ms,
        },
        owners,
        speaking: raw.speaking,
        singing: raw.singing,
        music_playing: raw.music_playing,
        music,
        capabilities: raw
            .capabilities
            .into_iter()
            .filter(|cap| RIG_STATE_CAPABILITIES.contains(&cap.as_str()))
            .take(RIG_STATE_CAPABILITIES.len())
            .collect(),
        recent_intents: raw
            .recent_intents
            .into_iter()
            .filter(|intent| PERFORMANCE_CUE_INTENTS.contains(&intent.as_str()))
            .take(MAX_RECENT_ACTIONS)
            .collect(),
        motion_style: allow(raw.motion_style.as_deref(), RIG_STATE_MOTION_STYLES, "even"),
        page_visible: raw.page_visible,
        face_visible: raw.face_visible,
    }
}

fn owner(value: Option<&str>) -> String {
    allow(value, RIG_STATE_CHANNEL_OWNERS, "idle")
}

fn allow(value: Option<&str>, allowed: &[&str], fallback: &str) -> String {
    value
        .filter(|item| allowed.contains(item))
        .unwrap_or(fallback)
        .to_string()
}

pub fn plan_is_empty(plan: &ChatPerformancePlan) -> bool {
    plan.baseline.is_none() && plan.cues.is_empty()
}

/// Client `motionStyle` is a fallback. A loaded persona always wins.
pub fn round_motion_style(
    client_style: Option<&str>,
    persona_json: Option<&serde_json::Value>,
    persona_found: bool,
    mood: i32,
) -> String {
    if persona_found {
        return motion_style_from_persona_json(persona_json, mood).to_string();
    }
    allow(client_style, RIG_STATE_MOTION_STYLES, "even")
}

pub fn motion_style_from_persona_json(
    persona_json: Option<&serde_json::Value>,
    mood: i32,
) -> &'static str {
    let temperament = persona_json
        .and_then(|value| value.get("temperament"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let social = persona_json
        .and_then(|value| value.get("socialStyle"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    motion_style_from_persona(&temperament, social, mood)
}

pub fn motion_style_from_persona(
    temperament: &[String],
    social_style: &str,
    mood: i32,
) -> &'static str {
    let blob = format!("{} {}", temperament.join(" "), social_style).to_lowercase();
    if mood <= 40
        || contains_any(
            &blob,
            &[
                "克制",
                "慢热",
                "内向",
                "quiet",
                "reserved",
                "shy",
                "restrained",
            ],
        )
    {
        return "restrained";
    }
    if mood >= 75
        || contains_any(
            &blob,
            &[
                "活泼", "开放", "外向", "bright", "playful", "open", "cheerful",
            ],
        )
    {
        return "open";
    }
    "even"
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

/// Drop cues the current face cannot play, and keep music groove when music owns the body.
///
/// Sticker expressions still need their layers. Generic acting is allowed when
/// `capabilities` is empty so a missing summary does not wipe the face. Callers
/// that have no `rigState` at all skip this function so old clients keep prior
/// behavior.
pub fn refine_performance_plan(
    mut plan: ChatPerformancePlan,
    state: &RigStateSummary,
) -> ChatPerformancePlan {
    let music_owns_body = state.owners.head_body == "music" || state.singing;
    if let Some(baseline) = plan.baseline.as_mut() {
        if baseline.posture != "neutral" && music_owns_body {
            baseline.posture = "neutral".to_string();
        }
    }
    plan.cues.retain(|cue| {
        capability_allows(&state.capabilities, &cue.intent)
            && !(music_owns_body && RIG_STATE_HEAD_BODY_INTENTS.contains(&cue.intent.as_str()))
            && !(state.speaking && cue_takes_mouth(&cue.intent))
    });
    plan
}

fn has_cap(capabilities: &[String], name: &str) -> bool {
    capabilities.iter().any(|cap| cap == name)
}

fn cue_takes_mouth(intent: &str) -> bool {
    RIG_STATE_MOUTH_INTENTS.contains(&intent)
}

fn capability_allows(capabilities: &[String], intent: &str) -> bool {
    match intent {
        "dizzy" => has_cap(capabilities, "dizzy-eye"),
        "cry" => has_cap(capabilities, "cry-eye") || has_cap(capabilities, "cry-mouth"),
        "silly" => has_cap(capabilities, "silly-eye") || has_cap(capabilities, "silly-mouth"),
        "maniac" => has_cap(capabilities, "maniac-mouth"),
        "lovestruck" => has_cap(capabilities, "lovestruck"),
        _ => {
            if capabilities.is_empty() {
                return true;
            }
            !RIG_STATE_HEAD_BODY_INTENTS.contains(&intent) || has_cap(capabilities, "head-body")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn drops_driver_and_angle_fields() {
        let summary = sanitize_rig_state(&json!({
            "expression": "warm",
            "posture": "open",
            "angleX": 0.4,
            "driver": { "mouthOpen": 1 },
            "owners": { "mouth": "speech", "headBody": "music" },
            "speaking": true,
            "singing": true,
            "recentIntents": ["delight", "angleZ"],
            "capabilities": ["dizzy-eye", "psd-layer"],
            "motionStyle": "open"
        }))
        .unwrap();
        let encoded = serde_json::to_value(&summary).unwrap();
        assert!(encoded.get("angleX").is_none());
        assert!(encoded.get("driver").is_none());
        assert_eq!(summary.owners.mouth, "speech");
        assert_eq!(summary.owners.head_body, "music");
        assert_eq!(summary.recent_intents, vec!["delight"]);
        assert_eq!(summary.capabilities, vec!["dizzy-eye"]);
    }

    #[test]
    fn persona_style_is_restrained_when_mood_is_low() {
        assert_eq!(motion_style_from_persona(&[], "", 20), "restrained");
        assert_eq!(motion_style_from_persona(&["活泼".into()], "", 80), "open");
    }

    #[test]
    fn refine_keeps_a_repeated_special_when_the_face_can_play_it() {
        let state = sanitize_rig_state(&json!({
            "recentIntents": ["silly"],
            "capabilities": ["silly-eye", "head-body"]
        }))
        .unwrap();
        let plan = ChatPerformancePlan {
            baseline: None,
            cues: vec![crate::ChatPerformanceCue {
                intent: "silly".into(),
                at_ms: 0,
                intensity: 1.0,
                tempo: 1.0,
                fade_in_ms: 80,
                fade_out_ms: 120,
                interrupt: "replace".into(),
            }],
        };
        let refined = refine_performance_plan(plan, &state);
        assert_eq!(refined.cues.len(), 1);
        assert_eq!(refined.cues[0].intent, "silly");
    }

    #[test]
    fn refine_drops_body_cues_while_singing() {
        let state = sanitize_rig_state(&json!({
            "owners": { "headBody": "music" },
            "singing": true,
            "recentIntents": ["silly"],
            "capabilities": ["silly-eye", "head-body"]
        }))
        .unwrap();
        let plan = ChatPerformancePlan {
            baseline: None,
            cues: vec![
                crate::ChatPerformanceCue {
                    intent: "silly".into(),
                    at_ms: 0,
                    intensity: 1.0,
                    tempo: 1.0,
                    fade_in_ms: 80,
                    fade_out_ms: 120,
                    interrupt: "replace".into(),
                },
                crate::ChatPerformanceCue {
                    intent: "greet".into(),
                    at_ms: 0,
                    intensity: 1.0,
                    tempo: 1.0,
                    fade_in_ms: 80,
                    fade_out_ms: 120,
                    interrupt: "replace".into(),
                },
                crate::ChatPerformanceCue {
                    intent: "listen".into(),
                    at_ms: 0,
                    intensity: 1.0,
                    tempo: 1.0,
                    fade_in_ms: 80,
                    fade_out_ms: 120,
                    interrupt: "if-lower".into(),
                },
            ],
        };
        let refined = refine_performance_plan(plan, &state);
        assert_eq!(refined.cues.len(), 1);
        assert_eq!(refined.cues[0].intent, "listen");
    }

    #[test]
    fn refine_neutralizes_open_posture_while_singing() {
        let state = sanitize_rig_state(&json!({
            "owners": { "headBody": "music" },
            "singing": true,
            "capabilities": ["head-body"]
        }))
        .unwrap();
        let plan = ChatPerformancePlan {
            baseline: Some(crate::ChatPerformanceBaseline {
                expression: "warm".into(),
                posture: "open".into(),
                motion_energy: 1.0,
                attention: 1.0,
            }),
            cues: vec![crate::ChatPerformanceCue {
                intent: "listen".into(),
                at_ms: 0,
                intensity: 1.0,
                tempo: 1.0,
                fade_in_ms: 80,
                fade_out_ms: 120,
                interrupt: "replace".into(),
            }],
        };
        let refined = refine_performance_plan(plan, &state);
        assert_eq!(refined.baseline.as_ref().unwrap().posture, "neutral");
        assert_eq!(refined.cues[0].intent, "listen");
    }

    #[test]
    fn refine_drops_mouth_cues_while_speaking() {
        assert!(
            !RIG_STATE_MOUTH_INTENTS.is_empty(),
            "emptying RIG_STATE_MOUTH_INTENTS silently disables the speaking-mouth filter",
        );
        for intent in RIG_STATE_MOUTH_INTENTS {
            assert!(
                PERFORMANCE_CUE_INTENTS.contains(intent),
                "{intent} must survive parse_performance_plan",
            );
        }
        let state = sanitize_rig_state(&json!({
            "speaking": true,
            "capabilities": ["head-body", "maniac-mouth", "cry-mouth", "silly-mouth"]
        }))
        .unwrap();
        let plan = crate::parse_performance_plan(
            r#"{"cues":[{"intent":"listen","atMs":0,"intensity":1,"tempo":1,"fadeInMs":80,"fadeOutMs":120,"interrupt":"replace"},{"intent":"maniac","atMs":0,"intensity":1,"tempo":1,"fadeInMs":80,"fadeOutMs":120,"interrupt":"replace"}]}"#,
        )
        .unwrap();
        assert_eq!(plan.cues.len(), 2);
        let refined = refine_performance_plan(plan, &state);
        assert_eq!(refined.cues.len(), 1);
        assert_eq!(refined.cues[0].intent, "listen");
    }

    #[test]
    fn empty_capabilities_drop_stickers_but_keep_generic_acting() {
        let state = sanitize_rig_state(&json!({
            "capabilities": []
        }))
        .unwrap();
        assert!(state.capabilities.is_empty());
        let plan = ChatPerformancePlan {
            baseline: Some(crate::ChatPerformanceBaseline {
                expression: "warm".into(),
                posture: "open".into(),
                motion_energy: 1.0,
                attention: 0.8,
            }),
            cues: vec![
                crate::ChatPerformanceCue {
                    intent: "dizzy".into(),
                    at_ms: 0,
                    intensity: 1.0,
                    tempo: 1.0,
                    fade_in_ms: 80,
                    fade_out_ms: 120,
                    interrupt: "replace".into(),
                },
                crate::ChatPerformanceCue {
                    intent: "greet".into(),
                    at_ms: 0,
                    intensity: 1.0,
                    tempo: 1.0,
                    fade_in_ms: 80,
                    fade_out_ms: 120,
                    interrupt: "replace".into(),
                },
                crate::ChatPerformanceCue {
                    intent: "listen".into(),
                    at_ms: 0,
                    intensity: 1.0,
                    tempo: 1.0,
                    fade_in_ms: 80,
                    fade_out_ms: 120,
                    interrupt: "if-lower".into(),
                },
                crate::ChatPerformanceCue {
                    intent: "think".into(),
                    at_ms: 40,
                    intensity: 1.0,
                    tempo: 1.0,
                    fade_in_ms: 80,
                    fade_out_ms: 120,
                    interrupt: "if-lower".into(),
                },
            ],
        };
        let refined = refine_performance_plan(plan, &state);
        assert_eq!(refined.baseline.as_ref().unwrap().posture, "open");
        let intents: Vec<_> = refined.cues.iter().map(|cue| cue.intent.as_str()).collect();
        assert_eq!(intents, vec!["greet", "listen", "think"]);
    }

    #[test]
    fn persona_overrides_client_motion_style() {
        assert_eq!(
            round_motion_style(
                Some("open"),
                Some(&json!({"socialStyle": "内向"})),
                true,
                70
            ),
            "restrained"
        );
        assert_eq!(round_motion_style(Some("open"), None, false, 20), "open");
        assert_eq!(round_motion_style(None, None, false, 20), "even");
    }
}
