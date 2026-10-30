//! Her nodes, folded from her log by the same rules on every host core.
//!
//! A node is one thing she keeps in mind, of one of three kinds
//! ([`key::Kind`]): something in progress (it has a "done"), a topic (no
//! "done", it keeps coming back; it may be about an outside object such as
//! a work, a repository or a place), or a person (who speaks and has their
//! own privacy: the partner, her, someone else, a group).
//!
//! A node has a head and a body. The head is what she makes of it: its
//! name, a summary whose every sentence names its evidence, her views
//! (each with evidence and history), what is still open, what she means to
//! say, who knows, and its status. The body is what it rests on: spans of
//! experience, entries (facts and preferences), and references to the
//! originals. Every span and entry carries three tags ([`tags`]): whether it
//! may sync or stays on one device, who said it, and its audience.
//!
//! Every change is a row of her log (`myriad-persona-log`) about
//! `node:<id>`, whose payload is a body kept beside it ([`body`]). A node's
//! id comes from its audience and a stable key that is the same on every
//! device ([`key`]); its kind is a field of the node and can change. A
//! revision names the version it was written against, so when two sides
//! revise the same thing from the same version the fold keeps both and
//! marks it, instead of letting the later one win ([`fold`]). Nodes relate
//! in five ways, inside one audience, `part_of` never in a cycle.
//!
//! Placing a talk ([`place`]) cuts it into spans, one thing each, and puts
//! each span on a node or in the inbox. Only the pure half is here: what
//! goes to the model, cut to limits both hosts share ([`limits`]), and
//! reading what comes back. The prompt comes later.
//!
//! Her upkeep works a queue ([`queue`]): a node is due only when something
//! it watches changed since she last looked (its parts, its whole, what
//! blocks it, a clash or a mutual block that raises a hand), oldest change
//! first. For each node she judges the two judgments, in her words; the
//! pure half of that ([`upkeep`]) shows her the node and what is one
//! relation away, and turns her answer into rows: a new node, a part split
//! off, the inbox filed, a merge or folding back, a relation made or taken
//! back, a kind that follows from what "done" is, a status, a hand raised,
//! a summary, a view or an open item rewritten (a view only on what
//! counts). Other nodes of the audience the caller offers sit beside it, so
//! she can merge two that are one thing or file the inbox where it belongs;
//! with no node due she files the inbox alone. What the partner set stands;
//! against it she can only raise a hand. The
//! fold carries what follows: spans that move with a split or a merge, the
//! lineage they leave, a whole's parts rolled up, how often recall used a
//! node or a relation (what fades is what is unused, not what is old).
//!
//! Between host cores ([`wire`]): one audience's rows and their bodies in
//! batches, nothing to a side that may not hold the audience, and every
//! body checked again where it lands (shared, no local path, no secret,
//! its own canonical bytes).
//!
//! Not here yet: wakes beyond what a blocker's change brings, objections,
//! and forgetting a single span.
//!
//! Pure: nothing here reads a clock, a file, the network, a key or a random
//! source. Row ids and minted keys come from the caller; times are the rows'
//! clocks.

use std::fmt;

pub mod body;
pub mod fold;
pub mod key;
pub mod limits;
pub mod place;
pub mod queue;
pub mod tags;
pub mod upkeep;
pub mod wire;

pub use body::{BODY_VERSION, Body};
pub use fold::{FoldInput, Folded, Node, fold};
pub use key::{Kind, node_id, repo_key};
pub use place::{prepare, read};
pub use tags::{SaidBy, Sync};
pub use wire::{Batch, send, take};

/// Why an input was refused. It names a field or a rule, never a value: a
/// node's input holds her words and the partner's things.
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

/// A token (an id, a key, a row id): 1 to 512 bytes, no control character.
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

#[cfg(test)]
mod sync_tests;
