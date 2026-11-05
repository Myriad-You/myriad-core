//! Whether she is the same person on every end: her replies by the numbers,
//! and two ends' whole prompts for one scene set side by side
//! (`myriad_merope::consistency`).

use myriad_merope::consistency::{self, Measures, PromptMessage, PromptRecording, Turn};
use serde::Deserialize;
use serde_json::{Value, json};

use super::input;
use crate::Failure;

pub(super) fn measure(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        turns: Vec<Turn>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "measures": consistency::measure(&req.turns) }))
}

pub(super) fn measure_runs(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        runs: Vec<Vec<Turn>>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "measures": consistency::measure_runs(&req.runs) }))
}

pub(super) fn apart(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        left: Measures,
        right: Measures,
    }
    let req: In = input(raw)?;
    Ok(json!({ "apart": consistency::apart(&req.left, &req.right) }))
}

pub(super) fn prompt_segments(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        messages: Vec<PromptMessage>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "segments": consistency::segments(&req.messages) }))
}

pub(super) fn prompt_diff(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        left: PromptRecording,
        right: PromptRecording,
    }
    let req: In = input(raw)?;
    if !consistency::readable(&req.left) || !consistency::readable(&req.right) {
        return Err(Failure::BadInput(
            "a prompt recording of another version".into(),
        ));
    }
    Ok(json!({ "diff": consistency::prompt_diff(&req.left, &req.right) }))
}
