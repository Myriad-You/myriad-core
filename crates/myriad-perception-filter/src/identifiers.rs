//! Card numbers and national identity numbers (cards, Singapore NRIC/FIN,
//! Japan's My Number, China's resident identity number): wiped before any
//! model reads the text, and a hit means the window shows the owner's own
//! identity.
//!
//! Only numbers whose check digit is right are taken; a well-formed number
//! with a wrong check digit stays. Order and tracking numbers that happen to
//! pass Luhn are wiped too: one number fewer seen.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::secrets::Wiped;

/// What a wiped number is replaced with.
pub const MARK: &str = "[号码已抹去]";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentifierKind {
    /// 13–19 digits starting 3–6, Luhn.
    Card,
    /// Singapore NRIC / FIN, S T F G series (the 2022 M series is not taken yet).
    SgNric,
    /// Japan's individual number, 12 digits.
    JpMyNumber,
    /// China's resident identity number: 18 characters, a birth date in
    /// them and an ISO 7064 MOD 11-2 check character (`0`–`9` or `X`).
    CnResidentId,
}

/// Digit groups joined by single spaces or hyphens.
static DIGIT_RUN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9]+(?:[ \-][0-9]+)*").unwrap());
static NRIC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b[STFG][0-9]{7}[A-Z]\b").unwrap());
/// Within 20 characters of a My Number, unless it is written 4-4-4. The
/// label as Japan writes it, and as Chinese writes the same words.
static MY_NUMBER_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)マイナンバー|個人番号|个人番号|个人编号|my\s?number").unwrap()
});
/// A China resident identity number's shape: area (first digit 1–9), birth
/// date 19xx or 20xx, sequence, check character. The date and the check
/// character are checked in [`cn_resident_id`].
static CN_RESIDENT_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"[1-9][0-9]{5}(?:19|20)[0-9]{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12][0-9]|3[01])[0-9]{3}[0-9Xx]",
    )
    .unwrap()
});

pub fn scan(text: &str) -> Wiped<IdentifierKind> {
    let mut found: Vec<(usize, usize, IdentifierKind)> = Vec::new();
    // Resident ids first: one ending in `X` is no digit run, and one ending in
    // a digit is not also read as a card.
    for id in CN_RESIDENT_ID.find_iter(text) {
        if alone(text, id.start(), id.end()) && cn_resident_id(id.as_str()) {
            found.push((id.start(), id.end(), IdentifierKind::CnResidentId));
        }
    }
    let taken = found.clone();
    for run in DIGIT_RUN.find_iter(text) {
        // A digit glued to a letter or another digit is part of something else.
        if !alone(text, run.start(), run.end())
            || taken
                .iter()
                .any(|(start, end, _)| *start < run.end() && run.start() < *end)
        {
            continue;
        }
        found.extend(numbers_in(text, run.start(), run.as_str()));
    }
    for nric in NRIC.find_iter(text) {
        if nric_valid(nric.as_str()) {
            found.push((nric.start(), nric.end(), IdentifierKind::SgNric));
        }
    }
    found.sort_by_key(|(start, _, _)| *start);
    let mut out = text.to_string();
    for (start, end, _) in found.iter().rev() {
        out.replace_range(*start..*end, MARK);
    }
    Wiped {
        text: out,
        wiped: found.into_iter().map(|(_, _, kind)| kind).collect(),
    }
}

/// Nothing glued to `start..end` on either side: no ASCII letter or digit.
fn alone(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();
    !before.is_some_and(|c| c.is_ascii_alphanumeric())
        && !after.is_some_and(|c| c.is_ascii_alphanumeric())
}

