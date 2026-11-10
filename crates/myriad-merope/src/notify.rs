//! Which events may interrupt someone who is away, and which are said only
//! to someone here. Each end decides what she does with an event (the
//! decision, the face, the toast); what kind of event it is is decided once,
//! here.
//!
//! Keys are the notification catalog's (`agent.task_completed`,
//! `phantasi.source_error`, ...) and her own (`agent.merope.*`). The site
//! keeps the whole catalog (every key, its preference switch); it reads
//! from here which of its keys interrupt.

use crate::wander;

/// A touch on her face answers the moment and is never a notification.
pub const TOUCH: &str = "agent.merope.touch";
/// Her own moments: any of these may be a notification unless said only in
/// person.
pub const OWN_PREFIX: &str = "agent.merope.";

/// The end of a task, or a question it waits on the owner for.
pub const TASK_OUTCOMES: [&str; 4] = [
    "agent.task_completed",
    "agent.task_failed",
    "agent.task_cancelled",
    "agent.clarification",
];

/// Besides task outcomes, what is worth interrupting someone who is away.
pub const INTERRUPTS_WHEN_AWAY: [&str; 5] = [
    "phantasi.source_error",
    "platform.sync.failed",
    "agent.merope.platform_activity",
    "agent.merope.report_ready",
    "agent.merope.reach_out",
];

/// What a child end saw for her and told her host (her link's `moment`,
/// `shared/her_link_contract.json`): another coding agent's session there,
/// the owner's conversation with another AI there, that machine's own
/// reading of what they are doing in front of it, and a look at their
/// screen its own screen gate let through.
pub const PEER_AGENT_EVENT: &str = "agent.merope.peer_agent";
pub const AI_CHAT_EVENT: &str = "agent.merope.ai_chat";
pub const SITUATION_EVENT: &str = "agent.merope.situation";
/// A look at the owner's screen that the child's own gate let through: who
/// and what about in a chat window (never anyone's words), or the app, the
/// window and its filtered text otherwise.
pub const SCREEN_EVENT: &str = "agent.merope.screen";
pub const TOLD_EVENTS: [&str; 4] = [
    PEER_AGENT_EVENT,
    AI_CHAT_EVENT,
    SITUATION_EVENT,
    SCREEN_EVENT,
];

/// Something she found out for herself, something she just did on her own,
/// a game she is playing.
pub const FOUND_OUT_EVENT: &str = "agent.merope.found_out";
pub const DOING_EVENT: &str = "agent.merope.doing";
pub const PLAYING_EVENT: &str = "agent.merope.playing";

/// Her own time with the owner on a Mac: reading or watching together, and
/// her stream gone quiet while she is live.
pub const TOGETHER_EVENT: &str = "agent.merope.together";
pub const ON_AIR_EVENT: &str = "agent.merope.on_air";

/// Said to someone here or not at all: a passing thought, something she
/// found out, something she just did on her own, what a child end saw for
/// her, her time with the owner. Never a notification, never a diary entry.
pub fn said_only_in_person(kind: &str) -> bool {
    [
        wander::THOUGHT_EVENT,
        FOUND_OUT_EVENT,
        DOING_EVENT,
        PLAYING_EVENT,
        TOGETHER_EVENT,
        ON_AIR_EVENT,
    ]
    .contains(&kind)
        || TOLD_EVENTS.contains(&kind)
}

pub fn is_task_outcome(kind: &str) -> bool {
    TASK_OUTCOMES.contains(&kind)
}

/// Worth her interrupting someone who is away for, among the catalog's keys.
pub fn interrupts_when_away(kind: &str) -> bool {
    is_task_outcome(kind) || INTERRUPTS_WHEN_AWAY.contains(&kind)
}

/// Whether she would tell someone who is not looking about it at all: what
/// interrupts when away, and her own moments, but never a touch or what is
/// said only in person.
pub fn worth_notifying(kind: &str) -> bool {
    if kind == TOUCH || said_only_in_person(kind) {
        return false;
    }
    interrupts_when_away(kind) || kind.starts_with(OWN_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_moment_kind_is_said_only_in_person() {
        let contract: serde_json::Value =
            serde_json::from_str(include_str!("../../../shared/her_link_contract.json")).unwrap();
        let kinds = contract["moments"]["kinds"].as_array().unwrap();
        assert!(!kinds.is_empty());
        for kind in kinds {
            let kind = kind.as_str().unwrap();
            assert!(TOLD_EVENTS.contains(&kind), "{kind}");
            assert!(
                said_only_in_person(kind) && !worth_notifying(kind),
                "{kind}"
            );
        }
    }

    #[test]
    fn task_outcomes_interrupt_and_a_touch_never_notifies() {
        for kind in TASK_OUTCOMES {
            assert!(
                interrupts_when_away(kind) && worth_notifying(kind),
                "{kind}"
            );
        }
        assert!(!worth_notifying(TOUCH));
        assert!(worth_notifying("agent.merope.anything_new"));
        assert!(!worth_notifying("agent.task_progress"));
        assert!(!interrupts_when_away("agent.merope.thought"));
    }
}
