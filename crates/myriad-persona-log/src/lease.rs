//! The lease: which host core her one mind is on, numbered by an epoch that
//! only grows. Only the server issues it.
//!
//! She lives on the server and stays there (2026-11-16): her mind no
//! longer follows the owner from device to device. The server still issues
//! a new grant for these, and while she moves house it is held by no one
//! (below):
//! 1. **Handover**: the holder gives it up, with its last count in each
//!    audience ([`Through`]); the target catches up to them before it
//!    thinks. A move's commit is one too.
//! 2. **Force**: the server brings her back to itself from a side that
//!    held her from before. The old holder may still be running, offline:
//!    its rows from after the move are folded in and marked ([`standing`]).
//!
//! Grants of [`How::Takeover`] stay in the history and fold as they did;
//! the server no longer issues them. A holder cut off from the server stops
//! by itself all the same ([`here`]).
//!
//! **Frozen**: while she moves to a host core joining for the first time
//! (or is taken back from one), she is nowhere: a [`How::Freeze`] grant has
//! no holder, and every side stops ([`Here::Frozen`]). It ends one of two
//! ways, both issued by the server: the move is called off and the lease
//! goes back to whom it was frozen from ([`How::Thaw`]), or the move commits
//! and the lease goes to its target as a handover from that same side,
//! carrying what it wrote ([`Request::Commit`]). Nothing else moves a frozen
//! lease.
//!
//! Before a side has ever held or seen a lease (used alone, never connected
//! to the server) she thinks there alone and writes her rows at epoch
//! [`ALONE`]. Those rows go up as they are, in their place in that side's
//! count, the first time it pairs or syncs; from the first lease it knows
//! on, it never writes [`ALONE`] again (`admit` refuses an origin's epoch
//! going down). Only one side was ever alone if she lived nowhere else yet;
//! a side alone while she already lived elsewhere is folded like a force
//! ([`standing`]).
//!
//! **Interim**: the server may give a side that does not hold the lease a
//! [`Deputy`] ahead of time. Cut off from the holder for the stop time with
//! the owner in front of it, that side thinks as her interim host until
//! the deputy runs out or the holder is back in reach ([`Here::Interim`]),
//! writing at the epoch the deputy stands under. The holder may be thinking
//! too, out of its reach: those rows are kept and marked like a force's
//! ([`standing`], given the deputies the server issued). A deputy is not a
//! grant: it moves nothing and no epoch counts it.
//!
//! The times are first values, to be tuned once she is used for real. The
//! one the owner notices, how long a holder out of reach with the owner gone
//! keeps thinking, is the server's setting: it gives the value with the
//! lease, and a holder stops by it ([`HereInput::stop_after`]); an interim
//! host waits as long before it starts. Three minutes when nothing says
//! otherwise.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use myriad_persona_sync::{Rev, hlc};
use serde::{Deserialize, Serialize};

use crate::{
    LogError, Row,
    row::{Audience, Author, Replicate},
};

/// How long a holder cut off from the server, with no owner in its place,
/// keeps thinking before it stops by itself: three minutes.
pub const SELF_STOP_MICROS: i64 = 3 * 60 * 1_000_000;
/// The shortest a configured stop time may be: a minute. Shorter, a holder
/// would stop while the owner only looked away.
pub const MIN_STOP_AFTER_MICROS: i64 = 60 * 1_000_000;
/// The longest: a day.
pub const MAX_STOP_AFTER_MICROS: i64 = 24 * 60 * 60 * 1_000_000;

/// The stop time to use: `configured`, or three minutes when absent
/// ([`SELF_STOP_MICROS`]).
pub fn stop_after(configured: Option<i64>) -> Result<i64, LogError> {
    let Some(micros) = configured else {
        return Ok(SELF_STOP_MICROS);
    };
    if !(MIN_STOP_AFTER_MICROS..=MAX_STOP_AFTER_MICROS).contains(&micros) {
        return Err(LogError::new(format!(
            "stopAfter must be {MIN_STOP_AFTER_MICROS} to {MAX_STOP_AFTER_MICROS} microseconds"
        )));
    }
    Ok(micros)
}

