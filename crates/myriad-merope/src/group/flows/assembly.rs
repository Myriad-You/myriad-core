//! Her whole prompt for an answer in a group, put together once for
//! whomever she answers there: who she is; what she knows of them and how
//! she is; the group's own part (the game at the table, the words and memes
//! in the talk and the groups' own, her stickers, how the group reads); how
//! she is this moment, last, nearest their words; then the talk and how she
//! replies. Whom she answers only decides what some of it holds: for a
//! member, what she knows of them comes whole from the end that hosts her;
//! for someone from outside, it is her note on them and whom she takes them
//! for, put together from what was read (see `stranger`).

use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// The lines of the talk the words and memes in it are looked for in.
const TALK_LINES: usize = 12;

/// What her answer is put together from.
pub(crate) struct Assembly<'a> {
    pub soul: &'a str,
    /// What she knows of whom she answers and how she is, as sections. How
    /// she is this moment among them (`speaking::is_her_moment`) is taken
    /// out and goes last.
    pub known: &'a [String],
    /// What she is doing on her own now, if anything.
    pub doing: Option<&'a str>,
    /// The group, as sessions keep it (`telegram:-100123`).
    pub venue: &'a str,
    pub transcript: &'a [Spoken],
    pub said: &'a str,
    /// The words and memes she looked up before that the talk has in it.
    pub memes: &'a [(String, String)],
    /// The groups' own words, as kept (those the talk has in it are shown).
    pub jargon: &'a [MemoryRow],
    /// What the groups named in what she brings in call themselves.
    pub titles: &'a BTreeMap<String, Option<String>>,
    /// The game at the group's table: this line judged, or how she would
    /// start one.
    pub game: Option<&'a str>,
    pub room: Option<&'a crate::talk_shape::Shape>,
    /// What she made of the talk before answering.
    pub making_sense: Option<&'a str>,
    /// How long ago the line she answers was said, when she only now sees it.
    pub late: Option<&'a str>,
    /// How her lines here differ from how people type here.
    pub differs: Option<&'a str>,
    /// The stickers offered, best first; none when she has no section.
    pub stickers: Option<&'a [StickerRow]>,
    /// How many she can still make this month.
    pub left: usize,
    /// Nobody called her: why she speaks up.
    pub chime: Option<&'a str>,
}

