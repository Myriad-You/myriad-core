//! Whether a frame is taken, and the text it goes out as: the core's
//! `myriad_her_link::check`.

use serde_json::{Map, Value, json};

use crate::contract::contract;

/// Why a frame of this end's does not go out: the core's refusal code, never
/// its body or the core's `detail`, which may quote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsent {
    pub code: String,
}

/// Why the core does not take the frame `text` from the other end: its refusal
/// code, never its `detail`, which may quote the frame. None when it is taken.
pub(crate) fn refused(text: &str) -> Option<String> {
    myriad_her_link::check_bytes(text.as_bytes(), &[])
        .err()
        .map(|refusal| refusal.code)
}

/// Whether the frame `text` from the other end is taken.
#[cfg(test)]
pub(crate) fn takes(text: &str) -> bool {
    refused(text).is_none()
}

/// The text a frame goes out as, or why the core will not send it.
pub(crate) fn text(
    t: &str,
    seq: u64,
    ack: u64,
    id: Option<&str>,
    at: i64,
    body: &Value,
) -> Result<String, Unsent> {
    let mut frame = Map::new();
    frame.insert("v".into(), json!(contract().encoding.frame_version));
    frame.insert("t".into(), json!(t));
    frame.insert("seq".into(), json!(seq));
    frame.insert("ack".into(), json!(ack));
    frame.insert("at".into(), json!(at));
    frame.insert("body".into(), body.clone());
    if let Some(id) = id {
        frame.insert("id".into(), json!(id));
    }
    myriad_her_link::check(&Value::Object(frame), &[])
        .map(|checked| checked.canonical)
        .map_err(|refusal| Unsent { code: refusal.code })
}