/// The longest a [`Deputy`] may run, from its issue to its end: a day (the
/// contract's `timing.deputyMaxMs`). Past that her memory is no longer
/// settled on the side thinking alone, and its last backup is older than
/// what she wrote there.
pub const DEPUTY_MAX_MICROS: i64 = 24 * 60 * 60 * 1_000_000;

/// The epoch of her rows written before their side ever held or saw a
/// lease. No grant has it: the server's first grant is epoch 1.
pub const ALONE: u64 = 0;

/// How a grant came about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    /// The first one, when she first lives anywhere.
    First,
    Handover,
    Takeover,
    Force,
    /// She is moving house: no one holds it ([`Grant::holder`] is none).
    Freeze,
    /// The move was called off: back to the side it was frozen from.
    Thaw,
}

/// One issue of the lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Grant {
    pub epoch: u64,
    /// The host core that holds it (a clock node); none while she moves
    /// house ([`How::Freeze`]).
    pub holder: Option<String>,
    /// The server's clock when it issued it.
    pub issued: Rev,
    pub how: How,
    /// The holder it moved from: the one that stood when this one was
    /// issued. Every grant but the first names it, so the new holder knows
    /// whose rows to wait for ([`behind`]); grants kept from before it was
    /// written have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// For a handover, or a freeze: the previous holder's last shared count
    /// in each audience when it gave the lease up. A commit after a freeze
    /// carries the freeze's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub through: Option<Through>,
    /// For a freeze: the move it is frozen for.
    #[serde(default, rename = "move", skip_serializing_if = "Option::is_none")]
    pub moving: Option<String>,
}

/// A side's last shared count in each audience it wrote in: `{audience:
/// dseq}`. Counts are per audience ([`Row::dseq`]), so one number cannot
/// say it. An audience it wrote nothing in is left out, never 0.
pub type Through = BTreeMap<String, u64>;

/// Checks a [`Through`]: real audiences, counts from 1.
pub fn check_through(through: &Through) -> Result<(), LogError> {
    for (audience, &count) in through {
        Audience::parse(audience)?;
        if count == 0 {
            return Err(LogError::new(
                "`through` leaves out an audience with nothing in it",
            ));
        }
    }
    Ok(())
}

impl Grant {
    pub fn check(&self) -> Result<(), LogError> {
        if self.epoch == 0 {
            return Err(LogError::new("epochs start at 1"));
        }
        let frozen = self.how == How::Freeze;
        match &self.holder {
            Some(holder) => {
                if frozen {
                    return Err(LogError::new("a freeze has no holder"));
                }
                hlc::check_node(holder).map_err(|error| LogError::new(error.to_string()))?;
            }
            None if !frozen => return Err(LogError::new("only a freeze has no holder")),
            None => {}
        }
        match &self.moving {
            Some(moving) if frozen => {
                if moving.is_empty() || moving.chars().count() > 64 {
                    return Err(LogError::new("a freeze names its move"));
                }
            }
            Some(_) => return Err(LogError::new("only a freeze names a move")),
            None if frozen => return Err(LogError::new("a freeze names its move")),
            None => {}
        }
        if let Some(from) = &self.from {
            if self.how == How::First {
                return Err(LogError::new("the first grant moves from no one"));
            }
            hlc::check_node(from).map_err(|error| LogError::new(error.to_string()))?;
            if self.holder.as_deref() == Some(from.as_str()) {
                return Err(LogError::new("a grant moves from another holder"));
            }
        } else if frozen {
            return Err(LogError::new("a freeze names whom it is frozen from"));
        }
        if let Some(through) = &self.through {
            if !matches!(self.how, How::Handover | How::Freeze) {
                return Err(LogError::new(
                    "only a handover or a freeze carries `through`",
                ));
            }
            check_through(through)?;
        }
        Ok(())
    }

    fn rank(
        &self,
    ) -> (
        u64,
        std::cmp::Reverse<&Rev>,
        std::cmp::Reverse<Option<&str>>,
    ) {
        (
            self.epoch,
            std::cmp::Reverse(&self.issued),
            std::cmp::Reverse(self.holder.as_deref()),
        )
    }

    /// Whether `node` holds it.
    pub fn held_by(&self, node: &str) -> bool {
        self.holder.as_deref() == Some(node)
    }
}

