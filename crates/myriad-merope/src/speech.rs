//! What she means to say, everywhere: one shape for it, the ledger of what
//! she said, and whether a wish to speak goes on to be said.
//!
//! Every line she might say, from any path on any side, is a [`Wish`]: who
//! it is for, why she wants to say it, what set it off (not yet a line), the
//! thing it is about, whether it only makes sense face to face, how long it
//! can wait, and whether she was called on. What she said goes into one
//! ledger ([`Said`], the `speech.said` rows of her log), and every judgment
//! here reads it.
//!
//! Admission ([`admit`]) answers "should this be said now". Only four
//! things are rules, all about permission or privacy, never hers to weigh:
//! their do-not-disturb, the audience, the grants, and not saying the same
//! line or the same thing to the same audience again. Everything else that
//! used to hold her back (cooldowns, the same topic a while ago, her night,
//! being busy) is no longer a gate: it is told to her as facts ([`facts`])
//! and she judges. A wide fuse stops a runaway loop, not her.
//!
//! When several admitted lines wait for the same person, one call picks one
//! ([`choice`]). The `[[…]]` marks her lines may carry, who reads each and
//! who strips it, are one table ([`markers`]).
//!
//! Delivery (which way out, how much of it, interruption) and arbitration
//! stay with each side; what came of it comes back as a [`Said`].
//!
//! Times are Unix microseconds.

use myriad_agent_rules::memory::audience::{Audience, Venue};
use serde::{Deserialize, Serialize};

mod admit;
pub mod choice;
mod ledger;
pub mod markers;

pub use admit::{
    Admission, Admit, FUSE_PER_HOUR, FUSE_WINDOW, LATELY_SHOWN, Refusal, admit, facts,
};
pub use ledger::{Elsewhere, Told, same_audience, told};

/// Microseconds in a second.
pub const SECOND: i64 = 1_000_000;
/// Microseconds in a minute.
pub const MINUTE: i64 = 60 * SECOND;
/// Microseconds in an hour.
pub const HOUR: i64 = 60 * MINUTE;

/// Why she wants to say it. The first group is speaking unasked; the last
/// is being called on. Whether she was called on is [`Wish::called`], kept
/// apart from this so a side does not have to agree on every source to
/// agree on that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Something happened she wants to tell them about.
    Event,
    /// A greeting: they came back, a new day.
    Greeting,
    /// Writing to someone first after a while (`reach`).
    Reach,
    /// A thought that wandered to them.
    Wander,
    /// Something of hers she wants to share (a song, a find).
    Share,
    /// Work she was handed is done: telling how it went.
    WorkDone,
    /// The first line of a talk she opens.
    FirstLine,
    /// Something she saw on their screen.
    Screen,
    /// Another AI's words she heard.
    OtherAi,
    /// A lull while she is live.
    LiveLull,
    /// A short remark on a game being played.
    GameRemark,
    /// Breaking into a group's talk.
    GroupInterject,
    /// Sharing what she is doing with a group.
    GroupShare,
    /// Speaking first in a quiet group.
    GroupFirst,
    /// Answering what was said to her.
    Reply,
    /// @-ed, or replied to, in a group.
    GroupAt,
    /// Touched.
    Touch,
    /// Her turn in a game.
    GameTurn,
}

impl Source {
    /// Why she wants to speak, as she is told it.
    pub fn why(self) -> &'static str {
        match self {
            Self::Event => "something happened that you want to tell them about",
            Self::Greeting => "you want to greet them",
            Self::Reach => "you are thinking of writing to them first",
            Self::Wander => "your thoughts wandered to them",
            Self::Share => "you have something of yours you want to share",
            Self::WorkDone => "something you were doing for them is over",
            Self::FirstLine => "you are opening a talk with them",
            Self::Screen => "you saw something on their screen",
            Self::OtherAi => "you heard what another AI said to them",
            Self::LiveLull => "it has gone quiet while you are live",
            Self::GameRemark => "you have a remark on the game being played",
            Self::GroupInterject => "you want to join what the group is talking about",
            Self::GroupShare => "you want to tell the group what you are doing",
            Self::GroupFirst => "the group is quiet and you want to say something first",
            Self::Reply => "they said something to you",
            Self::GroupAt => "someone in the group spoke to you",
            Self::Touch => "they touched you",
            Self::GameTurn => "it is your turn in the game",
        }
    }
}

/// Something she means to say, before it is a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Wish {
    /// Who it is for: a private talk (the partner, someone) or a group.
    pub to: Audience<String>,
    /// Why she wants to say it.
    pub source: Source,
    /// What set it off, or a gist of it: not yet a line. She is asked
    /// before anything is written, so a line not said costs nothing.
    pub seed: String,
    /// Where what it is made of was heard: her own, a private talk, a
    /// group. Checked against `to` by the audience rule.
    pub seed_from: Audience<String>,
    /// The thing it is about, by a key: the same key is the same thing.
    /// What makes it new (another day, something new in it) belongs in the
    /// key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thing: Option<String>,
    /// Only face to face: not said when they are not there (what she saw,
    /// what they are playing, a wandering thought).
    #[serde(default)]
    pub in_person: bool,
    /// Until when it may wait for them to be free or back; `None` is now
    /// or never.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keeps_until: Option<i64>,
    /// Called on (a reply, an @, a touch, her turn in a game): she is not
    /// asked whether to speak, only how.
    #[serde(default)]
    pub called: bool,
    /// The grants what it would do takes (a piece of work it offers to do
    /// takes none: offering is not doing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    /// When it came to her.
    pub at: i64,
    /// Her line, once written: checked against the last line she said to
    /// the same audience.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// How much of a said line went out (`speech.said`'s `extent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Extent {
    /// All of it.
    Full,
    /// A gist of it (they were away).
    Gist,
    /// Only a notification with it.
    Notice,
    /// Only her mark lit: none of its words reached them.
    Marker,
    /// Not sent.
    None,
}

impl Extent {
    /// Whether its words reached them: a line that only lit her mark, or
    /// never went, was not said.
    pub fn reached(self) -> bool {
        matches!(self, Self::Full | Self::Gist | Self::Notice)
    }
}

/// One line she said: a row of the ledger, as a side reads it back from
/// her log (`speech.said`: the row's audience is `to`, its payload the
/// rest). Sides give the rows that bear on a wish; a side holds only the
/// audiences it may (a group's lines stay on the server).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Said {
    pub at: i64,
    /// Who it was said to.
    pub to: Audience<String>,
    /// Her words.
    pub text: String,
    pub source: Source,
    pub called: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thing: Option<String>,
    /// Where: `web`, `app`, `group`, or another surface.
    pub place: String,
    pub extent: Extent,
    /// The group's name, when it was said in one: what she is told of it
    /// elsewhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_name: Option<String>,
}

impl Said {
    /// Whether it counts as said: its words reached them.
    pub fn reached(&self) -> bool {
        self.extent.reached()
    }
}

/// Whether `audience` is one she can speak to: a private talk with someone
/// in it, or a group. Her own is not anyone to speak to.
fn speakable(audience: &Audience<String>) -> bool {
    match audience.venue {
        Venue::Private => !audience.members.is_empty(),
        Venue::Group(_) => true,
        Venue::Own => false,
    }
}

#[cfg(test)]
mod tests;
