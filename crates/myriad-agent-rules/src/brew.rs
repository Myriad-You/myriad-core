//! Brew subscribe name and write caps. No I/O.

/// 订阅源名称最大长度
pub const MAX_FEED_NAME_LEN: usize = 255;
/// 订阅 URL 最大尝试数
pub const MAX_FEED_URLS: usize = 10;
/// platform.write 单次最大写入条目数
pub const MAX_PLATFORM_WRITE_ITEMS: usize = 500;
/// update_interval 最小值（分钟）
pub const MIN_UPDATE_INTERVAL: i32 = 5;
/// update_interval 最大值（分钟）
pub const MAX_UPDATE_INTERVAL: i32 = 1440;

/// 清洗并验证用户提供的订阅源名称。
///
/// - 限制最大长度
/// - 去除首尾空白
/// - 拒绝纯空白字符串
pub fn sanitize_feed_name(name: &str) -> Result<String, String> {
    let trimmed: String = name.chars().take(MAX_FEED_NAME_LEN).collect();
    let trimmed = trimmed.trim().to_string();
    if trimmed.is_empty() {
        return Err("Feed name is required".to_string());
    }
    Ok(trimmed)
}

/// Clamp brew.subscribe update_interval minutes into contract bounds.
pub fn clamp_update_interval_minutes(raw: i32) -> i32 {
    raw.clamp(MIN_UPDATE_INTERVAL, MAX_UPDATE_INTERVAL)
}

/// Whether a platform.write items array exceeds the per-call cap.
pub fn platform_write_items_over_cap(count: usize) -> bool {
    count > MAX_PLATFORM_WRITE_ITEMS
}

pub fn platform_write_cap_error() -> String {
    "Too many items to write at once".to_string()
}

use std::net::IpAddr;

/// Disallowed hostname forms for subscribe URLs (before DNS).
pub fn is_disallowed_subscribe_host(host: &str) -> bool {
    host == "localhost" || host.ends_with(".local") || host.ends_with(".internal")
}

/// Whether a resolved IP must never be used as a subscribe target.
pub fn is_disallowed_subscribe_ip(ip: IpAddr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() {
        return true;
    }
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.octets()[0] == 169,
        IpAddr::V6(v6) => {
            // 阻止 IPv6 回环和链路本地
            v6.is_loopback() || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Parse and apply pure URL policy for brew.subscribe (scheme + host string + IP literals).
///
/// Hostname DNS resolution remains in the handler (IO). When the host is an IP
/// literal it is checked here; hostname-only URLs pass if the host string is allowed.
pub fn validate_subscribe_url_policy(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|_| "Invalid URL".to_string())?;

    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("This address is not allowed".to_string()),
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "This URL is missing a host".to_string())?;

    if is_disallowed_subscribe_host(host) {
        return Err("This address is not allowed".to_string());
    }

    // If host is already an IP literal, reject private ranges without DNS.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_disallowed_subscribe_ip(ip) {
            return Err("This address is not allowed".to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_feed_name_trims_and_limits() {
        assert_eq!(sanitize_feed_name("  天利  ").unwrap(), "天利");
        assert!(sanitize_feed_name("   ").is_err());
        let long = "a".repeat(300);
        assert_eq!(
            sanitize_feed_name(&long).unwrap().chars().count(),
            MAX_FEED_NAME_LEN
        );
    }

    #[test]
    fn update_interval_and_platform_write_cap() {
        assert_eq!(clamp_update_interval_minutes(1), MIN_UPDATE_INTERVAL);
        assert_eq!(clamp_update_interval_minutes(9999), MAX_UPDATE_INTERVAL);
        assert_eq!(clamp_update_interval_minutes(30), 30);
        assert!(!platform_write_items_over_cap(10));
        assert!(platform_write_items_over_cap(MAX_PLATFORM_WRITE_ITEMS + 1));
        assert_eq!(
            platform_write_cap_error(),
            "Too many items to write at once"
        );
    }
}