/// The server's leave, issued ahead of time, for `holder`, a side that
/// does not hold the lease, to think as her interim host while the holder
/// is out of its reach ([`here`]). It is not a grant: it moves nothing, no
/// epoch counts it, and the side writes at `under`, the epoch standing when
/// it was issued. The server keeps every one it issued and marks her rows
/// by those alone ([`standings`]); one it did not issue proves nothing. A
/// side observes `issued` into its clock when it takes one, so what it
/// writes under it comes after.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Deputy {
    /// The side it is for (a clock node).
    pub holder: String,
    /// The epoch that stood when it was issued, which the side writes at.
    pub under: u64,
    /// The server's clock when it issued it.
    pub issued: Rev,
    /// When it runs out, in microseconds: from then on the side does not
    /// think under it.
    pub until: i64,
    /// Where the server put this side among those it issued deputies to,
    /// from 1, the one the owner used last. Shown for now; nothing decides
    /// by it yet.
    pub order: u32,
}

impl Deputy {
    pub fn check(&self) -> Result<(), LogError> {
        hlc::check_node(&self.holder).map_err(|error| LogError::new(error.to_string()))?;
        if self.under == 0 {
            return Err(LogError::new("a deputy stands under an epoch from 1"));
        }
        if self.order == 0 {
            return Err(LogError::new("a deputy's order counts from 1"));
        }
        let issued = wall_micros(&self.issued);
        if self.until <= issued {
            return Err(LogError::new("a deputy runs out after it is issued"));
        }
        if self.until - issued > DEPUTY_MAX_MICROS {
            return Err(LogError::new(format!(
                "a deputy runs {DEPUTY_MAX_MICROS} microseconds at most"
            )));
        }
        Ok(())
    }

    /// Whether `row` was written under it: one of her rows, by its holder,
    /// at the epoch it stands under, from its issue until it runs out.
    pub fn covers(&self, row: &Row) -> bool {
        row.author == Author::Her
            && row.origin == self.holder
            && row.epoch == Some(self.under)
            && row.hlc >= self.issued
            && wall_micros(&row.hlc) < self.until
    }
}

/// A revision's wall time in microseconds.
fn wall_micros(rev: &Rev) -> i64 {
    i64::try_from(rev.wall_ms)
        .unwrap_or(i64::MAX)
        .saturating_mul(1000)
}

/// Which of two grants stands: the higher epoch. Two grants of one epoch
/// should never exist (only the server issues, one at a time); if they do,
/// the one issued first stands, then the lower holder, so every side picks
/// the same one.
pub fn newer<'a>(a: &'a Grant, b: &'a Grant) -> &'a Grant {
    match a.rank().cmp(&b.rank()) {
        Ordering::Less => b,
        _ => a,
    }
}

