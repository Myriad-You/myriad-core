//! The decisions questions put to a chat model instead: it answers each in
//! the shape a decisions model does, so the plan comes out the same way.
//! Asking it is the platform's (the site's `direct_by_chat`).
use super::*;

/// The same questions put to a chat model, which answers each in the shape a
/// decisions model does; the plan then comes out of the same answers.
pub fn chat_system_prompt(questions: &Value) -> String {
    let mut prompt = String::from(
        "You judge how a character acts at one moment of a chat: the input says who she is, what they said, what she replies and her mood. \
Answer every question below about her, as JSON. `confidence` is how sure you are, from 0 to 1: \
low when the moment does not show it plainly; what she does not show cannot be wrong. \
For a yes/no question, `noul` is the probability that the answer is true. \
For a scale, `score` runs from 0 (its first level) to the number of its last level.\n",
    );
    for (key, question) in questions.as_object().into_iter().flatten() {
        prompt.push_str(&format!(
            "\n{key}: {}\n",
            question["instructions"].as_str().unwrap_or_default()
        ));
        match &question["criteria"] {
            Value::Object(options) => {
                for (option, means) in options {
                    prompt.push_str(&format!(
                        "  - {option}: {}\n",
                        means.as_str().unwrap_or_default()
                    ));
                }
            }
            Value::Array(levels) => {
                for (level, means) in levels.iter().enumerate() {
                    prompt.push_str(&format!(
                        "  {level}: {}\n",
                        means.as_str().unwrap_or_default()
                    ));
                }
            }
            _ => {}
        }
    }
    prompt
}

/// The answer each question takes from a chat model, constrained.
pub fn chat_schema(questions: &Value) -> Value {
    let unit = json!({"type": "number", "minimum": 0, "maximum": 1});
    let mut properties = Map::new();
    for (key, question) in questions.as_object().into_iter().flatten() {
        let answer = match question["type"].as_str() {
            Some("noul") => json!({"type": "object", "additionalProperties": false,
                "required": ["noul"], "properties": {"noul": unit}}),
            Some("score") => {
                let top = question["criteria"]
                    .as_array()
                    .map_or(0, |levels| levels.len().saturating_sub(1));
                json!({"type": "object", "additionalProperties": false,
                    "required": ["score", "confidence"],
                    "properties": {"score": {"type": "number", "minimum": 0, "maximum": top}, "confidence": unit}})
            }
            _ => {
                let options: Vec<&String> = question["criteria"]
                    .as_object()
                    .map(|options| options.keys().collect())
                    .unwrap_or_default();
                json!({"type": "object", "additionalProperties": false,
                    "required": ["choice", "confidence"],
                    "properties": {"choice": {"type": "string", "enum": options}, "confidence": unit}})
            }
        };
        properties.insert(key.clone(), answer);
    }
    let required: Vec<String> = properties.keys().cloned().collect();
    json!({"type": "object", "additionalProperties": false, "required": required, "properties": properties})
}
