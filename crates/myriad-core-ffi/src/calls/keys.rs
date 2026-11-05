//! What a side keeps sealed at rest (`myriad_persona_log::keys`) and a
//! computer's keys for its own database files
//! (`myriad_persona_log::store_keys`). Keys, nonces and sealed bytes cross
//! as base64. The host draws every key and nonce; none of them, and nothing
//! sealed or opened, ever appears in a refusal (inputs are read with
//! [`quiet_input`]).

use base64::Engine as _;
use myriad_persona_log::{
    keys::{self, Bound, KEY_BYTES, NONCE_BYTES},
    store_keys::{self, Event, Found, StoreKeys},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::quiet_input;
use crate::Failure;

fn refused(error: impl std::fmt::Display) -> Failure {
    Failure::BadInput(error.to_string())
}

fn bytes(raw: &str, what: &str) -> Result<Vec<u8>, Failure> {
    base64::engine::general_purpose::STANDARD
        .decode(raw.as_bytes())
        .map_err(|_| refused(format!("{what} is not base64")))
}

fn base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn fixed<const N: usize>(raw: &str, what: &str) -> Result<[u8; N], Failure> {
    bytes(raw, what)?
        .try_into()
        .map_err(|_| refused(format!("{what} is {N} bytes")))
}

/// `{key, nonce, bound, data}` (32 and 12 bytes; `bound` is
/// `{kind: object_key, holder, audience, object}`, `{kind: payload, holder,
/// row}` or `{kind: store_key, file, generation}`) → `{sealed}`: the nonce,
/// then ciphertext and tag.
pub(super) fn seal(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        key: String,
        nonce: String,
        bound: Bound,
        data: String,
    }
    let req: In = quiet_input(raw)?;
    let key: [u8; KEY_BYTES] = fixed(&req.key, "the key")?;
    let nonce: [u8; NONCE_BYTES] = fixed(&req.nonce, "the nonce")?;
    let data = bytes(&req.data, "the data")?;
    let sealed = keys::seal(&key, &nonce, &req.bound, &data).map_err(refused)?;
    Ok(json!({ "sealed": base64(&sealed) }))
}

/// `{key, sealed, bound}` → `{data}`; refused when it does not open with
/// this key for this place.
pub(super) fn open(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        key: String,
        sealed: String,
        bound: Bound,
    }
    let req: In = quiet_input(raw)?;
    let key: [u8; KEY_BYTES] = fixed(&req.key, "the key")?;
    let sealed = bytes(&req.sealed, "the sealed value")?;
    let data = keys::open(&key, &sealed, &req.bound).map_err(refused)?;
    Ok(json!({ "data": base64(&data) }))
}

/// `{audience, target}` (a ledger entry) → `{audience, object}` or
/// `{audience, subject}`: the object keys it deletes.
pub(super) fn forget_reach(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        audience: String,
        target: String,
    }
    let req: In = quiet_input(raw)?;
    let reach = keys::forget_reach(&req.audience, &req.target).map_err(refused)?;
    serde_json::to_value(reach).map_err(refused)
}

/// `{keys?, file}` → `{file}`: that file's key as the store keys hold it,
/// null when they hold none; refused when the store keys do not check.
pub(super) fn store_read(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        keys: Option<StoreKeys>,
        file: String,
    }
    let req: In = quiet_input(raw)?;
    let keys = req.keys.unwrap_or_default();
    keys.check().map_err(refused)?;
    Ok(json!({ "file": keys.files.get(&req.file) }))
}

/// `{keys?, file, event}` → `{keys}`: the store keys after one step, to be
/// written whole.
pub(super) fn store_step(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        keys: Option<StoreKeys>,
        file: String,
        event: Event,
    }
    let req: In = quiet_input(raw)?;
    let keys = store_keys::step(req.keys, &req.file, req.event).map_err(refused)?;
    Ok(json!({ "keys": keys }))
}

/// `{keys?, file, found: {main, encrypting, aside}}` → `{action,
/// generation?}`: what to do next with the file.
pub(super) fn store_settle(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        keys: Option<StoreKeys>,
        file: String,
        found: Found,
    }
    let req: In = quiet_input(raw)?;
    let next = store_keys::settle(req.keys.as_ref(), &req.file, req.found).map_err(refused)?;
    serde_json::to_value(next).map_err(refused)
}
