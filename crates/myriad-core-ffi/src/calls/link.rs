//! Her link's frames (`myriad_her_link`): whether a frame may be sent or
//! taken under protocol 2, and its one byte form. The rules read the
//! contract the core was built with.

use myriad_her_link::check;
use serde::Deserialize;
use serde_json::{Value, json};

use super::input;
use crate::Failure;

/// `{frame, roles?}` → `{accepted: {t, family, delivery, canonical}}` or
/// `{refused: {code, detail}}`. A refusal is an answer, not bad input: the
/// frame came from the other end.
pub(super) fn check_frame(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        frame: Value,
        #[serde(default)]
        roles: Vec<String>,
    }
    let In { frame, roles } = input(raw)?;
    Ok(match check(&frame, &roles) {
        Ok(checked) => json!({ "accepted": checked }),
        Err(refusal) => json!({ "refused": refusal }),
    })
}

/// `{frame}` → `{text}`: the frame this end is about to send, checked, in
/// its one byte form. A frame that does not pass is bad input.
pub(super) fn encode(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        frame: Value,
    }
    let In { frame } = input(raw)?;
    let checked = check(&frame, &[])
        .map_err(|refusal| Failure::BadInput(format!("{}: {}", refusal.code, refusal.detail)))?;
    Ok(json!({ "text": checked.canonical }))
}

/// `{t, body, sessions2?, choices?}` → `{session}` (`now.session`), `{drop}`
/// (`now.drop`) or `{refused: {code, detail}}`: a `now` family body read by
/// the contract's `sessions` rules. `sessions2` says the welcome listed
/// `now.sessions2` (false when left out): without it a body with
/// `ownerSaidAt`, `errors` or `overwrote` is refused. `choices` says it
/// listed `now.choices`: without it a body with `choices` is refused.
/// Another `t` is bad input.
pub(super) fn now_read(raw: &[u8]) -> Result<Value, Failure> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct In {
        t: String,
        body: Value,
        #[serde(default)]
        sessions2: bool,
        #[serde(default)]
        choices: bool,
    }
    let In {
        t,
        body,
        sessions2,
        choices,
    } = input(raw)?;
    let listed = myriad_her_link::now::Listed { sessions2, choices };
    let answer = match t.as_str() {
        "now.session" => myriad_her_link::now::read_listed(&body, listed)
            .map(|session| json!({ "session": session })),
        "now.drop" => myriad_her_link::now::read_drop(&body).map(|drop| json!({ "drop": drop })),
        _ => return Err(Failure::BadInput("t is not now.session or now.drop".into())),
    };
    Ok(answer.unwrap_or_else(|refusal| json!({ "refused": refusal })))
}
