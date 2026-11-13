//! The way from the look she holds to the one she has just taken, asked on
//! its own once that one is known. Put in the same round as the choice of
//! face, a decisions model names the new face again as the look on the way
//! (both models, 10-12); asked of the fixed change, it keeps most changes
//! straight and passes dozing to delight through waking up surprised.

use crate::{MAX_FACES_PASSED, PLAIN_FACE, RigStateSummary};
use serde_json::{Map, Value, json};

use super::control::{is_own, look_means, offered_faces, sure, taken_face};
use super::{Moment, MotionPhase};

/// The change of look the answers make, from the one she holds: each a
/// preset's name, or her ordinary face. None when the look stays, or when
/// there is no other look to pass through: the way can only go straight, and
/// a choice of `none` alone is refused by a decisions endpoint (422). The new
/// look is the one the plan takes (`control::lay_face`): someone else's
/// feeling she quotes, or a name the contract does not know, leaves her
/// ordinary face.
pub fn change(
    answers: &Value,
    context: &Moment,
    state: Option<&RigStateSummary>,
) -> Option<(String, String)> {
    let from = state
        .and_then(|state| state.face.clone())
        .unwrap_or_else(|| PLAIN_FACE.into());
    let to = taken_face(answers, is_own(answers, context))
        .unwrap_or(PLAIN_FACE)
        .to_owned();
    (from != to && looks(&from, &to, state).len() >= 2).then_some((from, to))
}

/// The looks the way from `from` to `to` may pass through, `none` first.
fn looks(from: &str, to: &str, state: Option<&RigStateSummary>) -> Map<String, Value> {
    let mut looks = Map::new();
    looks.insert(
        "none".into(),
        json!("nothing (more) in between: straight on"),
    );
    if from != PLAIN_FACE && to != PLAIN_FACE {
        looks.insert(
            PLAIN_FACE.into(),
            json!("her ordinary face: calming down, collecting herself"),
        );
    }
    for (name, means) in offered_faces(state) {
        if name != from && name != to {
            looks.insert(name.into(), json!(means));
        }
    }
    looks
}

/// The two questions of the way there: a look passed through first, and
/// rarely a second; her ordinary face is one of the looks.
pub fn questions(
    from: &str,
    to: &str,
    context: &Moment,
    state: Option<&RigStateSummary>,
) -> (Value, Value) {
    let looks = looks(from, to, state);
    let unprompted = context.user_text.trim().is_empty();
    let moment = match (&context.phase, context.response_text.as_deref()) {
        (MotionPhase::Presence, _) => format!("While she is {}", context.activity),
        (_, Some(says)) if unprompted => format!("She speaks up: 「{says}」"),
        (_, None) if unprompted => "She is about to speak up".to_owned(),
        (_, Some(reply)) => format!(
            "Someone said: 「{}」. She answers: 「{reply}」",
            context.user_text
        ),
        (_, None) => format!("Someone said: 「{}」", context.user_text),
    };
    let questions = json!({
        "first": {"type": "choice",
            "instructions": format!(
                "{moment}. Her face changes from {} to {}. Does it pass through another look on the way, \
                 as real faces sometimes do: dozing to delight through waking up surprised, tears to laughter \
                 through a wet smile, sulking to shyness through calming down, joy to sorrow through a moment \
                 of shock? Most changes go straight.",
                look_means(from), look_means(to)
            ),
            "criteria": looks.clone()},
        "then": {"type": "choice",
            "instructions": format!(
                "If it passes through one, does it pass through a second after that, before {}? Rarely.",
                look_means(to)
            ),
            "criteria": looks},
    });
    let state = json!({"her face now": look_means(from), "her new face": look_means(to)});
    (state, questions)
}

/// The looks passed through, in order: the first if she is sure of one,
/// then a second only after a first.
pub fn passed(answers: &Value) -> Vec<String> {
    let mut looks: Vec<String> = Vec::new();
    for key in ["first", "then"] {
        match sure(answers, key).filter(|look| *look != "none") {
            Some(look) if !looks.iter().any(|seen| seen == look) => looks.push(look.to_owned()),
            _ => break,
        }
    }
    looks.truncate(MAX_FACES_PASSED);
    looks
}

