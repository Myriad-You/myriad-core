//! Retry/failure types. Helpers that take `&RecipeStep` stay in backend.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 失败处理策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FailureStrategy {
    /// 终止整个方案
    Abort,
    /// 跳过并继续
    Skip,
    /// 使用默认值继续
    UseDefault(Value),
    /// 回退到备用能力
    Fallback(String),
}

/// 重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_attempts: u32,
    /// 重试间隔（毫秒）
    pub delay_ms: u64,
    /// 指数退避
    pub exponential_backoff: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn failure_strategy_and_retry_config_roundtrip() {
        assert_eq!(
            serde_json::to_value(&FailureStrategy::Abort).unwrap(),
            json!("abort")
        );
        assert_eq!(
            serde_json::to_value(&FailureStrategy::Skip).unwrap(),
            json!("skip")
        );
        let fallback: FailureStrategy =
            serde_json::from_value(json!({ "fallback": "ai.chat" })).unwrap();
        assert_eq!(fallback, FailureStrategy::Fallback("ai.chat".into()));
        let default: FailureStrategy =
            serde_json::from_value(json!({ "use_default": 1 })).unwrap();
        assert_eq!(default, FailureStrategy::UseDefault(json!(1)));

        let retry: RetryConfig = serde_json::from_value(json!({
            "max_attempts": 3,
            "delay_ms": 200,
            "exponential_backoff": true
        }))
        .unwrap();
        assert_eq!(retry.max_attempts, 3);
        assert_eq!(retry.delay_ms, 200);
        assert!(retry.exponential_backoff);
    }
}
