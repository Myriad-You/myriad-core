//! What she may use (`myriad_agent_rules::grants`): where the three
//! permission layers meet, the autonomy cap, the execute-time check, and her
//! own Work's personal autonomy grant. Only the runtime grant authorizes.
//!
//! `actor` is `{id, personal}`: the host's id for whoever acts, and whether
//! the host counts them as a person (never the heartbeat or system identity).
//! A grant is `{actor, allowed, revoked}`; `grant` absent or `null` is none.
//! `cap` absent or `null` is no autonomy cap. Lists keep the order of the
//! list being narrowed.

use myriad_agent_rules::grants::{self, Actor, AutonomyGrant, Layers, Source};
use serde::Deserialize;
use serde_json::{Value, json};

use super::input;
use crate::Failure;

type Grant = AutonomyGrant<String>;

/// `{declared, approved, runtimeAllowed}` → `{granted}`: approved, allowed
/// at run time and declared, in the approved order. An approval of
/// something not declared is bad input: the hosts do not agree on it.
pub(super) fn granted(raw: &[u8]) -> Result<Value, Failure> {
    let layers: Layers = input(raw)?;
    let granted = grants::granted(&layers).map_err(|error| Failure::BadInput(error.to_string()))?;
    Ok(json!({ "granted": granted }))
}

/// `{granted, cap?}` → `{effective}`.
pub(super) fn effective(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        granted: Vec<String>,
        #[serde(default)]
        cap: Option<Vec<String>>,
    }
    let req: In = input(raw)?;
    Ok(json!({ "effective": grants::effective(&req.granted, req.cap.as_deref()) }))
}

/// `{actor, grant?, granted, cap?, required}` → `{outcome: "allowed"}`,
/// `{outcome: "autonomy_withdrawn"}` or `{outcome: "missing", missing}`.
pub(super) fn check(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        actor: Actor<String>,
        #[serde(default)]
        grant: Option<Grant>,
        granted: Vec<String>,
        #[serde(default)]
        cap: Option<Vec<String>>,
        required: Vec<String>,
    }
    let req: In = input(raw)?;
    let check = grants::check(
        &req.actor,
        req.grant.as_ref(),
        &req.granted,
        req.cap.as_deref(),
        &req.required,
    );
    Ok(json!(check))
}

/// `{actor, grant?, granted}` → `{verdict: "require_user_review"}` or
/// `{verdict: "allow_personal_work", granted}`.
pub(super) fn autonomy(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        actor: Actor<String>,
        #[serde(default)]
        grant: Option<Grant>,
        granted: Vec<String>,
    }
    let req: In = input(raw)?;
    Ok(json!(grants::evaluate(
        &req.actor,
        req.grant.as_ref(),
        &req.granted
    )))
}

/// `{actor, grant?, granted, source}` → `{mayEnter}`; `source` is `user`
/// or `autonomy`.
pub(super) fn may_enter_work(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        actor: Actor<String>,
        #[serde(default)]
        grant: Option<Grant>,
        granted: Vec<String>,
        source: Source,
    }
    let req: In = input(raw)?;
    let may = grants::may_enter_work(&req.actor, req.grant.as_ref(), &req.granted, req.source);
    Ok(json!({ "mayEnter": may }))
}

/// `{actor, grant?, granted, cap?}` → `{allows}`.
pub(super) fn cap_still_allows(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        actor: Actor<String>,
        #[serde(default)]
        grant: Option<Grant>,
        granted: Vec<String>,
        #[serde(default)]
        cap: Option<Vec<String>>,
    }
    let req: In = input(raw)?;
    let allows = grants::cap_still_allows(
        &req.actor,
        req.grant.as_ref(),
        &req.granted,
        req.cap.as_deref(),
    );
    Ok(json!({ "allows": allows }))
}

/// `{actor, source, grant?, granted}` → `{cap}`: the cap to claim an
/// accepted proposal with, or `null` to leave it for the user.
pub(super) fn claim(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        actor: Actor<String>,
        source: Source,
        #[serde(default)]
        grant: Option<Grant>,
        granted: Vec<String>,
    }
    let req: In = input(raw)?;
    let cap = grants::claim(&req.actor, req.source, req.grant.as_ref(), &req.granted);
    Ok(json!({ "cap": cap }))
}

/// `{actor, requested, granted, candidates}` → `{grant}` or `{refused}`
/// (`heartbeat_identity`, `empty_after_filter`). A refusal is an answer.
pub(super) fn prepare(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        actor: Actor<String>,
        requested: Vec<String>,
        granted: Vec<String>,
        candidates: Vec<String>,
    }
    let req: In = input(raw)?;
    Ok(written(grants::prepare(
        &req.actor,
        &req.requested,
        &req.granted,
        &req.candidates,
    )))
}

/// `{grant, personal}` → `{grant}` (revoked) or `{refused}`; `personal` is
/// whether the grant's actor is a person.
pub(super) fn revoke(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        grant: Grant,
        personal: bool,
    }
    let req: In = input(raw)?;
    Ok(written(grants::revoke(&req.grant, req.personal)))
}

fn written(result: Result<Grant, grants::WriteRefusal>) -> Value {
    match result {
        Ok(grant) => json!({ "grant": grant }),
        Err(refusal) => json!({ "refused": refusal }),
    }
}
