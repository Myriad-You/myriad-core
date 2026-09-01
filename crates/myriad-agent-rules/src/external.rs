//! HTTP fetch body helpers. MCP/scrape/classify land in later slices.

use serde_json::{json, Value};
use std::collections::HashMap;

/// Max response body accepted by http.fetch (bytes).
pub const HTTP_FETCH_MAX_BODY_BYTES: u64 = 10 * 1024 * 1024;

/// Non-empty trimmed string from params.
pub fn optional_string_param(params: &HashMap<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// HTTP method for http.fetch (default GET; only POST is special-cased).
pub fn http_fetch_method(params: &HashMap<String, Value>) -> &str {
    params
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("GET")
}

/// Whether Content-Length / body exceeds http.fetch limit.
pub fn http_body_exceeds_limit(len: u64) -> bool {
    len > HTTP_FETCH_MAX_BODY_BYTES
}

/// User-facing error when body is too large (Content-Length path).
pub fn http_content_length_error(content_length: u64) -> String {
    format!("Response Content-Length ({content_length} bytes) exceeds 10MB limit")
}

/// User-facing error when body is too large (after read).
pub fn http_body_size_error() -> String {
    "Response body exceeds 10MB limit".to_string()
}

/// Parse response body as JSON or wrap as string Value.
pub fn parse_http_body_value(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|_| json!(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_string_and_http_helpers() {
        let mut params = HashMap::new();
        params.insert("username".into(), json!("  ada  "));
        params.insert("empty".into(), json!("  "));
        assert_eq!(
            optional_string_param(&params, "username").as_deref(),
            Some("ada")
        );
        assert!(optional_string_param(&params, "empty").is_none());

        params.insert("method".into(), json!("POST"));
        assert_eq!(http_fetch_method(&params), "POST");
        assert_eq!(http_fetch_method(&HashMap::new()), "GET");

        assert!(!http_body_exceeds_limit(100));
        assert!(http_body_exceeds_limit(HTTP_FETCH_MAX_BODY_BYTES + 1));
        assert!(http_content_length_error(99).contains("99"));
        assert_eq!(parse_http_body_value(r#"{"a":1}"#)["a"], 1);
        assert_eq!(parse_http_body_value("not-json"), json!("not-json"));
        assert_eq!(http_body_size_error(), "Response body exceeds 10MB limit");
    }
}
