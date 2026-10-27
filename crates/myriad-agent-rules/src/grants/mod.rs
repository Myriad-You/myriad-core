//! What she may use, and when her own Work may run without the user.
//!
//! Permissions come in three layers: what a capability declares, what the
//! user approved, and what the host grants at run time. Only the runtime
//! grant authorizes; a declaration, an approval, a proposal or a plan never
//! does. [`granted`] is where the layers meet. Her own (autonomous) Work runs
//! under a personal autonomy grant, re-read against what is granted now at
//! every check, and capped at what it was claimed with.
//!
//! Moved from the site backend (`backend/src/services/agent/consciousness/grant.rs`:
//! `evaluate_autonomy_grant`, `intention_may_enter_work`,
//! `autonomy_cap_still_allows`, `effective_granted_permissions`,
//! `autonomy_execute_permission_error`, `prepare_personal_grant`,
//! `revoke_personal_grant`; `…/consciousness/dispatch.rs`:
//! `autonomy_claim_decision`, `autonomy_cap_from_grant`). Unchanged but for
//! what the two hosts do not share, which the caller now passes:
//!
//! - who is a person: the site's account ids above zero (the heartbeat is
//!   0), the Mac's subjects other than `system` and the empty one. The host
//!   says it in [`Actor::personal`].
//! - what an empty autonomy request covers: the host's own candidate list
//!   (`max_user_agent_permissions` on the site).
//!
//! Lists keep the order of the list being narrowed (a grant's allowed names,
//! the granted names, the required names) and its duplicates, as the
//! backend's do; read as sets they are the Mac's. Answers are values, not
//! sentences: each host words its own messages.

use serde::{Deserialize, Serialize};

/// Who is acting: the host's id for them, and whether the host counts them
/// as a person (never the heartbeat or system identity).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Actor<A> {
    pub id: A,
    pub personal: bool,
}

/// A durable personal autonomy grant. `allowed` is a claimed subset only:
/// every use intersects it with what is granted now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomyGrant<A> {
    pub actor: A,
    pub allowed: Vec<String>,
    pub revoked: bool,
}

/// Who accepted a proposal: the user's click, or her own grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    User,
    Autonomy,
}

/// The three permission layers as one host holds them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Layers {
    pub declared: Vec<String>,
    pub approved: Vec<String>,
    pub runtime_allowed: Vec<String>,
}

/// What is granted now: approved, allowed at run time, and declared; in the
/// approved order.
///
/// Always all three, every time: an approval of something not declared is
/// dropped, never granted. This is the Mac's `PermissionState.granted`
/// (`Domain.swift`), as the owner decided on 2026-10-27 (G3 in the PR that
/// brought this in); the site used to select approvals out of the
/// declaration when installing or updating and not intersect the
/// declaration again at run time.
pub fn granted(layers: &Layers) -> Vec<String> {
    layers
        .approved
        .iter()
        .filter(|name| layers.declared.contains(name) && layers.runtime_allowed.contains(name))
        .cloned()
        .collect()
}

/// Outcome of a personal autonomy check. Allowing personal Work never means
/// she executes on her own: Work still goes through its ordinary path and
/// its confirmations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum Verdict {
    /// Missing, revoked, not a person, or nothing left once re-filtered:
    /// the proposal waits for the user.
    RequireUserReview,
    /// The grant's names that are granted now; never names only declared
    /// or approved.
    AllowPersonalWork { granted: Vec<String> },
}

fn holds<A: PartialEq>(actor: &Actor<A>, grant: &AutonomyGrant<A>) -> bool {
    actor.personal && !grant.revoked && grant.actor == actor.id
}

/// Check her own Work request against the actor's grant, re-filtered by
/// what is granted now (`granted`).
pub fn evaluate<A: PartialEq>(
    actor: &Actor<A>,
    grant: Option<&AutonomyGrant<A>>,
    granted: &[String],
) -> Verdict {
    let Some(grant) = grant.filter(|grant| holds(actor, grant)) else {
        return Verdict::RequireUserReview;
    };
    let now: Vec<String> = grant
        .allowed
        .iter()
        .filter(|name| granted.contains(name))
        .cloned()
        .collect();
    if now.is_empty() {
        return Verdict::RequireUserReview;
    }
    Verdict::AllowPersonalWork { granted: now }
}

/// Whether an accepted proposal may enter Work: never as someone who is not
/// a person; on the user's click always; on her own only with a live grant.
pub fn may_enter_work<A: PartialEq>(
    actor: &Actor<A>,
    grant: Option<&AutonomyGrant<A>>,
    granted: &[String],
    source: Source,
) -> bool {
    if !actor.personal {
        return false;
    }
    match source {
        Source::User => true,
        Source::Autonomy => matches!(
            evaluate(actor, grant, granted),
            Verdict::AllowPersonalWork { .. }
        ),
    }
}

