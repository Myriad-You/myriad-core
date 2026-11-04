//! The topics she keeps what she knows of someone under: a kind of thing in
//! their life they come back to (their plants, seeing doctors, their work),
//! never one item of it. Going over a stretch of talk, she puts each fact
//! she keeps under the topic it belongs to, or starts one, or leaves it
//! under none (`chat_remember::settle_system_prompt`). Asked for all of a
//! kind, what she kept under it comes to mind together, so a fact is found
//! through another of its kind even when it shares no word with the
//! question.
//!
//! A topic is a node of hers (`myriad-nodes`, a topic node with a minted
//! key); here is only what she is shown of them and how her answer is read,
//! so every host shows the same and reads the same.

use myriad_agent_rules::Concept;
use myriad_agent_rules::memory::lexical::{Document, score_all, uses_word};
use serde_json::{Value, json};

/// Topics offered to put the facts of one stretch under, at most (as many
/// as placing a talk offers, `myriad_nodes::limits::PLACE_CANDIDATES`).
pub const OFFERED: usize = 12;
/// A topic's name, and each other name, as she sees and gives it.
pub const NAME_CHARS: usize = 40;
/// Other names a topic she starts goes by, at most.
pub const ALIASES: usize = 5;
/// What is already under a topic, as she sees it.
pub const HOLDS_CHARS: usize = 160;

/// A topic as the host keeps it: its names, and the facts under it, newest
/// first.
pub struct Topic<'a> {
    pub name: &'a str,
    pub aliases: &'a [String],
    pub holds: &'a [String],
}

/// Which topics to offer for a stretch of `talk`, most likely first, at
/// most [`OFFERED`]: those it names by a name first, then those it shares
/// words with, then the rest in the order given (the host gives the most
/// recently used first). Indexes into `topics`.
pub fn offered(talk: &str, topics: &[Topic]) -> Vec<usize> {
    let named = |topic: &Topic| {
        std::iter::once(topic.name)
            .chain(topic.aliases.iter().map(String::as_str))
            .any(|name| uses_word(talk, name))
    };
    let texts: Vec<String> = topics
        .iter()
        .map(|topic| {
            std::iter::once(topic.name.to_string())
                .chain(topic.holds.iter().cloned())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    let concepts: Vec<[Concept; 1]> = topics
        .iter()
        .map(|topic| {
            [Concept {
                name: topic.name.to_string(),
                aliases: topic.aliases.to_vec(),
            }]
        })
        .collect();
    let documents: Vec<Document> = texts
        .iter()
        .zip(&concepts)
        .map(|(text, concepts)| Document { text, concepts })
        .collect();
    let scores = score_all(talk, &documents);
    let mut order: Vec<usize> = (0..topics.len()).collect();
    // Stable: equals keep the order given.
    order.sort_by(|left, right| {
        let standing = |index: usize| {
            let score = scores[index];
            (
                named(&topics[index]),
                score.strong,
                if score.strong { score.value } else { 0.0 },
            )
        };
        let (left, right) = (standing(*left), standing(*right));
        right
            .0
            .cmp(&left.0)
            .then(right.1.cmp(&left.1))
            .then(right.2.total_cmp(&left.2))
    });
    order.truncate(OFFERED);
    order
}

/// The `offered` topics as she sees them beside a stretch: labelled `t1`,
/// `t2`… (no id goes to the model), with their names and a little of what
/// is under them.
pub fn shown(topics: &[Topic], offered: &[usize]) -> Value {
    Value::Array(
        offered
            .iter()
            .enumerate()
            .map(|(n, index)| {
                let topic = &topics[*index];
                json!({
                    "label": format!("t{}", n + 1),
                    "name": cut(topic.name, NAME_CHARS),
                    "aliases": topic.aliases.iter().map(|alias| cut(alias, NAME_CHARS)).collect::<Vec<_>>(),
                    "holds": cut(&topic.holds.join(" / "), HOLDS_CHARS),
                })
            })
            .collect(),
    )
}

/// Where a fact she keeps goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Under {
    /// Under no topic: only about them, or a one-off.
    Nowhere,
    /// Under the offered topic at this place in `offered` (from 0).
    Offered(usize),
    /// Under a topic she starts.
    New { name: String, aliases: Vec<String> },
}

/// The schema of where a fact goes, in her answer: `on` a label, or `name`
/// (and `aliases`) for a topic she starts, or neither.
pub fn under_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "on": { "type": "string", "maxLength": 8 },
            "name": { "type": "string", "maxLength": NAME_CHARS },
            "aliases": { "type": "array", "items": { "type": "string", "maxLength": NAME_CHARS }, "maxItems": ALIASES }
        },
        "required": ["on", "name", "aliases"],
        "additionalProperties": false
    })
}

