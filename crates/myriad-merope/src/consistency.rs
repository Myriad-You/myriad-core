//! Whether she is the same person wherever she talks, measured. Two ends
//! count as one her only when both hold: the same scene gives the same whole
//! prompt on each (`prompt_diff`), and her real conversations, played on
//! each, read alike by the numbers in `measure` (`apart`): each end answers
//! them `RUNS_COMPARED` times and the runs are taken together
//! (`measure_runs`), since one draw of a reply is not her. A short judgment
//! of whether she sounds like a person is no evidence either way; these are
//! counts made in code, the same on every end.
//!
//! Only counts and short phrases come out, never a message of hers.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::chat_history::chat_safe_content;
use crate::vitals;

/// One turn of hers: what she sent back to one thing said to her.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Turn {
    /// Her messages in the turn, as stored. Each line goes as its own
    /// message (`CHAT_REPLY_INSTRUCTION`), so a stored message of several
    /// lines counts as that many; the directives riding on it do not count.
    pub said: Vec<String>,
    /// What she was answering, when known: their lines of the turn, joined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answering: Option<String>,
    /// Whether she was greeting them back after a pause marked in the talk.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub opens: bool,
}

/// How a count spreads over what was counted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spread {
    pub mean: f64,
    pub p50: f64,
    pub p90: f64,
    pub max: f64,
}

/// A phrase she leans on, and the share of turns (or greetings) it is in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Phrase {
    pub phrase: String,
    pub share: f64,
}

/// AI-flavored writing found, and in how many of her messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Hit {
    pub kind: String,
    pub term: String,
    pub messages: u32,
}

/// Her replies in one place, by the numbers. Shares are of turns or
/// messages, 0..1; a measure is `None` when nothing it needs was there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Measures {
    /// Times the same turns were answered and measured together
    /// (`measure_runs`); 1 for one answer each.
    #[serde(default = "one_run")]
    pub runs: u32,
    /// Turns in all the runs.
    pub turns: u32,
    pub messages: u32,
    /// Messages in a turn.
    pub per_turn: Option<Spread>,
    /// Characters in a message, spaces not counted.
    pub chars: Option<Spread>,
    /// Her characters in a turn over theirs in what she answered, the median.
    pub length_ratio: Option<f64>,
    /// Turns ending by asking them something.
    pub asking: Option<f64>,
    /// Phrases in many of her turns, and the turns with any of them.
    pub habits: Vec<Phrase>,
    pub habit_turns: Option<f64>,
    /// Her greetings after a pause, the phrases in many of them, and the
    /// greetings that reuse one of those phrases or an earlier first line.
    pub openers: u32,
    pub opener_phrases: Vec<Phrase>,
    pub opener_reuse: Option<f64>,
    /// Messages saying again, word for word, one she sent before.
    pub repeated: Option<f64>,
    /// Turns mostly saying her turn before again.
    pub echoed: Option<f64>,
    /// Messages with AI-flavored writing, and what was found.
    pub ai_flavored: Option<f64>,
    pub ai_hits: Vec<Hit>,
    /// Stored messages with a blank line in them, and messages so long they
    /// read as a block: both what the reply instruction rules out.
    pub blank_lines: u32,
    pub long_messages: u32,
}

fn one_run() -> u32 {
    1
}

/// The writing `CHAT_REPLY_INSTRUCTION` rules out as AI-flavored, by kind:
/// talking as an assistant, rounded-off reasoning, scripted comfort, and the
/// model's own thinking or markup left in the reply (`leak`). Matched
/// as written, Latin letters in any case. Actions in brackets or asterisks
/// (`stage`) are found by their shape, not from this list.
pub const AI_FLAVORED: &[(&str, &[&str])] = &[
    (
        "assistant",
        &[
            "作为一个ai",
            "作为ai",
            "作为一个人工智能",
            "作为人工智能",
            "我只是一个ai",
            "我是一个ai",
            "语言模型",
            "ai助手",
            "人工智能助手",
            "有什么可以帮",
            "有什么我可以帮",
            "有什么需要帮",
            "还有什么需要",
            "还有其他问题",
            "如果你还有其他",
            "希望对你有帮助",
            "希望对你有所帮助",
            "希望能帮到你",
            "希望这能帮到你",
            "希望这对你有帮助",
            "随时告诉我",
            "随时找我",
            "随时问我",
            "很高兴为你",
            "乐意为你",
            "为您服务",
            "as an ai",
            "language model",
            "hope this helps",
            "feel free to",
            "let me know if",
            "happy to help",
            "how can i help",
            "how can i assist",
            "anything else i can",
        ],
    ),
    (
        "rounded",
        &[
            "总的来说",
            "总而言之",
            "综上所述",
            "总结一下",
            "简而言之",
            "值得注意的是",
            "需要注意的是",
            "值得一提的是",
            "另一方面",
            "不仅如此",
            "首先，",
            "其次，",
            "此外，",
            "in conclusion",
            "in summary",
            "overall,",
            "it's worth noting",
            "on the other hand",
        ],
    ),
    (
        "comfort",
        &[
            "我理解你的感受",
            "我能理解你的感受",
            "我能感受到你",
            "你的感受是",
            "完全可以理解",
            "你并不孤单",
            "给你一个拥抱",
            "i understand how you feel",
            "your feelings are valid",
            "completely understandable",
        ],
    ),
    (
        "leak",
        &[
            "the user",
            "the question is",
            "wait, i need",
            "i need to reply",
            "i should reply",
            "i should say",
            "let me write",
            "let me rewrite",
            "<br>",
            "用户说",
            "用户问",
        ],
    ),
];

