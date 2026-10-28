//! What the payload of a row says, for the kinds every host core writes of
//! her talk, her memories and her mood: one shape, so a row one side wrote
//! reads the same on every other.
//!
//! A row never holds content ([`crate::Row`]); its payload is kept beside
//! it, sealed, and read back by its reference. These are the payloads'
//! shapes, as canonical JSON. A side that writes one of these kinds writes
//! this shape; a side that reads one takes fields it does not know and
//! ignores them, so a newer side may add a field without breaking an older
//! one. Times are milliseconds since the Unix epoch.
//!
//! The node kinds (`node.*`) and the judged cases (`case.*`) are not here:
//! the node kinds come with the nodes' own rules.
//!
//! | kind | object | payload |
//! |---|---|---|
//! | `segment.open` | `segment:<id>` | [`SegmentOpen`] |
//! | `segment.line` | `segment:<id>` | [`SegmentLine`] |
//! | `segment.close` | `segment:<id>` | [`SegmentClose`] |
//! | `memory.write` | `memory:<id>` | [`MemoryWrite`] |
//! | `memory.retire` | `memory:<id>` | [`MemoryRetire`] |
//! | `memory.restore` | `memory:<id>` | [`MemoryRestore`] |
//! | `affect.change` | `affect:<subject>` | [`AffectChange`] |
//!
//! Who said a line, or wrote a memory, is the row's `author` (the owner or
//! her); who a memory or a mood is about is its `subject`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::LogError;

/// A stretch of talk begins: where it happens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentOpen {
    pub at: i64,
    /// Where the talk is: `web` (the site's page), `app` (one of the
    /// owner's devices), `group` (a group chat), or another surface a side
    /// names.
    pub place: String,
}

/// One line said in a segment, by the row's author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentLine {
    pub at: i64,
    pub text: String,
    /// The writing side's own id for the line (a message id), when it has
    /// one: the same line is not shown twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// The line it answers, by that `line`, when it answers one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

/// A stretch of talk is over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentClose {
    pub at: i64,
}

/// A memory written: what it says and what kind of thing it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWrite {
    pub at: i64,
    pub text: String,
    /// `fact`, `preference`, `lesson`, `pattern`, `narrative`,
    /// `knowledge`, `done`, or another a side names.
    pub kind: String,
    /// The memories (`memory:<id>`) this one takes the place of; each also
    /// gets its own `memory.retire`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<String>,
}

/// A memory retired: why, and the one that holds it now when another does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRetire {
    pub at: i64,
    /// `superseded`, `deleted` (the owner took it back), `faded`, or
    /// another a side names.
    pub why: String,
    /// The memory (`memory:<id>`) that holds what it held.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
}

/// A retired memory back in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRestore {
    pub at: i64,
}

/// Her mood toward the row's subject changed: where it is now, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectChange {
    pub at: i64,
    /// How she feels, 0 to 100.
    pub valence: f64,
    /// How stirred she is, 0 to 100.
    pub arousal: f64,
    /// The band `valence` falls in, by the mood rules.
    pub band: String,
    /// What moved it: a category, never what was said.
    pub cause: String,
}

fn read<T: for<'de> Deserialize<'de>>(kind: &str, value: &Value) -> Result<T, LogError> {
    serde_json::from_value(value.clone())
        .map_err(|_| LogError::new(format!("a {kind} payload is not of its shape")))
}

fn check_text(kind: &str, text: &str) -> Result<(), LogError> {
    if text.trim().is_empty() {
        return Err(LogError::new(format!("a {kind} payload has no text")));
    }
    Ok(())
}

fn check_memory(kind: &str, object: &str) -> Result<(), LogError> {
    if crate::row::object_type(object) != Some("memory") {
        return Err(LogError::new(format!(
            "a {kind} payload names something that is not a memory"
        )));
    }
    Ok(())
}

fn check_scale(value: f64) -> Result<(), LogError> {
    if !(0.0..=100.0).contains(&value) {
        return Err(LogError::new("an affect.change payload is off its scale"));
    }
    Ok(())
}

/// Whether `payload` is of the shape rows of `kind` carry. `Ok(false)` for
/// a kind whose payload is not one of these (`node.*`, `case.*`, ...).
pub fn check(kind: &str, payload: &Value) -> Result<bool, LogError> {
    match kind {
        "segment.open" => {
            let open: SegmentOpen = read(kind, payload)?;
            if open.place.is_empty() {
                return Err(LogError::new("a segment.open payload has no place"));
            }
        }
        "segment.line" => check_text(kind, &read::<SegmentLine>(kind, payload)?.text)?,
        "segment.close" => {
            read::<SegmentClose>(kind, payload)?;
        }
        "memory.write" => {
            let memory: MemoryWrite = read(kind, payload)?;
            check_text(kind, &memory.text)?;
            for replaced in &memory.replaces {
                check_memory(kind, replaced)?;
            }
        }
        "memory.retire" => {
            let retire: MemoryRetire = read(kind, payload)?;
            if retire.why.is_empty() {
                return Err(LogError::new("a memory.retire payload has no why"));
            }
            if let Some(by) = &retire.by {
                check_memory(kind, by)?;
            }
        }
        "memory.restore" => {
            read::<MemoryRestore>(kind, payload)?;
        }
        "affect.change" => {
            let change: AffectChange = read(kind, payload)?;
            check_scale(change.valence)?;
            check_scale(change.arousal)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn each_kind_reads_its_shape_and_ignores_fields_it_does_not_know() {
        let cases = [
            ("segment.open", json!({"at": 1, "place": "web"})),
            (
                "segment.line",
                json!({"at": 2, "text": "早", "line": "m_1", "later": true}),
            ),
            ("segment.close", json!({"at": 3})),
            (
                "memory.write",
                json!({"at": 4, "text": "喜欢热可可", "kind": "preference", "replaces": ["memory:m-1"]}),
            ),
            (
                "memory.retire",
                json!({"at": 5, "why": "superseded", "by": "memory:m-2"}),
            ),
            ("memory.restore", json!({"at": 6})),
            (
                "affect.change",
                json!({"at": 7, "valence": 62.5, "arousal": 40.0, "band": "warm", "cause": "chat"}),
            ),
        ];
        for (kind, payload) in cases {
            assert_eq!(check(kind, &payload), Ok(true), "{kind}");
        }
        assert_eq!(check("node.noted", &json!({"anything": 1})), Ok(false));
    }

    #[test]
    fn a_payload_off_its_shape_is_refused() {
        for (kind, payload) in [
            ("segment.open", json!({"at": 1, "place": ""})),
            ("segment.line", json!({"at": 2, "text": "  "})),
            ("segment.line", json!({"text": "no time"})),
            (
                "memory.write",
                json!({"at": 4, "text": "x", "kind": "fact", "replaces": ["node:a"]}),
            ),
            ("memory.retire", json!({"at": 5, "why": ""})),
            (
                "affect.change",
                json!({"at": 7, "valence": 120.0, "arousal": 40.0, "band": "x", "cause": "y"}),
            ),
        ] {
            assert!(check(kind, &payload).is_err(), "{kind} {payload}");
        }
    }
}
