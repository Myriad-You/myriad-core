//! Admission: whether a wish to speak goes on to be said, and the facts she
//! judges by when it is hers to judge.

use chrono::{DateTime, TimeZone, Utc};
use myriad_agent_rules::memory::audience::{self, Venue};
use serde::{Deserialize, Serialize};

use super::ledger::{said_to, told};
use super::{Elsewhere, HOUR, MINUTE, Source, Wish, speakable};
use crate::{speaking, timing};

/// How far back the fuse counts.
pub const FUSE_WINDOW: i64 = HOUR;
/// Speaking up unasked to one audience this many times within
/// [`FUSE_WINDOW`] is a loop, not her: the fuse stops it and the side
/// reports an error. Far past anything she did when the gates were rules
/// (one every 180 seconds at most, so 20 an hour); only a bug reaches it.
pub const FUSE_PER_HOUR: usize = 60;
/// How many of her latest unasked lines to an audience she is shown.
pub const LATELY_SHOWN: usize = 3;
/// A talk ended longer ago than this is not "still talking".
const STILL_TALKING: i64 = 2 * HOUR;
/// A touch longer ago than this is not news.
const TOUCH_FRESH: i64 = 10 * MINUTE;

/// Everything admission reads: the wish, the ledger rows that bear on it,
/// the grants, and how things stand with them and with her.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Admit {
    pub wish: Wish,
    /// The ledger rows that bear on it: the side's lines to this audience
    /// lately and its lines about the same thing anywhere it holds.
    #[serde(default)]
    pub ledger: Vec<super::Said>,
    /// What is granted now (the runtime layer, after filtering): never what
    /// was declared or approved.
    #[serde(default)]
    pub granted: Vec<String>,
    /// They turned on do-not-disturb.
    #[serde(default)]
    pub dnd: bool,
    /// They opened the talk themselves just now (do-not-disturb does not
    /// keep her from answering it).
    #[serde(default)]
    pub they_opened: bool,
    /// They are there with her: her face is open in front of them, the
    /// app is in front, they are in the room.
    #[serde(default)]
    pub present: bool,
    pub now: i64,
    /// When anything was last said between them in this audience.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_talk_at: Option<i64>,
    /// When they last wrote or spoke to her (in a group: anyone in it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub they_wrote_at: Option<i64>,
    /// What she is in the middle of doing, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doing: Option<String>,
    /// When they last touched her.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touched_at: Option<i64>,
}

/// Why a wish is not said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "camelCase")]
pub enum Refusal {
    /// They turned on do-not-disturb.
    DoNotDisturb,
    /// What it is made of may not be said to whom it is for (a private
    /// thing into a group, one group's into another), or it is for no one.
    Audience,
    /// What it would do is not granted: the missing grants, in the wish's
    /// order.
    NotGranted { missing: Vec<String> },
    /// Word for word the last line she said to them.
    Verbatim,
    /// She already said this thing to them.
    SameThing { at: i64 },
    /// Only face to face, and they are not there.
    NotThere,
    /// It could wait only until a time that has passed.
    Stale,
    /// Far more unasked lines to them in the last hour than she ever says:
    /// a loop. The side reports it as an error.
    Fuse { count: usize },
}

/// What becomes of a wish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum Admission {
    /// Not said.
    Refused(Refusal),
    /// Not now: keep it until they are free or back, at most `until`.
    Wait { until: i64 },
    /// Hers to judge, from these facts.
    Ask { facts: Vec<String> },
    /// Called on: say it.
    Say,
}

/// Whether `input.wish` should be said now. A wish she was called on passes
/// only the audience and the grants, and is said. One she was not called on
/// passes the four rules (do-not-disturb, audience, grants, the same line or
/// thing), the fuse, and whether it can be said here and now; then it is
/// hers to judge from [`facts`]. Call it before her line is written, and
/// again with the line in `wish.text` before it goes.
pub fn admit<Z: TimeZone>(input: &Admit, zone: &Z) -> Admission
where
    Z::Offset: std::fmt::Display,
{
    let wish = &input.wish;
    if !speakable(&wish.to) || !audience::admits(&wish.seed_from, &wish.to) {
        return Admission::Refused(Refusal::Audience);
    }
    let missing: Vec<String> = wish
        .needs
        .iter()
        .filter(|need| !input.granted.contains(need))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Admission::Refused(Refusal::NotGranted { missing });
    }
    if wish.called {
        return Admission::Say;
    }
    // Not kept for later either: today a line under do-not-disturb is
    // dropped, not held until it ends.
    if input.dnd && !input.they_opened {
        return Admission::Refused(Refusal::DoNotDisturb);
    }
    let told = told(
        &input.ledger,
        &wish.to,
        wish.thing.as_deref(),
        wish.text.as_deref(),
        input.now,
    );
    if told.verbatim {
        return Admission::Refused(Refusal::Verbatim);
    }
    if let Some(at) = told.same_thing_at {
        return Admission::Refused(Refusal::SameThing { at });
    }
    if told.unasked_lately >= FUSE_PER_HOUR {
        return Admission::Refused(Refusal::Fuse {
            count: told.unasked_lately,
        });
    }
    let waits = wish.keeps_until.filter(|until| *until > input.now);
    if wish.keeps_until.is_some() && waits.is_none() {
        return Admission::Refused(Refusal::Stale);
    }
    if wish.in_person && !input.present {
        return match waits {
            Some(until) => Admission::Wait { until },
            None => Admission::Refused(Refusal::NotThere),
        };
    }
    Admission::Ask {
        facts: facts(input, zone),
    }
}