/// A message this long (characters, spaces not counted) reads as a block.
pub const LONG_MESSAGE: usize = 100;
/// A message needs this many letters before saying it again counts as a
/// repeat: 嗯, 哈哈 and 好呀 are said again by anyone.
pub const REPEAT_AT_LEAST: usize = 4;
/// A turn whose letter pairs are this much in her turn before says it again.
pub const ECHO_AT: f64 = 0.6;
const ECHO_PAIRS_AT_LEAST: usize = 4;
/// A phrase in this share of her turns, or of her greetings, is leaned on;
/// at most this many are named.
pub const HABIT_SHARE: f64 = vitals::LEANS_TOO_MUCH;
pub const OPENER_SHARE: f64 = 0.5;
const PHRASES: usize = 5;
/// A bracketed aside this short at the edge of a message is an action.
const STAGE_CHARS: usize = 12;

/// Her turns in one place, by the numbers. Each message is counted after
/// `chat_safe_content`, one per line.
pub fn measure(turns: &[Turn]) -> Measures {
    let said: Vec<Vec<String>> = turns.iter().map(|turn| messages_of(&turn.said)).collect();
    let messages: Vec<&str> = said.iter().flatten().map(String::as_str).collect();
    let texts: Vec<String> = said.iter().map(|lines| lines.join("\n")).collect();

    let per_turn = spread(
        &said
            .iter()
            .map(|lines| lines.len() as f64)
            .collect::<Vec<_>>(),
    );
    let chars = spread(
        &messages
            .iter()
            .map(|message| letters_and_marks(message) as f64)
            .collect::<Vec<_>>(),
    );
    let ratios: Vec<f64> = turns
        .iter()
        .zip(&texts)
        .filter_map(|(turn, hers)| {
            let theirs = letters_and_marks(turn.answering.as_deref()?);
            (theirs > 0 && !hers.is_empty()).then(|| letters_and_marks(hers) as f64 / theirs as f64)
        })
        .collect();
    let spoken: Vec<&str> = texts
        .iter()
        .map(String::as_str)
        .filter(|text| !text.is_empty())
        .collect();
    let asking = share(
        spoken
            .iter()
            .filter(|text| vitals::ends_asking(text))
            .count(),
        spoken.len(),
    );

    let habits = phrases(&spoken, HABIT_SHARE);
    let habit_turns = share(
        spoken
            .iter()
            .filter(|text| habits.iter().any(|habit| text.contains(&habit.phrase)))
            .count(),
        spoken.len(),
    );

    let greetings: Vec<&str> = turns
        .iter()
        .zip(&said)
        .filter(|(turn, lines)| turn.opens && !lines.is_empty())
        .map(|(_, lines)| lines[0].as_str())
        .collect();
    let opener_phrases = phrases(&greetings, OPENER_SHARE);
    let mut first_lines = HashSet::new();
    let reused = greetings
        .iter()
        .filter(|line| {
            let again = !first_lines.insert(normalized(line));
            again
                || opener_phrases
                    .iter()
                    .any(|phrase| line.contains(&phrase.phrase))
        })
        .count();

    let mut earlier = HashSet::new();
    let mut checked = 0;
    let mut repeated = 0;
    for message in &messages {
        let key = normalized(message);
        if key.chars().count() < REPEAT_AT_LEAST {
            continue;
        }
        checked += 1;
        if !earlier.insert(key) {
            repeated += 1;
        }
    }

    let mut followed = 0;
    let mut echoed = 0;
    for pair in texts.windows(2) {
        let now = letter_pairs(&pair[1]);
        if now.len() < ECHO_PAIRS_AT_LEAST {
            continue;
        }
        followed += 1;
        let before = letter_pairs(&pair[0]);
        if now.intersection(&before).count() as f64 >= ECHO_AT * now.len() as f64 {
            echoed += 1;
        }
    }

    let mut found: BTreeMap<(&str, String), u32> = BTreeMap::new();
    let mut flavored = 0;
    for message in &messages {
        let hits = flavor(message);
        flavored += usize::from(!hits.is_empty());
        for hit in hits {
            *found.entry(hit).or_default() += 1;
        }
    }
    let mut ai_hits: Vec<Hit> = found
        .into_iter()
        .map(|((kind, term), messages)| Hit {
            kind: kind.to_string(),
            term,
            messages,
        })
        .collect();
    ai_hits.sort_by_key(|hit| std::cmp::Reverse(hit.messages));

    Measures {
        runs: 1,
        turns: turns.len() as u32,
        messages: messages.len() as u32,
        per_turn,
        chars,
        length_ratio: vitals::quantile(&ratios, 0.5).map(rounded),
        asking,
        habits,
        habit_turns,
        openers: greetings.len() as u32,
        opener_phrases,
        opener_reuse: share(reused, greetings.len()),
        repeated: share(repeated, checked),
        echoed: share(echoed, followed),
        ai_flavored: share(flavored, messages.len()),
        ai_hits,
        blank_lines: turns
            .iter()
            .flat_map(|turn| &turn.said)
            .filter(|stored| chat_safe_content(stored).contains("\n\n"))
            .count() as u32,
        long_messages: messages
            .iter()
            .filter(|message| letters_and_marks(message) > LONG_MESSAGE)
            .count() as u32,
    }
}

