//! Her log across host cores (`myriad_persona_log`): a row's one byte
//! form, what a side holds and lacks, checking and merging what arrives,
//! the fold, the lease and the clock. The rules are the crate's; these calls only
//! carry them across.

use myriad_persona_log::{
    Grant, Holder, Row, Vector, admit as admit_rows, clock, fold as fold_rows, lease,
    merge as merge_rows, vector,
};
use myriad_persona_sync::Rev;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::input;
use crate::Failure;

fn refused(error: impl std::fmt::Display) -> Failure {
    Failure::BadInput(error.to_string())
}

fn answer(value: &impl Serialize) -> Result<Value, Failure> {
    serde_json::to_value(value).map_err(refused)
}

/// One row in, checked: the row, its canonical bytes as a string and its
/// digest.
pub(super) fn canonical(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        row: Row,
    }
    let In { row } = input(raw)?;
    row.check().map_err(refused)?;
    let canonical = String::from_utf8(row.canonical_bytes())
        .map_err(|_| refused("canonical bytes are UTF-8"))?;
    Ok(json!({ "row": row, "canonical": canonical, "digest": row.digest() }))
}

/// `{rows}` (one audience's) → `{vector, beyond?}`.
pub(super) fn holdings(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        rows: Vec<Row>,
    }
    let In { rows } = input(raw)?;
    answer(&vector::holdings(&rows).map_err(refused)?)
}

/// `{mine, theirs}` → `{missing: [{origin, from, through}]}`.
pub(super) fn missing(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        mine: Vector,
        theirs: Vector,
    }
    let In { mine, theirs } = input(raw)?;
    Ok(json!({ "missing": vector::missing(&mine, &theirs) }))
}

/// `{rows, theirs, holder}` → `{rows, notHeld?}`: what to send a side
/// holding `theirs`; nothing to a side that may not hold the audience.
/// Only for a log from before ABI 2: rows that name another node (`to`,
/// `takes`) or a retract are refused, and go by `nodes.wire_send`.
pub(super) fn send(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        rows: Vec<Row>,
        theirs: Vector,
        holder: Holder,
    }
    let In {
        rows,
        theirs,
        holder,
    } = input(raw)?;
    answer(&vector::answer_to(&rows, &theirs, &holder).map_err(refused)?)
}

/// A retract in the batch needs this side's ledger, so a home never loses
/// its node to a call that did not say what it keeps.
pub(super) fn admit(raw: &[u8]) -> Result<Value, Failure> {
    answer(&admit_rows(input(raw)?).map_err(refused)?)
}

pub(super) fn merge(raw: &[u8]) -> Result<Value, Failure> {
    answer(&merge_rows(input(raw)?).map_err(refused)?)
}

/// `{rows, leases?, ledger?}` → `{objects, standing?}`: the ledger is this
/// side's, as `nodes.fold` takes it.
pub(super) fn fold(raw: &[u8]) -> Result<Value, Failure> {
    answer(&fold_rows(input(raw)?).map_err(refused)?)
}

/// `{rows, ledger?}` → `{ledger}`: the ledger with the rows this side
/// wrote.
pub(super) fn ledger(raw: &[u8]) -> Result<Value, Failure> {
    answer(&myriad_persona_log::forget::record(input(raw)?).map_err(refused)?)
}

/// `{rows, ledger}` → `{stubbed}`: which of this side's rows its ledger
/// takes away now.
pub(super) fn reach(raw: &[u8]) -> Result<Value, Failure> {
    answer(&myriad_persona_log::forget::reach(input(raw)?).map_err(refused)?)
}

/// `{sent, serverTime, received}` (local, server, local; microseconds) →
/// `{offset, uncertainty}`.
pub(super) fn clock_offset(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct In {
        sent: i64,
        server_time: i64,
        received: i64,
    }
    let In {
        sent,
        server_time,
        received,
    } = input(raw)?;
    answer(&clock::offset(sent, server_time, received).map_err(refused)?)
}

/// `{last?, now, offset, node}` → `{rev, poisoned}`: the stamp for a write
/// at corrected time, and whether it is still too far ahead to send.
pub(super) fn clock_next(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        #[serde(default)]
        last: Option<Rev>,
        now: i64,
        offset: i64,
        node: String,
    }
    let In {
        last,
        now,
        offset,
        node,
    } = input(raw)?;
    let rev = clock::next(last.as_ref(), now, offset, &node).map_err(refused)?;
    let poisoned = clock::poisoned(&rev, now, offset).map_err(refused)?;
    Ok(json!({ "rev": rev, "poisoned": poisoned }))
}

/// `{last?, seen, now, offset, node}` → `{last}`.
pub(super) fn clock_observe(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        #[serde(default)]
        last: Option<Rev>,
        seen: Rev,
        now: i64,
        offset: i64,
        node: String,
    }
    let In {
        last,
        seen,
        now,
        offset,
        node,
    } = input(raw)?;
    let last = clock::observe(last.as_ref(), &seen, now, offset, &node).map_err(refused)?;
    Ok(json!({ "last": last }))
}

pub(super) fn clock_repair(raw: &[u8]) -> Result<Value, Failure> {
    answer(&clock::repair(input(raw)?).map_err(refused)?)
}

pub(super) fn lease_move(raw: &[u8]) -> Result<Value, Failure> {
    answer(&lease::decide(input(raw)?).map_err(refused)?)
}

pub(super) fn lease_here(raw: &[u8]) -> Result<Value, Failure> {
    answer(&lease::here(input(raw)?).map_err(refused)?)
}

/// `{through?, from, held}` → `{behind}`: what a new holder still waits
/// for before it thinks.
pub(super) fn lease_behind(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        #[serde(default)]
        through: Option<lease::Through>,
        from: String,
        held: std::collections::BTreeMap<String, myriad_persona_log::Vector>,
    }
    let In {
        through,
        from,
        held,
    } = input(raw)?;
    if let Some(through) = &through {
        lease::check_through(through).map_err(refused)?;
    }
    Ok(serde_json::json!({ "behind": lease::behind(through.as_ref(), &from, &held) }))
}

/// `{a, b}` → `{newer: "a" | "b"}`.
pub(super) fn lease_newer(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        a: Grant,
        b: Grant,
    }
    let In { a, b } = input(raw)?;
    a.check().map_err(refused)?;
    b.check().map_err(refused)?;
    let newer = if lease::newer(&a, &b) == &a { "a" } else { "b" };
    Ok(json!({ "newer": newer }))
}
