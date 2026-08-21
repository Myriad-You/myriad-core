use serde_json::{json, Map, Value};

use crate::sanitize_onboarding_tags;

const MAX_PERSONA_LIST_ITEMS: usize = 12;

pub fn fallback_persona_draft(
    name: &str,
    language: &str,
    tags: &[String],
    outfit_design: Option<&Value>,
) -> Value {
    let tags = sanitize_onboarding_tags(tags);
    let summary = fallback_summary(name, language, &tags);
    let visual_identity = outfit_design
        .and_then(Value::as_object)
        .map(|outfit| {
            json!({
                "hairShape": "",
                "outfitConstruction": string_from(outfit, &["layersEn", "designZh"], 1_200),
                "heroAccessory": string_from(outfit, &["heroAccessoryZh"], 500),
                "paletteHint": string_from(outfit, &["paletteHintZh"], 500),
            })
        })
        .unwrap_or_else(|| {
            json!({
                "hairShape": "",
                "outfitConstruction": "",
                "heroAccessory": "",
                "paletteHint": "",
            })
        });
    json!({
        "displayName": bounded_text(name, 50),
        "summary": summary,
        "temperament": tags,
        "traits": tags,
        "likes": [],
        "drives": [],
        "socialStyle": "",
        "speechStyle": "",
        "language": bounded_text(language, 16),
        "visualIdentity": visual_identity,
        "draftSource": "fallback",
    })
}

pub fn sanitize_persona_draft(value: &Value, fallback: &Value) -> Option<Value> {
    let source = value
        .get("persona")
        .unwrap_or(value)
        .as_object()?;
    let mut result = fallback.as_object()?.clone();

    replace_text(&mut result, source, "summary", &["summary"], 1_200);
    replace_list(
        &mut result,
        source,
        "temperament",
        &["temperament", "traits"],
    );
    if let Some(temperament) = result.get("temperament").cloned() {
        result.insert("traits".into(), temperament);
    }
    replace_list(&mut result, source, "likes", &["likes"]);
    replace_list(&mut result, source, "drives", &["drives", "motivations"]);
    replace_text(
        &mut result,
        source,
        "socialStyle",
        &["socialStyle", "social_style"],
        500,
    );
    replace_text(
        &mut result,
        source,
        "speechStyle",
        &["speechStyle", "speech_style", "voice"],
        500,
    );

    let fallback_visual = result
        .get("visualIdentity")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let visual_source = source
        .get("visualIdentity")
        .or_else(|| source.get("visual_identity"))
        .and_then(Value::as_object);
    let mut visual = fallback_visual;
    if let Some(visual_source) = visual_source {
        replace_text(
            &mut visual,
            visual_source,
            "hairShape",
            &["hairShape", "hair", "hair_shape"],
            500,
        );
        replace_text(
            &mut visual,
            visual_source,
            "outfitConstruction",
            &["outfitConstruction", "outfit", "outfit_construction"],
            1_200,
        );
        replace_text(
            &mut visual,
            visual_source,
            "heroAccessory",
            &["heroAccessory", "accessory", "hero_accessory"],
            500,
        );
        replace_text(
            &mut visual,
            visual_source,
            "paletteHint",
            &["paletteHint", "palette", "palette_hint"],
            500,
        );
    }
    result.insert("visualIdentity".into(), Value::Object(visual));
    result.insert("draftSource".into(), json!("ai"));
    Some(Value::Object(result))
}

pub fn persona_draft_is_complete(value: &Value) -> bool {
    let Some(source) = value.get("persona").unwrap_or(value).as_object() else {
        return false;
    };
    let summary_ready = source
        .get("summary")
        .and_then(Value::as_str)
        .is_some_and(|summary| summary.trim().chars().count() >= 8);
    let temperament_ready = list_from(source, &["temperament", "traits"])
        .is_some_and(|items| !items.is_empty());
    summary_ready && temperament_ready
}

