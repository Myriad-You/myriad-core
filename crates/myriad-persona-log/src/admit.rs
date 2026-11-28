//! Checking a batch of rows from another side before it goes into the log.
//!
//! If any row's clock is more than five minutes ahead of this side's, the
//! whole batch is refused and nothing is observed (`hlc::check_ahead`): one
//! row from the future would win every later change. Otherwise each row is
//! taken or refused on its own. A row already forgotten here is taken as a
//! stub, so the vector still fills.
//!
//! **Refused for good, or for now.** A row refused for its envelope (it
//! does not read here: a kind or a version of a newer core, a field this
//! core holds wrong) is refused the same however often it is sent
//! ([`Reason::lasting`]), and would keep this side's count of its side
//! short for ever: the side would ask for it again on every link, hold
//! every later row past a gap, and never be seen to keep up. Such a row is
//! taken as its place instead ([`Row::place_of_refused`], kind
//! `log.refused`): its id, side, count, clock and audience, nothing it
//! said. The refusal is still listed, marked `placed`, for the host to
//! count and warn of. The place is a function of the row alone, so every
//! side that refuses it holds the same one; it goes on like any shared
//! row, so a side that gets it from here keeps up too; and a side that
//! takes the row itself keeps the row ([`crate::merge`]).
//!
//! A row that may forget or reach other rows (a forget, a retract, a kind
//! named like one, a row about a subject or one row, a row naming another
//! node or listing rows) is never placed: its place would let every count
//! move past what it forgets, and whatever it forgets would read back for
//! good. It stays a gap and is asked for again, until a core that reads it
//! takes it. A row refused for what this side is told or what came first
//! (`audience_changed`: which audience an object is in is the first row's,
//! and a group's members may not be given; `epoch_back`, `not_a_member`,
//! `not_held`) takes no place either and still holds the count: two sides
//! could refuse different rows, and a place would hide that for good.
//!
//! One side's her rows in one audience never go down in epoch along its own
//! order: a side writes [`crate::lease::ALONE`] only before it knows any
//! lease, so those rows come before its first epoch in its count there, and
//! arrive first. Each audience is counted and sent on its own, so the check
//! is per audience: alone rows of one audience may well arrive after
//! another audience's later rows.

use std::collections::{BTreeMap, BTreeSet};

use myriad_persona_sync::hlc;
use serde::{Deserialize, Serialize};

use crate::{
    Holder, LogError, Row,
    forget::{self, Entry},
    partner,
    row::{Audience, Replicate, sort},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdmitInput {
    /// The batch, as it arrived.
    pub rows: Vec<Row>,
    /// This side's clock.
    pub now: i64,
    /// The audience of every object this side already holds rows of (only
    /// the objects the batch touches are needed). An object never changes
    /// audience within one partner's audiences ([`same_partner`]): node ids
    /// are unique only per (account, audience), so the same id under
    /// another partner's private talk is another account's node, and is
    /// not counted.
    #[serde(default)]
    pub known: BTreeMap<String, String>,
    /// Who is in a group, for the groups this side knows; a subject outside
    /// them is refused. Groups not listed are not checked.
    #[serde(default)]
    pub members: BTreeMap<String, BTreeSet<String>>,
    /// This side's ledger: what it forgot, took back and keeps, and the
    /// other names of its nodes ([`crate::forget`]). Refused without once a
    /// retract is in the batch: what this side keeps may be only on it.
    #[serde(default)]
    pub ledger: Option<Vec<Entry>>,
    /// For each audience, then each side, the highest epoch of its her
    /// rows this side holds there.
    #[serde(default)]
    pub epochs: Epochs,
    /// This side: a row it may not hold is refused (`not_held`,
    /// [`crate::Holder::holds_row`]).
    pub holder: Holder,
    /// The id the other side goes by, as its link names it: a batch from a
    /// side under another id is refused whole ([`crate::partner::exchange`]).
    pub peer: String,
}

/// The partner `audience` is of, on this side: the one member of a
/// partner's private talk, or this side's own partner (her own audience, a
/// group, someone else's private talk with her).
fn partner_of<'a>(audience: &'a str, holder: &'a Holder) -> &'a str {
    match Audience::parse(audience) {
        Ok(Audience::Personal(member)) if partner::is_partners(member) => member,
        _ => &holder.owner,
    }
}

/// Whether an object known under `known` keeps a row of `audience` out: only
/// when both are the same partner's. A partner's private talk is that
/// partner's; her own audience and someone else's private talk with her are
/// this side's partner's; a group may be any partner's, unless `members`
/// lists who is in it and the partner is not: what this side does not know
/// never loosens the check.
fn same_partner(
    known: &str,
    audience: &str,
    holder: &Holder,
    members: &BTreeMap<String, BTreeSet<String>>,
) -> bool {
    let partner = partner_of(audience, holder);
    match Audience::parse(known) {
        Ok(Audience::Personal(member)) if partner::is_partners(member) => member == partner,
        Ok(Audience::Group(_)) => members
            .get(known)
            .is_none_or(|group| group.is_empty() || group.contains(partner)),
        _ => partner == holder.owner,
    }
}

/// Audience → side → the highest epoch of that side's her rows held there.
pub type Epochs = BTreeMap<String, BTreeMap<String, u64>>;

/// Why one row was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// The envelope is not whole (see `detail`).
    Envelope,
    /// A local row: it never leaves the side that wrote it.
    Local,
    /// Its object is in another audience.
    AudienceChanged,
    /// Its subject is not in the group.
    NotAMember,
    /// Her row with a lower epoch than one its side wrote before it in the
    /// same audience (an alone row after the side knew a lease, most often).
    EpochBack,
    /// Its audience is not one this side may hold.
    NotHeld,
}

