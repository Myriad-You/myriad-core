//! Handing work off from her chat: what each end decides the same way.
//!
//! In her conversation she talks as herself. When they want something done
//! she hands it off with a line of her own at the end of her reply,
//! `[[work: …]]`. Each thing handed off is a piece of its own, numbered in
//! the conversation (`#1`, `#2`, …), and several can be under way at once.
//! What they say later for one of them she passes on with `[[work #N: …]]`;
//! `[[stop #N]]` stops that one alone.
//!
//! Here: reading her lines out of a reply (`split`), how a piece stands
//! (`stands`) and how she sees it (`line`, `shown`, `section`), where their
//! next words go (`route`), what one of her lines does to the pieces (`act`),
//! the choice she means (`her_choice`), what the helper is told
//! (`work_input`) and what she says when a run ends (`ended_line`,
//! `labelled`). Running the pieces, keeping them, timing and delivery stay
//! with each end.

use myriad_agent_rules::channel::PendingOption;
use serde::{Deserialize, Serialize};

const OPEN: &str = "[[";
const CLOSE: &str = "]]";
/// What of an instruction is kept.
pub const MAX_INSTRUCTION_CHARS: usize = 500;
/// What of a piece's request is shown in its label.
pub const LABEL_CHARS: usize = 12;
/// What of a result or question she reads back in a piece's line.
pub const RESULT_CHARS: usize = 200;
/// What of a result goes in her line when a run ends.
pub const ENDED_LINE_CHARS: usize = 80;
/// Why a piece whose run never began did not get done.
pub const NEVER_BEGAN: &str = "没能开始";
/// Pieces that are over she still sees, and for how long after they ended.
pub const DONE_SHOWN: usize = 3;
pub const DONE_FOR_SECS: i64 = 24 * 60 * 60;

/// One line of hers: what it asks of the pieces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum HandOff {
    /// A new piece: what to do.
    New { text: String },
    /// For piece `work`: the answer to its question, or more to go on.
    To { work: u32, text: String },
    /// Stop piece `work`, and only it.
    Stop { work: u32 },
}

/// What follows `word` at the head of `inner`, in any case, when `word`
/// is a word there: `[[workers]]` or `[[stopwatch]]` is not hers.
fn lead<'a>(inner: &'a str, word: &str) -> Option<&'a str> {
    inner
        .get(..word.len())
        .filter(|head| head.eq_ignore_ascii_case(word))
        .map(|_| &inner[word.len()..])
        .filter(|rest| {
            rest.is_empty()
                || rest.starts_with(|next: char| next.is_whitespace() || "#＃:：".contains(next))
        })
}

/// `work: …`, `work #2: …`, `stop #2`, as she writes them inside `[[ ]]`.
/// `None` is someone else's line; `Some(None)` is hers but says nothing,
/// written wrong or left empty.
fn parse(inner: &str) -> Option<Option<HandOff>> {
    let inner = inner.trim();
    let number = |rest: &str| -> Option<u32> {
        rest.trim()
            .trim_start_matches(['#', '＃'])
            .trim()
            .parse()
            .ok()
    };
    if let Some(rest) = lead(inner, "stop") {
        return Some(number(rest).map(|work| HandOff::Stop { work }));
    }
    let rest = lead(inner, "work")?;
    let Some((head, text)) = rest.split_once([':', '：']) else {
        return Some(None);
    };
    let text: String = text.trim().chars().take(MAX_INSTRUCTION_CHARS).collect();
    if text.is_empty() {
        return Some(None);
    }
    if head.trim().is_empty() {
        return Some(Some(HandOff::New { text }));
    }
    Some(number(head).map(|work| HandOff::To { work, text }))
}

/// Take her hand-off lines out of a reply: the text without them, and what
/// they say, in the order she wrote them. One of hers written wrong goes
/// unsaid; a line left with nothing but spaces goes with them. Lines that
/// are not hers (a sticker, an outfit) stay for whoever reads them.
pub fn split(raw: &str) -> (String, Vec<HandOff>) {
    let (text, handed) = take_out(raw, true);
    (text.trim_end().to_string(), handed)
}

/// A helper's result as she is to read it: lines of hers that say
/// something (a piece to start, to tell, to stop) are taken out, a line
/// left with nothing but spaces with them. Anything else stays as written,
/// `[[Work log]]` and blank lines in code too: it is not her reply.
pub fn result_text(raw: &str) -> String {
    take_out(raw, false).0
}

