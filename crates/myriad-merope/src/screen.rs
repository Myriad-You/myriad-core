//! What a look at the owner's screen may become of hers, the rules every
//! end keeps the same (an `agent.merope.screen` moment, on the Mac that saw
//! it or on the host it was told to): whether she took it in is hers to say
//! in the decision she already makes about it; these say only what of her
//! words may be kept, which of her notes she is offered for it, and how the
//! fact of a look she only knew of is written.
//!
//! - **Her words without the window's**: what she took away and her mood
//!   are kept without any run of the window's text she was shown that she
//!   repeated ([`unquoted`]), so a page's or a document's words never become
//!   a span or a head sentence of hers. A structural check of her words
//!   against what she was shown, not a rule about what they say.
//! - **The notes she is offered** ([`notes`]): those of the owner's own
//!   private audience the look's app and window share most words with, by
//!   recall's word score (`lexical::score_all`); never a person, at most
//!   [`NOTE_LIMIT`].
//! - **Only knew** ([`fact`]): when, and the app and window or file in
//!   front, in the persona's language; nothing of hers, nothing the page or
//!   document said.

use std::collections::HashSet;

use chrono::{DateTime, TimeZone, Utc};
use myriad_agent_rules::memory::lexical::{self, Document};

/// The most characters of what she took away from a look.
pub const TAKE_CHARS: usize = 320;
/// The most characters of her mood as she took it in.
pub const MOOD_CHARS: usize = 120;
/// The most characters of a note's label (`n1`…).
pub const NOTE_CHARS: usize = 8;
/// At most this many notes are offered with a look.
pub const NOTE_LIMIT: usize = 6;
/// The source a screen fact's memory is written with.
pub const FACT_SOURCE: &str = "screen";
/// How long a copied run is, by [`width`]: about eight wide characters or
/// sixteen narrow ones.
pub const QUOTE_RUN: usize = 16;

/// What she said of a look besides what she does now. `None` reads as
/// nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Take {
    /// What she took away, in her words; none when it only passed by.
    pub took_in: Option<String>,
    /// How she was as she took it in.
    pub mood: Option<String>,
    /// The label of the note it belongs with, when one offered fits.
    pub note: Option<String>,
}

/// `take` without any of `shown`, the window's text she was given with the
/// look: her words and her mood each lose every run she repeated of it.
/// Nothing of hers left: she only knew of it (no mood, no note).
pub fn unquoted(take: Take, shown: Option<&str>) -> Take {
    let Some(shown) = shown else {
        return take;
    };
    if take.took_in.is_none() {
        return take;
    }
    let took_in = take
        .took_in
        .as_deref()
        .and_then(|text| without_quotes(text, shown));
    if took_in.is_none() {
        return Take::default();
    }
    Take {
        took_in,
        mood: take
            .mood
            .as_deref()
            .and_then(|text| without_quotes(text, shown)),
        note: take.note,
    }
}

/// How wide a character is set: the CJK scripts, kana, hangul and the
/// full-width forms are two, any other one. A measure of length, not a list
/// of words.
pub fn width(ch: char) -> usize {
    const WIDE: [(u32, u32); 12] = [
        (0x1100, 0x115F),
        (0x2E80, 0x303E),
        (0x3041, 0x33FF),
        (0x3400, 0x4DBF),
        (0x4E00, 0x9FFF),
        (0xA960, 0xA97F),
        (0xAC00, 0xD7A3),
        (0xF900, 0xFAFF),
        (0xFE30, 0xFE4F),
        (0xFF00, 0xFF60),
        (0xFFE0, 0xFFE6),
        (0x20000, 0x3FFFD),
    ];
    let v = ch as u32;
    if WIDE.iter().any(|&(low, high)| (low..=high).contains(&v)) {
        2
    } else {
        1
    }
}

/// The half-width katakana and marks, U+FF61 on, as their full-width forms.
const HALF_KANA: &str = "。「」、・ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン゛゜";

/// `ch` as compared: its full-width or half-width form folded to the usual
/// one, then lowercased.
fn folded(ch: char) -> String {
    let v = ch as u32;
    let usual = match v {
        0xFF01..=0xFF5E => char::from_u32(v - 0xFEE0),
        0xFF61..=0xFF9F => HALF_KANA.chars().nth((v - 0xFF61) as usize),
        0xFFE0 => Some('¢'),
        0xFFE1 => Some('£'),
        0xFFE2 => Some('¬'),
        0xFFE3 => Some('¯'),
        0xFFE4 => Some('¦'),
        0xFFE5 => Some('¥'),
        0xFFE6 => Some('₩'),
        0x3000 => Some(' '),
        _ => None,
    }
    .unwrap_or(ch);
    usual.to_lowercase().collect()
}