/// A request to move the lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    /// The holder gives it to `to`.
    Handover {
        from: String,
        /// The epoch `from` holds.
        epoch: u64,
        to: String,
        /// `from`'s last shared count in each audience.
        #[serde(default)]
        through: Option<Through>,
    },
    /// She is brought to `to` (the server, from a side that held her
    /// from before).
    Force { to: String },
    /// She starts moving house (`move`): no one holds it until the move
    /// commits or is called off. The holder's last shared counts go with
    /// it, for the target to catch up to.
    Freeze {
        #[serde(rename = "move")]
        moving: String,
        #[serde(default)]
        through: Option<Through>,
    },
    /// The move is called off: back to the side it was frozen from.
    Thaw,
    /// The move commits: the lease goes to `to`, as a handover from the
    /// side it was frozen from.
    Commit { to: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveInput {
    /// The grant that stands now.
    pub lease: Grant,
    pub request: Request,
    /// The server's revision for a new grant (after the current one's).
    pub issued: Rev,
}

/// Why a move was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The target holds it already.
    AlreadyHolder,
    /// A handover from a side that does not hold it.
    NotHolder,
    /// A handover of an epoch that no longer stands.
    StaleEpoch,
    /// She is moving house: only the move's end moves the lease.
    Frozen,
    /// A thaw or a commit while she is not moving house.
    NotFrozen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Moved {
    Granted(Grant),
    Refused(Refusal),
}

/// Whether the server moves the lease as asked, and the new grant.
pub fn decide(input: MoveInput) -> Result<Moved, LogError> {
    let MoveInput {
        lease,
        request,
        issued,
    } = input;
    lease.check()?;
    if issued <= lease.issued {
        return Err(LogError::new(
            "a new grant is issued after the one standing",
        ));
    }
    let issue = |holder: Option<&str>, how, from: Option<String>, through, moving| {
        let grant = Grant {
            epoch: lease.epoch + 1,
            holder: holder.map(str::to_string),
            issued: issued.clone(),
            how,
            from,
            through,
            moving,
        };
        grant.check().map(|()| Moved::Granted(grant))
    };
    let refused = |refusal| Ok(Moved::Refused(refusal));
    // Frozen, only the move's end moves it: back to whom it was frozen
    // from, or on to the move's target as a handover from them.
    let Some(holder) = lease.holder.clone() else {
        let from = lease.from.clone();
        return match request {
            Request::Thaw => issue(from.as_deref(), How::Thaw, None, None, None),
            Request::Commit { to } => {
                if from.as_deref() == Some(to.as_str()) {
                    return refused(Refusal::AlreadyHolder);
                }
                issue(Some(&to), How::Handover, from, lease.through.clone(), None)
            }
            _ => refused(Refusal::Frozen),
        };
    };
    let grant = |to: &str, how, through| issue(Some(to), how, Some(holder.clone()), through, None);
    match request {
        Request::Thaw | Request::Commit { .. } => refused(Refusal::NotFrozen),
        Request::Freeze { moving, through } => issue(
            None,
            How::Freeze,
            Some(holder.clone()),
            through,
            Some(moving),
        ),
        Request::Handover {
            from,
            epoch,
            to,
            through,
        } => {
            if from != holder {
                return refused(Refusal::NotHolder);
            }
            if epoch != lease.epoch {
                return refused(Refusal::StaleEpoch);
            }
            if to == holder {
                return refused(Refusal::AlreadyHolder);
            }
            grant(&to, How::Handover, through)
        }
        Request::Force { to } => {
            if to == holder {
                return refused(Refusal::AlreadyHolder);
            }
            grant(&to, How::Force, None)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HereInput {
    /// This side's node.
    pub me: String,
    /// The newest grant this side knows; absent while it knows none (never
    /// connected): she thinks there alone, at epoch [`ALONE`].
    #[serde(default)]
    pub lease: Option<Grant>,
    /// This side's clock. A side that may be interim gives it as its rows
    /// read it (the wall of the revision it would stamp next, never behind
    /// real time): the server marks its rows by their clock against the
    /// deputy's `until`, so it stops when they would no longer be covered.
    /// That clock may run up to five minutes ahead of real time, so the
    /// stop time and the owner's last time here read against it too.
    pub now: i64,
    /// Since when this side has been cut off from the server; absent while
    /// it is connected.
    #[serde(default)]
    pub disconnected_since: Option<i64>,
    /// The last time the owner was in this side's place: input on it, or
    /// one of the owner's devices reaching it on the local network.
    #[serde(default)]
    pub owner_here_at: Option<i64>,
    /// The stop time the server gave with the lease; three minutes when
    /// absent ([`stop_after`]).
    #[serde(default)]
    pub stop_after: Option<i64>,
    /// When this side stopped by itself, while it holds the lease and the
    /// server has not yet said, since it came back in reach, that the lease
    /// still stands (a `lease.epoch` naming it). Stopped, it stays stopped:
    /// the owner coming back to it while it is cut off, or its coming back
    /// in reach, does not start her again, since the server may have taken
    /// her over meanwhile. The caller drops it once the server has said so.
    #[serde(default)]
    pub stopped_at: Option<i64>,
    /// The newest [`Deputy`] the server gave this side, when it gave one.
    #[serde(default)]
    pub deputy: Option<Deputy>,
    /// Since when the holder has been out of this side's reach, by this
    /// side's own judgment (the link silent, and not only this side's
    /// network down); absent while it is in reach.
    #[serde(default)]
    pub host_unreachable_since: Option<i64>,
}

/// What her mind does on this side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum Here {
    /// She thinks here: the holder, in reach or with the owner present.
    Thinking,
    /// The holder stopped by itself at `stoppedAt`: she no longer thinks or
    /// writes her rows here; it still takes the owner's rows and finishes
    /// work in hand. She thinks again only once the server, back in reach,
    /// has said the lease is unchanged ([`HereInput::stopped_at`]).
    #[serde(rename_all = "camelCase")]
    Stopped { stopped_at: i64 },
    /// Another side holds the lease (or a newer epoch moved it away): this
    /// side is a child.
    Child,
    /// No one holds it: she is moving house, and thinks nowhere until the
    /// move commits or is called off.
    Frozen,
    /// Another side holds the lease, but it has been out of this side's
    /// reach for the stop time, the owner is here, and this side holds the
    /// server's [`Deputy`] for the epoch that stands: she thinks here as
    /// her interim host, writing at that epoch, until `until`. She knows
    /// she is interim: somewhere out of sight she may be thinking too. The
    /// holder back in reach, the deputy run out, or the owner gone for the
    /// stop time, this side is a child again.
    #[serde(rename_all = "camelCase")]
    Interim { until: i64 },
}

pub fn here(input: HereInput) -> Result<Here, LogError> {
    let Some(lease) = &input.lease else {
        return Ok(Here::Thinking);
    };
    lease.check()?;
    if lease.holder.is_none() {
        return Ok(Here::Frozen);
    }
    if !lease.held_by(&input.me) {
        return interim(&input, lease);
    }
    if let Some(stopped_at) = input.stopped_at {
        return Ok(Here::Stopped { stopped_at });
    }
    let Some(cut_off) = input.disconnected_since else {
        return Ok(Here::Thinking);
    };
    let from = input
        .owner_here_at
        .map_or(cut_off, |seen| seen.max(cut_off));
    let stopped_at = from + stop_after(input.stop_after)?;
    Ok(if input.now >= stopped_at {
        Here::Stopped { stopped_at }
    } else {
        Here::Thinking
    })
}

/// A side that does not hold the lease: a child, or her interim host when
/// its deputy is for it and for the epoch that stands, has not run out,
/// the holder has been out of its reach for the stop time, and the owner
/// was here within it. Without a deputy, or with one that does not
/// check, a child as ever.
fn interim(input: &HereInput, lease: &Grant) -> Result<Here, LogError> {
    // One that does not check (from a newer server, say) is as none: a
    // child, as without one, never an error that leaves it not knowing.
    let Some(deputy) = input
        .deputy
        .as_ref()
        .filter(|deputy| deputy.check().is_ok())
    else {
        return Ok(Here::Child);
    };
    let stop = stop_after(input.stop_after)?;
    let (Some(cut_off), Some(owner_here_at)) = (input.host_unreachable_since, input.owner_here_at)
    else {
        return Ok(Here::Child);
    };
    let now = input.now;
    let thinks = deputy.holder == input.me
        && deputy.under == lease.epoch
        && now < deputy.until
        && now - cut_off >= stop
        && now - owner_here_at < stop;
    Ok(if thinks {
        Here::Interim {
            until: deputy.until,
        }
    } else {
        Here::Child
    })
}

/// What a new holder still lacks before it may think: of each audience in
/// `through` (the previous holder `from`'s counts when it gave the lease
/// up), the count it waits for, when what it holds of `from` there
/// (`held`, one vector per audience) does not reach it yet. Empty: caught
/// up, she thinks. A takeover or a force has no `through`: nothing to wait
/// for (what the old holder did not send in time is her own memory
/// catching up later, not a reason to wait).
pub fn behind(
    through: Option<&Through>,
    from: &str,
    held: &BTreeMap<String, crate::Vector>,
) -> Through {
    through
        .into_iter()
        .flatten()
        .filter(|(audience, count)| {
            let have = held
                .get(*audience)
                .and_then(|vector| vector.get(from))
                .copied()
                .unwrap_or(0);
            have < **count
        })
        .map(|(audience, count)| (audience.clone(), *count))
        .collect()
}

/// Where one of her rows stands against the lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Standing {
    pub id: String,
    pub origin: String,
    pub epoch: u64,
    pub standing: Stand,
    /// For [`Stand::Concurrent`]: the epoch it overlaps (the forced one,
    /// the one standing elsewhere while it was written alone, or the one a
    /// deputy it was written under stands under).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stand {
    /// Written while its epoch stood, or before the move that ended it
    /// (rows that arrive late after a takeover are her own memory catching
    /// up, not another her).
    Normal,
    /// Written by the old holder after the owner forced a move, written
    /// alone ([`ALONE`]) while a lease stood elsewhere, or written by an
    /// interim host under its deputy: she was thinking in two places for a
    /// while. Kept and marked.
    Concurrent,
    /// Not written by the holder of its epoch (nor under a deputy for it),
    /// or written after it handed the lease over: a lease bug, to look
    /// into.
    NotHolder,
    AfterHandover,
}

/// Where `row` stands against `grants` (any order, any subset of the
/// history) and the `deputies` the server issued (none: as before there
/// were any). The owner's rows, and her rows of an epoch not in `grants`,
/// are normal.
pub fn standing(row: &Row, grants: &[Grant], deputies: &[Deputy]) -> Stand {
    placed(row, grants, deputies).0
}

/// The standing, and for [`Stand::Concurrent`] the epoch it overlaps.
fn placed(row: &Row, grants: &[Grant], deputies: &[Deputy]) -> (Stand, Option<u64>) {
    let normal = (Stand::Normal, None);
    let (Author::Her, Some(epoch)) = (row.author, row.epoch) else {
        return normal;
    };
    if epoch == ALONE {
        return alone(row, grants);
    }
    let Some(own) = issue(grants, epoch) else {
        return normal;
    };
    if !own.held_by(&row.origin) {
        // Written by an interim host under its deputy: concurrent with the
        // epoch it stands under, whatever came after it.
        if deputies.iter().any(|deputy| deputy.covers(row)) {
            return (Stand::Concurrent, Some(epoch));
        }
        return (Stand::NotHolder, None);
    }
    let Some(next) = next_epoch(epoch, grants).and_then(|next| issue(grants, next)) else {
        return normal;
    };
    let after_issue = row.hlc > next.issued;
    match next.how {
        How::Handover | How::Freeze => {
            // An audience the handover leaves out had none of the old
            // holder's rows in it: every one there came after.
            let after = match (row.replicate, row.dseq, &next.through) {
                (Replicate::Shared, Some(dseq), Some(through)) => {
                    dseq > through.get(&row.audience).copied().unwrap_or(0)
                }
                _ => after_issue,
            };
            if after {
                (Stand::AfterHandover, None)
            } else {
                normal
            }
        }
        How::Force if after_issue => (Stand::Concurrent, Some(next.epoch)),
        How::Force | How::Takeover | How::First | How::Thaw => normal,
    }
}

/// A row written alone: concurrent with the lease that stood elsewhere when
/// it was written, if one did. The lease standing then is the newest issued
/// before the row's clock; one held by the row's own side is its own first
/// lease, issued while the row was still on its way (a clock a little
/// ahead puts the row just after it).
fn alone(row: &Row, grants: &[Grant]) -> (Stand, Option<u64>) {
    let then = grants
        .iter()
        .filter(|grant| grant.issued < row.hlc)
        .reduce(|a, b| newer(a, b));
    match then {
        Some(grant) if !grant.held_by(&row.origin) => (Stand::Concurrent, Some(grant.epoch)),
        _ => (Stand::Normal, None),
    }
}

/// Every one of `rows` that does not stand normal, in log order, against
/// `grants` and `deputies` as in [`standing`].
pub fn standings(rows: &[Row], grants: &[Grant], deputies: &[Deputy]) -> Vec<Standing> {
    rows.iter()
        .filter_map(|row| {
            let (stand, with) = placed(row, grants, deputies);
            (stand != Stand::Normal).then(|| Standing {
                id: row.id.clone(),
                origin: row.origin.clone(),
                epoch: row.epoch.unwrap_or(ALONE),
                standing: stand,
                with,
            })
        })
        .collect()
}

/// The grant of `epoch` that stands, when `grants` has one.
fn issue(grants: &[Grant], epoch: u64) -> Option<&Grant> {
    grants
        .iter()
        .filter(|grant| grant.epoch == epoch)
        .reduce(|a, b| newer(a, b))
}

/// The first epoch after `epoch` in `grants`.
fn next_epoch(epoch: u64, grants: &[Grant]) -> Option<u64> {
    grants
        .iter()
        .map(|grant| grant.epoch)
        .filter(|&next| next > epoch)
        .min()
}
