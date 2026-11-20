//! What each capability an end may say in `hello` brings with it: the frames
//! it then sends and the frames her host may then send it.
//!
//! The contract lists the names (`capabilities`) and tells the rest in each
//! frame's description; this table is that reading, checked against how her
//! host switches on them. It is here, not in the contract, until the contract
//! takes it: change both ends' understanding before changing a row.
//!
//! An end tests itself with [`unread`]: every frame a capability it says
//! brings down must be one it reads.

use std::collections::BTreeSet;

/// What saying a capability changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Frames of its own go up or come down.
    Frames,
    /// No frame of its own: fields of frames every child has, or of her log's rows.
    Fields,
    /// Her host reads nothing from it today: saying it changes nothing.
    Unheeded,
}

/// One capability's row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub capability: &'static str,
    pub kind: Kind,
    /// Capabilities that must be said with it for its frames to flow.
    pub with: &'static [&'static str],
    /// Frame types the child then sends.
    pub sends: &'static [&'static str],
    /// Frame types her host may then send the child.
    pub takes: &'static [&'static str],
}

const fn frames(
    capability: &'static str,
    with: &'static [&'static str],
    sends: &'static [&'static str],
    takes: &'static [&'static str],
) -> Rule {
    Rule {
        capability,
        kind: Kind::Frames,
        with,
        sends,
        takes,
    }
}

const fn bare(capability: &'static str, kind: Kind) -> Rule {
    Rule {
        capability,
        kind,
        with: &[],
        sends: &[],
        takes: &[],
    }
}

/// What a child sends whatever it says it can do.
pub const BASE_SENDS: &[&str] = &[
    "ack",
    "hello",
    "input",
    "ping",
    "pong",
    "presence",
    "run.resume",
];

/// What her host may send a child whatever it says it can do. `perform` here is the one that
/// goes with a turn's reply. `host.freeze`, `host.thaw` and `host.moved` are left out: they
/// come only on the link of the device she lives on, which no child's session reads.
pub const BASE_TAKES: &[&str] = &[
    "error",
    "mood",
    "perform",
    "ping",
    "pong",
    "run.accepted",
    "run.end",
    "run.event",
    "run.rejected",
    "welcome",
];

/// Every capability the contract names, in its byte order.
pub const RULES: &[Rule] = &[
    // Her host takes an `act.result` whatever the link said: a run may end after the switch
    // was turned off.
    frames("act", &[], &["act.result"], &["act.cancel", "act.request"]),
    // One end asking another to do one thing for her (calls); the frames go both ways.
    frames(
        "calls",
        &[],
        &[
            "call.accepted",
            "call.ask",
            "call.cancel",
            "call.event",
            "call.offer",
            "call.progress",
            "call.result",
        ],
        &[
            "call.accepted",
            "call.ask",
            "call.cancel",
            "call.event",
            "call.granted",
            "call.progress",
            "call.result",
        ],
    ),
    frames(
        "cards",
        &["conversations.live"],
        &["card.answer"],
        &["card.answered"],
    ),
    frames("conversations", &[], &[], &["conversations"]),
    frames("conversations.live", &[], &[], &["conversation"]),
    bare("cua", Kind::Unheeded),
    // Only an end that can host her: a deputy to be her interim host, and what of it and of a
    // grant it took. No child's session reads either.
    frames("deputy", &[], &["lease.taken"], &["lease.deputy"]),
    frames(
        "experience",
        &[],
        &["experience", "work.recall"],
        &["experience.taken", "work.recalled"],
    ),
    // `input.glance`: what the child saw on the screen as the owner spoke.
    bare("glance", Kind::Fields),
    bare("images", Kind::Fields),
    // Lent only to the end that holds her pen, so these do not flow while that is her host.
    frames(
        "lend",
        &[],
        &["chat.say", "group.say"],
        &["chat.lend", "chat.said", "group.lend", "group.said"],
    ),
    frames("log.full", &[], &[], &["log.full"]),
    // `log.rows` `more` and `log.vector` `after`: what is missing comes a page at a time.
    bare("log.pages", Kind::Fields),
    bare("log2", Kind::Fields),
    bare("log3", Kind::Fields),
    bare("log4", Kind::Fields),
    // ABI 5 reads the same rows as 4: said for the versions it shows.
    bare("log5", Kind::Fields),
    frames("moments", &[], &["moment"], &["moment.taken"]),
    frames("nodes", &[], &["node.forget"], &["nodes"]),
    frames("notify", &[], &[], &["notice"]),
    bare("now.choices", Kind::Fields),
    frames(
        "now.sessions",
        &[],
        &["now.drop", "now.session"],
        &["now.taken"],
    ),
    bare("now.sessions2", Kind::Fields),
    bare("panel", Kind::Unheeded),
    bare("partner", Kind::Fields),
    bare("perception.music", Kind::Fields),
    bare("perception.pointer", Kind::Fields),
    bare("perception.screen", Kind::Fields),
    bare("perception.window", Kind::Fields),
    bare("pi", Kind::Unheeded),
    // The `perform` it brings is her director's: the one with `live`, and the one with `sayId`.
    frames(
        "playback",
        &[],
        &["playback.end", "playback.observe", "speech.delivered"],
        &["perform"],
    ),
    frames(
        "proposals",
        &[],
        &["proposal.answer"],
        &["proposal.answered", "proposals"],
    ),
    // The `notice` of kind `reminder`: nothing comes without `notify`.
    frames("reminders", &["notify"], &[], &["notice"]),
    bare("rig", Kind::Unheeded),
    frames("say", &[], &[], &["say"]),
    frames(
        "streams",
        &[],
        &["stream.event", "stream.state"],
        &["stream.event", "stream.state"],
    ),
    frames(
        "touch",
        &[],
        &["touch", "touch.appraise"],
        &["touch.reaction"],
    ),
    frames(
        "versions",
        &[],
        &["link.versions"],
        &["link.verdict", "link.versions"],
    ),
    bare("voice.in", Kind::Fields),
    bare("voice.out", Kind::Unheeded),
    frames("wardrobe", &[], &["outfit.wear"], &["outfit"]),
    frames("wardrobe.list", &[], &[], &["wardrobe"]),
];