fn ago(micros: i64) -> String {
    match micros.max(0) / MINUTE {
        0 => "just now".to_string(),
        1 => "1 minute ago".to_string(),
        minutes @ 2..90 => format!("{minutes} minutes ago"),
        minutes @ 90..2880 => format!("{} hours ago", (minutes + 30) / 60),
        minutes => format!("{} days ago", minutes / 1440),
    }
}

fn clock<Z: TimeZone>(at: i64, zone: &Z) -> String
where
    Z::Offset: std::fmt::Display,
{
    DateTime::<Utc>::from_timestamp_micros(at)
        .unwrap_or_default()
        .with_timezone(zone)
        .format("%H:%M")
        .to_string()
}

/// What she is told when whether to speak is hers to judge: why she wants
/// to, what she said to them unasked lately and whether they answered, how
/// long since they talked, when she last greeted them, today or yesterday
/// (when she wants to greet them), where else she said the same thing, her night,
/// what she is doing, a touch. These were the gates; now she weighs them.
/// English, like the rest of what she is asked; one fact a line.
pub fn facts<Z: TimeZone>(input: &Admit, zone: &Z) -> Vec<String>
where
    Z::Offset: std::fmt::Display,
{
    let wish = &input.wish;
    let now = input.now;
    let group = matches!(wish.to.venue, Venue::Group(_));
    let them = if group { "the group" } else { "them" };
    let mut facts = vec![format!("Why you want to speak: {}.", wish.source.why())];

    // Not given is not known: never "you have not talked".
    if let Some(last) = input.last_talk_at.filter(|last| *last <= now) {
        if now - last < STILL_TALKING {
            facts.push(format!(
                "You were last talking with {them} {}.",
                ago(now - last)
            ));
        } else {
            facts.push(format!("You last talked with {them} {}.", ago(now - last)));
        }
    }

    let lately: Vec<String> = said_to(&input.ledger, &wish.to)
        .into_iter()
        .filter(|said| !said.called && said.at <= now)
        .take(LATELY_SHOWN)
        .map(|said| {
            let answered = input.they_wrote_at.is_some_and(|wrote| wrote > said.at);
            speaking::said_lately_line((now - said.at) / MINUTE, answered, said.text.trim())
        })
        .collect();
    if !lately.is_empty() {
        facts.push(format!(
            "What you said to {them} unasked lately, latest first:\n{}",
            lately.join("\n")
        ));
    }

    // Wanting to greet them: when she last did, and how, by the date on
    // the clock (already today, or yesterday), as anyone would say it: told before
    // she gets up, yesterday evening's greeting is yesterday's. Lately's
    // lines may have scrolled past it, and a greeting is the one line a day
    // has once.
    if wish.source == Source::Greeting {
        let date = |at: i64| {
            DateTime::<Utc>::from_timestamp_micros(at)
                .unwrap_or_default()
                .with_timezone(zone)
                .date_naive()
        };
        let today = date(now);
        if let Some((told, day, greeted)) = said_to(&input.ledger, &wish.to)
            .into_iter()
            .filter(|said| said.source == Some(Source::Greeting) && said.at <= now)
            .find_map(|said| match (today - date(said.at)).num_days() {
                0 => Some(("You already greeted", "today", said)),
                1 => Some(("You last greeted", "yesterday", said)),
                _ => None,
            })
        {
            facts.push(format!(
                "{told} {them} {day} at {}: \"{}\"",
                clock(greeted.at, zone),
                greeted.text.trim()
            ));
        }
    }

    let told = told(&input.ledger, &wish.to, wish.thing.as_deref(), None, now);
    if let Some(place) = told.elsewhere.first() {
        facts.push(match place {
            Elsewhere::Private { at } => {
                format!("You already told this in a private talk {}.", ago(now - at))
            }
            Elsewhere::Group { at, name, .. } => match name {
                Some(name) => format!(
                    "You already told this in the group {name} {}.",
                    ago(now - at)
                ),
                None => format!("You already told this in a group {}.", ago(now - at)),
            },
        });
    }

    let at = DateTime::<Utc>::from_timestamp_micros(now).unwrap_or_default();
    if let Some(wakes_in) = timing::asleep_at(at, zone) {
        let up = now + (wakes_in as i64) * super::SECOND;
        facts.push(format!(
            "By your usual hours it is your night now ({}); you get up about {}.",
            clock(now, zone),
            clock(up, zone)
        ));
    }

    if let Some(doing) = input
        .doing
        .as_deref()
        .map(str::trim)
        .filter(|doing| !doing.is_empty())
    {
        facts.push(format!("You are in the middle of: {doing}"));
    }

    if let Some(touched) = input
        .touched_at
        .filter(|touched| *touched <= now && now - touched < TOUCH_FRESH)
    {
        facts.push(format!("They touched you {}.", ago(now - touched)));
    }

    facts
}
