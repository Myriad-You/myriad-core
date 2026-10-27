//! Who may hear a memory: what was learned in front of one audience may be
//! said again only in front of an audience it admits. In a group, only what
//! was learned in that same group; in private, never what a group heard,
//! and only if everyone present was there when it was learned.
//!
//! Moved from the site backend (`backend/src/services/agent/memory/unified.rs`,
//! `admits` and `audience_admits`) with the stored row replaced by the
//! audience it was learned in. Members are whatever the host identifies
//! people by (the site's account ids, the Mac's subject strings); the rule
//! only compares them.
//!
//! What is not here, because the two hosts do not agree on it (see the PR
//! that brought it in): a memory of her own (`own`), which the site keeps
//! out of `admits` and reads by its own queries while the Mac admits it
//! everywhere; reading a stored row's members, where the site takes a row
//! with none recorded as its person's; and how long a group's id may be.

use serde::{Deserialize, Serialize};

/// Where something was said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Venue {
    /// One person, or a few, and her.
    Private,
    /// A group chat, by the host's id for it (`telegram:-100123`). Open:
    /// people outside the community may be listening.
    Group(String),
}

/// Who was present when something was said, and where. In JSON:
/// `{venue: "private", members}` or `{venue: "group", group, members}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "Wire<M>",
    into = "Wire<M>",
    bound(
        deserialize = "M: Deserialize<'de>",
        serialize = "M: Serialize + Clone"
    )
)]
pub struct Audience<M> {
    pub members: Vec<M>,
    pub venue: Venue,
}

/// Whether a memory learned in front of `learned` may be said in front of
/// `present`. In a group: only what was learned in that same group,
/// whoever said it. In private: never what a group heard, and only if
/// everyone present was there.
pub fn admits<M: PartialEq>(learned: &Audience<M>, present: &Audience<M>) -> bool {
    match (&present.venue, &learned.venue) {
        (Venue::Group(here), Venue::Group(there)) => here == there,
        (Venue::Group(_), Venue::Private) | (Venue::Private, Venue::Group(_)) => false,
        (Venue::Private, Venue::Private) => members_admit(&learned.members, &present.members),
    }
}

/// Whether a memory learned before `original` may be said in front of
/// `present`: someone is present, and everyone present was there.
pub fn members_admit<M: PartialEq>(original: &[M], present: &[M]) -> bool {
    !present.is_empty() && present.iter().all(|id| original.contains(id))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Private,
    Group,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Wire<M> {
    venue: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    members: Vec<M>,
}

impl<M> TryFrom<Wire<M>> for Audience<M> {
    type Error = &'static str;

    fn try_from(wire: Wire<M>) -> Result<Self, Self::Error> {
        let venue = match (wire.venue, wire.group) {
            (Kind::Private, None) => Venue::Private,
            (Kind::Group, Some(id)) => Venue::Group(id),
            (Kind::Private, Some(_)) => return Err("a private audience names no group"),
            (Kind::Group, None) => return Err("a group audience names its group"),
        };
        Ok(Self {
            members: wire.members,
            venue,
        })
    }
}

impl<M> From<Audience<M>> for Wire<M> {
    fn from(audience: Audience<M>) -> Self {
        let (venue, group) = match audience.venue {
            Venue::Private => (Kind::Private, None),
            Venue::Group(id) => (Kind::Group, Some(id)),
        };
        Self {
            venue,
            group,
            members: audience.members,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private(members: &[i32]) -> Audience<i32> {
        Audience {
            members: members.to_vec(),
            venue: Venue::Private,
        }
    }

    fn group(id: &str, members: &[i32]) -> Audience<i32> {
        Audience {
            members: members.to_vec(),
            venue: Venue::Group(id.into()),
        }
    }

    // The backend's unified/tests.rs cases, unchanged.
    #[test]
    fn a_memory_is_said_only_where_everyone_present_was_there() {
        assert!(members_admit(&[7], &[7]));
        assert!(!members_admit(&[7], &[8]));
        assert!(members_admit(&[7, 8, 9], &[7, 8]));
        assert!(!members_admit(&[7], &[7, 8]));
        assert!(!members_admit::<i32>(&[7], &[]));
    }

    #[test]
    fn a_group_hears_only_what_was_said_in_that_group() {
        let here = group("telegram:-100123", &[7]);
        let learned_private = private(&[7]);
        assert!(
            !admits(&learned_private, &here),
            "a private fact stays out of a group"
        );
        let learned_here = group("telegram:-100123", &[8]);
        assert!(
            admits(&learned_here, &here),
            "whoever said it, the group heard it"
        );
        let elsewhere = group("telegram:-100999", &[8]);
        assert!(!admits(&elsewhere, &here), "another group is other people");
        assert!(
            !admits(&learned_here, &private(&[8])),
            "what a group heard stays in the group"
        );
        assert!(admits(&learned_private, &private(&[7])));
    }

    #[test]
    fn nobody_recorded_admits_nobody() {
        assert!(!admits(&private(&[]), &private(&[7])));
        assert!(!admits(&private(&[7]), &private(&[])));
    }

    #[test]
    fn the_json_form_names_a_group_only_in_a_group() {
        let parsed: Audience<String> =
            serde_json::from_str(r#"{"venue":"group","group":"onebot:1","members":["7"]}"#)
                .unwrap();
        assert_eq!(parsed.venue, Venue::Group("onebot:1".into()));
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            r#"{"venue":"group","group":"onebot:1","members":["7"]}"#
        );
        for bad in [
            r#"{"venue":"group","members":["7"]}"#,
            r#"{"venue":"private","group":"onebot:1","members":["7"]}"#,
            r#"{"venue":"own","members":[]}"#,
            r#"{"venue":"private","members":["7"],"extra":1}"#,
        ] {
            assert!(
                serde_json::from_str::<Audience<String>>(bad).is_err(),
                "{bad}"
            );
        }
    }
}
