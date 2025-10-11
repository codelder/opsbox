//! Agent 数据模型

use serde::{Deserialize, Serialize};

/// Agent 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
  /// Agent 唯一标识
  pub id: String,

  /// Agent 显示名称
  pub name: String,

  /// Agent 版本
  pub version: String,

  /// 主机名
  pub hostname: String,

  /// 标签（如 production, dev）
  pub tags: Vec<String>,

  /// 可搜索的根目录
  pub search_roots: Vec<String>,

  /// 最后心跳时间戳（Unix timestamp）
  pub last_heartbeat: i64,

  /// Agent 状态
  pub status: AgentStatus,
}

/// Agent 状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "PascalCase")]
pub enum AgentStatus {
  /// 在线
  Online,

  /// 忙碌（正在执行任务）
  Busy { tasks: usize },

  /// 离线
  Offline,
}

/// Agent 列表响应
#[derive(Debug, Serialize, Deserialize)]
pub struct AgentListResponse {
  pub agents: Vec<AgentInfo>,
  pub total: usize,
}

/// Agent 注册请求
pub type AgentRegisterRequest = AgentInfo;

/// Agent 心跳响应
#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatResponse {
  pub success: bool,
  pub message: String,
}

impl AgentInfo {
  /// 检查 Agent 是否在线（根据最后心跳时间）
  pub fn is_online(&self, timeout_secs: i64) -> bool {
    let now = chrono::Utc::now().timestamp();
    now - self.last_heartbeat < timeout_secs
  }

  /// 更新心跳时间
  pub fn update_heartbeat(&mut self) {
    self.last_heartbeat = chrono::Utc::now().timestamp();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agent_info_is_online() {
    let mut agent = AgentInfo {
      id: "test".to_string(),
      name: "Test Agent".to_string(),
      version: "1.0.0".to_string(),
      hostname: "localhost".to_string(),
      tags: vec![],
      search_roots: vec![],
      last_heartbeat: chrono::Utc::now().timestamp(),
      status: AgentStatus::Online,
    };

    // 刚更新心跳，应该在线
    assert!(agent.is_online(60));

    // 设置为1小时前，应该离线
    agent.last_heartbeat = chrono::Utc::now().timestamp() - 3600;
    assert!(!agent.is_online(60));
  }
}
