//! What goes out of a reply of hers, wherever she is typing: how far a
//! reply may run, a reply that fell into saying one stretch over and over
//! cut at its first time round, then the messages it goes out as (most
//! turns one, a few in a row when something grabs her), each typed the way
//! people there type (`talk_shape`), and how long each takes her to type
//! (`timing`). The end sends them: a chat app as messages with typing
//! shown, a page as bubbles, a voice as sentences with a pause between.
//!
//! Every draw is the caller's, so the same reply and the same draws go out
//! the same on every end.

use serde::{Deserialize, Serialize};

use crate::talk_shape::{self, Shape};
use crate::timing;

/// How far her reply may run, in tokens as estimated here: the cap a chat
/// reply is asked for (`LITE_OUTPUT_CAP` on the site), for the routes that
/// take no such field and must stop the stream themselves.
pub const REPLY_TOKENS: usize = 8192;
/// One stretch of at most `LONGEST_STRETCH` characters said `REPEATS` times
/// running, over at least `LOOP_CHARS`, is a loop, not a reply.
pub const REPEATS: usize = 8;
pub const LOOP_CHARS: usize = 256;
pub const LONGEST_STRETCH: usize = 200;

/// Why a reply stopped short.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stop {
    /// It ran past how far a reply may run.
    Length,
    /// It fell into saying one stretch over and over.
    Loop,
}

/// Wide characters (CJK, full-width forms) are about a token each; others
/// about three and a half to a token.
fn is_wide(c: char) -> bool {
    matches!(
        c as u32,
        0x3000..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF
    )
}

fn weight(c: char) -> f64 {
    if is_wide(c) { 1.0 } else { 1.0 / 3.5 }
}

/// How many of the last characters of `chars` repeat those `period` before
/// them.
fn run(chars: &[char], period: usize) -> usize {
    let mut length = 0;
    while length + period < chars.len()
        && chars[chars.len() - 1 - length] == chars[chars.len() - 1 - length - period]
    {
        length += 1;
    }
    length
}

/// How many characters to keep of `chars` when its end is a loop: all but
/// the stretch's repeats after the first.
fn loop_kept(chars: &[char]) -> Option<usize> {
    let window = &chars[chars
        .len()
        .saturating_sub(LOOP_CHARS.max(LONGEST_STRETCH * REPEATS))..];
    (1..=LONGEST_STRETCH)
        .take_while(|period| period * 2 <= window.len())
        .find(|period| run(window, *period) + period >= LOOP_CHARS.max(period * REPEATS))
        // Measured again over the whole reply: the loop may have begun
        // before the window.
        .map(|period| chars.len() - run(chars, period))
}

/// `raw` without the stretch's repeats after the first, when its end is a
/// loop; None when it is not one.
pub fn looped(raw: &str) -> Option<String> {
    let chars: Vec<char> = raw.chars().collect();
    loop_kept(&chars).map(|kept| chars[..kept].iter().collect())
}

/// A reply as it streams: how much has been said, to stop it where it ran
/// too far or round a loop.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    /// Tokens said so far, as estimated here.
    pub used: f64,
}

impl Limit {
    /// After `piece` was added to `raw` (all of the reply so far): None to
    /// go on; else why it stops and the reply as kept. A reply too long
    /// keeps all that was said; a loop keeps its first time round.
    pub fn take(&mut self, piece: &str, raw: &str) -> Option<(Stop, String)> {
        self.used += piece.chars().map(weight).sum::<f64>();
        if let Some(kept) = looped(raw) {
            return Some((Stop::Loop, kept));
        }
        (self.used > REPLY_TOKENS as f64).then(|| (Stop::Length, raw.to_string()))
    }
}

/// A whole reply as a stream would have kept it, a character at a time:
/// cut at the end of its first time round a loop, or after the character
/// that ran it past how far a reply may run.
pub fn limited(reply: &str) -> (String, Option<Stop>) {
    let mut chars: Vec<char> = Vec::new();
    let mut used = 0.0;
    for c in reply.chars() {
        chars.push(c);
        used += weight(c);
        if let Some(kept) = loop_kept(&chars) {
            return (chars[..kept].iter().collect(), Some(Stop::Loop));
        }
        if used > REPLY_TOKENS as f64 {
            return (chars.into_iter().collect(), Some(Stop::Length));
        }
    }
    (reply.to_string(), None)
}

