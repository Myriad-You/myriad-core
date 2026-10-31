//! The partner's one id on every side (`myriad_persona_log::partner`):
//! minting it on the site, taking it over the link, taking a file, and
//! moving a side's rows, node bodies and ledger to it
//! (`myriad_nodes::migrate`). The rules are the crates'; these calls only
//! carry them across. An id is not a credential; a refusal still names a
//! field, never a value.

use base64::Engine as _;
use myriad_persona_log::partner::{self, Here};
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

/// `{at, random}` → `{id}`: a new id minted at `at` (Unix milliseconds)
/// from ten random bytes (base64). The site mints one per account, once.
pub(super) fn mint(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        at: i64,
        random: String,
    }
    let In { at, random } = quiet_input(raw)?;
    let random = base64::engine::general_purpose::STANDARD
        .decode(random)
        .map_err(|_| refused("random is base64"))?;
    Ok(json!({ "id": partner::mint(at, &random).map_err(refused)? }))
}

/// `{here: {id?, linked?, audiences?, filed?, moving?}, theirs}` → `{id, from?,
/// confirm?}`: the site's id as the link brings it, the id to move from
/// first, and whether to ask the partner before moving.
pub(super) fn link(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        here: Here,
        theirs: String,
    }
    let In { here, theirs } = quiet_input(raw)?;
    answer(&partner::link(&here, &theirs).map_err(refused)?)
}

/// `{here: {id?, linked?, audiences?, filed?, moving?}, header?}` → `{id, adopt?,
/// filed?}`: whether a file whose header names `header` (none: a file from
/// before) comes in, and under which id.
pub(super) fn file(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        here: Here,
        #[serde(default)]
        header: Option<String>,
    }
    let In { here, header } = quiet_input(raw)?;
    answer(&partner::file(&here, header.as_deref()).map_err(refused)?)
}

/// `{header?, audience}` → `{}`: whether a page of `audience` may be in a
/// file whose header names the partner id `header` (none: no partner).
pub(super) fn page(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        #[serde(default)]
        header: Option<String>,
        audience: String,
    }
    let In { header, audience } = quiet_input(raw)?;
    partner::page(header.as_deref(), &audience).map_err(refused)?;
    Ok(json!({}))
}

/// `{rows, bodies?, ledger, from, to}` → `{rows, bodies?, ledger, moved?,
/// clashes?, unread?}`: `from` is `local-owner`, nothing else.
pub(super) fn migrate(raw: &[u8]) -> Result<Value, Failure> {
    answer(&myriad_nodes::migrate::migrate(quiet_input(raw)?).map_err(refused)?)
}
