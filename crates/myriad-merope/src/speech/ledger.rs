//! Reading the ledger: whether a line or a thing was already said to an
//! audience, where else a thing was said, and how often she spoke up unasked.

use myriad_agent_rules::memory::audience::{Audience, Venue};
use serde::{Deserialize, Serialize};

use super::{FUSE_WINDOW, Said};

/// Whether `a` and `b` are the same audience: the same group (whoever is in
/// it now), or a private talk with the same people.
pub fn same_audience(a: &Audience<String>, b: &Audience<String>) -> bool {
    match (&a.venue, &b.venue) {
        (Venue::Group(one), Venue::Group(other)) => one == other,
        (Venue::Private, Venue::Private) => {
            a.members.iter().all(|id| b.members.contains(id))
                && b.members.iter().all(|id| a.members.contains(id))
        }
        _ => false,
    }
}

/// Somewhere else a thing was said: when, and where. Never the words: a
/// private talk learns only that a group heard it and which group, and a
/// group only that it was said in private, not to whom (as across groups,
/// she is told the group's name and nothing of private talk).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "venue", rename_all = "camelCase")]
pub enum Elsewhere {
    Private {
        at: i64,
    },
    Group {
        at: i64,
        group: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
}

/// What the ledger says about a wish to speak to `to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Told {
    /// `text` is word for word the last line she said to `to`.
    pub verbatim: bool,
    /// When she last said the same thing to `to`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_thing_at: Option<i64>,
    /// Where else she said the same thing, latest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elsewhere: Vec<Elsewhere>,
    /// How many times she spoke to `to` unasked in the [`FUSE_WINDOW`]
    /// before `now`.
    pub unasked_lately: usize,
}

/// The lines of `ledger` that reached `to`, latest first; equal times keep
/// the ledger's order.
pub(super) fn said_to<'a>(ledger: &'a [Said], to: &Audience<String>) -> Vec<&'a Said> {
    let mut said: Vec<&Said> = ledger
        .iter()
        .filter(|said| said.reached() && same_audience(&said.to, to))
        .collect();
    said.sort_by_key(|said| std::cmp::Reverse(said.at));
    said
}

/// What `ledger` says about saying `text` (when written) about `thing` (when
/// it has a key) to `to` at `now`. Only lines whose words reached someone
/// count; lines after `now` are read like any other.
pub fn told(
    ledger: &[Said],
    to: &Audience<String>,
    thing: Option<&str>,
    text: Option<&str>,
    now: i64,
) -> Told {
    let here = said_to(ledger, to);
    let verbatim = match (text.map(str::trim), here.first()) {
        (Some(text), Some(last)) => !text.is_empty() && last.text.trim() == text,
        _ => false,
    };
    let same_thing_at = thing.and_then(|thing| {
        here.iter()
            .find(|said| said.thing.as_deref() == Some(thing))
            .map(|said| said.at)
    });
    let mut elsewhere: Vec<(i64, Elsewhere)> = match thing {
        Some(thing) => ledger
            .iter()
            .filter(|said| {
                said.reached()
                    && said.thing.as_deref() == Some(thing)
                    && !same_audience(&said.to, to)
            })
            .filter_map(|said| match &said.to.venue {
                Venue::Private => Some((said.at, Elsewhere::Private { at: said.at })),
                Venue::Group(group) => Some((
                    said.at,
                    Elsewhere::Group {
                        at: said.at,
                        group: group.clone(),
                        name: said.group_name.clone(),
                    },
                )),
                Venue::Own => None,
            })
            .collect(),
        None => Vec::new(),
    };
    elsewhere.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    let unasked_lately = here
        .iter()
        .filter(|said| !said.called && said.at <= now && now - said.at < FUSE_WINDOW)
        .count();
    Told {
        verbatim,
        same_thing_at,
        elsewhere: elsewhere.into_iter().map(|(_, place)| place).collect(),
        unasked_lately,
    }
}
