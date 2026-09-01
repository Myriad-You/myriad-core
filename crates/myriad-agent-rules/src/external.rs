//! HTTP fetch, MCP projection, scrape gates, and outbound-fetch classification.

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

/// Parse `mcp.{server_id}.{tool_name}` capability id.
///
/// This split is only safe when `server_id` contains no `.`. Config ids allow
/// `.` (`[A-Za-z0-9._-]`); callers that have the advertised tool list must use
/// [`match_mcp_capability_id`] instead, or they will call the wrong tool name
/// on the right server (`mcp.com.example.search` → tool `example.search`).
pub fn parse_mcp_capability_id(capability_id: &str) -> Result<(&str, &str), String> {
    let rest = capability_id
        .strip_prefix("mcp.")
        .ok_or_else(|| "Invalid MCP capability ID".to_string())?;
    rest.split_once('.')
        .ok_or_else(|| "MCP capability ID must include server and tool names".to_string())
}

/// Match `mcp.{server}.{tool}` against advertised `(server_id, tool_name)` pairs.
///
/// Prefers the longest `server_id` if two pairs stringify to the same id
/// (`mcp.a.b.c` as `a`/`b.c` vs `a.b`/`c`).
pub fn match_mcp_capability_id(
    capability_id: &str,
    advertised: &[(String, String)],
) -> Result<(String, String), String> {
    if !capability_id.starts_with("mcp.") {
        return Err("Invalid MCP capability ID".to_string());
    }
    advertised
        .iter()
        .filter(|(server, tool)| capability_id == format!("mcp.{server}.{tool}"))
        .max_by_key(|(server, _)| server.len())
        .cloned()
        .ok_or_else(|| format!("Unknown MCP capability: {capability_id}"))
}

