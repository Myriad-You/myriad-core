//! Whether she answers a line said to her, on every end and in every place:
//! hers to judge, as it is for anyone. Nothing in the code makes her answer
//! (being called by name, a question, a private chat): the turn that would
//! answer is told what usually moves a person to answer ([`GUIDE`]) and may
//! leave it, or only put a reaction on the line.
//!
//! What she does is written in her reply as marks (see `speech::markers`):
//! `[[quiet]]` alone, she read it and leaves it; `[[react:👍]]`, a reaction
//! on the line they wrote, with or without words. Only the mark says she
//! left it: an empty reply is a failure (the model, the network), never a
//! choice of hers.
//!
//! What she did stays in the conversation as she has it next time
//! ([`in_history`]): a turn she left reads as the mark she wrote, `[[quiet]]`,
//! so when they write again she knows she did not answer. The mark, not a
//! narration: a narration on her own line is what she would copy as words.

/// What usually moves a person to answer, and how to leave it: part of the
/// reply instruction everywhere (`chat_history::CHAT_REPLY_INSTRUCTION`,
/// which writes it in).
pub const GUIDE: &str = crate::chat_history::REPLY_GUIDE;

/// The mark for leaving a line, as a host keeps it for her turn.
pub const QUIET: &str = "[[quiet]]";

/// Her reply read for what she did with the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// Her reply without the `quiet` and `react` marks; the other marks
    /// stay for whoever reads them.
    pub said: String,
    /// She read it and leaves it: the mark, and nothing else said or sent.
    pub quiet: bool,
    /// The reaction she puts on their line, one every platform takes.
    pub react: Option<&'static str>,
}

impl Reply {
    /// The mark a host keeps with her turn, so the conversation she has next
    /// time shows what she did: `[[quiet]]`, or `[[react:…]]`; none when
    /// she only said things.
    pub fn kept(&self) -> Option<String> {
        if self.quiet {
            return Some(QUIET.to_string());
        }
        self.react.map(|emoji| format!("[[react:{emoji}]]"))
    }
}

/// `words` with `mark` (see [`Reply::kept`]) on a line of its own after
/// them, or the mark alone: how a turn's words carry what she did with the
/// line past whatever peels marks off her words in between.
pub fn marked(words: &str, mark: Option<&str>) -> String {
    match mark {
        Some(mark) if words.trim().is_empty() => mark.to_string(),
        Some(mark) => format!("{}\n{mark}", words.trim_end()),
        None => words.to_string(),
    }
}

const OPEN: &str = "[[";
const CLOSE: &str = "]]";

/// What `raw` (her reply as the model wrote it) does with the line: leaves
/// it only when `[[quiet]]` is there and nothing is said, sent or reacted
/// with it; a reaction is the first `[[react:…]]` she can put on a line.
pub fn read(raw: &str) -> Reply {
    // Where a mark was taken out, so only a line it emptied goes.
    const GONE: char = '\u{0}';
    let mut said = String::with_capacity(raw.len());
    let mut rest = raw;
    let mut marked_quiet = false;
    let mut took = false;
    let mut react = None;
    while let Some(at) = rest.find(OPEN) {
        let after = &rest[at + OPEN.len()..];
        let Some(end) = after.find(CLOSE) else {
            break;
        };
        let inner = after[..end].trim();
        let (name, value) = match inner.split_once([':', '：']) {
            Some((name, value)) => (name.trim(), Some(value.trim())),
            None => (inner, None),
        };
        if name.eq_ignore_ascii_case("quiet") {
            marked_quiet = true;
        } else if name.eq_ignore_ascii_case("react") {
            if react.is_none() {
                react = value.and_then(crate::joining::reaction);
            }
        } else {
            // Not one of these: past its opening only, as `markers::strip`
            // reads it, so a mark after a stray `[[` is still found.
            said.push_str(&rest[..at + OPEN.len()]);
            rest = after;
            continue;
        }
        said.push_str(&rest[..at]);
        said.push(GONE);
        took = true;
        rest = &after[end + CLOSE.len()..];
    }
    said.push_str(rest);
    let said = if took {
        said.split('\n')
            .filter(|line| !(line.contains(GONE) && line.replace(GONE, "").trim().is_empty()))
            .map(|line| line.replace(GONE, ""))
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string()
    } else {
        said
    };
    // Anything else at all (words, a sticker, a piece of work, music) is
    // something she does with the turn: it is not left.
    let nothing_else = said.trim().is_empty();
    Reply {
        quiet: marked_quiet && nothing_else && react.is_none(),
        said,
        react,
    }
}

