//! Mind-wandering while someone is here and quiet, the rules of it: when her
//! mind may drift to them, and what the thought is when it settles. Where it
//! drifts is `myriad_agent_rules::memory::recall::wander`; who is present,
//! what she thought of lately and handing the thought on are each end's.
//!
//! The thought goes through the same event decision as anything else she
//! might bring up and is usually let go. It is only ever said in person
//! (`notify::said_only_in_person`): never a notification, never her diary.

use chrono::{DateTime, Utc};

/// What a passing thought is, for the event decision.
pub const THOUGHT_EVENT: &str = "agent.merope.thought";
/// Quiet this long before the mind starts to drift.
pub const QUIET_SECS: i64 = 3 * 60;
/// Between two thoughts about the same person.
pub const BETWEEN_THOUGHTS_SECS: i64 = 10 * 60;

/// Whether it has been quiet long enough, since they spoke, since she last
/// spoke up (`cooldown_secs`, her own, longer when she is tired) and since
/// her last thought, for her mind to drift to them.
pub fn quiet_enough(
    last_user_message_at: Option<DateTime<Utc>>,
    last_proactive_at: Option<DateTime<Utc>>,
    last_thought_at: Option<DateTime<Utc>>,
    cooldown_secs: i64,
    now: DateTime<Utc>,
) -> bool {
    let since = |at: Option<DateTime<Utc>>| at.map(|at| (now - at).num_seconds());
    since(last_user_message_at).is_none_or(|secs| secs >= QUIET_SECS)
        && since(last_proactive_at).is_none_or(|secs| secs >= cooldown_secs)
        && since(last_thought_at).is_none_or(|secs| secs >= BETWEEN_THOUGHTS_SECS)
}

/// What the thought is, for the event decision: the memory it settled on
/// and, where that is something she knows only a little about, that fact
/// (`recall::Wandered::gap`). Whether it makes her want to know more is the
/// decision's to judge.
pub fn thought(content: &str, gap: Option<(&str, usize)>) -> String {
    match gap {
        Some((gap, known)) => format!(
            "你忽然想起关于对方的一件事：{content}。关于「{gap}」，你只记得{}。",
            if known <= 1 {
                "这一件事"
            } else {
                "两件事"
            }
        ),
        None => format!("你忽然想起关于对方的一件事：{content}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mind_drifts_only_after_quiet_and_not_too_often() {
        let now = DateTime::<Utc>::from_timestamp(1_793_750_400, 0).unwrap();
        let ago = |secs: i64| Some(now - chrono::Duration::seconds(secs));
        assert!(quiet_enough(ago(600), None, None, 180, now));
        assert!(!quiet_enough(ago(30), None, None, 180, now), "just spoke");
        assert!(
            !quiet_enough(ago(600), ago(100), None, 180, now),
            "she spoke up a moment ago"
        );
        assert!(
            !quiet_enough(ago(600), ago(300), None, 600, now),
            "a tired cooldown is longer"
        );
        assert!(
            !quiet_enough(ago(600), None, ago(60), 180, now),
            "just had a thought"
        );
        assert!(quiet_enough(None, None, None, 180, now));
    }

    #[test]
    fn a_thin_gap_comes_with_the_thought() {
        assert_eq!(
            thought("她养了一只猫", None),
            "你忽然想起关于对方的一件事：她养了一只猫"
        );
        assert_eq!(
            thought("她养了一只猫", Some(("猫", 1))),
            "你忽然想起关于对方的一件事：她养了一只猫。关于「猫」，你只记得这一件事。"
        );
        assert!(thought("x", Some(("猫", 2))).ends_with("你只记得两件事。"));
    }
}