/// Where her answer puts a fact, given `offered` topics were shown. A label
/// she was shown wins over a name; a label she was not shown, or nothing
/// readable, is no topic: the fact is kept all the same.
pub fn read_under(value: Option<&Value>, offered: usize) -> Under {
    let Some(value) = value else {
        return Under::Nowhere;
    };
    let text = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default()
    };
    let on = text("on");
    if let Some(n) = on
        .strip_prefix('t')
        .and_then(|n| n.parse::<usize>().ok())
        .filter(|n| (1..=offered).contains(n))
    {
        return Under::Offered(n - 1);
    }
    if !on.is_empty() {
        return Under::Nowhere;
    }
    let name = text("name");
    if name.is_empty() {
        return Under::Nowhere;
    }
    let name = cut(&name, NAME_CHARS);
    let mut aliases: Vec<String> = Vec::new();
    for alias in value
        .get("aliases")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let alias = cut(
            &alias.split_whitespace().collect::<Vec<_>>().join(" "),
            NAME_CHARS,
        );
        if !alias.is_empty()
            && !alias.eq_ignore_ascii_case(&name)
            && !aliases.iter().any(|seen| seen.eq_ignore_ascii_case(&alias))
        {
            aliases.push(alias);
        }
    }
    aliases.truncate(ALIASES);
    Under::New { name, aliases }
}

/// Whether `name` is the name of `topic`: the same words, whatever the case.
/// A topic she starts under a name she already keeps one by is that one.
pub fn same_name(name: &str, topic: &str) -> bool {
    let words = |text: &str| {
        text.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    words(name) == words(topic)
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn topic<'a>(name: &'a str, aliases: &'a [String], holds: &'a [String]) -> Topic<'a> {
        Topic {
            name,
            aliases,
            holds,
        }
    }

    #[test]
    fn a_topic_the_talk_names_comes_first_then_what_it_shares_words_with() {
        let none: Vec<String> = Vec::new();
        let plant_aliases = vec!["plants".to_string()];
        let plants = vec!["has a snake plant on the balcony".to_string()];
        let doctor = vec!["sees Dr. Lee about her skin".to_string()];
        let topics = [
            topic("work", &none, &none),
            topic("seeing doctors", &none, &doctor),
            topic("houseplants", &plant_aliases, &plants),
        ];
        let talk = "my plants are dying, and Dr. Lee said the rash is fine";
        let offered = offered(talk, &topics);
        assert_eq!(offered, vec![2, 1, 0]);
    }

    #[test]
    fn no_more_than_twelve_are_offered_and_the_given_order_breaks_ties() {
        let none: Vec<String> = Vec::new();
        let names: Vec<String> = (0..20).map(|n| format!("topic {n}")).collect();
        let topics: Vec<Topic> = names.iter().map(|name| topic(name, &none, &none)).collect();
        assert_eq!(
            offered("nothing in common", &topics),
            (0..OFFERED).collect::<Vec<_>>()
        );
    }

    #[test]
    fn she_sees_labels_and_never_an_id() {
        let none: Vec<String> = Vec::new();
        let holds = vec!["a".repeat(300)];
        let topics = [topic("houseplants", &none, &holds)];
        let shown = shown(&topics, &[0]);
        assert_eq!(shown[0]["label"], "t1");
        assert_eq!(
            shown[0]["holds"].as_str().unwrap().chars().count(),
            HOLDS_CHARS
        );
    }

    #[test]
    fn a_label_she_was_shown_wins_and_one_she_was_not_is_no_topic() {
        let answer = json!({"on": "t2", "name": "plants", "aliases": []});
        assert_eq!(read_under(Some(&answer), 3), Under::Offered(1));
        let unseen = json!({"on": "t9", "name": "plants", "aliases": []});
        assert_eq!(read_under(Some(&unseen), 3), Under::Nowhere);
        assert_eq!(read_under(None, 3), Under::Nowhere);
        let neither = json!({"on": "", "name": " ", "aliases": ["x"]});
        assert_eq!(read_under(Some(&neither), 3), Under::Nowhere);
    }

    #[test]
    fn a_topic_she_starts_keeps_distinct_other_names_only() {
        let answer = json!({
            "on": "",
            "name": "  seeing   doctors ",
            "aliases": ["Seeing Doctors", "看病", "看病", "doctor visits", "a", "b", "c", "d"]
        });
        assert_eq!(
            read_under(Some(&answer), 0),
            Under::New {
                name: "seeing doctors".to_string(),
                aliases: vec![
                    "看病".to_string(),
                    "doctor visits".to_string(),
                    "a".to_string(),
                    "b".to_string(),
                    "c".to_string(),
                ],
            }
        );
        assert!(same_name("Seeing  doctors", "seeing doctors"));
        assert!(!same_name("seeing doctors", "doctors"));
    }
}