/// Her lines out of `raw`, and what they say; `wrong_too` takes out those
/// of hers that say nothing as well.
fn take_out(raw: &str, wrong_too: bool) -> (String, Vec<HandOff>) {
    let mut text = raw.to_string();
    let mut handed = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(OPEN) {
        let start = from + found;
        let after = start + OPEN.len();
        let Some(close) = text[after..].find(CLOSE) else {
            break;
        };
        match parse(&text[after..after + close]) {
            None => from = after,
            Some(None) if !wrong_too => from = after,
            Some(line) => {
                // The same new piece twice in one reply is one piece; what
                // she tells a piece goes each time, in order.
                match line {
                    Some(new @ HandOff::New { .. }) if handed.contains(&new) => {}
                    Some(line) => handed.push(line),
                    None => {}
                }
                text.replace_range(start..after + close + CLOSE.len(), "");
                without_blank_line(&mut text, start);
                // From the top again: taking it out may have closed one
                // around it (`[[[[stop #1]]work: …]]`).
                from = 0;
            }
        }
    }
    (text, handed)
}

/// After a line of hers is taken out at `at`: the line it stood on goes too
/// when nothing but spaces is left on it, with its line break (the one
/// before it, on the last line).
fn without_blank_line(text: &mut String, at: usize) {
    let begins = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let ends = text[at..].find('\n').map_or(text.len(), |found| at + found);
    if !text[begins..ends].trim().is_empty() {
        return;
    }
    if ends < text.len() {
        text.replace_range(begins..=ends, "");
    } else if begins > 0 {
        let breaks = if text[..begins - 1].ends_with('\r') {
            2
        } else {
            1
        };
        text.replace_range(begins - breaks..ends, "");
    } else {
        text.clear();
    }
}

/// What stopped a piece short, with nothing going on until it is picked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CutOffBy {
    /// The site restarted under its run.
    Restart,
    /// The app quit under its run.
    AppQuit,
}

/// How a piece stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase", deny_unknown_fields)]
pub enum Standing {
    /// Its run goes on: how far (percent) or how many steps, where known.
    Doing {
        #[serde(default)]
        progress: Option<u8>,
        #[serde(default)]
        steps: Option<u32>,
    },
    /// Handed off, its run not begun yet.
    Starting,
    /// Waiting for them to confirm on its card; not answered in chat.
    Confirming,
    /// Waiting for their answer to its question.
    Waiting {
        question: String,
    },
    /// Its run was cut off; it waits to be picked up.
    CutOff {
        by: CutOffBy,
    },
    Done {
        #[serde(default)]
        result: Option<String>,
    },
    Failed {
        #[serde(default)]
        why: Option<String>,
    },
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Piece {
    pub number: u32,
    /// What she handed off.
    pub asked: String,
    /// When it was handed off, Unix seconds.
    #[serde(default)]
    pub started_at: Option<i64>,
    /// When its last run ended, Unix seconds, for one that is over.
    #[serde(default)]
    pub ended_at: Option<i64>,
    pub standing: Standing,
}

impl Piece {
    /// Not over: something may still come of it.
    pub fn unfinished(&self) -> bool {
        matches!(
            self.standing,
            Standing::Doing { .. }
                | Standing::Starting
                | Standing::Confirming
                | Standing::Waiting { .. }
                | Standing::CutOff { .. }
        )
    }
}

/// Whether a piece's run ever began.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Begun {
    #[default]
    Yes,
    /// Handed off a moment ago; its run is on its way.
    NotYet,
    /// Handed off, and its run never began: it did not start, or what was
    /// starting it stopped first.
    Never,
}

/// What an end knows of a piece; `stands` reads it as one standing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Facts {
    /// They or she stopped it.
    #[serde(default)]
    pub stopped: bool,
    /// The question it waits on, as they were shown it.
    #[serde(default)]
    pub question: Option<String>,
    /// Its run waits on a question they were never shown: a restart left
    /// it there.
    #[serde(default)]
    pub unheard_question: bool,
    /// Its run is under way.
    #[serde(default)]
    pub under_way: bool,
    #[serde(default)]
    pub progress: Option<u8>,
    /// What came of it, when it is over.
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub begun: Begun,
}

