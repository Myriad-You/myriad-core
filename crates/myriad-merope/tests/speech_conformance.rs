//! The shared conformance cases for what she means to say
//! (`crates/conformance/v1/speech/`) through the crate, as the site backend
//! calls it. `myriad-core-ffi` runs the same files through the C entry
//! point. `MYRIAD_CONFORMANCE_BLESS=1` rewrites their `output` from the
//! crate, only with a change that says why.
//!
//! Each admission case also says what the site and the Mac decide today
//! (`today`: the verdict, what decided it, and where). What a rule decided
//! (do-not-disturb, the audience, the grants, the same line or thing, a wish
//! past its time, face to face only) admission still decides the same way;
//! what a gate decided (cooldowns, the same kind of thing a while ago, her
//! night, being busy) is no longer refused but told to her.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use myriad_agent_rules::memory::audience::Audience;
use myriad_merope::speech::{
    self, Admit, Said,
    choice::{self, Hand, Held},
    markers,
};
use serde::Deserialize;
use serde_json::{Value, json};

fn conformance() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/speech")
}

fn read<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, Value> {
    serde_json::from_value(value).map_err(|_| json!({ "error": "bad_input" }))
}

fn run(call: &str, input: &Value) -> Result<Value, Value> {
    match call {
        "speech.admit" => {
            let mut fields = input.as_object().cloned().unwrap();
            let zone = fields
                .remove("zone")
                .and_then(|zone| zone.as_str().map(str::to_string));
            let zone = zone
                .and_then(|zone| chrono_tz::Tz::from_str(&zone).ok())
                .ok_or_else(|| json!({ "error": "bad_input" }))?;
            let admit: Admit = read(Value::Object(fields))?;
            Ok(serde_json::to_value(speech::admit(&admit, &zone)).unwrap())
        }
        "speech.told" => {
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
            let req: In = read(input.clone())?;
            Ok(serde_json::to_value(speech::told(
                &req.ledger,
                &req.to,
                req.thing.as_deref(),
                req.text.as_deref(),
                req.now,
            ))
            .unwrap())
        }
        "speech.choice_prompt" => {
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
            let req: In = read(input.clone())?;
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
        "speech.choice_parse" => {
            let raw = input["raw"].as_str().unwrap();
            let hands: Vec<Hand> = read(input["hands"].clone())?;
            Ok(json!({ "choice": choice::parse(raw, &hands) }))
        }
        "speech.markers" => Ok(json!({ "markers": markers::MARKERS })),
        "speech.strip_markers" => {
            let (text, held) = markers::shown(input["text"].as_str().unwrap());
            Ok(json!({ "text": text, "held": held }))
        }
        "speech.texts" => Ok(json!({
            "fusePerHour": speech::FUSE_PER_HOUR,
            "fuseWindowSeconds": speech::FUSE_WINDOW / speech::SECOND,
            "latelyShown": speech::LATELY_SHOWN,
            "choiceTimeoutSeconds": choice::TIMEOUT_SECS,
            "choiceLineChars": choice::LINE_CHARS,
            "choiceSchemaName": choice::SCHEMA_NAME,
            "choiceSchema": choice::schema(),
        })),
        other => panic!("no call {other}"),
    }
}

/// What decided a case today, and what admission must answer for it.
fn check_today(name: &str, today: &Value, output: &Value) {
    let outcome = output["outcome"].as_str().unwrap_or("error");
    let by = today["by"].as_str().unwrap();
    match today["verdict"].as_str().unwrap() {
        "refused" => match by {
            // The four rules, a wish past its time, face to face only, and
            // the fuse: refused, for the same reason.
            "do_not_disturb" | "audience" | "grant" | "verbatim" | "same_thing" | "stale"
            | "not_there" | "fuse" => {
                let why = match by {
                    "do_not_disturb" => "doNotDisturb",
                    "grant" => "notGranted",
                    "same_thing" => "sameThing",
                    "not_there" => "notThere",
                    other => other,
                };
                assert_eq!(outcome, "refused", "{name}");
                assert_eq!(output["why"], why, "{name}");
            }
            // A gate: no longer refused; hers to judge, or said when she
            // was called on.
            _ => {
                assert!(
                    outcome == "ask" || outcome == "say",
                    "{name}: a gate is a fact now, got {output}"
                );
                if outcome == "ask" {
                    assert!(output["facts"].as_array().unwrap().len() > 1, "{name}");
                }
            }
        },
        "spoken" => assert!(outcome == "ask" || outcome == "say", "{name}: {output}"),
        "waited" => assert_eq!(outcome, "wait", "{name}"),
        other => panic!("{name}: verdict {other}"),
    }
}

#[test]
fn every_speech_case_answers_through_the_crate() {
    let bless = std::env::var("MYRIAD_CONFORMANCE_BLESS").as_deref() == Ok("1");
    let mut files: Vec<PathBuf> = std::fs::read_dir(conformance())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let mut cases = 0;
    for path in files {
        let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap().to_string();
        for case in file["cases"].as_array_mut().unwrap() {
            let got = run(&call, &case["input"]).unwrap_or_else(|error| error);
            let name = case["name"].as_str().unwrap().to_string();
            if let Some(today) = case.get("today") {
                check_today(&name, today, &got);
            }
            if bless {
                case["output"] = got;
            } else {
                assert_eq!(got, case["output"], "{} › {name}", path.display());
            }
            cases += 1;
        }
        if bless {
            let mut text = serde_json::to_string_pretty(&file).unwrap();
            text.push('\n');
            std::fs::write(&path, text).unwrap();
        }
    }
    assert!(cases >= 60, "{cases}");
}