/// Cards and My Numbers among one run's groups: the longest valid window of
/// whole groups wins, then the search goes on after it.
fn numbers_in(text: &str, offset: usize, run: &str) -> Vec<(usize, usize, IdentifierKind)> {
    // (start, end) of each group of digits, in bytes within `run`.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut start = None;
    for (at, c) in run.char_indices() {
        match (c.is_ascii_digit(), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                groups.push((from, at));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        groups.push((from, run.len()));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < groups.len() {
        let mut best: Option<(usize, IdentifierKind)> = None;
        for j in (i..groups.len()).rev() {
            let window = &run[groups[i].0..groups[j].1];
            let digits: Vec<u32> = window.chars().filter_map(|c| c.to_digit(10)).collect();
            let sizes: Vec<usize> = (i..=j).map(|k| groups[k].1 - groups[k].0).collect();
            if card(&digits) {
                best = Some((j, IdentifierKind::Card));
                break;
            }
            let grouped = sizes == [4, 4, 4];
            let labelled =
                sizes == [12] && labelled_near(text, offset + groups[i].0, offset + groups[j].1);
            if (grouped || labelled) && my_number(&digits) {
                best = Some((j, IdentifierKind::JpMyNumber));
                break;
            }
        }
        match best {
            Some((j, kind)) => {
                out.push((offset + groups[i].0, offset + groups[j].1, kind));
                i = j + 1;
            }
            None => i += 1,
        }
    }
    out
}

fn card(digits: &[u32]) -> bool {
    (13..=19).contains(&digits.len()) && (3..=6).contains(&digits[0]) && luhn(digits)
}

pub fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(index, &digit)| {
            let value = if index % 2 == 1 { digit * 2 } else { digit };
            if value > 9 { value - 9 } else { value }
        })
        .sum();
    sum.is_multiple_of(10)
}

pub fn nric_valid(raw: &str) -> bool {
    let chars: Vec<char> = raw.to_ascii_uppercase().chars().collect();
    if chars.len() != 9 {
        return false;
    }
    let body: Vec<u32> = chars[1..8].iter().filter_map(|c| c.to_digit(10)).collect();
    if body.len() != 7 {
        return false;
    }
    let mut sum: u32 = body
        .iter()
        .zip([2, 7, 6, 5, 4, 3, 2])
        .map(|(d, w)| d * w)
        .sum();
    if matches!(chars[0], 'T' | 'G') {
        sum += 4;
    }
    let letters: &[u8] = match chars[0] {
        'S' | 'T' => b"JZIHGFEDCBA",
        'F' | 'G' => b"XWUTRQPNMLK",
        _ => return false,
    };
    letters[(sum % 11) as usize] as char == chars[8]
}

pub fn my_number(digits: &[u32]) -> bool {
    if digits.len() != 12 {
        return false;
    }
    // From the right, the n-th digit before the check digit weighs n+1 (n ≤ 6) or n−5.
    let sum: u32 = (1..=11)
        .map(|n| digits[11 - n] * if n <= 6 { n as u32 + 1 } else { n as u32 - 5 })
        .sum();
    let check = if sum % 11 <= 1 { 0 } else { 11 - sum % 11 };
    digits[11] == check
}

/// A China resident identity number (GB 11643): a real calendar date in
/// characters 7–14 and the ISO 7064 MOD 11-2 check character last.
pub fn cn_resident_id(raw: &str) -> bool {
    let chars: Vec<char> = raw.chars().collect();
    if chars.len() != 18 {
        return false;
    }
    let body: Vec<u32> = chars[..17].iter().filter_map(|c| c.to_digit(10)).collect();
    if body.len() != 17 {
        return false;
    }
    let number = |from: usize, to: usize| body[from..to].iter().fold(0, |n, d| n * 10 + d);
    let (year, month, day) = (number(6, 10), number(10, 12), number(12, 14));
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    if !(1..=days).contains(&day) {
        return false;
    }
    let sum: u32 = body
        .iter()
        .zip([7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2])
        .map(|(d, w)| d * w)
        .sum();
    let check = b"10X98765432"[(sum % 11) as usize] as char;
    chars[17].to_ascii_uppercase() == check
}

/// A My Number label within 20 characters before or after `start..end`.
fn labelled_near(text: &str, start: usize, end: usize) -> bool {
    let from = text[..start]
        .char_indices()
        .rev()
        .nth(19)
        .map_or(0, |(at, _)| at);
    let to = text[end..]
        .char_indices()
        .nth(20)
        .map_or(text.len(), |(at, _)| end + at);
    MY_NUMBER_LABEL.is_match(&text[from..to])
}
