//! Her present across people (`myriad_merope::elsewhere`): how today has
//! left her, and her talks with others since she got up, as the section
//! that tells them. Every input is a time, a kind or a number; people are
//! told apart by whatever key the end has for them (`person`), never named.

use std::collections::BTreeMap;

use myriad_merope::{
    elsewhere::{self, Cause, Facing, Heard, Moved, Place, Push, Stung},
    sore::Weight,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{at, input, opt_at, text};
use crate::Failure;

pub(super) fn own_feeling(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PushIn {
        at: i64,
        /// How far it moved her emotion (after less before).
        valence: f64,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        /// Every push of her emotion toward anyone since she got up.
        pushes: Vec<PushIn>,
        now: i64,
    }
    let req: In = input(raw)?;
    let pushes = req
        .pushes
        .iter()
        .map(|push| {
            Ok(Push {
                at: at(push.at)?,
                valence: push.valence,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    // An empty sum is -0.0; `+ 0.0` makes it the plain 0 every end writes alike.
    Ok(json!({ "feeling": elsewhere::own_feeling(&pushes, at(req.now)?) + 0.0 }))
}

/// How far today has left her, as `elsewhere.own_feeling` gave it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FeelingIn {
    feeling: f64,
}

/// Whether how today has left her is low enough to narrow her, as
/// tiredness does (recall keeps to what was said).
pub(super) fn own_feeling_is_low(raw: &[u8]) -> Result<Value, Failure> {
    let req: FeelingIn = input(raw)?;
    Ok(json!({ "low": elsewhere::own_feeling_is_low(req.feeling) }))
}

/// How today has left her in words, for the facts a decision reads; `null`
/// when it says nothing.
pub(super) fn own_feeling_words(raw: &[u8]) -> Result<Value, Failure> {
    let req: FeelingIn = input(raw)?;
    Ok(json!({ "words": elsewhere::own_feeling_words(req.feeling) }))
}

/// Where a talk happened: `{kind: "private"}`, `{kind: "group", venue}`,
/// `{kind: "work"}`.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum PlaceIn {
    Private,
    Group { venue: String },
    Work,
}

impl From<PlaceIn> for Place {
    fn from(place: PlaceIn) -> Self {
        match place {
            PlaceIn::Private => Self::Private,
            PlaceIn::Group { venue } => Self::Group(venue),
            PlaceIn::Work => Self::Work,
        }
    }
}

/// People as the crate counts them: each key the end gave, by its order.
#[derive(Default)]
struct People(BTreeMap<String, i32>);

impl People {
    fn of(&mut self, key: &str) -> i32 {
        let next = self.0.len() as i32;
        *self.0.entry(key.to_string()).or_insert(next)
    }
}

pub(super) fn since_up_section(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct HeardIn {
        person: String,
        place: PlaceIn,
        at: i64,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct MovedIn {
        person: String,
        at: i64,
        /// The recorded cause (`user_turn`, `user_praise`, `task_ok`, …).
        cause: String,
        /// Her emotion toward them after it, off even (0 is even).
        felt_after: f64,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StungIn {
        person: String,
        place: PlaceIn,
        weight: Weight,
        since: i64,
        #[serde(default)]
        mended: Option<i64>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct FacingIn {
        person: String,
        /// The group's venue, in a group.
        #[serde(default)]
        group: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        /// Messages others sent her since she got up.
        heard: Vec<HeardIn>,
        /// Changes of her affect toward anyone since she got up.
        moved: Vec<MovedIn>,
        /// Her open sore spots, and the ones made right.
        #[serde(default)]
        stung: Vec<StungIn>,
        /// People who first talked with her since she got up.
        #[serde(default)]
        first_met: Vec<String>,
        /// When she got up.
        since: i64,
        /// Who she is facing: left out, as they are in front of her.
        facing: FacingIn,
        now: i64,
    }
    let req: In = input(raw)?;
    let mut people = People::default();
    let facing_person = people.of(&req.facing.person);
    let heard = req
        .heard
        .into_iter()
        .map(|heard| {
            Ok(Heard {
                person: people.of(&heard.person),
                place: heard.place.into(),
                at: at(heard.at)?,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let moved = req
        .moved
        .into_iter()
        .map(|moved| {
            Ok(Moved {
                person: people.of(&moved.person),
                at: at(moved.at)?,
                cause: Cause::parse(&moved.cause),
                felt_after: moved.felt_after,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let stung = req
        .stung
        .into_iter()
        .map(|sore| {
            Ok(Stung {
                person: people.of(&sore.person),
                place: sore.place.into(),
                weight: sore.weight,
                since: at(sore.since)?,
                mended: opt_at(sore.mended)?,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let first_met: Vec<i32> = req.first_met.iter().map(|key| people.of(key)).collect();
    let facing = Facing {
        person: facing_person,
        group: req.facing.group.as_deref(),
    };
    let talks = elsewhere::talks(&heard, &moved, &stung, &first_met, at(req.since)?, facing);
    Ok(text(Some(elsewhere::section(&talks, at(req.now)?))))
}
