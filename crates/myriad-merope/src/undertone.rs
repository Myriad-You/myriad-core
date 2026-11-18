//! Her undertone (底色): where her arousal settles toward when no one is
//! pulling at it, grown from her own life and from nothing else.
//!
//! What passes between her and people is already in her mood toward each
//! of them and in what still stings. Carried into her undertone, what one
//! person did would reach everyone she meets next, and the person she
//! talks with most would get a standing say in how she is with all the
//! others. So only her own days go in: how much of her own time she spent
//! at things rather than lazing, and how often something in them landed
//! hard (a [`gap`]). A day is numbers only; no one's name or words can be
//! in it.
//!
//! It is worked out once a night on her host ([`Undertone::after`]) and
//! kept as one of her records, which go with her log; an end reads that
//! record ([`crate::affect::baseline`]) and never works it out again from
//! its own rows.
//!
//! Her persona's words (quiet, outgoing) are only where she starts: the
//! more days she has lived, the less they weigh, with no threshold. A
//! stretch of days is taken against her own long run and bounded: a low
//! spell narrows her and she brings less up, which would leave fewer
//! things to land, so an unbounded undertone could only sink.
//!
//! Only her arousal is moved for now (11-18): her valence stays where it
//! was until days lived show it does not sink ([`Undertone::would_move_mood`]
//! is for showing that, and is applied nowhere).

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::doing::Reaction;
use crate::serial::Held;
use crate::sources::Kept;

/// A thing she did stayed with her well after it ended.
pub const MOVED: f64 = 12.0;
/// Her guess at the next part of a serial held, or did not.
pub const GUESS: f64 = 8.0;
/// A question of her own got its answer.
pub const FOUND_OUT: f64 = 8.0;
/// Looking it up showed she had thought it wrong.
pub const THOUGHT_WRONG: f64 = 6.0;
/// Something she wanted came true.
pub const CAME_TRUE: f64 = 12.0;
/// She stopped wanting something she had wanted.
pub const LET_GO: f64 = 6.0;

/// How far a thing she did moved her, when it moved her at all: it moved
/// her, her guess held or did not, what she set out to find out was found or
/// showed her wrong. Sizes are on the scale of a push of her emotion (see
/// `elsewhere::own_feeling`): a compliment moves it about this far. They
/// are guesses, not measured.
///
/// Only what landed hard counts. Most songs are just fine, and some are not
/// for her; counting those, she would come out low only because she listens
/// a lot. A guess she made from a book she already knew is a memory, not a
/// guess; one not yet borne out or refuted, a half-held one and a question
/// she only thought over, or looked up and found nothing on, are none.
pub fn gap(reaction: Option<Reaction>, kept: &Kept) -> Option<f64> {
    let mut moved = None;
    let mut add = |by: f64| *moved.get_or_insert(0.0) += by;
    if reaction == Some(Reaction::Moved) {
        add(MOVED);
    }
    if let Some(guessed) = kept.guessed.as_ref().filter(|guessed| !guessed.remembered) {
        match guessed.held {
            Held::Yes => add(GUESS),
            Held::No => add(-GUESS),
            Held::Partly | Held::NotYet => {}
        }
    }
    if let Some(compared) = kept
        .explored
        .as_ref()
        .and_then(|explored| explored.compared.as_ref())
    {
        if compared.surprise == "much" && !compared.already_known {
            add(-THOUGHT_WRONG);
        } else if compared.answered == "yes" || compared.answered == "partly" {
            add(FOUND_OUT);
        }
    }
    moved
}

/// How a want ending moved her: it came true, or she let it go.
pub fn want_ended(came_true: bool) -> f64 {
    if came_true { CAME_TRUE } else { -LET_GO }
}

/// One day of her own life, as counted on her host: nothing about anyone.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Day {
    /// Things of her own she did (songs, notes, parts of a serial, questions).
    pub things: u32,
    /// The [`gap`]s and [`want_ended`]s of the day.
    pub gaps: u32,
    /// What they added up to, the ups less the downs.
    pub lift: f64,
    /// Minutes she spent at her own things.
    pub spent_minutes: f64,
    /// Minutes she lazed about.
    pub lazed_minutes: f64,
}

impl Day {
    /// How lively the day was, `0..=1`: half how much of her own time went
    /// into doing rather than lazing, half how often what she did landed
    /// hard. None for a day she did not live: she did nothing of her own,
    /// nothing landed and she lazed no time (she was not running). A day
    /// she only lazed is lived, and the quietest.
    pub fn activation(&self) -> Option<f64> {
        let spent = finite_minutes(self.spent_minutes);
        let lazed = finite_minutes(self.lazed_minutes);
        if self.things == 0 && self.gaps == 0 && spent + lazed <= 0.0 {
            return None;
        }
        let busy = if spent + lazed > 0.0 {
            spent / (spent + lazed)
        } else {
            0.0
        };
        let stirred = if self.things == 0 {
            if self.gaps > 0 { 1.0 } else { 0.0 }
        } else {
            (f64::from(self.gaps) / f64::from(self.things)).min(1.0)
        };
        Some((busy + stirred) / 2.0)
    }