/// Each character other than a space, folded, its width as folded, and
/// where it stands.
fn letters(text: &str) -> Vec<(String, usize, usize)> {
    text.chars()
        .enumerate()
        .filter(|(_, ch)| !ch.is_whitespace())
        .map(|(at, ch)| {
            let folded = folded(ch);
            let width = folded.chars().map(width).sum();
            (folded, width, at)
        })
        .collect()
}

/// The shortest run from each start that is [`QUOTE_RUN`] long: its first
/// and past-last letter and the letters it is.
fn runs(letters: &[(String, usize, usize)]) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    for start in 0..letters.len() {
        let (mut length, mut key, mut end) = (0, String::new(), start);
        while end < letters.len() && length < QUOTE_RUN {
            length += letters[end].1;
            key.push_str(&letters[end].0);
            key.push('\u{1f}');
            end += 1;
        }
        if length < QUOTE_RUN {
            break;
        }
        out.push((start, end, key));
    }
    out
}

/// A space within a line: what is trimmed off her words' ends.
fn line_space(ch: char) -> bool {
    ch.is_whitespace()
        && !matches!(
            ch,
            '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        )
}

/// `text` with every run of [`QUOTE_RUN`] or more that stands in `shown`
/// character for character (spaces and line breaks aside on both sides,
/// case and width aside) as one `…`, the spaces inside it with it. None
/// when fewer than two letters or digits of hers are left.
pub fn without_quotes(text: &str, shown: &str) -> Option<String> {
    let copies: HashSet<String> = runs(&letters(shown))
        .into_iter()
        .map(|(_, _, key)| key)
        .collect();
    if copies.is_empty() {
        return Some(text.to_string());
    }
    let words = letters(text);
    let mut copied = HashSet::new();
    for (start, end, key) in runs(&words) {
        if copies.contains(&key) {
            copied.extend(words[start..end].iter().map(|letter| letter.2));
        }
    }
    if copied.is_empty() {
        return Some(text.to_string());
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut in_copy = false;
    for (index, &ch) in chars.iter().enumerate() {
        if copied.contains(&index) {
            if !in_copy {
                out.push('…');
            }
            in_copy = true;
            continue;
        }
        if ch.is_whitespace()
            && in_copy
            && chars[index..]
                .iter()
                .position(|next| !next.is_whitespace())
                .is_some_and(|next| copied.contains(&(index + next)))
        {
            continue;
        }
        out.push(ch);
        in_copy = false;
    }
    let left = out
        .chars()
        .filter(|ch| ch.is_alphabetic() || ch.is_numeric())
        .count();
    (left >= 2).then(|| out.trim_matches(line_space).to_string())
}

/// One of the owner's notes as a look is matched against it: a node of the
/// owner's own private audience, as it stands (never one gone into another;
/// the host leaves those out).
#[derive(Debug, Clone, Copy)]
pub struct Note<'a> {
    /// The node's kind (`person` is never offered).
    pub kind: &'a str,
    pub title: &'a str,
    /// The name of what it is about, when it has one.
    pub name: Option<&'a str>,
    /// Her summary of it.
    pub summary: Option<&'a str>,
    pub aliases: &'a [String],
    pub threads: &'a [String],
    /// Whether it is one of hers in play now.
    pub active: bool,
    /// When it last changed, in any unit the host keeps (larger is later).
    pub updated_at: i64,
}

/// The kind of node never offered with a look: someone, the owner or her
/// included.
pub const PERSON: &str = "person";

/// Which of `candidates` a look at `app` with `thing` (its window or file)
/// is offered, in order, at most [`NOTE_LIMIT`]: those sharing a full
/// word with it, the ones in play first, then by score, then the latest.
/// Their labels are [`label`] of their place in the answer.
pub fn notes(app: &str, thing: Option<&str>, candidates: &[Note]) -> Vec<usize> {
    let offered: Vec<usize> = (0..candidates.len())
        .filter(|&at| candidates[at].kind != PERSON)
        .collect();
    if offered.is_empty() {
        return Vec::new();
    }
    let query = [Some(app), thing]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let texts: Vec<String> = offered
        .iter()
        .map(|&at| {
            let note = &candidates[at];
            [Some(note.title), note.name, note.summary]
                .into_iter()
                .flatten()
                .chain(note.aliases.iter().map(String::as_str))
                .chain(note.threads.iter().map(String::as_str))
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    let documents: Vec<Document> = texts
        .iter()
        .map(|text| Document {
            text,
            concepts: &[],
        })
        .collect();
    let scores = lexical::score_all(&query, &documents);
    let mut order: Vec<usize> = (0..offered.len()).filter(|&at| scores[at].strong).collect();
    order.sort_by(|&a, &b| {
        let (left, right) = (&candidates[offered[a]], &candidates[offered[b]]);
        right
            .active
            .cmp(&left.active)
            .then_with(|| scores[b].value.total_cmp(&scores[a].value))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    order
        .into_iter()
        .take(NOTE_LIMIT)
        .map(|at| offered[at])
        .collect()
}

/// The label of the `place`-th note offered (from 0): `n1`, `n2`, …; no id
/// reaches her.
pub fn label(place: usize) -> String {
    format!("n{}", place + 1)
}

/// The fact of a look she only knew of: when, in `zone`, and `app` with
/// `thing` (its window or file) in front, in `language` (`ja`, `en`, else
/// Chinese).
pub fn fact<Tz: TimeZone>(
    app: &str,
    thing: Option<&str>,
    at: DateTime<Utc>,
    language: &str,
    zone: &Tz,
) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let at = at.with_timezone(zone).format("%Y-%m-%d %H:%M").to_string();
    let language = language.to_lowercase();
    let thing = thing.filter(|thing| !thing.is_empty());
    match (language.get(..2).unwrap_or(&language), thing) {
        ("ja", Some(thing)) => format!("画面：{at} {app} で「{thing}」を開いていた"),
        ("ja", None) => format!("画面：{at} {app} を使っていた"),
        ("en", Some(thing)) => format!("On screen: at {at} you had «{thing}» open in {app}"),
        ("en", None) => format!("On screen: at {at} you were using {app}"),
        (_, Some(thing)) => format!("屏幕上：{at} 你在 {app} 里开着「{thing}」"),
        (_, None) => format!("屏幕上：{at} 你在用 {app}"),
    }
}

#[cfg(test)]
mod tests {
    use chrono::FixedOffset;

    use super::*;

    #[test]
    fn the_half_width_kana_are_all_there() {
        assert_eq!(HALF_KANA.chars().count(), 0xFF9F - 0xFF61 + 1);
        assert_eq!(folded('ｶ'), "カ");
        assert_eq!(folded('ﾟ'), "゜");
        assert_eq!(folded('Ａ'), "a");
    }

    #[test]
    fn a_run_she_repeated_is_one_ellipsis() {
        let shown = "The quick brown fox jumps over the lazy dog";
        assert_eq!(
            without_quotes("I liked: the QUICK brown fox jumps, so fun", shown).as_deref(),
            Some("I liked: …, so fun")
        );
        // Shorter than a run: hers.
        assert_eq!(
            without_quotes("quick brown fox", shown).as_deref(),
            Some("quick brown fox")
        );
        // Wide characters count two: eight make a run.
        assert_eq!(
            without_quotes("她说今天天气真好啊我们出去", "今天天气真好啊我们").as_deref(),
            Some("她说…出去")
        );
        // Full-width letters fold to the usual ones.
        assert_eq!(
            without_quotes("ｔｈｅ ｑｕｉｃｋ ｂｒｏｗｎ ｆｏｘ", shown),
            None
        );
    }

    #[test]
    fn nothing_of_hers_left_is_only_knowing() {
        let take = Take {
            took_in: Some("The quick brown fox jumps over".into()),
            mood: Some("开心".into()),
            note: Some("n1".into()),
        };
        assert_eq!(
            unquoted(
                take.clone(),
                Some("the quick brown fox jumps over the lazy dog")
            ),
            Take::default()
        );
        assert_eq!(unquoted(take.clone(), None), take);
    }

    #[test]
    fn notes_share_words_with_the_look_never_a_person() {
        let none: [String; 0] = [];
        let rust = ["rust".to_string()];
        let candidates = [
            Note {
                kind: "project",
                title: "Myriad 后端",
                name: None,
                summary: Some("Rust 写的站点"),
                aliases: &none,
                threads: &none,
                active: false,
                updated_at: 1,
            },
            Note {
                kind: PERSON,
                title: "Myriad",
                name: None,
                summary: None,
                aliases: &none,
                threads: &none,
                active: true,
                updated_at: 9,
            },
            Note {
                kind: "project",
                title: "做饭",
                name: None,
                summary: None,
                aliases: &none,
                threads: &none,
                active: true,
                updated_at: 5,
            },
            Note {
                kind: "topic",
                title: "Myriad 前端",
                name: None,
                summary: None,
                aliases: &rust,
                threads: &none,
                active: true,
                updated_at: 2,
            },
        ];
        assert_eq!(
            notes("Zed", Some("Myriad · main.rs"), &candidates),
            vec![3, 0]
        );
        assert_eq!(label(0), "n1");
    }

    #[test]
    fn a_fact_says_when_and_what_was_in_front() {
        let at = DateTime::from_timestamp(1_793_750_400, 0).unwrap();
        let zone = FixedOffset::east_opt(8 * 3600).unwrap();
        assert_eq!(
            fact("Word", Some("报告.docx"), at, "zh-CN", &zone),
            "屏幕上：2026-11-04 08:00 你在 Word 里开着「报告.docx」"
        );
        assert_eq!(
            fact("Safari", None, at, "ja", &zone),
            "画面：2026-11-04 08:00 Safari を使っていた"
        );
        assert_eq!(
            fact("Safari", Some(""), at, "EN", &zone),
            "On screen: at 2026-11-04 08:00 you were using Safari"
        );
    }
}
