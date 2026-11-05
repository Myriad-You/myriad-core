//! What kind of event a key is (`myriad_merope::notify`): said only in
//! person, a task's outcome, worth interrupting someone away for, worth a
//! notification at all.

use myriad_merope::notify;
use serde::Deserialize;
use serde_json::{Value, json};

use super::input;
use crate::Failure;

pub(super) fn kind(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        kind: String,
    }
    let req: In = input(raw)?;
    let kind = req.kind.as_str();
    Ok(json!({
        "inPerson": notify::said_only_in_person(kind),
        "taskOutcome": notify::is_task_outcome(kind),
        "interruptsWhenAway": notify::interrupts_when_away(kind),
        "worthNotifying": notify::worth_notifying(kind),
    }))
}