    /// Which way the day leaned, `-1..=1`: what landed, ups less downs, for
    /// how much she did.
    pub fn tone(&self) -> f64 {
        if !self.lift.is_finite() {
            return 0.0;
        }
        let scale = MOVED * f64::from(self.things.max(self.gaps).max(1));
        (self.lift / scale).clamp(-1.0, 1.0)
    }
}

fn finite_minutes(minutes: f64) -> f64 {
    if minutes.is_finite() {
        minutes.max(0.0)
    } else {
        0.0
    }
}

/// Where a run of days leans: how lively, and which way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lean {
    /// `0..=1`, see [`Day::activation`].
    pub activation: f64,
    /// `-1..=1`, see [`Day::tone`].
    pub tone: f64,
}

impl Lean {
    fn toward(self, day: Lean, weight: f64) -> Lean {
        Lean {
            activation: self.activation + weight * (day.activation - self.activation),
            tone: self.tone + weight * (day.tone - self.tone),
        }
    }

    fn holds(&self) -> bool {
        (0.0..=1.0).contains(&self.activation) && (-1.0..=1.0).contains(&self.tone)
    }
}

/// How many days of her own count as much as her persona's words: after
/// this many, where she started and what she lived weigh the same.
pub const PERSONA_WEIGHS_DAYS: f64 = 14.0;
/// Half of what the last stretch says is a day this old, for the recent
/// run; and for her long run.
pub const RECENT_HALF_DAYS: f64 = 4.0;
pub const LONG_HALF_DAYS: f64 = 30.0;
/// How far her arousal goes for her whole range of liveliness: the span
/// her persona's words already have (quiet 42 to outgoing 56).
pub const QUIET: f64 = crate::affect::QUIET_AROUSAL;
pub const LIVELY: f64 = crate::affect::LIVELY_AROUSAL;
/// How far a recent stretch moves her off her long run, per unit of
/// liveliness off it, and at most.
pub const SWING: f64 = 20.0;
pub const BOUND: f64 = 6.0;
/// The same for her valence, were it opened.
pub const MOOD_SWING: f64 = 20.0;
pub const MOOD_BOUND: f64 = 6.0;

/// Her undertone as kept: the days of her own folded in so far, and where
/// the recent and the long run of them lean. Numbers and a date, nothing
/// else.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Undertone {
    /// The last of her days folded in, her day (`YYYY-MM-DD`).
    pub day: NaiveDate,
    /// How many days she lived are in it.
    pub days: u32,
    pub recent: Lean,
    pub long: Lean,
}

impl Undertone {
    /// Her undertone with `day` folded in: once a night, from that day of
    /// her own life. A day already in (the same night again, or one after
    /// it) changes nothing; a day she did not live is not folded in. None
    /// only while she has lived no day at all.
    pub fn after(before: Option<&Undertone>, day: NaiveDate, lived: &Day) -> Option<Undertone> {
        if before.is_some_and(|before| before.day >= day) {
            return before.copied();
        }
        let Some(activation) = lived.activation() else {
            return before.copied();
        };
        let today = Lean {
            activation,
            tone: lived.tone(),
        };
        Some(match before {
            None => Undertone {
                day,
                days: 1,
                recent: today,
                long: today,
            },
            // Counted in days she lived, as her persona's weight is: days
            // she was not running are not days of her life, and do not
            // let the next one stand for them.
            Some(before) => Undertone {
                day,
                days: before.days.saturating_add(1),
                recent: before.recent.toward(today, weight(RECENT_HALF_DAYS)),
                long: before.long.toward(today, weight(LONG_HALF_DAYS)),
            },
        })
    }

    /// Where her arousal settles: her persona's `persona` arousal, weighing
    /// less the more days she has lived, toward how lively her long run has
    /// been; then off that by how her recent days differ from her long run,
    /// within [`BOUND`].
    pub fn arousal(&self, persona: f64) -> f64 {
        let lived = QUIET + (LIVELY - QUIET) * self.long.activation.clamp(0.0, 1.0);
        let days = f64::from(self.days);
        let start = (persona * PERSONA_WEIGHS_DAYS + lived * days) / (PERSONA_WEIGHS_DAYS + days);
        let off = (SWING * (self.recent.activation - self.long.activation)).clamp(-BOUND, BOUND);
        start + off
    }

    /// Where her valence would settle were it opened: off `persona` by how
    /// her recent days lean against her long run, within [`MOOD_BOUND`].
    /// Applied nowhere yet (see the module note).
    pub fn would_move_mood(&self, persona: f64) -> f64 {
        persona + (MOOD_SWING * (self.recent.tone - self.long.tone)).clamp(-MOOD_BOUND, MOOD_BOUND)
    }

    /// Whether it is one a side could have kept: a real day, at least one
    /// day lived, every number in its range.
    pub fn holds(&self) -> bool {
        self.days > 0 && self.recent.holds() && self.long.holds()
    }
}

/// How much a day she lived weighs against a run with half-life `half`
/// days she lived.
fn weight(half: f64) -> f64 {
    1.0 - 0.5f64.powf(1.0 / half)
}

#[cfg(test)]
#[path = "undertone_tests.rs"]
mod tests;
