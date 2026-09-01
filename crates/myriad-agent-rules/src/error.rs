//! Error classification types. Analysis tables land in later slices.

use serde_json::Value;
use std::collections::HashMap;

/// 错误分类
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCategory {
    /// 内容策略违规（如 NSFW 内容被拒）
    ContentPolicy,
    /// 参数缺失或无效
    MissingParameter,
    /// 参数值格式/范围错误
    InvalidParameter,
    /// API 速率限制
    RateLimited,
    /// 服务暂时不可用（网络超时等）
    ServiceUnavailable,
    /// API 响应解析失败
    ParseError,
    /// 权限不足
    PermissionDenied,
    /// 资源未找到（404 等）
    NotFound,
    /// 配置缺失（API Key 未配置等）—— 不可重试，不应消耗 retry budget
    Configuration,
    /// 未知错误
    Unknown,
}

/// 错误分析结果
#[derive(Debug, Clone)]
pub struct ErrorAnalysis {
    /// 错误分类
    pub category: ErrorCategory,
    /// 是否值得重试（修改参数后）
    pub retryable: bool,
    /// 参数修改建议
    pub param_fixes: HashMap<String, ParamFix>,
    /// 人类可读的分析描述
    pub description: String,
    /// 建议的重试延迟倍率（1.0 = 正常，5.0 = 长等待）
    pub delay_multiplier: f64,
    /// 建议在重试前先执行的能力（如缺少数据时先搜索）
    pub suggested_prepend_capability: Option<String>,
    /// 建议的前置步骤参数（避免生成空参数的无效步骤）
    pub suggested_prepend_params: HashMap<String, Value>,
}

/// 参数修复动作
#[derive(Debug, Clone)]
pub enum ParamFix {
    /// 从提示词中移除指定关键词/模式
    RemoveFromPrompt(Vec<String>),
    /// 替换提示词中的内容
    ReplaceInPrompt { from: String, to: String },
    /// 设置参数到指定值
    SetValue(Value),
    /// 追加文本到已有参数
    AppendToParam(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_types_are_constructible() {
        let analysis = ErrorAnalysis {
            category: ErrorCategory::RateLimited,
            retryable: true,
            param_fixes: HashMap::new(),
            description: "slow".into(),
            delay_multiplier: 5.0,
            suggested_prepend_capability: None,
            suggested_prepend_params: HashMap::new(),
        };
        assert_eq!(analysis.category, ErrorCategory::RateLimited);
        assert!(analysis.retryable);
        let _ = ParamFix::SetValue(Value::from("x"));
        let _ = ParamFix::RemoveFromPrompt(vec!["nsfw".into()]);
        let _ = ParamFix::ReplaceInPrompt {
            from: "a".into(),
            to: "b".into(),
        };
        let _ = ParamFix::AppendToParam("hint".into());
    }
}
