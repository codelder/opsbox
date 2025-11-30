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
  // 禁用代理，避免访问本地 Server 时被系统代理拦截
  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(10))
    .no_proxy()
    .build()?;

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

  let response = match client.post(&url).json(&payload).send().await {
    Ok(resp) => resp,
    Err(e) => {
      let error_msg = if e.is_connect() {
        format!(
          "无法连接到 Server ({}): {}\n提示: 请检查 Server 是否正在运行，以及 server_endpoint 配置是否正确",
          config.server_endpoint, e
        )
      } else if e.is_timeout() {
        format!(
          "连接 Server 超时 ({}): {}\n提示: 请检查网络连接和 Server 是否响应",
          config.server_endpoint, e
        )
      } else {
        format!("连接 Server 失败 ({}): {}", config.server_endpoint, e)
      };
      error!("{}", error_msg);
      return Err(error_msg.into());
    }
  };

  if response.status().is_success() {
    info!("✓ 已成功向 Server 注册");
    Ok(())
  } else {
    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();

    let error_msg = if status == 502 {
      format!(
        "注册失败: {} Bad Gateway - {}\n提示: Server 可能未运行或路由未正确注册。请检查:\n  1. Server 是否正在运行在 {}\n  2. Server 是否启用了 agent-manager 模块\n  3. 网络连接是否正常",
        status.as_u16(),
        if body_text.is_empty() {
          "无响应内容"
        } else {
          &body_text
        },
        config.server_endpoint
      )
    } else {
      format!(
        "注册失败: {} - {}",
        status,
        if body_text.is_empty() {
          "无响应内容"
        } else {
          &body_text
        }
      )
    };

    error!("{}", error_msg);
    Err(error_msg.into())
  }
}

/// 心跳循环
pub async fn heartbeat_loop(config: Arc<AgentConfig>, shutdown: Arc<Notify>) {
  // 禁用代理，避免访问本地 Server 时被系统代理拦截
  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(5))
    .no_proxy()
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
            let error_msg = if status == 502 {
              format!(
                "心跳失败: {} Bad Gateway - {}\n提示: Server 可能未运行或路由未正确注册",
                status.as_u16(),
                if body.is_empty() { "无响应内容" } else { &body }
              )
            } else if status == 404 {
              format!(
                "心跳失败: {} Not Found - {}\n提示: Agent 可能未在 Server 上注册，请先完成注册",
                status.as_u16(),
                if body.is_empty() { "无响应内容" } else { &body }
              )
            } else {
              format!("心跳失败: {} - {}", status, if body.is_empty() { "无响应内容" } else { &body })
            };
            warn!("{}", error_msg);
          }
          Err(e) => {
            let error_msg = if e.is_connect() {
              format!("心跳发送出错: 无法连接到 Server ({}) - {}", config.server_endpoint, e)
            } else if e.is_timeout() {
              format!("心跳发送出错: 连接超时 ({}) - {}", config.server_endpoint, e)
            } else {
              format!("心跳发送出错: {}", e)
            };
            warn!("{}", error_msg);
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
