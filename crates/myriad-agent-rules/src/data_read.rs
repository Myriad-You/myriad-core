//! data.read JSON extraction. No I/O, no clock.

use serde_json::Value;

/// Extract a JSON array from free-form AI text (raw, markdown fences).
pub fn extract_json_array_from_ai_response(text: &str) -> Vec<Value> {
    let json_start = text.find('[');
    let json_end = text.rfind(']');

    if let (Some(start), Some(end)) = (json_start, json_end) {
        if end > start {
            let json_str = &text[start..=end];
            if let Ok(arr) = serde_json::from_str::<Vec<Value>>(json_str) {
                return arr;
            }
        }
    }

    if text.contains("```json") {
        let parts: Vec<&str> = text.split("```json").collect();
        if parts.len() > 1 {
            if let Some(json_part) = parts[1].split("```").next() {
                if let Ok(arr) = serde_json::from_str::<Vec<Value>>(json_part.trim()) {
                    return arr;
                }
            }
        }
    }

    if text.contains("```") {
        let parts: Vec<&str> = text.split("```").collect();
        for part in parts {
            let trimmed = part.trim();
            if trimmed.starts_with('[') {
                if let Ok(arr) = serde_json::from_str::<Vec<Value>>(trimmed) {
                    return arr;
                }
            }
        }
    }

    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_json_array_from_raw_and_fenced_text() {
        let raw = extract_json_array_from_ai_response("noise [1, 2] tail");
        assert_eq!(raw, vec![json!(1), json!(2)]);
        let fenced = extract_json_array_from_ai_response("```json\n[{\"a\":1}]\n```");
        assert_eq!(fenced[0]["a"], 1);
        let generic_fence = extract_json_array_from_ai_response("```\n[true]\n```");
        assert_eq!(generic_fence, vec![json!(true)]);
        assert!(extract_json_array_from_ai_response("no array").is_empty());
    }
}