/// The same turns answered more than once on one end, by the numbers: each
/// run measured on its own, then taken together. Shares, ratios and spreads
/// are the runs' mean (the max the largest), counts their sum, phrases and
/// hits gathered across them with their shares averaged; so a greeting or a
/// line said again in another run is not counted as said again. No runs is
/// `measure(&[])`.
pub fn measure_runs(runs: &[Vec<Turn>]) -> Measures {
    let each: Vec<Measures> = runs.iter().map(|turns| measure(turns)).collect();
    let Some(first) = each.first() else {
        return measure(&[]);
    };
    if each.len() == 1 {
        return first.clone();
    }
    let sum = |of: fn(&Measures) -> u32| each.iter().map(of).sum::<u32>();
    let mean = |of: fn(&Measures) -> Option<f64>| {
        let values: Vec<f64> = each.iter().filter_map(of).collect();
        (!values.is_empty()).then(|| rounded(values.iter().sum::<f64>() / values.len() as f64))
    };
    let spreads = |of: fn(&Measures) -> Option<Spread>| {
        let values: Vec<Spread> = each.iter().filter_map(of).collect();
        let n = values.len() as f64;
        (!values.is_empty()).then(|| Spread {
            mean: rounded(values.iter().map(|spread| spread.mean).sum::<f64>() / n),
            p50: rounded(values.iter().map(|spread| spread.p50).sum::<f64>() / n),
            p90: rounded(values.iter().map(|spread| spread.p90).sum::<f64>() / n),
            max: values
                .iter()
                .map(|spread| spread.max)
                .fold(f64::MIN, f64::max),
        })
    };
    let gathered = |of: fn(&Measures) -> &Vec<Phrase>| {
        let mut shares: BTreeMap<&str, f64> = BTreeMap::new();
        for measures in &each {
            for phrase in of(measures) {
                *shares.entry(phrase.phrase.as_str()).or_default() += phrase.share;
            }
        }
        let mut phrases: Vec<Phrase> = shares
            .into_iter()
            .map(|(phrase, share)| Phrase {
                phrase: phrase.to_string(),
                share: rounded(share / each.len() as f64),
            })
            .collect();
        phrases.sort_by(|a, b| b.share.total_cmp(&a.share));
        phrases.truncate(PHRASES);
        phrases
    };
    let mut hits: BTreeMap<(&str, &str), u32> = BTreeMap::new();
    for measures in &each {
        for hit in &measures.ai_hits {
            *hits
                .entry((hit.kind.as_str(), hit.term.as_str()))
                .or_default() += hit.messages;
        }
    }
    Measures {
        runs: each.len() as u32,
        turns: sum(|m| m.turns),
        messages: sum(|m| m.messages),
        per_turn: spreads(|m| m.per_turn),
        chars: spreads(|m| m.chars),
        length_ratio: mean(|m| m.length_ratio),
        asking: mean(|m| m.asking),
        habits: gathered(|m| &m.habits),
        habit_turns: mean(|m| m.habit_turns),
        openers: sum(|m| m.openers),
        opener_phrases: gathered(|m| &m.opener_phrases),
        opener_reuse: mean(|m| m.opener_reuse),
        repeated: mean(|m| m.repeated),
        echoed: mean(|m| m.echoed),
        ai_flavored: mean(|m| m.ai_flavored),
        ai_hits: hits
            .into_iter()
            .map(|((kind, term), messages)| Hit {
                kind: kind.to_string(),
                term: term.to_string(),
                messages,
            })
            .collect(),
        blank_lines: sum(|m| m.blank_lines),
        long_messages: sum(|m| m.long_messages),
    }
}

