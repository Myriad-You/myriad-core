//! What stays on her mind: the one thing she keeps turning over when
//! nothing else has her, where it came from, since when, and how far she has
//! got with it. Without it, when no one is there she only does things; when
//! someone comes and she says she was just thinking of something, she would
//! be making it up.
//!
//! - **What it can be.** Only what is already hers: something of her own
//!   time that stayed with her (a song, a part of her book, a question she
//!   looked into, a note), or something an exchange left her with. It is one
//!   thing; nothing on her mind is the usual.
//! - **When it changes.** When something happens to her: she finishes a
//!   thing of her own, or an exchange leaves her with something. The calls
//!   that already happen then ask her ([`ask`]) whether something of it will
//!   keep turning over in her head, more than what is on her mind now; she
//!   judges, no rule weighs one against the other.
//! - **Thinking it further.** When she is idle with something on her mind,
//!   about every [`STEP_EVERY_MINUTES`] she thinks one step further
//!   ([`step_system`]): where she has got to, whether she is done with it,
//!   whether she would want to tell someone. Nothing on her mind, nothing
//!   to think: no call is made on a clock alone.
//! - **Who may hear of it.** About her own things, anyone. About someone,
//!   only that one: to anyone else it is only that something about someone
//!   else is on her mind ([`OnMind::seen_by`]), as with how her day went.
//!
//! The record names no one: who it is about is an id the host keeps.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The field where something may come to stay on her mind.
pub const FIELD: &str = "onMind";
/// Where what is on her mind now goes in, as she may see it there.
pub const INPUT: &str = "onYourMind";
/// What it is, in her words.
pub const MAX_WHAT_CHARS: usize = 120;
/// Where she has got to with it.
pub const MAX_SO_FAR_CHARS: usize = 200;
/// Idle with something on her mind, she thinks it one step further about
/// this often.
pub const STEP_EVERY_MINUTES: i64 = 30;
/// Where it came from when an exchange left it.
pub const FROM_TALK: &str = "talk";

/// What is on her mind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OnMind {
    /// What it is, in her words.
    pub what: String,
    /// Where she has got to with it; empty until she has thought it on.
    #[serde(default)]
    pub so_far: String,
    /// Who it is about: none for her own things.
    #[serde(default)]
    pub about: Option<i32>,
    /// What it came from: the key of a thing of her own (`song:…`,
    /// `serial:…`, `inquiry:…`), or [`FROM_TALK`].
    pub from: String,
    pub since: DateTime<Utc>,
    /// When she last thought it further (`since` until she has).
    pub thought_at: DateTime<Utc>,
    #[serde(default)]
    pub steps: u32,
}

fn tidy(text: &str, at_most: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(at_most)
        .collect()
}

impl OnMind {
    /// What she said stays with her, from `from`, about `about`: none when
    /// she said nothing.
    pub fn new(what: &str, about: Option<i32>, from: &str, now: DateTime<Utc>) -> Option<Self> {
        let what = tidy(what, MAX_WHAT_CHARS);
        (!what.is_empty() && what != "null").then(|| Self {
            what,
            so_far: String::new(),
            about,
            from: from.to_string(),
            since: now,
            thought_at: now,
            steps: 0,
        })
    }

    /// Whether, idle, it is time she thought it one step further.
    pub fn step_due(&self, now: DateTime<Utc>) -> bool {
        now.signed_duration_since(self.thought_at).num_minutes() >= STEP_EVERY_MINUTES
    }

    /// One step further: where she has got to now.
    pub fn stepped(mut self, thought: &str, now: DateTime<Utc>) -> Self {
        let thought = tidy(thought, MAX_SO_FAR_CHARS);
        if !thought.is_empty() {
            self.so_far = thought;
        }
        self.thought_at = now;
        self.steps += 1;
        self
    }

    /// Whether `who` may hear what it is: her own things anyone, something
    /// about someone only they. `None` is no one in particular (a group, a
    /// look of her own).
    pub fn whole_to(&self, who: Option<i32>) -> bool {
        match self.about {
            None => true,
            Some(about) => who == Some(about),
        }
    }

    /// What is on her mind as `who` may see it there, as a line: all of it,
    /// or only that something about someone else is.
    pub fn seen_by(&self, who: Option<i32>, now: DateTime<Utc>) -> String {
        let how_long = for_how_long(now.signed_duration_since(self.since));
        if !self.whole_to(who) {
            return format!(
                "Something about someone else has been on your mind for {how_long}; it is theirs, not for anyone else to hear."
            );
        }
        let mut line = format!("On your mind for {how_long}: {}.", self.what);
        if !self.so_far.is_empty() {
            line.push_str(&format!(" Where you have got to with it: {}", self.so_far));
        }
        line
    }
}