fn fallback_summary(name: &str, language: &str, tags: &[String]) -> String {
    let name = bounded_text(name, 50);
    let traits = tags.iter().take(5).cloned().collect::<Vec<_>>();
    if traits.is_empty() {
        return match language {
            "ja-JP" => format!("{name}は、これから個性を育てていくデジタルコンパニオンです。"),
            "en-US" => format!("{name} is a digital companion whose personality will grow through shared experiences."),
            _ => format!("{name}是一个会在相处中逐渐形成独特个性的数字生命。"),
        };
    }
    match language {
        "ja-JP" => format!("{name}は、{}という気質を持つデジタルコンパニオンです。", traits.join("、")),
        "en-US" => format!("{name} is a digital companion with a {} temperament.", traits.join(", ")),
        _ => format!("{name}是一个带有{}气质的数字生命。", traits.join("、")),
    }
}

fn replace_text(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    target_key: &str,
    source_keys: &[&str],
    max_chars: usize,
) {
    if let Some(value) = source_keys
        .iter()
        .find_map(|key| source.get(*key).and_then(Value::as_str))
        .map(|value| bounded_text(value, max_chars))
        .filter(|value| !value.is_empty())
    {
        target.insert(target_key.to_string(), json!(value));
    }
}

fn replace_list(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    target_key: &str,
    source_keys: &[&str],
) {
    if let Some(values) = list_from(source, source_keys).filter(|values| !values.is_empty()) {
        target.insert(target_key.to_string(), json!(values));
    }
}

fn list_from(source: &Map<String, Value>, keys: &[&str]) -> Option<Vec<String>> {
    let value = keys.iter().find_map(|key| source.get(*key))?;
    let raw = match value {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>(),
        Value::String(value) => value
            .split(['、', ',', '，', ';', '/', '|'])
            .map(str::to_string)
            .collect(),
        _ => return None,
    };
    let mut sanitized = Vec::new();
    for item in raw {
        let item = bounded_text(&item, 120);
        if item.is_empty() || sanitized.iter().any(|existing| existing == &item) {
            continue;
        }
        sanitized.push(item);
        if sanitized.len() == MAX_PERSONA_LIST_ITEMS {
            break;
        }
    }
    Some(sanitized)
}

fn string_from(source: &Map<String, Value>, keys: &[&str], max_chars: usize) -> String {
    keys.iter()
        .find_map(|key| source.get(*key).and_then(Value::as_str))
        .map(|value| bounded_text(value, max_chars))
        .unwrap_or_default()
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(max_chars)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_persona_is_character_only_and_uses_outfit() {
        let draft = fallback_persona_draft(
            "Nova",
            "en-US",
            &["calm".into(), "curious".into()],
            Some(&json!({
                "layersEn": "layered short coat and structured boots",
                "heroAccessoryZh": "星形胸针",
                "paletteHintZh": "雾蓝 / 银",
            })),
        );
        assert!(persona_draft_is_complete(&draft));
        assert_eq!(draft["draftSource"], "fallback");
        assert_eq!(
            draft["visualIdentity"]["outfitConstruction"],
            "layered short coat and structured boots"
        );
        for forbidden in ["room", "world", "furniture", "environment"] {
            assert!(draft.get(forbidden).is_none());
        }
    }

    #[test]
    fn sanitizes_model_persona_without_dropping_fallback_identity() {
        let fallback = fallback_persona_draft("Nova", "zh-CN", &["安静".into()], None);
        let draft = sanitize_persona_draft(
            &json!({
                "persona": {
                    "summary": "安静但会认真回应重要事情的数字生命。",
                    "temperament": ["克制", "细心", "克制"],
                    "likes": "雨声、旧书",
                    "visualIdentity": { "hair": "银灰短发" }
                }
            }),
            &fallback,
        )
        .unwrap();
        assert_eq!(draft["displayName"], "Nova");
        assert_eq!(draft["temperament"], json!(["克制", "细心"]));
        assert_eq!(draft["visualIdentity"]["hairShape"], "银灰短发");
        assert_eq!(draft["draftSource"], "ai");
    }

    #[test]
    fn incomplete_model_payload_is_detected_before_sanitized_fallback() {
        assert!(!persona_draft_is_complete(&json!({"summary": "短"})));
        assert!(!persona_draft_is_complete(&json!({"temperament": ["calm"]})));
    }
}