/// Her messages in a turn: each stored message after `chat_safe_content`,
/// one per line, blank lines dropped.
fn messages_of(stored: &[String]) -> Vec<String> {
    stored
        .iter()
        .flat_map(|message| {
            chat_safe_content(message)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn letters_and_marks(text: &str) -> usize {
    text.chars().filter(|ch| !ch.is_whitespace()).count()
}

/// The letters and digits of a message, lower case: what is said, not how
/// it was punctuated.
fn normalized(message: &str) -> String {
    message
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Neighbouring letters, within runs of letters and digits.
fn letter_pairs(text: &str) -> HashSet<(char, char)> {
    let mut pairs = HashSet::new();
    let mut last: Option<char> = None;
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            if let Some(previous) = last {
                pairs.insert((previous, ch));
            }
            last = Some(ch);
        } else {
            last = None;
        }
    }
    pairs
}

fn phrases(texts: &[&str], at: f64) -> Vec<Phrase> {
    vitals::leaned_on(texts, at, PHRASES)
        .into_iter()
        .map(|(phrase, share)| Phrase {
            phrase,
            share: rounded(share),
        })
        .collect()
}

/// What in one message is AI-flavored, each kind and term once.
fn flavor(message: &str) -> Vec<(&'static str, String)> {
    let lower: String = message.chars().flat_map(char::to_lowercase).collect();
    let mut hits: Vec<(&'static str, String)> = AI_FLAVORED
        .iter()
        .flat_map(|(kind, terms)| {
            terms
                .iter()
                .filter(|term| lower.contains(*term))
                .map(|term| (*kind, term.to_string()))
        })
        .collect();
    if let Some(shape) = stage_shape(message) {
        hits.push(("stage", shape.to_string()));
    }
    hits
}

/// An action written into the line: `*…*`, or a short aside in brackets
/// that is the whole message or stands at its start or end, as in （笑）.
fn stage_shape(message: &str) -> Option<&'static str> {
    let message = message.trim();
    let starred = message.split('*').count() >= 3
        && message
            .split('*')
            .skip(1)
            .step_by(2)
            .any(|inside| (1..=30).contains(&inside.chars().count()));
    if starred {
        return Some("*…*");
    }
    for (open, close) in [('（', '）'), ('(', ')')] {
        let aside = |inside: &str| {
            let count = inside.chars().count();
            (1..=STAGE_CHARS).contains(&count) && !inside.chars().any(|ch| ch.is_ascii_digit())
        };
        if let Some(rest) = message.strip_prefix(open)
            && let Some(end) = rest.find(close)
            && aside(&rest[..end])
        {
            return Some("（…）");
        }
        if let Some(rest) = message.strip_suffix(close)
            && let Some(start) = rest.rfind(open)
            && aside(&rest[start + open.len_utf8()..])
        {
            return Some("（…）");
        }
    }
    None
}

fn share(count: usize, of: usize) -> Option<f64> {
    (of > 0).then(|| rounded(count as f64 / of as f64))
}

fn spread(values: &[f64]) -> Option<Spread> {
    let max = values.iter().copied().reduce(f64::max)?;
    Some(Spread {
        mean: rounded(values.iter().sum::<f64>() / values.len() as f64),
        p50: vitals::quantile(values, 0.5)?,
        p90: vitals::quantile(values, 0.9)?,
        max,
    })
}

/// Three places: what a measure can tell, and the same bits on every end.
fn rounded(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Two measures of her too far apart to call one way of talking.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Apart {
    pub measure: String,
    pub left: Option<f64>,
    pub right: Option<f64>,
}

/// Turns on each side, in a run, before two measures are worth comparing.
pub const COMPARED_AFTER: u32 = 20;

/// Times each end answers the same turns before two ends are compared. A
/// reply is drawn: answering the same 40 prompts again moved one end's
/// `asking` by up to 12 points (2026-11-05), more than its tolerance, while
/// three runs taken together kept within it.
pub const RUNS_COMPARED: u32 = 3;

/// How far apart a measure may be on two ends and still read as the same
/// way of talking: at least `floor`, or `relative` of the larger one; for
/// `RUNS_COMPARED` runs taken together. Starting values, to be tightened
/// once two ends have been put side by side often enough to know their
/// noise. `asking` was 0.1: with the same 40 real turns answered three
/// times on each of two ends (2026-11-05), one run's `asking` sways about 0.067 (sd)
/// on an end, a binomial share's own sway at that size; three runs together
/// sway 0.039, and two ends' gap 0.054, so 0.1 is 1.8 of those and would
/// call one comparison in 15 apart on chance alone. 0.12 is 2.2 (about one
/// in 35) and still finds the 0.15 two ends were apart before the private
/// chat was put together alike.
const TOLERANCE: &[(&str, f64, f64)] = &[
    ("perTurn.mean", 0.5, 0.25),
    ("perTurn.p90", 1.0, 0.25),
    ("chars.p50", 3.0, 0.3),
    ("chars.p90", 6.0, 0.3),
    ("lengthRatio", 0.5, 0.3),
    ("asking", 0.12, 0.0),
    ("habitTurns", 0.15, 0.0),
    ("openerReuse", 0.2, 0.0),
    ("repeated", 0.05, 0.0),
    ("echoed", 0.05, 0.0),
    ("aiFlavored", 0.05, 0.0),
];

/// The measures in which two ends' replay of the same conversations are too
/// far apart, in the order of `TOLERANCE`; `turns` first when either side
/// has fewer than `COMPARED_AFTER` turns a run, and then nothing else. A
/// measure one side has and the other does not is apart. Either side
/// measured from fewer than `RUNS_COMPARED` runs puts `runs` first: the
/// rest is listed for reference, and the two are not called alike.
pub fn apart(left: &Measures, right: &Measures) -> Vec<Apart> {
    let per_run = |measures: &Measures| measures.turns / measures.runs.max(1);
    if per_run(left) < COMPARED_AFTER || per_run(right) < COMPARED_AFTER {
        return vec![Apart {
            measure: "turns".to_string(),
            left: Some(f64::from(per_run(left))),
            right: Some(f64::from(per_run(right))),
        }];
    }
    let runs = (left.runs < RUNS_COMPARED || right.runs < RUNS_COMPARED).then(|| Apart {
        measure: "runs".to_string(),
        left: Some(f64::from(left.runs)),
        right: Some(f64::from(right.runs)),
    });
    runs.into_iter()
        .chain(TOLERANCE.iter().filter_map(|(measure, floor, relative)| {
            let (a, b) = (value_of(left, measure), value_of(right, measure));
            let far = match (a, b) {
                (Some(a), Some(b)) => (a - b).abs() > floor.max(relative * a.abs().max(b.abs())),
                (None, None) => false,
                _ => true,
            };
            far.then(|| Apart {
                measure: measure.to_string(),
                left: a,
                right: b,
            })
        }))
        .collect()
}

fn value_of(measures: &Measures, name: &str) -> Option<f64> {
    match name {
        "perTurn.mean" => measures.per_turn.map(|spread| spread.mean),
        "perTurn.p90" => measures.per_turn.map(|spread| spread.p90),
        "chars.p50" => measures.chars.map(|spread| spread.p50),
        "chars.p90" => measures.chars.map(|spread| spread.p90),
        "lengthRatio" => measures.length_ratio,
        "asking" => measures.asking,
        "habitTurns" => measures.habit_turns,
        "openerReuse" => measures.opener_reuse,
        "repeated" => measures.repeated,
        "echoed" => measures.echoed,
        "aiFlavored" => measures.ai_flavored,
        other => unreachable!("no measure {other}"),
    }
}

/// The whole prompt one end put together for one scene, as it went to the
/// model, and the scene it was given. Two ends handed the same `scene` and
/// `input` should send the same prompt; `prompt_diff` lists where not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptRecording {
    /// The shape of this record: `PROMPT_RECORDING_VERSION`.
    pub version: u32,
    /// Which scene, named the same on every end: `private/greeting-after-days`.
    pub scene: String,
    /// Which end put it together: `site`, `native`, …
    pub end: String,
    /// The core commit that end was pinned to, when it knows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub core: Option<String>,
    /// What the scene gave the end: who she is, the hour and her zone, how
    /// she is, the talk so far, the latest line. Free in shape; compared as
    /// JSON.
    pub input: serde_json::Value,
    /// The messages that went to the model, in order.
    pub messages: Vec<PromptMessage>,
}

pub const PROMPT_RECORDING_VERSION: u32 = 1;

/// Whether a recording is in the shape this core reads: another version is
/// refused, not compared.
pub fn readable(recording: &PromptRecording) -> bool {
    recording.version == PROMPT_RECORDING_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptMessage {
    /// `system`, `user`, `assistant`.
    pub role: String,
    pub text: String,
}

/// A piece of a prompt: a paragraph, or a fenced block whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Segment {
    pub role: String,
    /// What it is: a section's heading, a fenced block's tag, or how it
    /// starts.
    pub key: String,
    pub text: String,
}

/// How a key starts when the piece has no heading.
const KEY_CHARS: usize = 24;

/// A prompt cut where its sections are joined: at blank lines and before a
/// `## ` heading (one end joins sections with a blank line, another with a
/// line break), keeping a fenced block (`<untrusted_…>` … `</untrusted_…>`)
/// whole.
pub fn segments(messages: &[PromptMessage]) -> Vec<Segment> {
    let mut out = Vec::new();
    for message in messages {
        let mut open: Option<String> = None;
        let mut pending: Vec<&str> = Vec::new();
        let mut flush = |pending: &mut Vec<&str>| {
            let text = pending.join("\n");
            pending.clear();
            if !text.trim().is_empty() {
                out.push(Segment {
                    role: message.role.clone(),
                    key: key_of(&text),
                    text,
                });
            }
        };
        for line in message.text.split('\n') {
            if open.is_none() {
                if line.trim().is_empty() {
                    flush(&mut pending);
                    continue;
                }
                if line.starts_with("## ") {
                    flush(&mut pending);
                }
            }
            pending.push(line);
            let line = line.trim();
            if let Some(tag) = line
                .strip_prefix("<untrusted_")
                .and_then(|rest| rest.strip_suffix('>'))
            {
                open = Some(tag.to_string());
            } else if open
                .as_ref()
                .is_some_and(|tag| line == format!("</untrusted_{tag}>"))
            {
                open = None;
            }
        }
        flush(&mut pending);
    }
    out
}

fn key_of(text: &str) -> String {
    let text = text.trim_start();
    let first = text.lines().next().unwrap_or_default().trim();
    if let Some(heading) = first.strip_prefix("## ") {
        return heading.trim().to_string();
    }
    if let Some(at) = text.find("<untrusted_")
        && let Some(tag) = text[at + "<untrusted_".len()..].split('>').next()
        && text[..at].lines().count() <= 1
    {
        let fence = format!("<untrusted_{tag}>");
        let lead = text[..at].trim();
        return if lead.is_empty() {
            fence
        } else {
            format!("{} {fence}", cut(lead))
        };
    }
    cut(first)
}

/// How a line starts, marked when it goes on.
fn cut(line: &str) -> String {
    let lead: String = line.chars().take(KEY_CHARS).collect();
    if line.chars().count() > KEY_CHARS {
        format!("{lead}…")
    } else {
        lead
    }
}

/// How one piece differs between two ends' prompts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SegmentDiff {
    /// `changed` (the same piece, other words), `role` (the same words in
    /// another role: `role` reads `left → right`), `left` or `right` (only
    /// on that side).
    pub kind: String,
    pub role: String,
    pub key: String,
    /// Where it is among each side's segments, counting from 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_at: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_at: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<String>,
}

/// Two ends' prompts for one scene, set side by side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptDiff {
    /// Whether both were the same scene given the same input: if not, a
    /// difference in the prompt says nothing about the ends.
    pub same_scene: bool,
    pub same_input: bool,
    /// Pieces word for word the same on both, and the ones that are not, in
    /// the order they stand.
    pub same: u32,
    pub differ: Vec<SegmentDiff>,
}

