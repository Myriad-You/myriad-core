//! How far she may take a piece of what she keeps (三档可信): from who said
//! it and where it came from, never from what it says. The core never reads
//! the text; the host supplies what only it knows (the provenance, and which
//! evidence ids are whose).
//!
//! **Who `partner` is.** In a private audience (`personal:<one>`) the one
//! person whose talk it is is `partner` (本人): the partner on the native
//! app, and on the site each account in its own `personal:<account>`. In a
//! group everyone is `other_person`, except that a person's own words about
//! themselves, kept for that person's own private talk, are tagged
//! `partner` there ([`crate::tags::audience_fits`]).
//!
//! The three tiers, lowest first:
//!
//! - **seen** (只是看到过): everything else: a group member's words, what
//!   she saw without the person there, an agent's unproven claim, someone
//!   else's content not about the person. Side evidence on that experience
//!   only: never a fact, never asked about.
//! - **heard** (待确认): someone else's content (`third_party`) about the
//!   person that the person saw with her (`together`). Kept with where it
//!   came from, used as 「听说」; she may ask once.
//! - **counts** (算数): the person's own words and hers; what the person
//!   was typing (`typed`); a claim a real result bore out (`verified`); a
//!   heard piece the person confirmed. A fact.
//!
//! A piece said by several is as low as its lowest sayer. One the person
//! denied is void (no tier): never taken again.
//!
//! **A rule's rows** (author `rule`: what a device reported, laid out on
//! nodes by a fixed rule, no model) are seen, whoever their `saidBy` names:
//! a device saying the partner asked something is not the partner saying
//! it to her. What a rule wrote is never her view and never evidence for
//! one; how a thing stands (`node.standing`) weighs by who wrote it
//! ([`standing_tier`]).
//!
//! What the host supplies, never the core:
//!
//! - **provenance** (on each piece it keeps): `together` (the person was
//!   there, looking at the same thing), `typed` (the person was typing it),
//!   `verified` (a real result bore it out), `answer` (`confirmed` or
//!   `denied`, when she asked);
//! - **known** (for evidence ids): which are the person's own words in
//!   their own private talk (`partner`), hers there (`her`), the person's
//!   typing (`typed`), real results (`verified`). Anything not named is
//!   seen. A message in a group is never the person's here.

use std::collections::BTreeSet;

use myriad_persona_log::row::{Audience, Author};
use serde::{Deserialize, Serialize};

use crate::tags::SaidBy;

/// The three tiers, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Seen,
    Heard,
    Counts,
}

/// The person's answer when she asked about something heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Confirmed,
    Denied,
}

/// Where a piece came from beyond who said it, as the host keeps it.
/// Absent fields read as no.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Provenance {
    #[serde(default)]
    pub together: bool,
    #[serde(default)]
    pub typed: bool,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub answer: Option<Answer>,
}

/// The tier of what `said_by` said with this provenance; `None`: void.
/// `about_partner`: it was noted about the one person of the private
/// audience (it hangs on 「关于你」), as she decided when she noted it.
pub fn tier(
    said_by: &[SaidBy],
    provenance: Option<&Provenance>,
    about_partner: bool,
) -> Option<Tier> {
    let provenance = provenance.cloned().unwrap_or_default();
    match provenance.answer {
        Some(Answer::Denied) => return None,
        Some(Answer::Confirmed) => return Some(Tier::Counts),
        None => {}
    }
    if said_by.is_empty() {
        return Some(Tier::Seen);
    }
    if provenance.typed || provenance.verified {
        return Some(Tier::Counts);
    }
    said_by
        .iter()
        .map(|said| match said {
            SaidBy::Partner | SaidBy::Her => Tier::Counts,
            SaidBy::ThirdParty if provenance.together && about_partner => Tier::Heard,
            SaidBy::ThirdParty | SaidBy::OtherPerson => Tier::Seen,
        })
        .min()
}

/// Whether `private` is a private audience of one person (`personal:<one>`).
pub fn is_private(private: &str) -> bool {
    matches!(Audience::parse(private), Ok(Audience::Personal(members)) if !members.contains(','))
}

/// Whether a piece said in `audience` by `said_by` may be used in the
/// private talk of `private` (`personal:<one>`) at all: never a group
/// member's words, never a void one, never one from another person's
/// private talk. The person's own words in a group (only `partner`) are
/// theirs to use. Not a private audience of one: never.
pub fn usable_in_private(
    said_by: &[SaidBy],
    audience: &str,
    private: &str,
    provenance: Option<&Provenance>,
) -> bool {
    if !is_private(private) {
        return false;
    }
    if provenance.is_some_and(|provenance| provenance.answer == Some(Answer::Denied))
        || said_by.contains(&SaidBy::OtherPerson)
    {
        return false;
    }
    if audience == private || matches!(Audience::parse(audience), Ok(Audience::Own)) {
        return true;
    }
    said_by == [SaidBy::Partner] && matches!(Audience::parse(audience), Ok(Audience::Group(_)))
}

