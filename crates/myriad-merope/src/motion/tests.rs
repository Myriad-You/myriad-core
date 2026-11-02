//! Unit tests for `motion.rs`, kept beside it so the module stays readable.

use super::*;
use crate::{ChatPerformanceCue, PERFORMANCE_CUE_INTENTS, cue_survives_state};

#[test]
fn phases_have_stable_wire_names() {
    assert_eq!(MotionPhase::Reaction.as_str(), "reaction");
    assert_eq!(MotionPhase::Reaction.activity(), "thinking");
    assert_eq!(MotionPhase::Delivery.activity(), "talking");
    assert_eq!(MotionPhase::Mood.activity(), "idle");
    assert_eq!(
        serde_json::to_string(&MotionPhase::Delivery).unwrap(),
        "\"delivery\""
    );
}

#[test]
fn directive_wire_shape_is_camel_case_and_semantic_only() {
    let value = serde_json::to_value(PerformanceDirective {
        phase: MotionPhase::Reaction,
        mood_revision: 42,
        motion_style: "open".to_string(),
        phrases: vec![SpeechPhrase {
            text: "你觉得呢？".to_string(),
            intent: "check-in".to_string(),
        }],
        score: Vec::new(),
        loop_ms: None,
        plan: ChatPerformancePlan {
            baseline: None,
            cues: vec![crate::ChatPerformanceCue {
                intent: "listen".to_string(),
                at_ms: 0,
                intensity: 1.0,
                tempo: 1.0,
                fade_in_ms: 100,
                fade_out_ms: 200,
                interrupt: "if-lower".to_string(),
            }],
        },
    })
    .unwrap();
    assert_eq!(value["phase"], "reaction");
    assert_eq!(value["moodRevision"], 42);
    assert_eq!(value["motionStyle"], "open");
    assert!(value.pointer("/plan/cues/0/atMs").is_some());
    assert!(value.get("driver").is_none());
    assert_eq!(value["phrases"][0]["intent"], "check-in");
    assert_eq!(value["phrases"][0]["text"], "你觉得呢？");
}

fn rig(capabilities: &[&str], speaking: bool) -> RigStateSummary {
    crate::sanitize_rig_state(&serde_json::json!({
        "expression": "steady",
        "posture": "neutral",
        "owners": { "mouth": "idle", "expression": "idle", "gaze": "idle", "headBody": "idle" },
        "speaking": speaking,
        "capabilities": capabilities,
    }))
    .expect("summary")
}

/// Refining keeps exactly the cues that survive the rig state.
#[test]
fn refining_keeps_exactly_the_cues_that_survive_the_state() {
    for (capabilities, speaking) in [
        (&["head-body", "mouth-shapes"][..], false),
        (&["head-body", "cry-eye", "silly-eye"][..], false),
        (&["head-body", "maniac-mouth", "silly-mouth"][..], true),
        (&[][..], false),
    ] {
        let state = rig(capabilities, speaking);
        let surviving: Vec<&str> = PERFORMANCE_CUE_INTENTS
            .iter()
            .copied()
            .filter(|intent| cue_survives_state(&state, intent))
            .collect();
        assert!(!surviving.is_empty(), "{capabilities:?}");

        let plan = ChatPerformancePlan {
            baseline: None,
            cues: PERFORMANCE_CUE_INTENTS
                .iter()
                .map(|intent| ChatPerformanceCue {
                    intent: (*intent).to_string(),
                    at_ms: 0,
                    intensity: 1.0,
                    tempo: 1.0,
                    fade_in_ms: 120,
                    fade_out_ms: 200,
                    interrupt: "replace".to_string(),
                })
                .collect(),
        };
        let survived: Vec<String> = refine_performance_plan(plan, &state)
            .cues
            .into_iter()
            .map(|cue| cue.intent)
            .collect();
        assert_eq!(survived, surviving, "{capabilities:?} speaking={speaking}");
    }
}