/// "a few minutes", "about 40 minutes", "about 2 hours", "since yesterday".
fn for_how_long(span: chrono::Duration) -> String {
    match span.num_minutes().max(0) {
        0..=9 => "a few minutes".to_string(),
        minutes @ 10..=89 => format!("about {} minutes", (minutes + 5) / 10 * 10),
        minutes @ 90..=1439 => format!("about {} hours", (minutes + 30) / 60),
        _ => "since yesterday or longer".to_string(),
    }
}

/// Asked when she finishes a thing of her own; added to what that call
/// already asks.
pub const IN_DIGEST: &str = " onMind: if something of this will keep turning over in your head after, weighing on you more than what is on your mind now (onYourMind, if given), what it is, in a few words of your own; otherwise empty. Most things do not stay like that.";

/// Asked after an exchange in private; added to what the reflection already
/// asks.
pub const IN_REFLECTION: &str = "\n\nonMind: if something from this exchange will keep turning over in your head after they go, weighing on you more than what is on your mind now (onYourMind, if given), what it is, in a few words of your own; otherwise empty. Most exchanges leave nothing like that.";

/// The field asked where something may come to stay on her mind.
pub fn ask() -> (&'static str, Value) {
    (
        FIELD,
        json!({ "type": "string", "maxLength": MAX_WHAT_CHARS }),
    )
}

/// `schema` asking [`FIELD`] as well.
pub fn with_ask(mut schema: Value) -> Value {
    let (field, shape) = ask();
    schema["properties"][field] = shape;
    if let Some(required) = schema["required"].as_array_mut() {
        required.push(json!(field));
    }
    schema
}

/// What she said stays on her mind, if anything.
pub fn read_ask(answer: Option<&Value>) -> Option<String> {
    let what = tidy(answer?.as_str()?, MAX_WHAT_CHARS);
    (!what.is_empty() && what != "null").then_some(what)
}

pub const STEP_SCHEMA: &str = "merope_on_mind_step";

/// Idle, she thinks what is on her mind one step further.
pub fn step_system(soul: &str) -> String {
    format!(
        "{soul}\n\n\
You are idle, and something has been on your mind (onYourMind; soFar is where you had got to with it). Think it one step further, the way a mind does when nothing else has it: a new angle, a question it raises, a memory it brings back, a small conclusion, or that you are going round in circles. \
Go only by what you have of it (whatYouHaveOfIt) and what you truly know; make up no facts, no events, nothing anyone said. \
thought: where you have got to now, in the first person, in your own words, one or two short sentences, in the language you yourself speak in; not a repeat of soFar. \
settled: whether you are done with it for now (you have your answer, or it no longer pulls at you). \
tell: whether you would want to tell someone this if they were here right now; when it is about someone (aboutSomeone), whether you would want to tell them."
    )
}

pub fn step_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "thought": { "type": "string", "maxLength": MAX_SO_FAR_CHARS },
            "settled": { "type": "boolean" },
            "tell": { "type": "boolean" }
        },
        "required": ["thought", "settled", "tell"],
        "additionalProperties": false
    })
}

/// One step of thinking it further.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub thought: String,
    pub settled: bool,
    pub tell: bool,
}

pub fn read_step(raw: &str) -> Option<Step> {
    let json = myriad_agent_rules::extract_json_object_from_ai_response(raw.trim());
    let step: Step = serde_json::from_str(json.as_deref().unwrap_or(raw.trim())).ok()?;
    (!tidy(&step.thought, MAX_SO_FAR_CHARS).is_empty()).then_some(step)
}

/// What the step is given: what is on her mind, where she had got to, for
/// how long, and what she has of it (her notes on the thing it came from,
/// untrusted text).
pub fn step_input(on_mind: &OnMind, have: &[String], now: DateTime<Utc>) -> String {
    let mut input = json!({
        INPUT: on_mind.what,
        "soFar": on_mind.so_far,
        "forHowLong": for_how_long(now.signed_duration_since(on_mind.since)),
        "aboutSomeone": on_mind.about.is_some(),
    });
    if !have.is_empty() {
        input["whatYouHaveOfIt"] = json!(myriad_agent_rules::untrusted_block(
            "what_you_have_of_it",
            &have.join("\n"),
        ));
    }
    input.to_string()
}