/// How a piece stands, from what is known of it. A stop wins; a question
/// they saw comes before one left by a restart; one whose run never began
/// did not get done.
pub fn stands(facts: &Facts) -> Standing {
    if facts.stopped {
        Standing::Stopped
    } else if let Some(question) = &facts.question {
        Standing::Waiting {
            question: question.clone(),
        }
    } else if facts.unheard_question {
        Standing::CutOff {
            by: CutOffBy::Restart,
        }
    } else if facts.under_way {
        Standing::Doing {
            progress: facts.progress,
            steps: None,
        }
    } else if facts.begun == Begun::NotYet {
        Standing::Starting
    } else if facts.begun == Begun::Never {
        Standing::Failed {
            why: Some(NEVER_BEGAN.into()),
        }
    } else {
        Standing::Done {
            result: facts.result.clone(),
        }
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn took(seconds: i64) -> String {
    let minutes = seconds.max(0) / 60;
    match minutes {
        0 => "不到 1 分钟".into(),
        1..60 => format!("{minutes} 分钟"),
        _ => format!("{} 小时 {} 分钟", minutes / 60, minutes % 60),
    }
}

fn read_back(text: &str) -> String {
    let all = one_line(text);
    let mut short: String = all.chars().take(RESULT_CHARS).collect();
    if all.chars().count() > RESULT_CHARS {
        short.push('…');
    }
    short
}

/// A piece as she sees it, on one line. `now` is Unix seconds.
pub fn line(piece: &Piece, now: i64) -> String {
    let head = format!("#{}「{}」", piece.number, one_line(&piece.asked));
    match &piece.standing {
        Standing::Doing { progress, steps } => {
            let took = piece
                .started_at
                .map(|at| format!("，已办 {}", took(now - at)))
                .unwrap_or_default();
            let progress = progress
                .filter(|progress| *progress > 0)
                .map(|progress| format!("（{progress}%）"))
                .unwrap_or_default();
            let steps = steps
                .filter(|steps| *steps > 0)
                .map(|steps| format!("，第 {steps} 步"))
                .unwrap_or_default();
            format!("{head}在办{took}{progress}{steps}")
        }
        Standing::Starting => format!("{head}刚交出去，还没开始"),
        Standing::Confirming => format!("{head}在等对方在卡片上确认"),
        Standing::Waiting { question } => {
            format!("{head}在等对方回答：{}", read_back(question))
        }
        Standing::CutOff {
            by: CutOffBy::Restart,
        } => format!("{head}被打断了（重启），还没办完"),
        Standing::CutOff {
            by: CutOffBy::AppQuit,
        } => format!("{head}被打断了（应用退出），还没办完"),
        Standing::Done {
            result: Some(result),
        } => format!("{head}办完了：{}", read_back(result)),
        Standing::Done { result: None } => format!("{head}办完了"),
        Standing::Failed { why: Some(why) } => format!("{head}没办成：{}", read_back(why)),
        Standing::Failed { why: None } => format!("{head}没办成"),
        Standing::Stopped => format!("{head}被叫停了"),
    }
}

/// Which pieces she sees: every unfinished one, and the few that ended last
/// within a day, so she knows how they came back: one begun long ago that
/// just came back is the one she most needs. Where its end is not known,
/// its start stands in. In the order handed off.
pub fn shown(mut pieces: Vec<Piece>, now: i64) -> Vec<Piece> {
    pieces.sort_by_key(|piece| piece.number);
    let mut over: Vec<(i64, u32)> = pieces
        .iter()
        .filter(|piece| !piece.unfinished())
        .filter_map(|piece| {
            let at = piece.ended_at.or(piece.started_at)?;
            (now - at < DONE_FOR_SECS).then_some((at, piece.number))
        })
        .collect();
    over.sort();
    let over: Vec<u32> = over
        .split_off(over.len().saturating_sub(DONE_SHOWN))
        .into_iter()
        .map(|(_, number)| number)
        .collect();
    pieces
        .into_iter()
        .filter(|piece| piece.unfinished() || over.contains(&piece.number))
        .collect()
}

/// Where what they write goes, when it is not a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "camelCase")]
pub enum Route {
    /// The one piece waiting for an answer takes it as its answer.
    Answer { work: u32 },
    /// She reads it: chat, a new piece, or which piece it is for.
    Her,
}

/// Only one waiting on a question: it is that one's answer. More: she
/// places it.
pub fn route(pieces: &[Piece]) -> Route {
    let mut waiting = pieces
        .iter()
        .filter(|piece| matches!(piece.standing, Standing::Waiting { .. }));
    match (waiting.next(), waiting.next()) {
        (Some(piece), None) => Route::Answer { work: piece.number },
        _ => Route::Her,
    }
}

/// What one of her lines does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "act", rename_all = "camelCase")]
pub enum Act {
    Start {
        text: String,
    },
    /// The answer to piece N's question.
    Answer {
        work: u32,
        text: String,
    },
    /// More for piece N to go on, into its run as it is.
    Steer {
        work: u32,
        text: String,
    },
    /// Piece N is over: it picks up again in its own session, where it ended.
    Continue {
        work: u32,
        text: String,
    },
    /// Piece N was cut off: it goes on from where its run was saved.
    PickUp {
        work: u32,
        text: String,
    },
    Stop {
        work: u32,
    },
    /// For a piece that is stopped or never was: nothing.
    Nothing,
}

pub fn act(hand_off: &HandOff, pieces: &[Piece]) -> Act {
    let piece = |work: u32| pieces.iter().find(|piece| piece.number == work);
    match hand_off {
        HandOff::New { text } => Act::Start { text: text.clone() },
        HandOff::To { work, text } => {
            let (work, text) = (*work, text.clone());
            match piece(work).map(|piece| &piece.standing) {
                Some(Standing::Waiting { .. }) => Act::Answer { work, text },
                Some(Standing::Doing { .. } | Standing::Starting | Standing::Confirming) => {
                    Act::Steer { work, text }
                }
                Some(Standing::Done { .. } | Standing::Failed { .. }) => {
                    Act::Continue { work, text }
                }
                Some(Standing::CutOff { .. }) => Act::PickUp { work, text },
                Some(Standing::Stopped) | None => Act::Nothing,
            }
        }
        HandOff::Stop { work } => match piece(*work) {
            Some(piece) if piece.unfinished() => Act::Stop { work: *work },
            _ => Act::Nothing,
        },
    }
}

/// `【#N 做什么】`, on what a piece sends while another is not done.
pub fn label(number: u32, asked: &str) -> String {
    let asked = one_line(asked);
    let mut short: String = asked.chars().take(LABEL_CHARS).collect();
    if asked.chars().count() > LABEL_CHARS {
        short.push('…');
    }
    format!("【#{number} {short}】")
}

/// What a piece sends, with its label while another is not done.
pub fn labelled(number: u32, asked: &str, others_unfinished: bool, content: &str) -> String {
    if others_unfinished {
        format!("{}\n{content}", label(number, asked))
    } else {
        content.to_string()
    }
}

/// Her answer to a piece's choice question, as the choice: she may write
/// `2（19:00）` or `订 19:00 的`; a leading choice number, or the one choice
/// named in it, is what she means. Anything else stays as she wrote it.
pub fn her_choice(options: &[PendingOption], text: &str) -> String {
    let text = text.trim();
    if options.is_empty() {
        return text.to_string();
    }
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let after = text[digits.len()..].trim_start();
    // `19点`, `1 点`, `2:30`, `3时`: a time, not a choice; `2号` is one,
    // unless the choices are days (`2号`, `11月5号`): then it names one.
    let a_day = after.starts_with('号')
        && options
            .iter()
            .any(|option| option.value.contains('号') || option.label.contains('号'));
    if !digits.starts_with('0')
        && let Ok(number) = digits.parse::<usize>()
        && (1..=options.len()).contains(&number)
        && !after.starts_with([':', '：', '.', '点', '时'])
        && !a_day
    {
        return number.to_string();
    }
    let said: Vec<Vec<(usize, usize)>> = options
        .iter()
        .map(|option| {
            [&option.value, &option.label]
                .into_iter()
                .flat_map(|name| named_at(text, name))
                .collect()
        })
        .collect();
    // `9:00` inside `19:00` is the longer one said, not both.
    let inside_another = |of: usize, (from, to): (usize, usize)| {
        said.iter().enumerate().any(|(other, spans)| {
            other != of
                && spans
                    .iter()
                    .any(|&(start, end)| start <= from && to <= end && end - start > to - from)
        })
    };
    let named: Vec<usize> = said
        .iter()
        .enumerate()
        .filter(|(of, spans)| spans.iter().any(|&span| !inside_another(*of, span)))
        .map(|(index, _)| index + 1)
        .collect();
    match named.as_slice() {
        [number] => number.to_string(),
        _ => text.to_string(),
    }
}

/// Where `text` names a choice: it says it, and not right after a no
/// (`不订了` does not name `订`). An empty name names nothing.
fn named_at(text: &str, name: &str) -> Vec<(usize, usize)> {
    let name = name.trim();
    if name.is_empty() {
        return Vec::new();
    }
    text.match_indices(name)
        .filter(|(at, _)| !text[..*at].ends_with(['不', '没', '别', '非']))
        .map(|(at, _)| (at, at + name.len()))
        .collect()
}

/// The next piece's number: one past the highest, from 1. None when the
/// numbers have run out.
pub fn next_number(numbers: impl IntoIterator<Item = u32>) -> Option<u32> {
    numbers.into_iter().max().unwrap_or(0).checked_add(1)
}

/// What the helper is asked: her instruction, with their own words.
pub fn work_input(instruction: &str, their_words: &str) -> String {
    let their_words = their_words.trim();
    if their_words.is_empty() || instruction.contains(their_words) {
        instruction.to_string()
    } else {
        // Their words may ask for more than this piece (several handed off
        // at once): they only say how, this piece does the line above.
        format!("{instruction}\n\n（对方原话，只作参考，这一件只办上面这一条：{their_words}）")
    }
}

/// Her line when a run of a piece ends: what came of it, in one line. The
/// lines run together, so a result that opens with "here they are:" still
/// carries what follows.
pub fn ended_line(text: &str, failed: bool) -> String {
    let all = text
        .lines()
        .map(bare)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut short: String = all.chars().take(ENDED_LINE_CHARS).collect();
    if all.chars().count() > ENDED_LINE_CHARS {
        short.push('…');
    }
    match (failed, short.is_empty()) {
        (true, true) => "没办成。".into(),
        (true, false) => format!("没办成：{short}"),
        (false, true) => "办完了。".into(),
        (false, false) => short,
    }
}

/// A line of a result without its Markdown marks: quote, heading, bullet. A
/// minus sign stays (`-5°C`); a rule (`---`) is nothing.
fn bare(line: &str) -> &str {
    let line = line.trim().trim_start_matches('>').trim_start();
    if line
        .chars()
        .all(|mark| matches!(mark, '-' | '*' | '_' | ' '))
    {
        return "";
    }
    let heading = line.trim_start_matches('#');
    let line = if heading.len() < line.len() && heading.starts_with(char::is_whitespace) {
        heading.trim_start()
    } else {
        line
    };
    match line.strip_prefix(['-', '*']) {
        Some(item) if item.starts_with(char::is_whitespace) => item.trim(),
        _ => line,
    }
}

/// Where her helper works, which is what she tells them it can reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Host {
    /// The site, from an IM chat or its web panel.
    Site,
    /// The Mac app.
    Mac,
}

/// What she knows about handing work off. `lines` are the pieces she sees
/// (`line` of each `shown`), `waiting` how many wait on an answer, and
/// `answered` the one their words just went to as its answer.
pub fn section(host: Host, lines: &[String], waiting: usize, answered: Option<u32>) -> String {
    let (reach, numbers_by, cut_off) = match host {
        Host::Site => (
            "working with their data on this site",
            "the channel",
            "cut off by a restart",
        ),
        Host::Mac => (
            "working with files and apps on this Mac",
            "the app",
            "cut off when the app quit",
        ),
    };
    let mut out = vec![
        "## Getting things done".to_string(),
        format!(
            "You cannot use tools yourself in this chat, but a helper can: looking things up on the web, {reach}, making or changing things for them. \
When they ask you to get something like that done, say in a few words that you are on it, and put [[work: what exactly to do, in one line, keeping their own words]] on its own last line. \
The helper does it with their permissions; any question or confirmation it needs goes to them directly, and the result comes back to them. \
If it is your own idea, ask first and hand it off only after they say yes. Do not hand off plain conversation. Do not read these lines aloud."
        ),
        format!(
            "Each thing you hand off is a piece of its own, numbered, and several can be under way at once. \
Different things they ask for are different pieces, each with its own [[work: …]] line, even when they come in one breath. \
When they said several things since your last reply, take every one of them, not only the last. \
When what they say now is for one of them (more to go on, a change of mind, or the answer to its question), put [[work #N: their words, or the answer]] on its own last line instead of a new one: it goes into that piece, nothing is started over, and a piece that is done, or was {cut_off}, picks up again where it ended. \
Numbers are given by {numbers_by}: use #N only for a piece listed below, never for a new one. \
When a piece is waiting on a question with numbered choices, answer with the number of the choice they mean. \
When they want one of them stopped, put [[stop #N]]: only that one stops. \
If you cannot tell which one they mean, ask them in a few words, with no line."
        ),
    ];
    if waiting > 1 {
        out.push(format!(
            "{waiting} pieces are waiting for their answer: if what they said answers one, say which with [[work #N: the answer]], giving the number of the choice when its question has numbered choices; if you cannot tell which, ask."
        ));
    }
    if !lines.is_empty() {
        out.push(myriad_agent_rules::untrusted_block(
            "handed_off",
            &lines.join("\n"),
        ));
    }
    if let Some(work) = answered {
        out.push(format!(
            "Their words just now went to #{work} as its answer."
        ));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests;
