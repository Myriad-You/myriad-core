//! Mind-wandering while someone is here and quiet (`myriad_merope::wander`):
//! when her mind may drift to them, and what the thought is. Where it drifts
//! is `memory.wander`.

use myriad_merope::wander;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{at, input, opt_at};
use crate::Failure;

pub(super) fn quiet(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct In {
        /// When they last said anything.
        user_at: Option<i64>,
        /// When she last spoke up to them unasked.
        proactive_at: Option<i64>,
        /// When her mind last settled on a thought about them.
        thought_at: Option<i64>,
        /// Her own cooldown between speaking up, longer when she is tired.
        cooldown_seconds: i64,
        now: i64,
    }
    let req: In = input(raw)?;
    Ok(json!({
        "quiet": wander::quiet_enough(
            opt_at(req.user_at)?,
            opt_at(req.proactive_at)?,
            opt_at(req.thought_at)?,
            req.cooldown_seconds,
            at(req.now)?,
        ),
    }))
}

pub(super) fn thought(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Gap {
        name: String,
        known: usize,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        content: String,
        gap: Option<Gap>,
    }
    let req: In = input(raw)?;
    let gap = req.gap.as_ref().map(|gap| (gap.name.as_str(), gap.known));
    Ok(json!({ "text": wander::thought(&req.content, gap), "event": wander::THOUGHT_EVENT }))
}
