//! Agent task status. Lane/session and retention helpers land in later slices.

use serde::{Deserialize, Serialize};

/// 任务状态枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// 等待执行
    Pending,
    /// 执行中
    Running,
    /// 等待用户输入
    WaitingForInput,
    /// 已暂停
    Paused,
    /// 已完成
    Completed,
    /// 失败
    Failed,
    /// 已取消
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn task_status_serde_snake_case() {
        assert_eq!(
            serde_json::to_value(&TaskStatus::WaitingForInput).unwrap(),
            json!("waiting_for_input")
        );
        let status: TaskStatus = serde_json::from_value(json!("paused")).unwrap();
        assert_eq!(status, TaskStatus::Paused);
        let cancelled: TaskStatus = serde_json::from_value(json!("cancelled")).unwrap();
        assert_eq!(cancelled, TaskStatus::Cancelled);
    }
}
