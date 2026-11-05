//! The marks a line may carry, in one table: what each `[[…]]` means, whose
//! it is, which call of the core reads it, and which sides act on it today.
//! Every mark is taken out before a line is shown or said: none is ever
//! read aloud. The two sides knew different sets (the site never stripped
//! `work`, `sticker` or `game` from what it showed; the Mac had `stance`
//! and the helper's `ask` the site does not); this table is both.

use serde::Serialize;

/// Whose line carries the mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Whose {
    /// Hers, in a reply or a line she says.
    Her,
    /// The helper's, the one doing a piece of work she handed off.
    Helper,
}

/// One kind of mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Marker {
    /// The word it opens with, inside `[[`.
    pub name: &'static str,
    /// How it is written.
    pub forms: &'static [&'static str],
    pub means: &'static str,
    pub whose: Whose,
    /// The core function that reads it out of a line, when the core reads
    /// it.
    pub read_by: &'static [&'static str],
    /// The sides that act on it today: `site`, `app`.
    pub acted_on: &'static [&'static str],
}

/// Every mark, by name.
pub const MARKERS: &[Marker] = &[
    Marker {
        name: "ask",
        forms: &["[[ask: question]]"],
        means: "the helper needs their answer to go on",
        whose: Whose::Helper,
        read_by: &[],
        acted_on: &["app"],
    },
    Marker {
        name: "game",
        forms: &[
            "[[game:soup]]",
            "[[game:undercover]]",
            "[[game:werewolf]]",
            "[[game:avalon]]",
            "[[game:jubensha]]",
        ],
        means: "she starts hosting that game; it follows her words",
        whose: Whose::Her,
        read_by: &["soup::split_start", "hosting::split_seat_game"],
        // The Mac hosts turtle soup alone, in her own chat.
        acted_on: &["site", "app"],
    },
    Marker {
        name: "music",
        forms: &[
            "[[music:play]]",
            "[[music:pause]]",
            "[[music:toggle]]",
            "[[music:next]]",
            "[[music:prev]]",
            "[[music:join]]",
            "[[music:share N]]",
        ],
        means: "what she does with the music playing between them",
        whose: Whose::Her,
        read_by: &["chat_music::split_chat_music_directive"],
        acted_on: &["site", "app"],
    },
    Marker {
        name: "stance",
        forms: &[
            "[[stance:disagree|about|view]]",
            "[[stance:turned N worked]]",
            "[[stance:turned N failed]]",
            "[[stance:persuaded N]]",
            "[[stance:owned N]]",
        ],
        means: "where she stands on something they disagree about",
        whose: Whose::Her,
        read_by: &[],
        acted_on: &["app"],
    },
    Marker {
        name: "sticker",
        forms: &[
            "[[sticker:N]]",
            "[[sticker:new|what it shows|what it means]]",
        ],
        means: "she sends one of her stickers, or makes a new one",
        whose: Whose::Her,
        read_by: &["stickers::split_sticker_directive"],
        acted_on: &["site"],
    },
    Marker {
        name: "stop",
        forms: &["[[stop #N]]"],
        means: "stop piece N of the work she handed off, and only it",
        whose: Whose::Her,
        read_by: &["handoff::split"],
        acted_on: &["site", "app"],
    },
    Marker {
        name: "wear",
        forms: &["[[wear:outfit]]", "⟦wear:outfit⟧"],
        means: "she changes into that outfit",
        whose: Whose::Her,
        read_by: &["split_chat_wear_directive"],
        acted_on: &["site", "app"],
    },
    Marker {
        name: "work",
        forms: &["[[work: what to do]]", "[[work #N: more for it]]"],
        means: "she hands a piece of work off, or passes their words on to piece N",
        whose: Whose::Her,
        read_by: &["handoff::split"],
        acted_on: &["site", "app"],
    },
];

const PAIRS: [(&str, &str); 2] = [("[[", "]]"), ("⟦", "⟧")];

/// The mark whose name `inner` opens with, as a word: `[[workers]]` is not
/// a mark.
fn named(inner: &str) -> Option<&'static Marker> {
    let inner = inner.trim_start();
    MARKERS.iter().find(|marker| {
        inner
            .get(..marker.name.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(marker.name))
            && inner[marker.name.len()..]
                .chars()
                .next()
                .is_none_or(|next| next.is_whitespace() || "#＃:：|".contains(next))
    })
}

/// `text` without its marks: every closed `[[…]]` (or `⟦…⟧`) that opens with
/// a mark's name goes, and a line it leaves blank goes with it. Brackets
/// that are no mark stay. A mark not yet closed stays too: that is
/// [`unfinished`]'s.
pub fn strip(text: &str) -> String {
    // Where a mark was taken out, so only a line it emptied goes.
    const GONE: char = '\u{0}';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut took = false;
    while !rest.is_empty() {
        let next = PAIRS
            .iter()
            .filter_map(|(open, close)| rest.find(open).map(|at| (at, *open, *close)))
            .min_by_key(|(at, _, _)| *at);
        let Some((at, open, close)) = next else {
            out.push_str(rest);
            break;
        };
        let after = &rest[at + open.len()..];
        if let Some(end) = after.find(close)
            && named(&after[..end]).is_some()
        {
            out.push_str(&rest[..at]);
            out.push(GONE);
            took = true;
            rest = &after[end + close.len()..];
            continue;
        }
        out.push_str(&rest[..at + open.len()]);
        rest = after;
    }
    if !took {
        return out;
    }
    let lines: Vec<String> = out
        .split('\n')
        .filter(|line| !(line.contains(GONE) && line.replace(GONE, "").trim().is_empty()))
        .map(|line| line.replace(GONE, ""))
        .collect();
    lines.join("\n").trim_end().to_string()
}

/// Where a mark still being written begins at the end of `text`, so a side
/// streaming her words holds it back instead of flashing it: an opening
/// with no close after it whose inside is a mark's name or the start of
/// one, or a lone `[` at the very end. `None` when nothing is held.
pub fn unfinished(text: &str) -> Option<usize> {
    for (open, close) in PAIRS {
        if let Some(at) = text.rfind(open) {
            let inner = &text[at + open.len()..];
            if inner.contains(close) {
                continue;
            }
            let head = inner.trim_start();
            let partial = MARKERS.iter().any(|marker| {
                head.len() < marker.name.len()
                    && marker
                        .name
                        .get(..head.len())
                        .is_some_and(|start| start.eq_ignore_ascii_case(head))
            });
            if partial || named(inner).is_some() {
                return Some(at);
            }
        }
    }
    text.ends_with('[').then(|| text.len() - 1)
}

/// What of `text` is shown or said now: without its marks, and without a
/// mark still being written at its end; and whether one is held back.
pub fn shown(text: &str) -> (String, bool) {
    match unfinished(text) {
        Some(at) => (strip(&text[..at]).trim_end().to_string(), true),
        None => (strip(text), false),
    }
}
