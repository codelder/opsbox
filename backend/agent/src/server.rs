//! Server 通信功能
//!
//! 处理与 Server 的注册和心跳通信

use crate::config::AgentConfig;
use logseek::agent::AgentInfo;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tracing::{debug, error, info, warn};

/// 向 Server 注册
pub async fn register_to_server(config: &AgentConfig) -> Result<(), Box<dyn std::error::Error>> {
  let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build()?;

  #[derive(serde::Serialize)]
  struct AgentRegisterPayload {
    #[serde(flatten)]
    info: AgentInfo,
    listen_port: u16,
  }

  let payload = AgentRegisterPayload {
    info: config.to_agent_info(),
    listen_port: config.listen_port,
  };
  let url = format!("{}/api/v1/agents/register", config.server_endpoint);

  debug!("向 Server 注册: {}", url);

  let response = client.post(&url).json(&payload).send().await?;

  if response.status().is_success() {
    info!("✓ 已成功向 Server 注册");
    Ok(())
  } else {
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    error!("注册失败: {} - {}", status, body_text);
    Err(format!("注册失败: {} - {}", status, body_text).into())
  }
}

/// 心跳循环
pub async fn heartbeat_loop(config: Arc<AgentConfig>, shutdown: Arc<Notify>) {
  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(5))
    .build()
    .unwrap();

  let mut interval = tokio::time::interval(Duration::from_secs(config.heartbeat_interval_secs));

  loop {
    tokio::select! {
      _ = interval.tick() => {
        let url = format!("{}/api/v1/agents/{}/heartbeat", config.server_endpoint, config.agent_id);
        match client.post(&url).send().await {
          Ok(response) if response.status().is_success() => {
            debug!("心跳发送成功");
          }
          Ok(response) => {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("心跳失败: {} - {}", status, body);
          }
          Err(e) => {
            warn!("心跳发送出错: {}", e);
          }
        }
      }
      _ = shutdown.notified() => {
        info!("收到关闭通知，停止心跳任务");
        break;
      }
    }
  }
}