impl Reason {
    /// Whether the row is refused however often it comes, and whichever
    /// side refuses it: its envelope, which this core reads or not. Such a
    /// row is taken as its place, unless it may forget
    /// ([`Row::place_of_refused`]). The others hang on what this side is
    /// told (known audiences and which row came first, epochs, members, its
    /// own id), which may change or differ from side to side.
    pub fn lasting(self) -> bool {
        matches!(self, Reason::Envelope)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refusal {
    pub id: String,
    pub reason: Reason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Refused for good and taken as its place (`log.refused`, in
    /// `accepted`): its side's count here goes on past it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub placed: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The whole batch refused: a clock too far ahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockAhead {
    pub ahead_ms: u64,
    pub max_ahead_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Admitted {
    /// The rows to merge, in log order; forgotten ones already stubs.
    pub accepted: Vec<Row>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refused: Vec<Refusal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clock_ahead: Option<ClockAhead>,
    /// This side's ledger with the batch's forgets added.
    pub ledger: Vec<Entry>,
    /// `epochs` with the batch's her rows added.
    pub epochs: Epochs,
}

/// A batch checked. Refused as a whole when a retract is in the batch and
/// no ledger is given: a home never loses its node to a call that did not
/// say what it keeps.
pub fn admit(input: AdmitInput) -> Result<Admitted, LogError> {
    let AdmitInput {
        mut rows,
        now,
        mut known,
        members,
        ledger,
        mut epochs,
        holder,
        peer,
    } = input;
    holder.check()?;
    crate::partner::exchange(&holder.owner, &peer)?;
    forget::given(ledger.as_deref(), &rows)?;
    let ledger = ledger.unwrap_or_default();
    let ahead = rows
        .iter()
        .map(|row| hlc::ahead_ms(&row.hlc, now))
        .max()
        .unwrap_or(0);
    if ahead > hlc::MAX_AHEAD_MS {
        return Ok(Admitted {
            accepted: Vec::new(),
            refused: Vec::new(),
            clock_ahead: Some(ClockAhead {
                ahead_ms: ahead,
                max_ahead_ms: hlc::MAX_AHEAD_MS,
            }),
            ledger,
            epochs,
        });
    }
    sort(&mut rows);
    let mut accepted = Vec::new();
    let mut refused = Vec::new();
    // A row refused for good is taken as its place, when it has one this
    // side may hold.
    let refuse = |accepted: &mut Vec<Row>,
                  row: &Row,
                  reason: Reason,
                  detail: Option<String>| {
        let place = match reason.lasting() {
            true => row
                .place_of_refused()
                .filter(|place| holder.holds_row(place)),
            false => None,
        };
        let placed = place.is_some();
        accepted.extend(place);
        Refusal {
            id: row.id.clone(),
            reason,
            detail,
            placed,
        }
    };
    for row in rows {
        if let Err(error) = row.check() {
            let detail = Some(error.to_string());
            refused.push(refuse(&mut accepted, &row, Reason::Envelope, detail));
            continue;
        }
        if row.replicate == Replicate::Local {
            refused.push(refuse(&mut accepted, &row, Reason::Local, None));
            continue;
        }
        // A stub of a row about the partner under another id is held: it
        // keeps only its place, so this side's count has no gap. Never a
        // forget's.
        if !holder.holds_row(&row) && !holder.holds_stub(&row) {
            refused.push(refuse(&mut accepted, &row, Reason::NotHeld, None));
            continue;
        }
        if let Some(epoch) = row.epoch
            && epochs
                .get(&row.audience)
                .and_then(|sides| sides.get(&row.origin))
                .is_some_and(|&held| epoch < held)
        {
            refused.push(refuse(&mut accepted, &row, Reason::EpochBack, None));
            continue;
        }
        if known.get(&row.object).is_some_and(|audience| {
            *audience != row.audience && same_partner(audience, &row.audience, &holder, &members)
        }) {
            let reason = Reason::AudienceChanged;
            refused.push(refuse(&mut accepted, &row, reason, None));
            continue;
        }
        let outside = match (Audience::parse(&row.audience), row.subject.as_deref()) {
            (Ok(Audience::Group(_)), Some(subject)) => members
                .get(&row.audience)
                .is_some_and(|group| !group.contains(subject)),
            _ => false,
        };
        if outside {
            refused.push(refuse(&mut accepted, &row, Reason::NotAMember, None));
            continue;
        }
        known
            .entry(row.object.clone())
            .or_insert_with(|| row.audience.clone());
        if let Some(epoch) = row.epoch {
            let held = epochs
                .entry(row.audience.clone())
                .or_default()
                .entry(row.origin.clone())
                .or_insert(epoch);
            *held = (*held).max(epoch);
        }
        accepted.push(row);
    }
    let ledger = forget::joined(&ledger, &forget::ledger(&accepted));
    let (accepted, _) = forget::absorb(accepted, &ledger, forget::Side::Here);
    Ok(Admitted {
        accepted,
        refused,
        clock_ahead: None,
        ledger,
        epochs,
    })
}
