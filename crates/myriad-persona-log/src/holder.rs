//! Which audiences a side may hold, so a row never reaches a side that may
//! not keep it.
//!
//! Every host core keeps its own log, but not of every audience. The
//! partner's private talk and her own go to every host core of the
//! partner's; a group's log, and anyone else's private talk with her, stay
//! on the server, so what other people said never lands on the partner's
//! computers through sync. A child core (a phone, XR) keeps no log at all:
//! it uses the mind over the link and caches what it shows.
//!
//! The rule is per audience, because everything a row may say is bounded
//! by its audience: in the partner's private audience the only subject is
//! the partner, in hers there is none (`Row::check`), and a node's other
//! end is a key made into an id in the same audience. So a side that holds
//! an audience may hold every shared row of it, and a side that does not
//! gets none of them: not to answer its vector ([`crate::vector::answer_to`])
//! and not when it is offered ([`crate::admit`], `not_held`).
//!
//! This is the default the owner leaned towards (SYNC.md question 3) and
//! not yet their final word; it is one function, so changing it is
//! changing every side at once.

use serde::{Deserialize, Serialize};

use crate::{LogError, Row, row::Audience};

/// What kind of core a side is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Core {
    /// A computer of the partner's (a Mac, a Windows PC): a host core that
    /// may hold the pen.
    Pc,
    /// The server (today the site's backend): a host core that holds every
    /// audience.
    Server,
    /// A phone, XR or another child core: no log, only what it shows.
    Child,
}

/// The side a row would go to: its core and whose it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Holder {
    pub core: Core,
    /// The partner's id as it stands in their private audience
    /// (`personal:<owner>`).
    pub owner: String,
}

impl Holder {
    pub fn check(&self) -> Result<(), LogError> {
        let one = !self.owner.is_empty()
            && self.owner.len() <= crate::row::MAX_TOKEN_BYTES
            && !self
                .owner
                .chars()
                .any(|ch| ch == ',' || ch.is_control() || ch.is_whitespace());
        if !one {
            return Err(LogError::new(
                "the holder's owner is one id: no comma, space or control character",
            ));
        }
        Ok(())
    }

    /// Whether this side may hold rows of `audience`: the server any; a PC
    /// the partner's own private audience (theirs alone) and hers; a child
    /// core none. No side, the server included, holds the private talk of
    /// an id a partner goes by (`local-owner`, a partner id) but its own
    /// partner's: a computer that has not moved to the one id yet is
    /// refused, not stored under ids its log was not made for
    /// (`crate::partner`).
    pub fn holds(&self, audience: &str) -> bool {
        match (self.core, Audience::parse(audience)) {
            (_, Err(_)) | (Core::Child, _) => false,
            (_, Ok(Audience::Personal(member)))
                if crate::partner::is_partners(member) && member != self.owner =>
            {
                false
            }
            (Core::Server, Ok(_)) | (Core::Pc, Ok(Audience::Own)) => true,
            (Core::Pc, Ok(Audience::Personal(members))) => members == self.owner,
            (Core::Pc, Ok(Audience::Group(_))) => false,
        }
    }

    /// Whether this side may hold `row`: one of an audience it holds, and,
    /// in her own audience, no object about the partner under an id this
    /// side does not go by (`subject:` or `affect:` of `local-owner` or of
    /// another partner id): a row an unmoved computer wrote, or one about
    /// someone else's partner, is refused, not stored beside the moved ones.
    pub fn holds_row(&self, row: &Row) -> bool {
        if !self.holds(&row.audience) {
            return false;
        }
        if !matches!(Audience::parse(&row.audience), Ok(Audience::Own)) {
            return true;
        }
        !row.object.split_once(':').is_some_and(|(kind, id)| {
            crate::partner::PERSON_OBJECTS.contains(&kind)
                && crate::partner::is_partners(id)
                && id != self.owner
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holder(core: Core) -> Holder {
        Holder {
            core,
            owner: "7".into(),
        }
    }

    #[test]
    fn no_side_holds_another_partners_talk_or_an_unmoved_one() {
        let p0 = "partner_0199d5c0900033333333333333333333";
        for core in [Core::Server, Core::Pc] {
            let side = Holder {
                core,
                owner: p0.into(),
            };
            assert!(side.holds(&format!("personal:{p0}")));
            assert!(!side.holds("personal:local-owner"), "{core:?}");
            assert!(!side.holds("personal:partner_0199d5c0a00011111111111111111111"));
        }
        let site = Holder {
            core: Core::Server,
            owner: p0.into(),
        };
        assert!(site.holds("personal:9"));
        let unmoved = Holder {
            core: Core::Pc,
            owner: "local-owner".into(),
        };
        assert!(unmoved.holds("personal:local-owner"));
        assert!(!unmoved.holds(&format!("personal:{p0}")));
    }

    #[test]
    fn a_pc_holds_the_partners_talk_and_hers_only() {
        let pc = holder(Core::Pc);
        assert!(pc.holds("personal:7"));
        assert!(pc.holds("own:"));
        assert!(!pc.holds("personal:9"), "someone else's private talk");
        assert!(!pc.holds("personal:7,9"), "not the partner's alone");
        assert!(!pc.holds("group:g1"));
        assert!(!pc.holds("nonsense"));
        let server = holder(Core::Server);
        for audience in ["personal:7", "personal:9", "own:", "group:g1"] {
            assert!(server.holds(audience), "{audience}");
            assert!(!holder(Core::Child).holds(audience), "{audience}");
        }
    }

    #[test]
    fn an_owner_is_one_id() {
        assert!(holder(Core::Pc).check().is_ok());
        for owner in ["", "7,9", "7\n"] {
            let bad = Holder {
                core: Core::Pc,
                owner: owner.into(),
            };
            assert!(bad.check().is_err(), "{owner:?}");
        }
    }
}
