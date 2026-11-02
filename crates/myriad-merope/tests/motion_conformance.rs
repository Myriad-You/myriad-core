//! The shared conformance cases for her acting
//! (`crates/conformance/v1/motion/`) through the crate, as the site backend
//! calls it. `myriad-core-ffi` runs the same files through the C entry
//! point. The outputs were recorded from the site backend's director before
//! its judgments moved here (2026-11-02); nothing here rewrites them.

use std::path::PathBuf;

use myriad_merope::motion::{
    self, Moment, PerformanceDirective, questions,
    standing::{self, Doing},
    way,
};
use myriad_merope::sources::Thing;
use serde_json::{Value, json};

fn moment(input: &Value) -> Moment {
    serde_json::from_value(input["moment"].clone()).unwrap()
}

fn run(call: &str, input: &Value) -> Value {
    match call {
        "motion.asked" => {
            let moment = moment(input);
            let state = moment.round_rig_state();
            if moment.face_hidden() {
                return json!({ "hidden": true });
            }
            let (asked_state, asked) = questions::asked(
                &moment,
                state.as_ref(),
                input["persona"].as_str(),
                input["direct"].as_bool().unwrap(),
            );
            json!({ "hidden": false, "state": asked_state, "questions": asked })
        }
        "motion.chat_prompt" => json!({
            "system": questions::chat_system_prompt(&input["questions"]),
            "schema": questions::chat_schema(&input["questions"]),
        }),
        "motion.plan" => {
            let moment = moment(input);
            let state = moment.round_rig_state();
            let direct = input["direct"].as_bool().unwrap();
            json!({ "plan": questions::plan(&input["answers"], &moment, state.as_ref(), direct) })
        }
        "motion.direction" => {
            let direction = motion::direction(
                &moment(input),
                input["answer"].as_str(),
                input["fallbackToLocal"].as_bool().unwrap(),
            );
            json!({
                "outcome": direction.outcome.as_str(),
                "dropped": direction.dropped,
                "directive": direction.directive,
            })
        }
        "motion.floor" => json!({ "directive": motion::local_directive(&moment(input)) }),
        "motion.standing_floor" => json!({
            "directive": motion::standing_floor(&moment(input), input["way"].as_str().unwrap()),
        }),
        "motion.standing_doing" => {
            let thing: Option<Thing> = serde_json::from_value(input["thing"].clone()).unwrap();
            let doing = match (&thing, input["lazing"].as_str()) {
                (Some(thing), None) => Doing::Thing(thing),
                (None, Some(kind)) => Doing::Lazing(kind),
                _ => return json!({ "error": "bad_input" }),
            };
            let (way, activity) = standing::doing(doing, input["together"].as_bool().unwrap());
            json!({ "way": way, "activity": activity })
        }
        "motion.standing_acting" => {
            let directive: PerformanceDirective =
                serde_json::from_value(input["directive"].clone()).unwrap();
            json!({ "acting": standing::is_acting(&directive) })
        }
        "motion.way" => {
            let moment = moment(input);
            let state = moment.round_rig_state();
            let way = way::change(&input["answers"], state.as_ref()).map(|(from, to)| {
                let (way_state, way_questions) =
                    way::questions(&from, &to, &moment, state.as_ref());
                json!({ "from": from, "to": to, "state": way_state, "questions": way_questions })
            });
            json!({ "way": way })
        }
        "motion.lay_way" => {
            let mut plan = input["plan"].clone();
            way::lay(&mut plan, &input["answers"]);
            json!({ "plan": plan })
        }
        other => panic!("no such call {other}"),
    }
}

#[test]
fn every_motion_case_answers_through_the_crate() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/motion");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut cases = 0;
    for path in files {
        let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap();
        for case in file["cases"].as_array().unwrap() {
            // Values compare as JSON: the same keys, the same numbers to the bit.
            assert_eq!(
                run(call, &case["input"]),
                case["output"],
                "{} › {}",
                path.display(),
                case["name"]
            );
            cases += 1;
        }
    }
    assert!(cases >= 300, "{cases}");
}
