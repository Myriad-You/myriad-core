//! Agent task status and lane/session projection. Retention helpers land next.

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

/// Session id embedded in `user:{id}:session:{session_id}` lane keys.
///
/// Accepts `Option` for stored `lane_id` columns and bare lane key strings
/// via [`session_id_from_lane_key`].
pub fn session_id_from_lane_id(lane_id: Option<&str>) -> Option<String> {
    lane_id.and_then(session_id_from_lane_key)
}

/// Extract session id from a lane key of the form `user:{id}:session:{session_id}`.
pub fn session_id_from_lane_key(lane_key: &str) -> Option<String> {
    lane_key
        .split_once(":session:")
        .map(|(_, session_id)| session_id.to_string())
        .filter(|s| !s.is_empty())
}

/// Reconstruct lane id from user + session (legacy rows without `lane_id`).
pub fn lane_id_from_user_session(user_id: i32, session_id: &str) -> Option<String> {
    let sid = session_id.trim();
    if sid.is_empty() {
        return None;
    }
    Some(format!("user:{user_id}:session:{sid}"))
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

    #[test]
    fn session_and_lane_projection() {
        assert_eq!(
            session_id_from_lane_key("user:42:session:ses_abc"),
            Some("ses_abc".into())
        );
        assert_eq!(session_id_from_lane_key("user:42"), None);
        assert_eq!(session_id_from_lane_key("user:42:session:"), None);
        assert_eq!(session_id_from_lane_id(None), None);
        assert_eq!(
            session_id_from_lane_id(Some("user:1:session:s1")),
            Some("s1".into())
        );
        assert_eq!(
            lane_id_from_user_session(7, "ses_x"),
            Some("user:7:session:ses_x".into())
        );
        assert_eq!(lane_id_from_user_session(7, "  "), None);
        assert_eq!(lane_id_from_user_session(7, ""), None);
    }
}