/// The plan with the looks the way there passes through (`passed`), when
/// there are any and the plan sets a look.
pub fn lay(plan: &mut Value, answers: &Value) {
    let passed = passed(answers);
    if !passed.is_empty() && plan["baseline"].is_object() {
        plan["baseline"]["via"] = json!(passed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(face: Option<&str>) -> RigStateSummary {
        crate::sanitize_rig_state(&json!({"face": face, "capabilities":
            ["head-body", "independent-eyes", "mouth-shapes"]}))
        .unwrap()
    }

    fn answer(choice: &str, confidence: f64) -> Value {
        json!({"choice": choice, "confidence": confidence})
    }

    fn moment(phase: &str) -> Moment {
        serde_json::from_value(json!({"phase": phase,
            "mood": {"before": 70.0, "after": 70.0, "bandBefore": "calm", "bandAfter": "calm",
                "delta": 0.0, "cause": "test", "revision": 1},
            "activity": "thinking", "userText": "tell me about that", "motionStyle": "even"}))
        .unwrap()
    }

    #[test]
    fn a_body_with_no_other_look_is_not_asked_the_way() {
        // Only a mouth: no face is offered, so from a held look to her
        // ordinary face the way could only go straight.
        let mouth =
            crate::sanitize_rig_state(&json!({"face": "drowsy", "capabilities": ["mouth-shapes"]}))
                .unwrap();
        assert!(offered_faces(Some(&mouth)).is_empty());
        let plain = json!({"face": answer(PLAIN_FACE, 0.9)});
        assert_eq!(change(&plain, &moment("reaction"), Some(&mouth)), None);
        // A body with faces is still asked, every choice with two options or more.
        let drowsy = state(Some("drowsy"));
        let (from, to) = change(&plain, &moment("reaction"), Some(&drowsy)).unwrap();
        let (_, asked) = questions(&from, &to, &moment("reaction"), Some(&drowsy));
        for key in ["first", "then"] {
            assert!(
                asked[key]["criteria"].as_object().unwrap().len() >= 2,
                "{asked}"
            );
        }
    }

    #[test]
    fn only_a_new_look_asks_the_way_there() {
        let drowsy = state(Some("drowsy"));
        let context = moment("reaction");
        let excited = json!({"face": answer("excited", 0.9)});
        assert_eq!(
            change(&excited, &context, Some(&drowsy)),
            Some(("drowsy".into(), "excited".into()))
        );
        assert_eq!(
            change(
                &json!({"face": answer("drowsy", 0.9)}),
                &context,
                Some(&drowsy)
            ),
            None
        );
        // No look chosen is her ordinary face; from it to it is no change.
        assert_eq!(
            change(
                &json!({"face": answer("none", 0.9)}),
                &context,
                Some(&drowsy)
            ),
            Some(("drowsy".into(), PLAIN_FACE.into()))
        );
        assert_eq!(change(&json!({}), &context, Some(&state(None))), None);
    }

    #[test]
    fn the_way_goes_to_the_look_the_plan_takes() {
        let drowsy = state(Some("drowsy"));
        // Someone else's delight she quotes is not a look she takes (`lay_face`):
        // her face goes to her ordinary one, not through looks to theirs.
        let quoted = json!({"own": {"noul": 0.05}, "face": answer("excited", 0.9)});
        assert_eq!(
            change(&quoted, &moment("delivery"), Some(&drowsy)),
            Some(("drowsy".into(), PLAIN_FACE.into()))
        );
        assert_eq!(
            change(&quoted, &moment("delivery"), Some(&state(None))),
            None
        );
        // Standing acting is always hers.
        assert_eq!(
            change(&quoted, &moment("presence"), Some(&drowsy)),
            Some(("drowsy".into(), "excited".into()))
        );
        // A look the contract does not know is no look at all.
        let unknown = json!({"face": answer("cackling", 0.9)});
        assert_eq!(
            change(&unknown, &moment("delivery"), Some(&drowsy)),
            Some(("drowsy".into(), PLAIN_FACE.into()))
        );
        // The plan agrees: the same answers name no face in it.
        let plan = super::super::control::control_plan(&quoted, &moment("delivery"), Some(&drowsy));
        assert!(plan["baseline"]["face"].is_null());
    }

    #[test]
    fn the_way_there_is_asked_of_this_change_without_its_own_ends() {
        let drowsy = state(Some("drowsy"));
        let mut context = moment("reaction");
        context.user_text = "醒醒！你中奖了！".into();
        context.response_text = Some("诶？！真的吗？！".into());
        let (asked_state, asked) = questions("drowsy", "excited", &context, Some(&drowsy));
        let first = &asked["first"];
        let instructions = first["instructions"].as_str().unwrap();
        assert!(instructions.contains("中奖") && instructions.contains("lids heavy"));
        assert!(
            first["criteria"]["surprised"].is_string() && first["criteria"]["plain"].is_string()
        );
        assert!(first["criteria"]["drowsy"].is_null() && first["criteria"]["excited"].is_null());
        assert!(
            asked_state["her new face"]
                .as_str()
                .unwrap()
                .contains("sparkles")
        );
        // To her ordinary face, that face is the end, not a look on the way.
        assert!(questions("drowsy", PLAIN_FACE, &context, Some(&drowsy)).1["first"]["criteria"]["plain"].is_null());
    }

    #[test]
    fn the_looks_passed_through_come_in_order_and_only_when_sure() {
        let both = json!({"first": answer("surprised", 0.9), "then": answer("plain", 0.6)});
        assert_eq!(
            passed(&both),
            vec!["surprised".to_owned(), PLAIN_FACE.to_owned()]
        );
        let straight = json!({"first": answer("none", 0.9), "then": answer("plain", 0.9)});
        assert!(passed(&straight).is_empty());
        let unsure = json!({"first": answer("surprised", 0.1)});
        assert!(passed(&unsure).is_empty());
        let twice = json!({"first": answer("surprised", 0.9), "then": answer("surprised", 0.9)});
        assert_eq!(passed(&twice), vec!["surprised".to_owned()]);
    }
}
