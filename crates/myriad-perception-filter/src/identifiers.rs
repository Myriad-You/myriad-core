//! Card numbers and national identity numbers (cards, Singapore NRIC/FIN,
//! Japan's My Number, China's resident identity number, passports): wiped
//! before any model reads the text, and a hit means the window shows the
//! owner's own identity.
//!
//! Only numbers whose check digit is right are taken; a well-formed number
//! with a wrong check digit stays. Order and tracking numbers that happen to
//! pass Luhn are wiped too: one number fewer seen.
//!
//! Passport numbers have no check digit, and their shapes are those of order
//! numbers, model numbers and the like, so one is taken only with a passport
//! label just before it on the same line ([`passport_labelled`]), or as part
//! of a passport's machine readable zone, whose check digits are checked
//! ([`mrz_line`]).

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
    /// A passport number under a passport label on the same line (Japan: two
    /// letters and 7 digits; Singapore: a letter, 7 digits and a letter;
    /// China: `E` and 8 digits, `E`, a letter and 7 digits, or the older `G`
    /// and 8 digits), or the second line of a passport's machine readable
    /// zone (ICAO 9303 TD3) whose check digits are right.
    Passport,
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

/// The passport number shapes: Japan (two letters, 7 digits; China's `E`, a
/// letter and 7 digits is the same shape), Singapore (letter, 7 digits,
/// letter: which letters are in use is not checked) and China (`E` or the
/// older `G`, 8 digits). Upper case only, as printed.
static PASSPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Z]{2}[0-9]{7}|[A-Z][0-9]{7}[A-Z]|[EG][0-9]{8}").unwrap());
/// A passport label, as Chinese, Japanese and English write it, and the mixed
/// `Passport 号码`.
static PASSPORT_LABEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)护照号码?|旅券番号|パスポート番号|passport\s*(?:no\b\.?|number\b|#|号码)")
        .unwrap()
});
/// At most this many characters between a passport label and its number.
const PASSPORT_LABEL_GAP: usize = 12;
/// A candidate machine readable zone line: 44 of `A`–`Z`, `0`–`9` and `<`.
static MRZ_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Z0-9<]{44}").unwrap());

pub fn scan(text: &str) -> Wiped<IdentifierKind> {
    let mut found: Vec<(usize, usize, IdentifierKind)> = Vec::new();
    // Machine readable zones first: a passport number inside one is glued to
    // its check digit, and the whole line goes.
    for line in MRZ_LINE.find_iter(text) {
        let (start, end) = (line.start(), line.end());
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        let in_zone = |c: char| c.is_ascii_alphanumeric() || c == '<';
        if !before.is_some_and(in_zone) && !after.is_some_and(in_zone) && mrz_line(line.as_str()) {
            found.push((start, end, IdentifierKind::Passport));
        }
    }
    // Resident ids next: one ending in `X` is no digit run, and one ending in
    // a digit is not also read as a card.
    for id in CN_RESIDENT_ID.find_iter(text) {
        if alone(text, id.start(), id.end())
            && free(&found, id.start(), id.end())
            && cn_resident_id(id.as_str())
        {
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
        if free(&found, nric.start(), nric.end()) && nric_valid(nric.as_str()) {
            found.push((nric.start(), nric.end(), IdentifierKind::SgNric));
        }
    }
    for number in PASSPORT.find_iter(text) {
        let (start, end) = (number.start(), number.end());
        if alone(text, start, end) && free(&found, start, end) && passport_labelled(text, start) {
            found.push((start, end, IdentifierKind::Passport));
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

/// `start..end` overlaps nothing already found.
fn free(found: &[(usize, usize, IdentifierKind)], start: usize, end: usize) -> bool {
    !found.iter().any(|(from, to, _)| *from < end && start < *to)
}

/// A passport label before `start` on the same line, ending at most
/// [`PASSPORT_LABEL_GAP`] characters before it. A label on an earlier line
/// (a heading, an article about applying for a passport) takes nothing.
pub fn passport_labelled(text: &str, start: usize) -> bool {
    let line = text[..start]
        .rfind(['\n', '\r', '\u{2028}', '\u{2029}'])
        .map_or(0, |at| {
            at + text[at..].chars().next().map_or(1, char::len_utf8)
        });
    PASSPORT_LABEL
        .find_iter(&text[line..start])
        .last()
        .is_some_and(|label| text[line + label.end()..start].chars().count() <= PASSPORT_LABEL_GAP)
}

/// The second line of a passport's machine readable zone (ICAO 9303, TD3):
/// document number and its check digit, nationality, birth date and its check
/// digit, sex, expiry date and its check digit, optional data and its check
/// digit, and the check digit over all of them. The document number, both
/// dates and the whole line have to check out.
pub fn mrz_line(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() != 44 {
        return false;
    }
    let field = |from: usize, to: usize| &chars[from..to];
    let number = field(0, 9);
    if !number.iter().any(char::is_ascii_alphanumeric)
        || !field(10, 13)
            .iter()
            .all(|c| c.is_ascii_uppercase() || *c == '<')
        || !matches!(chars[20], 'M' | 'F' | 'X' | '<')
        || !date(field(13, 19))
        || !date(field(21, 27))
    {
        return false;
    }
    let checks = |data: &[char], digit: char| mrz_check(data) == digit.to_digit(10);
    let whole: Vec<char> = [field(0, 10), field(13, 20), field(21, 43)].concat();
    checks(number, chars[9])
        && checks(field(13, 19), chars[19])
        && checks(field(21, 27), chars[27])
        && checks(&whole, chars[43])
}

/// A `YYMMDD` date: six digits, a month and a day that can be.
fn date(chars: &[char]) -> bool {
    let digits: Vec<u32> = chars.iter().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != 6 {
        return false;
    }
    let (month, day) = (digits[2] * 10 + digits[3], digits[4] * 10 + digits[5]);
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// The ICAO 9303 check digit: digits as themselves, `A`–`Z` as 10–35, `<` as
/// 0, weighted 7, 3, 1 in turn, modulo 10. `None` for any other character.
pub fn mrz_check(data: &[char]) -> Option<u32> {
    let mut sum = 0;
    for (at, c) in data.iter().enumerate() {
        let value = match c {
            '0'..='9' => c.to_digit(10)?,
            'A'..='Z' => *c as u32 - 'A' as u32 + 10,
            '<' => 0,
            _ => return None,
        };
        sum += value * [7, 3, 1][at % 3];
    }
    Some(sum % 10)
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