/// One turn of hers as stored (directives and all), as it reads in the
/// conversation she has next time, when it was a turn she left or reacted
/// in: the mark she wrote, after her words as they read; none when it was
/// only words, which read as they are.
pub fn in_history(stored: &str) -> Option<String> {
    let reply = read(stored);
    let mark = reply.kept()?;
    let words = crate::chat_history::chat_history_content(&reply.said).replace('\n', " / ");
    Some(if reply.quiet || words.is_empty() {
        mark
    } else {
        format!("{words} {mark}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::joining::REACTIONS;

    #[test]
    fn only_the_mark_with_nothing_else_leaves_it() {
        let left = read("[[quiet]]");
        assert!(left.quiet && left.said.is_empty() && left.react.is_none());
        assert_eq!(left.kept().as_deref(), Some("[[quiet]]"));
        assert!(read("  [[ quiet ]]\n").quiet);
        // Words with it: she answered.
        let said = read("嗯嗯\n[[quiet]]");
        assert!(!said.quiet);
        assert_eq!(said.said, "嗯嗯");
        assert_eq!(said.kept(), None);
        // A sticker with it: she answered with the sticker.
        assert!(!read("[[quiet]]\n[[sticker:2]]").quiet);
        // Nothing at all is no choice of hers.
        assert!(!read("").quiet && !read("   ").quiet);
    }

    #[test]
    fn a_reaction_is_one_every_platform_takes() {
        let reacted = read("[[react:👍]]");
        assert_eq!(reacted.react, Some("👍"));
        assert!(!reacted.quiet && reacted.said.is_empty());
        assert_eq!(reacted.kept().as_deref(), Some("[[react:👍]]"));
        let with_words = read("好诶\n[[react：❤️]]");
        assert_eq!(with_words.react, Some("❤"));
        assert_eq!(with_words.said, "好诶");
        // Not one she can put on a line: no reaction, and nothing left.
        let unknown = read("[[react:🦄]]");
        assert_eq!(unknown.react, None);
        assert!(!unknown.quiet);
        // A reaction with the quiet mark: she reacted.
        let both = read("[[quiet]][[react:😁]]");
        assert!(!both.quiet && both.react == Some("😁"));
    }

    #[test]
    fn other_marks_stay_for_whoever_reads_them() {
        let reply = read("换一身[[wear:冬装]]\n[[react:👀]]");
        assert_eq!(reply.said, "换一身[[wear:冬装]]");
        assert_eq!(read("[[music:play]]").said, "[[music:play]]");
        // Brackets that are no mark are words, and a mark after a stray
        // opening is still found.
        assert_eq!(read("[[笑]]").said, "[[笑]]");
        assert_eq!(read("好[[\n[[quiet]]").said, "好[[");
        assert!(!read("好[[\n[[quiet]]").quiet);
        assert_eq!(read("[[a [[react:👍]]").react, Some("👍"));
        // A piece of work or a stop with the quiet mark: she did something.
        let stop = read("[[quiet]]\n[[stop #1]]");
        assert!(!stop.quiet && stop.said == "[[stop #1]]");
        // Without the marks, her words stay as they were, blank lines and all.
        assert_eq!(read("第一段\n\n第二段").said, "第一段\n\n第二段");
        assert_eq!(
            read("第一段\n\n第二段\n[[react:👍]]").said,
            "第一段\n\n第二段"
        );
    }

    #[test]
    fn the_mark_rides_after_her_words() {
        assert_eq!(marked("", Some("[[quiet]]")), "[[quiet]]");
        assert_eq!(marked("好诶\n", Some("[[react:👍]]")), "好诶\n[[react:👍]]");
        assert_eq!(marked("好诶", None), "好诶");
        let back = read(&marked("好诶", read("[[react:👍]]").kept().as_deref()));
        assert_eq!((back.said.as_str(), back.react), ("好诶", Some("👍")));
    }

    #[test]
    fn what_she_did_reads_in_the_conversation_next_time() {
        assert_eq!(in_history("[[quiet]]").as_deref(), Some("[[quiet]]"));
        assert_eq!(in_history("[[react:👍]]").as_deref(), Some("[[react:👍]]"));
        assert_eq!(
            in_history("好诶\n[[react:❤]]").as_deref(),
            Some("好诶 [[react:❤]]")
        );
        assert_eq!(
            in_history("第一段\n\n第二段\n[[react:👍]]").as_deref(),
            Some("第一段 / 第二段 [[react:👍]]")
        );
        assert_eq!(in_history("只是说话"), None);
    }

    #[test]
    fn the_guide_names_what_usually_moves_a_person_to_answer() {
        let guide = GUIDE;
        assert!(guide.contains("asked something"));
        assert!(guide.contains("hurting or low"));
        assert!(guide.contains("waiting on them"));
        assert!(guide.contains("[[quiet]]") && guide.contains("🤣"));
        assert!(
            guide.contains(&REACTIONS.join(" ")),
            "the reactions she can put on a line"
        );
        assert!(crate::chat_history::CHAT_REPLY_INSTRUCTION.contains(GUIDE));
    }
}