/// Strip executor-only `__*` keys before crossing the MCP trust boundary.
pub fn mcp_arguments(params: &HashMap<String, Value>) -> Value {
    Value::Object(
        params
            .iter()
            .filter(|(key, _)| !key.starts_with("__"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

/// Max HTML size for web.scrape (bytes).
pub const WEB_SCRAPE_MAX_HTML_BYTES: usize = 5 * 1024 * 1024;
/// Default max extracted text length for web.scrape.
pub const WEB_SCRAPE_DEFAULT_MAX_LENGTH: usize = crate::USER_TEXT_MAX_CHARS;

/// CSS selector for scrape (default body).
pub fn scrape_selector(params: &HashMap<String, Value>) -> &str {
    params
        .get("selector")
        .and_then(|v| v.as_str())
        .unwrap_or("body")
}

/// Max text length for scrape (default [`WEB_SCRAPE_DEFAULT_MAX_LENGTH`]).
pub fn scrape_max_length(params: &HashMap<String, Value>) -> usize {
    params
        .get("max_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(WEB_SCRAPE_DEFAULT_MAX_LENGTH as u64) as usize
}

/// Whether raw HTML exceeds scrape size gate.
#[allow(dead_code)] // 仅测试调用：生产在各自调用点内联同等判定。
pub fn scrape_html_too_large(html_len: usize) -> bool {
    html_len > WEB_SCRAPE_MAX_HTML_BYTES
}

/// Collapse whitespace and truncate to max_length; returns (text, truncated).
pub fn compress_and_truncate_text(text: &str, max_length: usize) -> (String, bool) {
    let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated = text.chars().count() > max_length;
    let text: String = text.chars().take(max_length).collect();
    (text, truncated)
}

/// Tags whose text nodes should be skipped during scrape.
pub const SCRAPE_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "svg", "iframe"];

pub fn scrape_should_skip_tag(tag: &str) -> bool {
    SCRAPE_SKIP_TAGS.contains(&tag)
}

/// Hitokoto type param (optional).
pub fn hitokoto_type(params: &HashMap<String, Value>) -> Option<&str> {
    params.get("type").and_then(|v| v.as_str())
}

/// Keep timeout / HTTP status / a short API phrase; drop client-stack and serde dumps.
pub fn classify_outbound_fetch(label: &str, detail: &str) -> String {
    if let Some(status) = http_status_from_text(detail) {
        return format!("{label} (HTTP {status})");
    }
    let lower = detail.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        return format!("{label}: timed out");
    }
    if lower.contains("could not connect")
        || lower.contains("connection refused")
        || lower.contains("error trying to connect")
    {
        return format!("{label}: could not connect");
    }
    if is_internal_outbound_dump(detail) {
        return label.to_string();
    }
    let rest = detail.trim();
    if rest.is_empty() || rest.len() > 120 || rest.starts_with('{') {
        return label.to_string();
    }
    format!("{label}: {rest}")
}

fn http_status_from_text(text: &str) -> Option<u16> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && (i == 0 || !bytes[i - 1].is_ascii_digit())
            && (i + 3 >= bytes.len() || !bytes[i + 3].is_ascii_digit())
        {
            let code = (bytes[i] - b'0') as u16 * 100
                + (bytes[i + 1] - b'0') as u16 * 10
                + (bytes[i + 2] - b'0') as u16;
            if (400..=599).contains(&code) {
                return Some(code);
            }
        }
        i += 1;
    }
    None
}

fn is_internal_outbound_dump(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("error sending request")
        || lower.contains("builder error")
        || lower.contains("os error")
        || lower.contains("for url (")
        || lower.contains("missing field")
        || lower.contains("at line ")
        || lower.contains("expected value")
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

    #[test]
    fn mcp_capability_and_arguments() {
        assert_eq!(
            parse_mcp_capability_id("mcp.github.search").unwrap(),
            ("github", "search")
        );
        assert!(parse_mcp_capability_id("github.search").is_err());
        assert!(parse_mcp_capability_id("mcp.only").is_err());

        let params = HashMap::from([
            ("query".to_string(), json!("myriad")),
            ("__directive".to_string(), json!("internal")),
            ("__user_request".to_string(), json!("private")),
            ("__steering".to_string(), json!("new direction")),
        ]);
        assert_eq!(mcp_arguments(&params), json!({ "query": "myriad" }));

        let advertised = vec![
            ("com.example".to_string(), "search".to_string()),
            ("github".to_string(), "search".to_string()),
            ("a".to_string(), "b.c".to_string()),
            ("a.b".to_string(), "c".to_string()),
        ];
        assert_eq!(
            match_mcp_capability_id("mcp.com.example.search", &advertised).unwrap(),
            ("com.example".to_string(), "search".to_string())
        );
        assert_eq!(
            match_mcp_capability_id("mcp.github.search", &advertised).unwrap(),
            ("github".to_string(), "search".to_string())
        );
        // Collision: longest server_id wins so the tool name stays intact.
        assert_eq!(
            match_mcp_capability_id("mcp.a.b.c", &advertised).unwrap(),
            ("a.b".to_string(), "c".to_string())
        );
        assert!(match_mcp_capability_id("mcp.com.example.missing", &advertised).is_err());
    }

    #[test]
    fn scrape_helpers() {
        let mut params = HashMap::new();
        params.insert("selector".into(), json!("article"));
        params.insert("max_length".into(), json!(10));
        assert_eq!(scrape_selector(&params), "article");
        assert_eq!(scrape_max_length(&params), 10);
        assert_eq!(scrape_selector(&HashMap::new()), "body");
        assert_eq!(
            scrape_max_length(&HashMap::new()),
            WEB_SCRAPE_DEFAULT_MAX_LENGTH
        );
        assert_eq!(WEB_SCRAPE_DEFAULT_MAX_LENGTH, crate::USER_TEXT_MAX_CHARS);

        assert!(!scrape_html_too_large(100));
        assert!(scrape_html_too_large(WEB_SCRAPE_MAX_HTML_BYTES + 1));

        let (text, truncated) = compress_and_truncate_text("  a   b  c  d  e  ", 5);
        assert!(truncated);
        assert_eq!(text.chars().count(), 5);
        assert!(scrape_should_skip_tag("script"));
        assert!(!scrape_should_skip_tag("p"));
        assert_eq!(
            hitokoto_type(&HashMap::from([("type".into(), json!("a"))])),
            Some("a")
        );
    }

    #[test]
    fn outbound_fetch_keeps_status_and_drops_internal_dumps() {
        assert_eq!(
            classify_outbound_fetch(
                "Failed to fetch Bangumi user",
                "Bangumi API error: 404 Not Found"
            ),
            "Failed to fetch Bangumi user (HTTP 404)"
        );
        assert_eq!(
            classify_outbound_fetch(
                "Failed to fetch Bilibili user",
                "error sending request for url (https://api.bilibili.com/x/space/acc/info)"
            ),
            "Failed to fetch Bilibili user"
        );
        assert_eq!(
            classify_outbound_fetch("Failed to fetch weather", "timed out"),
            "Failed to fetch weather: timed out"
        );
        assert_eq!(
            classify_outbound_fetch("Failed to fetch Bilibili user", "用户不存在"),
            "Failed to fetch Bilibili user: 用户不存在"
        );
        assert_eq!(
            classify_outbound_fetch(
                "Failed to fetch hitokoto",
                "expected value at line 1 column 1"
            ),
            "Failed to fetch hitokoto"
        );
    }
}
