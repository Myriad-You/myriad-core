//! The partner's one id on every side (`myriad_persona_log::partner`):
//! minting it, two ids meeting, and moving a side's rows and node bodies
//! to it (`myriad_nodes::migrate`). The rules are the crates'; these calls
//! only carry them across. An id is not a credential; a refusal still names
//! a field, never a value.

use base64::Engine as _;
use myriad_persona_log::{Partner, partner};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::quiet_input;
use crate::Failure;

fn refused(error: impl std::fmt::Display) -> Failure {
    Failure::BadInput(error.to_string())
}

fn answer(value: &impl Serialize) -> Result<Value, Failure> {
    serde_json::to_value(value).map_err(refused)
}

/// `{at, random, was?}` → `{partner}`: a new id minted at `at` (Unix
/// milliseconds) from ten random bytes (base64); `was`, the id this side
/// used until now, becomes its alias.
pub(super) fn mint(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        at: i64,
        random: String,
        #[serde(default)]
        was: Option<String>,
    }
    let In { at, random, was } = quiet_input(raw)?;
    let random = base64::engine::general_purpose::STANDARD
        .decode(random)
        .map_err(|_| refused("random is base64"))?;
    let partner = partner::minted(at, &random, was.as_deref()).map_err(refused)?;
    Ok(json!({ "partner": partner }))
}

/// `{mine, theirs}` → `{partner, changed?}`.
pub(super) fn meet(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        mine: Partner,
        theirs: Partner,
    }
    let In { mine, theirs } = quiet_input(raw)?;
    answer(&partner::meet(&mine, &theirs).map_err(refused)?)
}

/// `{rows, bodies?, partner}` → `{rows, bodies?, moved?, clashes?}`.
pub(super) fn migrate(raw: &[u8]) -> Result<Value, Failure> {
    answer(&myriad_nodes::migrate::migrate(quiet_input(raw)?).map_err(refused)?)
}
