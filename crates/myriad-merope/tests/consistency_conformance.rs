//! The shared conformance cases for whether she is the same on every end
//! (`crates/conformance/v1/consistency/`) through the crate, as the site
//! backend calls it. `myriad-core-ffi` runs the same files through the C
//! entry point. The outputs were written from these rules when they came in
//! (2026-11-03); `MYRIAD_CONFORMANCE_BLESS=1` rewrites the files whose
//! outputs a change written up with its reason moved, and only those.

use std::path::PathBuf;

use myriad_merope::consistency::{self, Measures, PromptMessage, PromptRecording, Turn};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn input<T: DeserializeOwned>(input: &Value) -> Option<T> {
    serde_json::from_value(input.clone()).ok()
}

fn run(call: &str, raw: &Value) -> Option<Value> {
    Some(match call {
        "consistency.measure" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                turns: Vec<Turn>,
            }
            let req: In = input(raw)?;
            json!({ "measures": consistency::measure(&req.turns) })
        }
        "consistency.measure_runs" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                runs: Vec<Vec<Turn>>,
            }
            let req: In = input(raw)?;
            json!({ "measures": consistency::measure_runs(&req.runs) })
        }
        "consistency.apart" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                left: Measures,
                right: Measures,
            }
            let req: In = input(raw)?;
            json!({ "apart": consistency::apart(&req.left, &req.right) })
        }
        "consistency.prompt_segments" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                messages: Vec<PromptMessage>,
            }
            let req: In = input(raw)?;
            json!({ "segments": consistency::segments(&req.messages) })
        }
        "consistency.prompt_diff" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct In {
                left: PromptRecording,
                right: PromptRecording,
            }
            let req: In = input(raw)?;
            if !consistency::readable(&req.left) || !consistency::readable(&req.right) {
                return None;
            }
            json!({ "diff": consistency::prompt_diff(&req.left, &req.right) })
        }
        other => panic!("no such call {other}"),
    })
}

#[test]
fn every_consistency_case_answers_through_the_crate() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/v1/consistency");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    let bless = std::env::var_os("MYRIAD_CONFORMANCE_BLESS").is_some();
    let mut cases = 0;
    for path in files {
        let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let call = file["call"].as_str().unwrap().to_owned();
        let mut moved = false;
        for case in file["cases"].as_array_mut().unwrap() {
            let got = run(&call, &case["input"]).unwrap_or_else(|| json!({ "error": "bad_input" }));
            cases += 1;
            if bless {
                moved |= got != case["output"];
                case["output"] = got;
                continue;
            }
            // Values compare as JSON: the same keys, the same numbers to the bit.
            assert_eq!(got, case["output"], "{} › {}", path.display(), case["name"]);
        }
        if moved {
            let text = serde_json::to_string_pretty(&file).unwrap() + "\n";
            std::fs::write(&path, text).unwrap();
        }
    }
    assert!(cases >= 14, "{cases}");
}