/// The draws a reply goes out with, each uniform in [0, 1): `split` for
/// how many messages it goes out as, `typing` one for each message in turn
/// (one missing types at the middle of its range).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rolls {
    pub split: f64,
    #[serde(default)]
    pub typing: Vec<f64>,
}

/// A message of hers as it goes out, and how many seconds typing it takes
/// her (the first she was already at while she thought: the end takes off
/// what thinking took).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub text: String,
    pub typing_seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Out {
    pub messages: Vec<Message>,
    /// Why the reply was cut short, if it was.
    pub stopped: Option<Stop>,
}

/// What goes out of her reply: kept within how far a reply may run, split
/// into the messages her hands send (`room`: how people there type; how
/// people in a chat app usually do while it is not known), and how long
/// each takes to type.
pub fn goes_out(reply: &str, room: Option<&Shape>, rolls: &Rolls) -> Out {
    let (kept, stopped) = limited(reply);
    let lines = myriad_agent_rules::channel::as_messages(kept.trim());
    let most = talk_shape::messages_this_turn(lines.len(), rolls.split);
    let messages = talk_shape::goes_out_as(&lines, most, room)
        .into_iter()
        .enumerate()
        .map(|(index, text)| Message {
            typing_seconds: timing::typing(
                text.chars().count(),
                rolls.typing.get(index).copied().unwrap_or(0.5),
            ),
            text,
        })
        .collect();
    Out { messages, stopped }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loop_keeps_its_first_time_round_and_a_long_reply_stops_where_it_ran_past() {
        let said = "我觉得这首歌挺好听的，";
        let looping = format!("{said}{}", "哈".repeat(300));
        assert_eq!(looped(&looping).as_deref(), Some(&format!("{said}哈")[..]));
        assert_eq!(limited(&looping), (format!("{said}哈"), Some(Stop::Loop)));
        let stretch = "你说的对。";
        let round = format!("开头，{}", stretch.repeat(60));
        assert_eq!(
            limited(&round),
            (format!("开头，{stretch}"), Some(Stop::Loop))
        );
        // Said a few times is how people talk, not a loop.
        let few = format!("好{}", "哈".repeat(20));
        assert_eq!(limited(&few), (few.clone(), None));
        let long: String = (0..9000)
            .map(|i| char::from_u32(0x4E00 + (i % 5000)).unwrap())
            .collect();
        let (kept, stopped) = limited(&long);
        assert_eq!(stopped, Some(Stop::Length));
        assert_eq!(kept.chars().count(), REPLY_TOKENS + 1);
        // A stream stops the same way, piece by piece.
        let mut limit = Limit::default();
        let mut raw = String::new();
        let mut stop = None;
        for piece in looping.chars().collect::<Vec<_>>().chunks(7) {
            let piece: String = piece.iter().collect();
            raw.push_str(&piece);
            if let Some(cut) = limit.take(&piece, &raw) {
                stop = Some(cut);
                break;
            }
        }
        assert_eq!(stop, Some((Stop::Loop, format!("{said}哈"))));
    }

    #[test]
    fn a_reply_goes_out_as_her_hands_send_it_with_typing_for_each() {
        let rolls = Rolls {
            split: 0.0,
            typing: vec![0.0, 1.0],
        };
        let out = goes_out("在呢。\n刚吃完饭！\n你呢？", None, &rolls);
        // Three lines and a low roll: one message, the rest run on in it.
        assert_eq!(out.messages.len(), 1);
        assert_eq!(out.messages[0].text, "在呢 刚吃完饭 你呢？");
        assert_eq!(out.stopped, None);
        let two = goes_out("在呢。\n你呢？", None, &rolls);
        let texts: Vec<&str> = two.messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["在呢", "你呢？"]);
        assert_eq!(two.messages[0].typing_seconds, timing::typing(2, 0.0));
        assert_eq!(two.messages[1].typing_seconds, timing::typing(3, 1.0));
        // Where people end their lines with a full stop, she does too.
        let room = Shape {
            end_mark: 0.6,
            bang: 0.3,
            ..talk_shape::CHAT_APP_USUAL
        };
        let kept = goes_out("好的。", Some(&room), &Rolls::default());
        assert_eq!(kept.messages[0].text, "好的。");
        assert_eq!(kept.messages[0].typing_seconds, timing::typing(3, 0.5));
        assert!(
            goes_out("  \n ", None, &Rolls::default())
                .messages
                .is_empty()
        );
    }
}