/// The talk the words and memes she knows are looked for in: the latest
/// lines, newest first, and what they said.
pub(crate) fn talk(transcript: &[Spoken], said: &str) -> String {
    transcript
        .iter()
        .rev()
        .take(TALK_LINES)
        .map(|line| line.content.as_str())
        .chain(std::iter::once(said))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `venue` (`group:<platform>:<chat>` or `<platform>:<chat>`) without its
/// prefix.
pub(crate) fn unprefixed(venue: &str) -> &str {
    venue.strip_prefix("group:").unwrap_or(venue)
}

pub(crate) fn title(titles: &BTreeMap<String, Option<String>>, venue: &str) -> Option<String> {
    titles.get(unprefixed(venue)).cloned().flatten()
}

/// The groups the stickers offered and the groups' own words in the talk
/// say they came from, added to `venues`.
pub(crate) fn venues_brought(
    venues: &mut BTreeSet<String>,
    venue: &str,
    talk: &str,
    jargon: &[MemoryRow],
    stickers: Option<&[StickerRow]>,
) {
    for sticker in stickers.into_iter().flatten() {
        if let Some(from) = &sticker.kept_from {
            venues.insert(unprefixed(from).to_string());
        }
    }
    let here = super::sense::stored(venue);
    for row in jargon {
        let Some(word) = crate::memes::read_kept(&row.content) else {
            continue;
        };
        if !talk.contains(&word.term) {
            continue;
        }
        if let Some(venue) = row.venue.as_deref().filter(|venue| *venue != here) {
            venues.insert(unprefixed(venue).to_string());
        }
    }
}

/// Her own lines her habits are read from: those in the group's record of
/// this talk. Once what she said is kept in one place for every end, they
/// are read from there instead, and only this changes.
fn habit_lines(transcript: &[Spoken]) -> Vec<&str> {
    transcript
        .iter()
        .filter(|line| line.role == "assistant")
        .map(|line| line.content.as_str())
        .collect()
}

/// Her whole prompt, put together.
pub(crate) fn prompt(cx: &Cx, a: &Assembly) -> String {
    let now = cx.now();
    // How she is this moment goes last, nearest their words: what she
    // answers from, not one more thing among the rest.
    let (moment, known): (Vec<&String>, Vec<&String>) = a
        .known
        .iter()
        .partition(|section| crate::speaking::is_her_moment(section));
    let mut sections: Vec<String> = known.into_iter().cloned().collect();
    // What they call her by there.
    if let Some(name) = &cx.group.shown_as {
        sections.push(crate::speaking::shown_as_section(name));
    }
    // What of her own time she has already told them in this talk.
    if let Some(about) = a.doing {
        let lines: Vec<&str> = a
            .transcript
            .iter()
            .filter(|line| line.role == "assistant")
            .flat_map(|line| line.content.lines())
            .collect();
        let told = crate::speaking::already_told(about, &lines);
        sections.extend(crate::speaking::format_already_told_section(&told));
    }
    // A habit of hers, when one shows: known, not ordered.
    sections.extend(crate::speaking::format_habits_section(&habit_lines(
        a.transcript,
    )));
    // Words and memes in the talk that she looked up before.
    let memes: Vec<crate::memes::Known> = a
        .memes
        .iter()
        .map(|(term, meaning)| crate::memes::Known {
            term: term.clone(),
            meaning: meaning.clone(),
        })
        .collect();
    sections.extend(crate::memes::section(&memes.iter().collect::<Vec<_>>()));
    // And the groups' own words in it.
    let here = super::sense::stored(a.venue);
    let talk = talk(a.transcript, a.said);
    let jargon: Vec<crate::jargon::Known> = a
        .jargon
        .iter()
        .filter_map(|row| {
            let word = crate::memes::read_kept(&row.content)?;
            if !talk.contains(&word.term) {
                return None;
            }
            let venue = row.venue.as_deref().unwrap_or_default();
            let (group, elsewhere) = if venue == here {
                (None, false)
            } else {
                (title(a.titles, venue), true)
            };
            Some(crate::jargon::Known {
                term: word.term,
                meaning: word.meaning,
                group,
                elsewhere,
            })
        })
        .collect();
    sections.extend(crate::jargon::section(&jargon));
    // A turtle soup on in this group, with this line judged; or how she
    // would start one for the group.
    sections.extend(a.game.map(str::to_string));
    sections.push(crate::speaking::chat_app_section().to_string());
    sections.extend(
        a.room
            .map(|room| crate::talk_shape::describe(room, "How people type here")),
    );
    // What she made of the talk before answering: she answers from it.
    sections.extend(a.making_sense.map(str::to_string));
    // She only now sees the line she answers.
    sections.extend(a.late.map(crate::joining::seeing_it_late));
    sections.extend(a.differs.map(str::to_string));
    // Stickers of her: hers, and this group's own.
    if let Some(stickers) = a.stickers {
        let meanings: Vec<String> = stickers
            .iter()
            .map(|sticker| {
                let line = crate::stickers::offered_line(
                    &sticker.meaning,
                    sticker.sent,
                    sticker.last_sent_at.map(|at| lines::age(now, at)),
                );
                match &sticker.kept_from {
                    Some(from) => {
                        crate::stickers::kept_line(&line, title(a.titles, from).as_deref())
                    }
                    None => line,
                }
            })
            .collect();
        sections.extend(crate::stickers::format_sticker_section(&meanings, a.left));
    }
    // Nobody called her by name: she chose to say something.
    if let Some(why) = a.chime {
        sections.push(crate::joining::speaking_up_section(why.trim()));
    }
    sections.extend(moment.into_iter().cloned());
    sections.retain(|section| !section.trim().is_empty());
    let history: Vec<crate::chat_history::HistoryMessage> = a
        .transcript
        .iter()
        .map(|line| crate::chat_history::HistoryMessage {
            role: &line.role,
            content: &line.content,
            sent_at: None,
        })
        .collect();
    crate::chat_history::group_chat_prompt(a.soul, &sections.join("\n\n"), &history, a.said)
}