/// What she would bring up, thinking it out loud to someone who may hear it.
pub fn tell_line(on_mind: &OnMind) -> String {
    format!("你一直在想「{}」，刚想到：{}", on_mind.what, on_mind.so_far)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000 + minutes * 60, 0).unwrap()
    }

    #[test]
    fn about_someone_it_is_only_theirs_to_hear() {
        let theirs = OnMind::new("阿杰说他下周要去面试", Some(7), FROM_TALK, at(0)).unwrap();
        let theirs = theirs.stepped("他好像其实很紧张", at(40));
        let to_them = theirs.seen_by(Some(7), at(45));
        assert!(to_them.contains("阿杰说他下周要去面试") && to_them.contains("很紧张"));
        for who in [Some(8), None] {
            let seen = theirs.seen_by(who, at(45));
            assert!(!seen.contains("阿杰") && !seen.contains("面试") && !seen.contains("紧张"));
            assert!(seen.contains("someone else"));
        }
        let own = OnMind::new("那首歌最后一句", None, "song:netease:1", at(0)).unwrap();
        assert!(own.seen_by(Some(8), at(5)).contains("那首歌最后一句"));
        assert!(own.seen_by(None, at(5)).contains("那首歌最后一句"));
    }

    #[test]
    fn she_thinks_further_only_every_so_often_and_never_says_nothing_for_it() {
        let mind = OnMind::new("  为什么   雨天的歌都慢 ", None, "song:netease:1", at(0)).unwrap();
        assert_eq!(mind.what, "为什么 雨天的歌都慢");
        assert!(!mind.step_due(at(STEP_EVERY_MINUTES - 1)));
        assert!(mind.step_due(at(STEP_EVERY_MINUTES)));
        let on = mind.clone().stepped("", at(30));
        assert_eq!(on.so_far, "", "an empty step keeps where she had got to");
        assert!(!on.step_due(at(45)));
        assert_eq!(on.steps, 1);
        assert!(OnMind::new("  ", None, FROM_TALK, at(0)).is_none());
        assert!(OnMind::new("null", None, FROM_TALK, at(0)).is_none());
    }

    #[test]
    fn the_ask_rides_on_calls_that_already_happen() {
        let schema = with_ask(json!({
            "type": "object",
            "properties": { "inner": { "type": "string" } },
            "required": ["inner"],
            "additionalProperties": false
        }));
        assert_eq!(schema["required"], json!(["inner", FIELD]));
        assert_eq!(
            schema["properties"][FIELD]["maxLength"],
            json!(MAX_WHAT_CHARS)
        );
        assert_eq!(
            read_ask(Some(&json!(" 他会不会 后悔 "))).as_deref(),
            Some("他会不会 后悔")
        );
        assert_eq!(read_ask(Some(&json!(""))), None);
        assert_eq!(read_ask(None), None);
        assert!(IN_DIGEST.contains(INPUT) && IN_REFLECTION.contains(INPUT));
    }

    #[test]
    fn a_step_is_read_only_when_it_says_something() {
        let step = read_step(r#"{"thought":"也许是因为雨声本身就慢","settled":false,"tell":true}"#)
            .unwrap();
        assert!(step.tell && !step.settled);
        assert!(read_step(r#"{"thought":" ","settled":true,"tell":false}"#).is_none());
        assert!(read_step(r#"{"thought":"x","settled":true,"tell":false,"more":1}"#).is_none());
        let mind = OnMind::new("雨天的歌", None, "song:netease:1", at(0)).unwrap();
        let input = step_input(&mind, &["我听完觉得很安静".to_string()], at(120));
        assert!(input.contains("about 2 hours") && input.contains("what_you_have_of_it"));
        assert!(step_schema()["required"] == json!(["thought", "settled", "tell"]));
        assert!(step_system("你是小满。").starts_with("你是小满。"));
    }

    #[test]
    fn the_record_holds_only_its_own_fields() {
        let mind = OnMind::new("雨天的歌", Some(3), FROM_TALK, at(0)).unwrap();
        let kept = serde_json::to_value(&mind).unwrap();
        assert_eq!(
            serde_json::from_value::<OnMind>(kept.clone()).unwrap(),
            mind
        );
        let mut more = kept;
        more["who"] = json!("阿杰");
        assert!(serde_json::from_value::<OnMind>(more).is_err());
    }
}
