//! The child core: what every end needs to keep her link to her host.
//!
//! Her link's protocol (`shared/her_link_contract.json`) from the side that
//! dials, as a value: the shell owns the socket and the clock, feeds frames
//! and the time in, and does what comes back ([`Output`]: send this text,
//! take this delivery, drop the link). Whether a frame may be sent or taken,
//! and its one byte form, are `myriad_her_link::check`'s; nothing here
//! restates a rule of it. Where the host is, and whether it is always the
//! same one, is the shell's to know: a [`Session`] is told only that a socket
//! came up or went.
//!
//! It links none of her rules: an end that only plays what her host sends
//! needs this crate and no more. What an end can do is its own to say
//! ([`Session::new`] takes its capabilities), and [`capabilities`] says which
//! frames each one brings, so an end can test that it handles all it claims.
//!
//! Pure: nothing here reads a clock, a file, the network or a key.

pub mod capabilities;
mod contract;
mod frames;
mod session;
mod wire;

pub use contract::{CloseRule, Contract, Encoding, FrameRule, Limits, Timing, contract};
pub use frames::Unsent;
pub use session::{
    After, Delivery, Down, Output, Phase, Refusal, Removal, Session, Turn, WardrobeItem,
};

#[cfg(test)]
mod tests;