/// What is granted now, within an autonomy cap; no cap is no narrowing. A
/// cap never widens.
pub fn effective(granted: &[String], cap: Option<&[String]>) -> Vec<String> {
    let Some(cap) = cap else {
        return granted.to_vec();
    };
    granted
        .iter()
        .filter(|name| cap.contains(name))
        .cloned()
        .collect()
}

/// Whether capped Work may go on: the grant still allows personal Work once
/// re-filtered, and the cap still meets what is granted. Uncapped Work (the
/// user accepted it) is not an autonomy question.
pub fn cap_still_allows<A: PartialEq>(
    actor: &Actor<A>,
    grant: Option<&AutonomyGrant<A>>,
    granted: &[String],
    cap: Option<&[String]>,
) -> bool {
    if cap.is_none() {
        return true;
    }
    matches!(
        evaluate(actor, grant, granted),
        Verdict::AllowPersonalWork { .. }
    ) && !effective(granted, cap).is_empty()
}

/// The check before a capability executes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Check {
    Allowed,
    /// Capped Work whose autonomy grant no longer allows it (revoked, or
    /// nothing left granted within the cap). Fails closed.
    AutonomyWithdrawn,
    /// These required names are not granted (within the cap), in the
    /// required order. The site names the first.
    Missing {
        missing: Vec<String>,
    },
}

/// Execute-time check. Capped Work re-reads the live grant and what is
/// granted now; a revocation or an empty intersection fails closed.
/// Uncapped Work (the user accepted it) only checks what is granted now
/// against what the capability requires.
pub fn check<A: PartialEq>(
    actor: &Actor<A>,
    grant: Option<&AutonomyGrant<A>>,
    granted: &[String],
    cap: Option<&[String]>,
    required: &[String],
) -> Check {
    if cap.is_some() && !cap_still_allows(actor, grant, granted, cap) {
        return Check::AutonomyWithdrawn;
    }
    let effective = effective(granted, cap);
    let missing: Vec<String> = required
        .iter()
        .filter(|name| !effective.contains(name))
        .cloned()
        .collect();
    if missing.is_empty() {
        Check::Allowed
    } else {
        Check::Missing { missing }
    }
}

/// The cap an autonomy-accepted proposal may be claimed with, or `None` to
/// leave it for the user: user-accepted, not a person, no live grant, or
/// nothing granted now.
pub fn claim<A: PartialEq>(
    actor: &Actor<A>,
    source: Source,
    grant: Option<&AutonomyGrant<A>>,
    granted: &[String],
) -> Option<Vec<String>> {
    if source != Source::Autonomy || !actor.personal {
        return None;
    }
    match evaluate(actor, grant, granted) {
        Verdict::AllowPersonalWork { granted } if !granted.is_empty() => Some(granted),
        _ => None,
    }
}

/// Why a grant could not be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteRefusal {
    /// The heartbeat or system identity cannot hold one.
    HeartbeatIdentity,
    /// Nothing requested is granted now.
    EmptyAfterFilter,
}

/// A grant for the actor, narrowed to what is granted now. An empty request
/// is the switch: what is granted now among the host's `candidates` (names
/// only an administrator holds stay out unless listed explicitly).
pub fn prepare<A: Clone>(
    actor: &Actor<A>,
    requested: &[String],
    granted: &[String],
    candidates: &[String],
) -> Result<AutonomyGrant<A>, WriteRefusal> {
    if !actor.personal {
        return Err(WriteRefusal::HeartbeatIdentity);
    }
    let narrowed: Vec<String>;
    let requested = if requested.is_empty() {
        narrowed = granted
            .iter()
            .filter(|name| candidates.contains(name))
            .cloned()
            .collect();
        &narrowed
    } else {
        requested
    };
    let allowed: Vec<String> = requested
        .iter()
        .filter(|name| granted.contains(name))
        .cloned()
        .collect();
    if allowed.is_empty() {
        return Err(WriteRefusal::EmptyAfterFilter);
    }
    Ok(AutonomyGrant {
        actor: actor.id.clone(),
        allowed,
        revoked: false,
    })
}

/// The grant, revoked: its allowed names are kept, it no longer counts.
/// `personal` is whether its actor is a person.
pub fn revoke<A: Clone>(
    grant: &AutonomyGrant<A>,
    personal: bool,
) -> Result<AutonomyGrant<A>, WriteRefusal> {
    if !personal {
        return Err(WriteRefusal::HeartbeatIdentity);
    }
    Ok(AutonomyGrant {
        revoked: true,
        ..grant.clone()
    })
}

#[cfg(test)]
mod tests;