/// What the host knows of evidence ids. Anything it does not name is seen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Known {
    /// The person's own words in their own private talk.
    #[serde(default)]
    pub partner: BTreeSet<String>,
    /// Her own words there.
    #[serde(default)]
    pub her: BTreeSet<String>,
    /// What the person was typing.
    #[serde(default)]
    pub typed: BTreeSet<String>,
    /// Results that really ran.
    #[serde(default)]
    pub verified: BTreeSet<String>,
}

impl Known {
    pub fn tier(&self, id: &str) -> Tier {
        if self.partner.contains(id)
            || self.her.contains(id)
            || self.typed.contains(id)
            || self.verified.contains(id)
        {
            Tier::Counts
        } else {
            Tier::Seen
        }
    }
}

/// A view may change only when everything it rests on counts. Empty
/// evidence never does.
pub fn view_may_change(because: &[String], known: &Known) -> bool {
    !because.is_empty() && because.iter().all(|id| known.tier(id) == Tier::Counts)
}

/// What a view resting on `because` may be written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewVerdict {
    /// A view.
    View,
    /// Only as 「听说……」: some of it heard (`heard`: the ids of pieces kept
    /// as heard), none merely seen.
    HeardOnly,
    /// Not at all.
    Refused,
}

pub fn view(because: &[String], known: &Known, heard: &BTreeSet<String>) -> ViewVerdict {
    if because.is_empty() {
        return ViewVerdict::Refused;
    }
    if view_may_change(because, known) {
        return ViewVerdict::View;
    }
    if because
        .iter()
        .all(|id| known.tier(id) == Tier::Counts || heard.contains(id))
    {
        ViewVerdict::HeardOnly
    } else {
        ViewVerdict::Refused
    }
}

/// An intent stands on the person's side of things: their words, what they
/// typed, or a real result. Content's own words count for nothing, and
/// neither do her own lines alone (a reply of hers may have repeated such
/// content).
pub fn intent_may_rest(because: &[String], known: &Known) -> bool {
    because.iter().any(|id| {
        known.partner.contains(id) || known.typed.contains(id) || known.verified.contains(id)
    })
}

/// How far how a thing stands (`node.standing`) may be taken: a rule's is
/// only seen; the partner's counts; hers is what she concluded, and counts
/// as far as who she wrote it from does.
pub fn standing_tier(author: Author, said_by: &[SaidBy]) -> Tier {
    match author {
        Author::Rule => Tier::Seen,
        Author::Owner => Tier::Counts,
        Author::Her => tier(said_by, None, false).unwrap_or(Tier::Seen),
    }
}

/// Whether a piece her upkeep was shown counts as evidence for a view: it
/// was not written by a rule, counts by its tier (no provenance travels in
/// a body), and was said where the person's words are theirs, not in a
/// group (as a host's [`Known`] never names a group's message as the
/// person's). An inbox item has no row yet, so no author: `None`.
pub(crate) fn counts_for_view(author: Option<Author>, said_by: &[SaidBy], audience: &str) -> bool {
    author != Some(Author::Rule)
        && tier(said_by, None, false) == Some(Tier::Counts)
        && matches!(
            Audience::parse(audience),
            Ok(Audience::Personal(_) | Audience::Own)
        )
}

// MARK: The calls

/// One piece to weigh: who said it, where it came from, and whether it was
/// noted about the one person of the private audience.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Piece {
    pub said_by: Vec<SaidBy>,
    #[serde(default)]
    pub provenance: Option<Provenance>,
    #[serde(default)]
    pub about_partner: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TierInput {
    pub items: Vec<Piece>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tiers {
    /// In order; `null`: void.
    pub tiers: Vec<Option<Tier>>,
}

/// `nodes.trust_tier`.
pub fn tiers(input: TierInput) -> Tiers {
    Tiers {
        tiers: input
            .items
            .iter()
            .map(|piece| {
                tier(
                    &piece.said_by,
                    piece.provenance.as_ref(),
                    piece.about_partner,
                )
            })
            .collect(),
    }
}

/// One piece said in `audience`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Said {
    pub said_by: Vec<SaidBy>,
    pub audience: String,
    #[serde(default)]
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PrivateInput {
    /// `personal:<one>`.
    pub private: String,
    pub items: Vec<Said>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usable {
    pub usable: Vec<bool>,
}

/// `nodes.usable_in_private`.
pub fn usable(input: PrivateInput) -> Result<Usable, crate::NodeError> {
    if !is_private(&input.private) {
        return Err(crate::NodeError::new(
            "private is a personal: audience of one",
        ));
    }
    Ok(Usable {
        usable: input
            .items
            .iter()
            .map(|piece| {
                usable_in_private(
                    &piece.said_by,
                    &piece.audience,
                    &input.private,
                    piece.provenance.as_ref(),
                )
            })
            .collect(),
    })
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewInput {
    pub because: Vec<String>,
    #[serde(default)]
    pub known: Known,
    /// The ids of pieces kept as heard (待确认).
    #[serde(default)]
    pub heard: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewAnswer {
    pub may_change: bool,
    pub verdict: ViewVerdict,
}

/// `nodes.view_may_change`.
pub fn view_answer(input: ViewInput) -> ViewAnswer {
    ViewAnswer {
        may_change: view_may_change(&input.because, &input.known),
        verdict: view(&input.because, &input.known, &input.heard),
    }
}