#[test]
fn explicit_continue_is_not_a_plan() {
    assert_eq!(
        parse_motion_decision(r#"{"continue":true}"#),
        Some(MotionDecision::Continue)
    );
    assert_eq!(
        parse_motion_decision(r#"{"continue":true,"cues":[]}"#),
        Some(MotionDecision::Continue)
    );
    assert_eq!(
        parse_motion_decision("```json\n{\"continue\": true}\n```"),
        Some(MotionDecision::Continue)
    );
}

#[test]
fn empty_object_is_invalid_not_continue() {
    assert_eq!(parse_motion_decision("{}"), None);
    assert_eq!(parse_motion_decision(r#"{"cues":[]}"#), None);
    assert_eq!(parse_motion_decision(r#"{"continue":false}"#), None);
}

#[test]
fn illegal_baseline_without_cues_is_invalid() {
    assert_eq!(
        parse_motion_decision(
            r#"{"baseline":{"expression":"angry","posture":"attack"},"cues":[]}"#
        ),
        None
    );
}

#[test]
fn legal_plan_is_perform() {
    let decision = parse_motion_decision(
        r#"{"cues":[{"intent":"listen","atMs":0,"intensity":1,"tempo":1,"fadeInMs":80,"fadeOutMs":120,"interrupt":"if-lower"}]}"#,
    );
    match decision {
        Some(MotionDecision::Perform(plan)) => {
            assert_eq!(plan.cues.len(), 1);
            assert_eq!(plan.cues[0].intent, "listen");
        }
        other => panic!("expected perform, got {other:?}"),
    }
}

#[test]
fn continue_with_a_plan_is_rejected() {
    assert_eq!(
        parse_motion_decision(r#"{"continue":true,"cues":[{"intent":"listen"}]}"#),
        None
    );
    assert_eq!(
        parse_motion_decision(
            r#"{"continue":true,"baseline":{"expression":"warm","posture":"open","motionEnergy":1,"attention":0.8}}"#
        ),
        None
    );
}

#[test]
fn chat_director_phrase_only_decision_needs_no_replacement_pose() {
    assert_eq!(
        parse_motion_decision(r#"{"phrases":[{"text":"你觉得呢？","intent":"check-in"}]}"#),
        Some(MotionDecision::Perform(ChatPerformancePlan::default()))
    );
    for invalid in [
        r#"{"phrases":[{"text":"你觉得呢？","intent":"driver"}]}"#,
        r#"{"baseline":{},"phrases":[{"text":"你觉得呢？","intent":"ask"}]}"#,
        r#"{"continue":true,"phrases":[{"text":"你觉得呢？","intent":"ask"}]}"#,
    ] {
        assert_eq!(parse_motion_decision(invalid), None);
    }
}

#[test]
fn a_director_with_no_answer_still_leaves_the_round_something_to_play() {
    let plan = local_performance_plan(
        MotionPhase::Delivery,
        &MoodTransition {
            before: 50.0,
            after: 50.0,
            arousal_before: 48.0,
            arousal_after: 48.0,
            band_before: "calm".to_string(),
            band_after: "calm".to_string(),
            delta: 0.0,
            cause: "test".to_string(),
            revision: 1,
        },
        None,
        "even",
    );
    assert!(!plan_is_empty(&plan));
}

#[test]
fn hidden_face_skips_motion() {
    let hidden = crate::sanitize_rig_state(&serde_json::json!({
        "pageVisible": false,
        "faceVisible": true
    }))
    .unwrap();
    assert!(face_is_hidden(Some(&hidden)));
    let no_face = crate::sanitize_rig_state(&serde_json::json!({
        "pageVisible": true,
        "faceVisible": false
    }))
    .unwrap();
    assert!(face_is_hidden(Some(&no_face)));
    assert!(!face_is_hidden(None));
}

#[test]
fn a_score_alone_is_a_performance_and_its_moves_follow_the_body() {
    // Only beats, no baseline: still new direction, e.g. while listening.
    assert!(matches!(
        parse_motion_decision(r#"{"score":[{"atMs":300,"move":{"kind":"nod","count":2}}]}"#),
        Some(MotionDecision::Perform(_))
    ));
    assert!(
        parse_motion_decision(r#"{"score":[{"atMs":300,"move":{"kind":"moonwalk"}}]}"#).is_none()
    );
    assert!(
        parse_motion_decision(r#"{"continue":true,"score":[{"atMs":0,"move":{"kind":"nod"}}]}"#)
            .is_none()
    );

    let head_only = rig(&["head-body"], false);
    let kinds = offered_score_moves(Some(&head_only));
    assert!(kinds.contains(&"nod") && kinds.contains(&"shrug"));
    assert!(!kinds.contains(&"beat"), "no hand beat without arms");
    assert!(
        !kinds.contains(&"glance"),
        "no eye glance without independent eyes"
    );
}

#[test]
fn standing_acting_is_directed_on_time_and_loops() {
    assert_eq!(MotionPhase::Presence.as_str(), "presence");
    assert_eq!(MotionPhase::Presence.activity(), "idle");
}

#[test]
fn a_face_she_agreed_to_make_plays_even_while_she_speaks() {
    let cue = ChatPerformanceCue {
        intent: "maniac".into(),
        at_ms: 0,
        intensity: 1.3,
        tempo: 1.0,
        fade_in_ms: 180,
        fade_out_ms: 420,
        interrupt: "replace".into(),
    };
    let plan = ChatPerformancePlan {
        baseline: None,
        cues: vec![cue.clone()],
    };
    let speaking = rig(&["head-body", "mouth-shapes", "maniac-mouth"], true);
    let filtered = refine_performance_plan(plan.clone(), &speaking);
    assert!(filtered.cues.is_empty(), "the state filter alone drops it");
    let kept = keep_agreed_face(filtered, Some(cue.clone()), &speaking);
    assert_eq!(kept.cues[0].intent, "maniac");
    let no_mouth = rig(&["head-body"], true);
    let dropped = keep_agreed_face(
        refine_performance_plan(plan, &no_mouth),
        Some(cue),
        &no_mouth,
    );
    assert!(dropped.cues.is_empty(), "not a face this body can make");
}

#[test]
fn offline_her_standing_acting_still_shows_what_she_is_doing() {
    for way in [
        "reading",
        "listening",
        "daydreaming",
        "looking-about",
        "resting",
    ] {
        let context = Moment {
            phase: MotionPhase::Presence,
            mood: serde_json::from_value(serde_json::json!({"before": 60.0, "after": 60.0,
                "bandBefore": "calm", "bandAfter": "calm", "delta": 0.0, "cause": "standing", "revision": 1}))
            .unwrap(),
            activity: format!("{way} on her own"),
            user_text: String::new(),
            response_text: None,
            previous_phrases: Vec::new(),
            task_success: None,
            rig_state: Some(rig(&["head-body", "independent-eyes", "mouth-shapes"], false)),
            motion_style: "even".into(),
        };
        let floor = standing_floor(&context, way).expect(way);
        assert_eq!(floor.loop_ms, Some(STANDING_LOOP_MS), "{way}");
        assert!(!floor.score.is_empty(), "{way} has beats");
    }
}