/// Where two ends' whole prompts for one scene differ, piece by piece. The
/// pieces are lined up by the longest run of words they share in order, so
/// an end that sends the talk as messages of their own and one that writes
/// it into one text still line up; the same words in another role are told
/// (`role`). Between two pieces both have, one with the same key on each
/// side is the same piece changed, the rest are only on one side.
pub fn prompt_diff(left: &PromptRecording, right: &PromptRecording) -> PromptDiff {
    let a = segments(&left.messages);
    let b = segments(&right.messages);
    let pairs = common(&a, &b);
    let mut differ = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut same = 0;
    for (next_i, next_j) in pairs
        .iter()
        .copied()
        .chain(std::iter::once((a.len(), b.len())))
    {
        between(&a, i..next_i, &b, j..next_j, &mut differ);
        if let (Some(x), Some(y)) = (a.get(next_i), b.get(next_j)) {
            if x.role == y.role {
                same += 1;
            } else {
                differ.push(SegmentDiff {
                    kind: "role".to_string(),
                    role: format!("{} → {}", x.role, y.role),
                    key: x.key.clone(),
                    left_at: Some(next_i as u32),
                    right_at: Some(next_j as u32),
                    left: None,
                    right: None,
                });
            }
        }
        i = next_i + 1;
        j = next_j + 1;
    }
    PromptDiff {
        same_scene: left.scene == right.scene,
        same_input: left.input == right.input,
        same,
        differ,
    }
}

