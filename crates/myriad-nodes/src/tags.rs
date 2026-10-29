//! The tags every body carries, and the checks texts and id lists share.
//!
//! Three tags on every span and entry (and, for one rule everywhere, on
//! every row's body):
//!
//! - **sync**: `shared` may go to every device of hers; `{"local": <device>}`
//!   stays on that one device (the row's `origin`).
//! - **who said it** (`saidBy`): `partner`, `her`, `third_party` (a page, a
//!   video, subtitles, another AI, a coding agent's output), `other_person`
//!   (someone in a group, or the person whose private talk it is). A span of
//!   a talk lists everyone in it.
//! - **audience**: where it was said. A span or entry keeps its own, which
//!   is the row's, with one exception: the partner's own words in a group may
//!   be kept in the partner's private audience.

use myriad_persona_log::row::{Audience, Replicate};
use serde::{Deserialize, Serialize};

use crate::{NodeError, check_token};

/// Whether something may leave the device it was written on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sync {
    Shared,
    /// Only on this device (a clock node name: `dev_…`, `loc_…`, `site`).
    Local(String),
}

impl Sync {
    /// Whether a row of `replicate` written on `origin` may carry it: a
    /// shared tag only on a shared row, a local one only on a local row of
    /// that device.
    pub fn fits(&self, replicate: Replicate, origin: &str) -> bool {
        match self {
            Self::Shared => replicate == Replicate::Shared,
            Self::Local(device) => replicate == Replicate::Local && device == origin,
        }
    }

    pub(crate) fn check(&self) -> Result<(), NodeError> {
        if let Self::Local(device) = self {
            check_token("sync device", device)?;
        }
        Ok(())
    }
}

/// Who said it. Sorted by name, so a sorted list is in this order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaidBy {
    Her,
    OtherPerson,
    Partner,
    ThirdParty,
}

/// A who-said-it list: not empty, sorted, no repeats.
pub(crate) fn check_said_by(said_by: &[SaidBy]) -> Result<(), NodeError> {
    if said_by.is_empty() {
        return Err(NodeError::new("saidBy is empty"));
    }
    if !said_by.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(NodeError::new("saidBy is not sorted or repeats"));
    }
    Ok(())
}

/// Whether a span or entry said in `tag` may be kept in a row of `row`,
/// said by `said_by`: the same audience, or the partner's own words in a
/// group kept in their private audience.
pub fn audience_fits(tag: &str, row: &str, said_by: &[SaidBy]) -> bool {
    if tag == row {
        return true;
    }
    matches!(
        (Audience::parse(tag), Audience::parse(row)),
        (Ok(Audience::Group(_)), Ok(Audience::Personal(members))) if !members.contains(',')
    ) && said_by == [SaidBy::Partner]
}

/// One of her texts: no space at either end, not empty, no control
/// character but a line break, at most `max` chars.
pub(crate) fn check_text(what: &str, text: &str, max: usize) -> Result<(), NodeError> {
    if text.trim().is_empty() {
        return Err(NodeError::new(format!("{what} is empty")));
    }
    if text.trim() != text {
        return Err(NodeError::new(format!(
            "{what} starts or ends with a space"
        )));
    }
    if text.chars().any(|ch| ch.is_control() && ch != '\n') {
        return Err(NodeError::new(format!("{what} holds a control character")));
    }
    if text.chars().count() > max {
        return Err(NodeError::new(format!("{what} is longer than {max} chars")));
    }
    Ok(())
}

/// A list of ids: each a token, sorted, no repeats, at most `max`, and at
/// least one when `needed`. With `typed`, each is `<type>:<id>`.
pub(crate) fn check_ids(
    what: &str,
    ids: &[String],
    needed: bool,
    typed: bool,
    max: usize,
) -> Result<(), NodeError> {
    if needed && ids.is_empty() {
        return Err(NodeError::new(format!("{what} is empty")));
    }
    if ids.len() > max {
        return Err(NodeError::new(format!("{what} holds more than {max}")));
    }
    for id in ids {
        check_token(what, id)?;
        if typed && myriad_persona_log::row::object_type(id).is_none() {
            return Err(NodeError::new(format!(
                "{what} holds an id that is not <type>:<id>"
            )));
        }
    }
    if !ids.windows(2).all(|pair| pair[0] < pair[1]) {
        return Err(NodeError::new(format!("{what} is not sorted or repeats")));
    }
    Ok(())
}

/// An item id she mints: 1 to 64 of A-Z a-z 0-9 - _.
pub(crate) fn check_item_id(what: &str, id: &str) -> Result<(), NodeError> {
    let max = crate::limits::ITEM_ID_BYTES;
    if id.is_empty()
        || id.len() > max
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(NodeError::new(format!(
            "{what} is 1 to {max} of A-Z a-z 0-9 - _"
        )));
    }
    Ok(())
}