/// The row of `capability`, when the contract names it.
pub fn rule(capability: &str) -> Option<&'static Rule> {
    RULES.iter().find(|rule| rule.capability == capability)
}

/// The rows of `said` whose frames flow: those said together with all they need. What a row
/// needs is checked one step deep; the tests hold that no needed row needs another.
fn flowing<'a>(said: &'a [&'a str]) -> impl Iterator<Item = &'static Rule> + 'a {
    said.iter()
        .filter_map(|capability| rule(capability))
        .filter(|rule| rule.with.iter().all(|needed| said.contains(needed)))
}

/// Every frame type her host may send a child that said `said`.
pub fn takes(said: &[&str]) -> BTreeSet<&'static str> {
    BASE_TAKES
        .iter()
        .copied()
        .chain(flowing(said).flat_map(|rule| rule.takes.iter().copied()))
        .collect()
}

/// Every frame type a child that said `said` sends.
pub fn sends(said: &[&str]) -> BTreeSet<&'static str> {
    BASE_SENDS
        .iter()
        .copied()
        .chain(flowing(said).flat_map(|rule| rule.sends.iter().copied()))
        .collect()
}

/// What an end claims and does not do: each `(capability, frame type)` her host may send a
/// child that said `said` and that `reads` says this end does not read; a frame every child
/// gets is under the capability `""`. Empty when the end reads all it claims.
///
/// `reads` is the end's own account of its handling code, down to its shell: start from
/// [`crate::Session::READS`] and take away what the shell lets fall.
pub fn unread(said: &[&str], reads: impl Fn(&str) -> bool) -> Vec<(&'static str, &'static str)> {
    let mut missing: Vec<(&str, &str)> = BASE_TAKES
        .iter()
        .map(|t| ("", *t))
        .chain(flowing(said).flat_map(|rule| rule.takes.iter().map(|t| (rule.capability, *t))))
        .filter(|(_, t)| !reads(t))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    missing
}

/// What an end says to no effect: the capabilities of `said` her host reads nothing from, and
/// those whose frames do not flow because one they need was not said with them. In the order
/// said; a name the contract does not know is not listed (hello drops it before it is said).
pub fn idle<'a>(said: &[&'a str]) -> Vec<&'a str> {
    said.iter()
        .copied()
        .filter(|capability| {
            rule(capability).is_some_and(|rule| {
                rule.kind == Kind::Unheeded || !rule.with.iter().all(|needed| said.contains(needed))
            })
        })
        .collect()
}
