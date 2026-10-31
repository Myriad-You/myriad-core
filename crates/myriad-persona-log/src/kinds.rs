//! Which kinds of row the log holds, what each is about and how it folds.
//!
//! An allow-list: a kind missing here is refused, so a new kind of row
//! never leaves a device until it is added here on purpose. The node kinds
//! are the native app's node log changes and the shared core's node rows
//! (`myriad-nodes`); the case kinds its judged cases.

use serde::Serialize;

/// How rows of one kind fold into their object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// Something that happened: every row stays, in log order.
    Event,
    /// A value that changes: the last row in log order is the current one,
    /// the earlier ones are its history.
    Latest,
    /// Retires the object, one way, unless a later [`Rule::Restore`].
    Retire,
    /// Undoes an earlier [`Rule::Retire`].
    Restore,
    /// Forgets the object it names (or everything about a subject), on
    /// every side and in any order.
    Forget,
    /// Takes a node back from sync (撤回同步): the partner's row, read as a
    /// forget on every side but its homes ([`crate::forget`]).
    Retract,
    /// The local row a side writes with its own retract: this side keeps
    /// the node. Local rows never come from another side, so no row
    /// another side sends makes this one a home ([`crate::forget`]).
    Home,
}

/// Which other node a kind's rows may name in their envelope (`to`,
/// `takes`, [`crate::Row`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Names {
    None,
    /// The node it went into (`node.absorbed`).
    Into,
    /// The part split off it, and the spans moved there (`node.split_off`).
    Split,
}

/// What other node rows of `kind` name in their envelope.
pub fn names(kind: &str) -> Names {
    match kind {
        "node.absorbed" => Names::Into,
        "node.split_off" => Names::Split,
        _ => Names::None,
    }
}

/// The object a row of `kind` must be about, as the part before `:`;
/// `None` takes any object.
type Object = Option<&'static str>;

/// Sorted by kind.
const KINDS: &[(&str, Object, Rule)] = &[
    ("affect.change", Some("affect"), Rule::Event),
    ("case.disagreed", Some("case"), Rule::Event),
    ("case.insisted", Some("case"), Rule::Event),
    ("case.outcome", Some("case"), Rule::Event),
    ("case.owned", Some("case"), Rule::Event),
    ("case.persuaded", Some("case"), Rule::Event),
    ("case.reconsidered", Some("case"), Rule::Event),
    ("case.tested", Some("case"), Rule::Event),
    ("case.verdict", Some("case"), Rule::Event),
    ("case.work", Some("case"), Rule::Event),
    ("forget", None, Rule::Forget),
    // What a thing (a talk, a loose end) hangs on: the row is about the
    // thing, its payload names the node. Re-hanging shows in its history.
    ("link.attach", None, Rule::Latest),
    ("memory.restore", Some("memory"), Rule::Restore),
    ("memory.retire", Some("memory"), Rule::Retire),
    ("memory.write", Some("memory"), Rule::Event),
    ("node.absorbed", Some("node"), Rule::Event),
    ("node.acted", Some("node"), Rule::Event),
    ("node.consolidated", Some("node"), Rule::Event),
    ("node.created", Some("node"), Rule::Event),
    ("node.decided", Some("node"), Rule::Event),
    ("node.done_when", Some("node"), Rule::Event),
    ("node.drafted", Some("node"), Rule::Event),
    // A fact or preference on a node, with its three tags.
    ("node.entry", Some("node"), Rule::Event),
    ("node.faded", Some("node"), Rule::Event),
    ("node.filed", Some("node"), Rule::Event),
    ("node.grown", Some("node"), Rule::Event),
    // Something she means to say, revised against the version it saw.
    ("node.intent", Some("node"), Rule::Event),
    // What a node is changed (in progress ↔ topic): history, not a relation.
    ("node.kind_changed", Some("node"), Rule::Event),
    ("node.loosened", Some("node"), Rule::Event),
    ("node.merged", Some("node"), Rule::Event),
    ("node.noted", Some("node"), Rule::Event),
    ("node.notified", Some("node"), Rule::Event),
    ("node.observed", Some("node"), Rule::Event),
    ("node.open_item", Some("node"), Rule::Event),
    ("node.opened", Some("node"), Rule::Event),
    ("node.raised", Some("node"), Rule::Event),
    // One of the five relations, from this node to another in its audience;
    // `node.unrelated` takes one back.
    ("node.related", Some("node"), Rule::Event),
    ("node.rested", Some("node"), Rule::Event),
    ("node.retagged", Some("node"), Rule::Event),
    ("node.revised", Some("node"), Rule::Event),
    ("node.revived", Some("node"), Rule::Event),
    ("node.shown", Some("node"), Rule::Event),
    // One span of experience, one thing, with its three tags.
    ("node.span", Some("node"), Rule::Event),
    ("node.split", Some("node"), Rule::Event),
    ("node.split_off", Some("node"), Rule::Event),
    ("node.talked", Some("node"), Rule::Event),
    ("node.unrelated", Some("node"), Rule::Event),
    ("node.upkept", Some("node"), Rule::Event),
    ("node.used", Some("node"), Rule::Event),
    ("node.view_changed", Some("node"), Rule::Latest),
    ("outfit.write", Some("outfit"), Rule::Latest),
    // One field of her persona per object: `persona:<field>`.
    ("persona.field", Some("persona"), Rule::Latest),
    // The partner took a node back from sync: it is forgotten on every
    // side but the ones that wrote it with a `retract.home` beside it.
    ("retract", Some("node"), Rule::Retract),
    ("retract.home", Some("node"), Rule::Home),
    ("segment.close", Some("segment"), Rule::Event),
    ("segment.line", Some("segment"), Rule::Event),
    ("segment.open", Some("segment"), Rule::Event),
];

/// The object type a forget names to forget everything about a subject in
/// its audience: `subject:<id>`. No other row is about it.
pub const SUBJECT: &str = "subject";

fn entry(kind: &str) -> Option<&'static (&'static str, Object, Rule)> {
    KINDS
        .binary_search_by(|(known, _, _)| (*known).cmp(kind))
        .ok()
        .map(|at| &KINDS[at])
}

/// How rows of `kind` fold, or `None` for a kind the log does not hold.
pub fn rule(kind: &str) -> Option<Rule> {
    entry(kind).map(|(_, _, rule)| *rule)
}

/// The object type rows of `kind` must be about; `None` for any (or for a
/// kind the log does not hold).
pub fn object_type(kind: &str) -> Option<&'static str> {
    entry(kind).and_then(|(_, object, _)| *object)
}

/// Every kind, sorted.
pub fn all() -> impl Iterator<Item = &'static str> {
    KINDS.iter().map(|(kind, _, _)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_sorted_and_unique() {
        assert!(KINDS.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert_eq!(rule("node.view_changed"), Some(Rule::Latest));
        assert_eq!(rule("node.unknown"), None);
        assert_eq!(object_type("persona.field"), Some("persona"));
        assert_eq!(object_type("forget"), None);
        assert_eq!(rule("retract"), Some(Rule::Retract));
        assert_eq!(object_type("retract"), Some("node"));
        assert_eq!(rule("retract.home"), Some(Rule::Home));
        assert_eq!(names("node.split_off"), Names::Split);
        assert_eq!(names("node.span"), Names::None);
    }
}
