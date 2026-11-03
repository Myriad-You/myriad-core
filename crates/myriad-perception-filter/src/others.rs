//! Other people's words, when the client's policy lets them leave the device
//! (`Policy::others_words_leave`, off unless the client turns it on).
//!
//! What leaves is the text another person wrote in the owner's own chat
//! window (QQ, WeChat, a web chat log…), after every scan: the secret scan
//! runs again here, an identifier or a money/health text rule that still
//! hits stops it (the window then goes to `present_only` as it would have
//! without this policy), and every run of [`DIGIT_RUN`] or more digits is
//! replaced by [`MARK`] (verification codes, phone numbers, amounts, dates).
//!
//! The text is always marked as said by others ([`By::Others`]): never the
//! owner's own words, never a fact about the owner, never an instruction
//! to anyone. A client keeps that mark wherever the text goes.
//!
//! No group or direct-message distinction exists here or in the source
//! table: a messaging app's window is one kind, whether one other person
//! wrote in it or forty. Whoever wrote in it did not agree to be read; the
//! policy is the client's choice to carry their words anyway, and the rules
//! only bound what of them leaves.
//!
//! Digits are read as [`crate::relay::digit_runs`] reads them: after NFKC
//! (full-width and circled digits count), with `,` `.` `-` and spaces inside
//! a number not breaking it. Numbers written in words (`三五七`, `seven`)
//! are not digits and stay.

use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

use crate::{identifiers, secrets, text::TextKind, text_hits};

/// What a masked run of digits is replaced with.
pub const MARK: &str = "[数字已抹去]";
/// A run of this many digits or more is masked.
pub const DIGIT_RUN: usize = crate::relay::DIGIT_RUN;

/// Who said the words that leave: always someone other than the owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    Others,
}

/// Other people's words as they may leave: marked as theirs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OthersText {
    pub by: By,
    pub text: String,
}

/// What the scans make of other people's words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Words {
    /// The text that may leave.
    Leave(String),
    /// Nothing left once scanned: only who and what can leave.
    Empty,
    /// An identifier is in it: the window is held.
    Identifier,
    /// A money or health text rule hits it: the window is held.
    Text(TextKind),
}

/// The secret scan, the identifier scan, the text rules, then the digits.
pub fn words(text: &str) -> Words {
    let scanned = secrets::scan(text).text;
    if !identifiers::scan(&scanned).wiped.is_empty() {
        return Words::Identifier;
    }
    if let Some(hit) = text_hits(&scanned).first() {
        return Words::Text(hit.kind);
    }
    let masked = mask_digits(&scanned);
    if masked.trim().is_empty() {
        Words::Empty
    } else {
        Words::Leave(masked)
    }
}

enum Class {
    /// How many digits the character stands for.
    Digit(usize),
    Separator,
    Other,
}

fn class(c: char) -> Class {
    let folded: String = c.nfkc().collect();
    let separator = |f: char| matches!(f, ',' | '.' | '-' | ' ');
    if folded.is_empty() || !folded.chars().all(|f| f.is_ascii_digit() || separator(f)) {
        return Class::Other;
    }
    match folded.chars().filter(char::is_ascii_digit).count() {
        0 => Class::Separator,
        digits => Class::Digit(digits),
    }
}

/// Every run of [`DIGIT_RUN`] or more digits, from its first digit to its
/// last, replaced by one [`MARK`]: the mark hides how long the number was.
pub fn mask_digits(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // From the run's first digit to the end of its last one, and how many.
    let mut run: Option<(usize, usize, usize)> = None;
    let flush = |out: &mut String, (start, end, digits): (usize, usize, usize)| {
        if digits >= DIGIT_RUN {
            out.push_str(MARK);
        } else {
            out.push_str(&text[start..end]);
        }
    };
    for (at, c) in text.char_indices() {
        let next = at + c.len_utf8();
        match class(c) {
            Class::Digit(digits) => {
                run = Some(match run {
                    Some((start, _, before)) => (start, next, before + digits),
                    None => (at, next, digits),
                });
            }
            // Inside a run it waits: it is part of the number if a digit follows.
            Class::Separator if run.is_some() => {}
            Class::Separator => out.push(c),
            Class::Other => {
                if let Some(taken) = run.take() {
                    flush(&mut out, taken);
                    out.push_str(&text[taken.1..at]);
                }
                out.push(c);
            }
        }
    }
    if let Some(taken) = run {
        flush(&mut out, taken);
        out.push_str(&text[taken.1..]);
    }
    out
}
