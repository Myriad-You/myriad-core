//! Her nodes, folded from her log by the same rules on every host core.
//!
//! A node is what she keeps about one thing the owner does: a repository
//! another coding agent works in, the owner's talks with other AIs, and
//! (from her upkeep, later) topics and each day's loose ends. It holds her
//! own words (the line she had for the owner, what she noted) and
//! references to where things came from; never an original.
//!
//! Every change to a node is a row of her log (`myriad-persona-log`), about
//! `node:<id>`. A row holds no content: its payload is a body kept beside
//! it, whose fields, version and one byte form are here ([`body`]). Which
//! node a thing files under and which rows a decision adds ([`file`]), and
//! what a node is once its rows are folded ([`fold`]), are here too, so the
//! server and a computer that held the pen at different times fold the same
//! rows into the same node.
//!
//! This is the first batch of node rules. Her upkeep (consolidating,
//! filing loose ends, growing, splitting, merging, resting), wakes, views
//! and objections come later: their rows are taken as they are and not read
//! ([`body::Body::Later`]).
//!
//! Pure: nothing here reads a clock, a file, the network, a key or a random
//! source. Row ids come from the caller; times are the rows' clocks.

use std::fmt;

pub mod body;
pub mod file;
pub mod fold;
pub mod id;

pub use body::{BODY_VERSION, Body, Form};
pub use file::{
    Decision, Draft, FileInput, Filed, PlaceInput, Placed, SaidInput, file, locate, said,
};
pub use fold::{FoldInput, Folded, Moment, Node, Ref, fold};
pub use id::{Place, node_id, place};

/// Why an input was refused. It names a field or a rule, never a value: a
/// node's input holds her words and the owner's things.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeError(String);

impl NodeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for NodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NodeError {}

/// A token (an id, a key, an event): 1 to 512 bytes, no control character.
pub(crate) fn check_token(what: &str, value: &str) -> Result<(), NodeError> {
    if value.is_empty() || value.len() > myriad_persona_log::row::MAX_TOKEN_BYTES {
        return Err(NodeError::new(format!(
            "{what} must be 1 to {} bytes",
            myriad_persona_log::row::MAX_TOKEN_BYTES
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(NodeError::new(format!("{what} holds a control character")));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
