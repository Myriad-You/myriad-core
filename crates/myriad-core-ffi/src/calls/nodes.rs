//! Her nodes (`myriad_nodes`): a node row's body and its one byte form,
//! which node a thing files under and what a decision writes, and the fold.
//! The rules are the crate's; these calls only carry them across. Inputs
//! hold her words, so a refusal names a field, never a value.

use myriad_nodes::{Body, NodeError, file, fold as fold_rows, id, locate, said as said_rows};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::quiet_input;
use crate::Failure;

fn refused(error: NodeError) -> Failure {
    Failure::BadInput(error.to_string())
}

fn answer(value: &impl Serialize) -> Result<Value, Failure> {
    serde_json::to_value(value).map_err(|_| Failure::BadInput("answer does not serialize".into()))
}

/// `{kind, body}` → `{body, canonical, digest}`: one row's body, checked.
pub(super) fn body(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        kind: String,
        body: Value,
    }
    let In { kind, body } = quiet_input(raw)?;
    let body = Body::read(&kind, &body).map_err(refused)?;
    let canonical = String::from_utf8(body.canonical_bytes())
        .map_err(|_| Failure::BadInput("canonical bytes are UTF-8".into()))?;
    Ok(json!({ "body": body.to_value(), "canonical": canonical, "digest": body.digest() }))
}

/// `{owner, audience?, key}` → `{id}`.
pub(super) fn node_id(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        owner: String,
        #[serde(default)]
        audience: Option<String>,
        key: String,
    }
    let In {
        owner,
        audience,
        key,
    } = quiet_input(raw)?;
    let audience = audience.unwrap_or_else(|| id::private(&owner));
    Ok(json!({ "id": myriad_nodes::node_id(&owner, &audience, &key).map_err(refused)? }))
}

/// `{owner, audience?, kind, safeFacts?}` → `{place?}`.
pub(super) fn place(raw: &[u8]) -> Result<Value, Failure> {
    Ok(match locate(quiet_input(raw)?).map_err(refused)? {
        Some(place) => json!({ "place": place }),
        None => json!({}),
    })
}

pub(super) fn file_decision(raw: &[u8]) -> Result<Value, Failure> {
    answer(&file(quiet_input(raw)?).map_err(refused)?)
}

/// `{owner, node, event, how, line?, id}` → `{rows}`.
pub(super) fn said(raw: &[u8]) -> Result<Value, Failure> {
    Ok(json!({ "rows": said_rows(quiet_input(raw)?).map_err(refused)? }))
}

pub(super) fn fold(raw: &[u8]) -> Result<Value, Failure> {
    answer(&fold_rows(quiet_input(raw)?).map_err(refused)?)
}