/// The longest run of pieces both have in order, by their words alone:
/// (left, right) positions.
fn common(a: &[Segment], b: &[Segment]) -> Vec<(usize, usize)> {
    let same = |x: &Segment, y: &Segment| x.text == y.text;
    let mut longest = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            longest[i][j] = if same(&a[i], &b[j]) {
                longest[i + 1][j + 1] + 1
            } else {
                longest[i + 1][j].max(longest[i][j + 1])
            };
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if same(&a[i], &b[j]) {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if longest[i + 1][j] >= longest[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

fn between(
    a: &[Segment],
    left: std::ops::Range<usize>,
    b: &[Segment],
    right: std::ops::Range<usize>,
    differ: &mut Vec<SegmentDiff>,
) {
    let mut taken = vec![false; right.len()];
    for i in left {
        let found = right
            .clone()
            .position(|j| !taken[j - right.start] && a[i].key == b[j].key);
        match found {
            Some(at) => {
                taken[at] = true;
                let j = right.start + at;
                differ.push(SegmentDiff {
                    kind: "changed".to_string(),
                    role: if a[i].role == b[j].role {
                        a[i].role.clone()
                    } else {
                        format!("{} → {}", a[i].role, b[j].role)
                    },
                    key: a[i].key.clone(),
                    left_at: Some(i as u32),
                    right_at: Some(j as u32),
                    left: Some(a[i].text.clone()),
                    right: Some(b[j].text.clone()),
                });
            }
            None => differ.push(SegmentDiff {
                kind: "left".to_string(),
                role: a[i].role.clone(),
                key: a[i].key.clone(),
                left_at: Some(i as u32),
                right_at: None,
                left: Some(a[i].text.clone()),
                right: None,
            }),
        }
    }
    for (at, j) in right.enumerate() {
        if !taken[at] {
            differ.push(SegmentDiff {
                kind: "right".to_string(),
                role: b[j].role.clone(),
                key: b[j].key.clone(),
                left_at: None,
                right_at: Some(j as u32),
                left: None,
                right: Some(b[j].text.clone()),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(said: &[&str]) -> Turn {
        Turn {
            said: said.iter().map(|line| line.to_string()).collect(),
            ..Turn::default()
        }
    }

    #[test]
    fn lines_of_a_stored_reply_count_as_messages_and_directives_do_not() {
        let measures = measure(&[
            turn(&["早呀\n今天好冷\n[[wear:coat]]"]),
            turn(&["嗯", "你呢？"]),
        ]);
        assert_eq!(measures.turns, 2);
        assert_eq!(measures.messages, 4);
        let per_turn = measures.per_turn.unwrap();
        assert_eq!((per_turn.mean, per_turn.max), (2.0, 2.0));
        assert_eq!(measures.asking, Some(0.5));
    }

    #[test]
    fn nothing_said_measures_nothing() {
        let measures = measure(&[]);
        assert_eq!(measures.per_turn, None);
        assert_eq!(measures.asking, None);
        assert_eq!(measures.ai_flavored, None);
        assert!(measures.habits.is_empty());
    }

    #[test]
    fn saying_a_line_again_word_for_word_is_a_repeat_and_short_ones_are_not() {
        let measures = measure(&[
            turn(&["今天也好困啊", "嗯"]),
            turn(&["今天也好困啊！", "嗯"]),
            turn(&["去吃饭了"]),
        ]);
        // 嗯 is too short to count; the second 今天也好困啊 is the repeat.
        assert_eq!(measures.repeated, Some(0.333));
    }

    #[test]
    fn a_turn_saying_the_one_before_again_is_an_echo() {
        let measures = measure(&[
            turn(&["我刚刚在看那部电影"]),
            turn(&["刚刚在看那部电影呢"]),
            turn(&["明天要早起"]),
        ]);
        assert_eq!(measures.echoed, Some(0.5));
    }

    #[test]
    fn assistant_talk_and_actions_in_brackets_are_ai_flavored() {
        let measures = measure(&[
            turn(&["希望对你有帮助！"]),
            turn(&["（歪头）真的吗"]),
            turn(&["*伸了个懒腰* 好困"]),
            turn(&["As an AI, I can't"]),
            turn(&["我等了(大概)三个小时(其实是2个)"]),
            turn(&["猫跑哪去了\n\nSo the question is where the cat went."]),
            turn(&["好"]),
        ]);
        assert_eq!(measures.ai_flavored, Some(0.625));
        let terms: Vec<(&str, &str)> = measures
            .ai_hits
            .iter()
            .map(|hit| (hit.kind.as_str(), hit.term.as_str()))
            .collect();
        assert_eq!(
            terms,
            [
                ("assistant", "as an ai"),
                ("assistant", "希望对你有帮助"),
                ("leak", "the question is"),
                ("stage", "*…*"),
                ("stage", "（…）"),
            ]
        );
    }

    #[test]
    fn greetings_reusing_words_are_counted_and_only_greetings() {
        let mut turns: Vec<Turn> = [
            "回来啦！今天怎么样",
            "回来啦～",
            "回来啦，吃了吗",
            "哦你来了",
        ]
        .iter()
        .map(|line| Turn {
            opens: true,
            ..turn(&[line])
        })
        .collect();
        turns.push(turn(&["回来啦"]));
        let measures = measure(&turns);
        assert_eq!(measures.openers, 4);
        assert_eq!(measures.opener_phrases[0].phrase, "回来啦");
        assert_eq!(measures.opener_reuse, Some(0.75));
    }

    #[test]
    fn length_is_against_what_she_answered() {
        let measures = measure(&[
            Turn {
                answering: Some("在干嘛".to_string()),
                ..turn(&["在看书，你呢"])
            },
            Turn {
                answering: Some("今天去爬山了，累死".to_string()),
                ..turn(&["好耶"])
            },
        ]);
        assert_eq!(measures.length_ratio, Some(2.0));
    }

    #[test]
    fn a_blank_line_and_a_block_are_counted() {
        let long = "长".repeat(LONG_MESSAGE + 1);
        let measures = measure(&[turn(&["第一段\n\n第二段"]), turn(&[&long])]);
        assert_eq!(measures.blank_lines, 1);
        assert_eq!(measures.long_messages, 1);
        assert_eq!(measures.messages, 3);
    }

    /// `turns` of the same line, answered `RUNS_COMPARED` times.
    fn many(said: &str, turns: u32) -> Measures {
        let run: Vec<Turn> = (0..turns).map(|_| turn(&[said])).collect();
        measure_runs(&vec![run; RUNS_COMPARED as usize])
    }

    #[test]
    fn runs_are_measured_each_on_its_own_and_taken_together() {
        let greeting = |said: &str| Turn {
            said: vec![said.to_string()],
            answering: None,
            opens: true,
        };
        // Each run greets three times, in words of its own each time, the
        // same words in every run; one run asks in its last turn.
        let run = |last: &str| {
            vec![
                greeting("你好呀今天下雨了"),
                greeting("早上好你醒得真早"),
                greeting("晚上见我先去吃饭"),
                turn(&[last]),
            ]
        };
        let runs = [
            run("你在做什么呢？"),
            run("我在看书呢。"),
            run("我在看书呢。"),
        ];
        let together = measure_runs(&runs);
        assert_eq!(
            (together.runs, together.turns, together.openers),
            (3, 12, 9)
        );
        assert_eq!(together.asking, Some(0.083));
        // A greeting or a line again in another run is not said again.
        assert_eq!(together.opener_reuse, Some(0.0));
        assert_eq!(together.repeated, Some(0.0));
        let one_long_run = measure(&runs.concat());
        assert!(one_long_run.opener_reuse > Some(0.5));
        assert!(one_long_run.repeated > Some(0.5));
        assert_eq!(measure_runs(&[]), measure(&[]));
    }

    #[test]
    fn one_run_is_listed_for_reference_but_not_called_alike() {
        let one = |said: &str| measure(&(0..30).map(|_| turn(&[said])).collect::<Vec<_>>());
        let found: Vec<String> = apart(&one("好呀"), &one(&"好呀".repeat(20)))
            .into_iter()
            .map(|apart| apart.measure)
            .collect();
        assert_eq!(
            found,
            ["runs", "chars.p50", "chars.p90", "habitTurns", "repeated"]
        );
        assert_eq!(apart(&one("好呀"), &one("好呀")).len(), 1);
    }

    #[test]
    fn too_few_turns_compare_nothing_but_the_count() {
        let found = apart(&many("好", 5), &many("好", 30));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].measure, "turns");
    }

    #[test]
    fn the_same_talk_is_not_apart_and_a_wordier_one_is() {
        assert_eq!(apart(&many("好呀好呀", 30), &many("好呀好呀", 30)), []);
        let found: Vec<String> = apart(&many("好呀", 30), &many(&"好呀".repeat(20), 30))
            .into_iter()
            .map(|apart| apart.measure)
            .collect();
        // Forty characters a line also lean on 好呀 and repeat whole lines.
        assert_eq!(found, ["chars.p50", "chars.p90", "habitTurns", "repeated"]);
    }

    fn recording(end: &str, system: &str, user: &str) -> PromptRecording {
        PromptRecording {
            version: PROMPT_RECORDING_VERSION,
            scene: "private/greeting".to_string(),
            end: end.to_string(),
            core: None,
            input: serde_json::json!({ "latest": "在吗" }),
            messages: vec![
                PromptMessage {
                    role: "system".to_string(),
                    text: system.to_string(),
                },
                PromptMessage {
                    role: "user".to_string(),
                    text: user.to_string(),
                },
            ],
        }
    }

    #[test]
    fn a_fenced_block_stays_whole_and_keys_name_the_piece() {
        let text = "You are Merope.\n\n## Mood\nCalm.\n\nRecent lines:\n<untrusted_group_transcript>\nnotice\na：hi\n\nb：yo\n</untrusted_group_transcript>\n\nReply as yourself.";
        let keys: Vec<String> = segments(&[PromptMessage {
            role: "system".to_string(),
            text: text.to_string(),
        }])
        .into_iter()
        .map(|segment| segment.key)
        .collect();
        assert_eq!(
            keys,
            [
                "You are Merope.",
                "Mood",
                "Recent lines: <untrusted_group_transcript>",
                "Reply as yourself.",
            ]
        );
    }

    #[test]
    fn a_heading_after_a_line_break_starts_a_piece_but_not_inside_a_fence() {
        let text = "You are Merope.\n## Mood\nCalm.\n## Your day\nQuiet.\n<untrusted_memory>\n## not a section\n</untrusted_memory>";
        let pieces: Vec<(String, String)> = segments(&[PromptMessage {
            role: "system".to_string(),
            text: text.to_string(),
        }])
        .into_iter()
        .map(|segment| (segment.key, segment.text))
        .collect();
        assert_eq!(
            pieces,
            [
                ("You are Merope.".to_string(), "You are Merope.".to_string()),
                ("Mood".to_string(), "## Mood\nCalm.".to_string()),
                (
                    "Your day".to_string(),
                    "## Your day\nQuiet.\n<untrusted_memory>\n## not a section\n</untrusted_memory>"
                        .to_string()
                ),
            ]
        );
    }

    #[test]
    fn the_same_prompt_differs_nowhere() {
        let left = recording("site", "Soul.\n\n## Mood\nCalm.", "在吗");
        let diff = prompt_diff(
            &left,
            &recording("native", "Soul.\n\n## Mood\nCalm.", "在吗"),
        );
        assert!(diff.same_scene && diff.same_input);
        assert_eq!((diff.same, diff.differ.len()), (3, 0));
    }

    #[test]
    fn a_changed_section_a_missing_one_and_an_extra_one_are_listed_in_place() {
        let left = recording(
            "site",
            "Soul.\n\n## Mood\nCalm.\n\n## Habits\nYou ask a lot.\n\nReply as yourself.",
            "在吗",
        );
        let right = recording(
            "native",
            "Soul.\n\n## Mood\nLow.\n\nReply as yourself.\n\n## Now\nNight.",
            "在吗",
        );
        let diff = prompt_diff(&left, &right);
        assert_eq!(diff.same, 3);
        let found: Vec<(&str, &str)> = diff
            .differ
            .iter()
            .map(|item| (item.kind.as_str(), item.key.as_str()))
            .collect();
        assert_eq!(
            found,
            [("changed", "Mood"), ("left", "Habits"), ("right", "Now")]
        );
        assert_eq!(diff.differ[0].right.as_deref(), Some("## Mood\nLow."));
    }

    #[test]
    fn the_same_words_in_another_role_line_up_and_are_told() {
        let left = recording("site", "Soul.\n\nReply as yourself.", "在吗");
        let mut right = recording("native", "Soul.", "在吗");
        right.messages.insert(
            1,
            PromptMessage {
                role: "user".to_string(),
                text: "Reply as yourself.".to_string(),
            },
        );
        let diff = prompt_diff(&left, &right);
        assert_eq!(diff.same, 2);
        assert_eq!(diff.differ.len(), 1);
        assert_eq!(
            (diff.differ[0].kind.as_str(), diff.differ[0].role.as_str()),
            ("role", "system → user")
        );
    }

    #[test]
    fn another_input_is_told() {
        let left = recording("site", "Soul.", "在吗");
        let mut right = recording("native", "Soul.", "在吗");
        right.input = serde_json::json!({ "latest": "在不在" });
        assert!(!prompt_diff(&left, &right).same_input);
    }
}
