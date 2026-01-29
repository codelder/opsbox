use serde::de::DeserializeOwned;
use std::time::Duration;
use thiserror::Error;
use tracing::warn;

use super::models::AgentInfo;

/// Agent 客户端错误
#[derive(Debug, Error)]
pub enum AgentClientError {
  #[error("IO错误: {0}")]
  Io(#[from] std::io::Error),
  #[error("连接错误: {0}")]
  ConnectionError(String),
  #[error("Agent 不可用: {0}")]
  AgentUnavailable(String),
  #[error("超时")]
  Timeout,
  #[error("其他错误: {0}")]
  Other(String),
}

/// 通用 Agent 客户端
///
/// 处理基础的 HTTP 连接、健康检查和信息获取。
/// 具体的业务逻辑（如 Search）应由上层应用扩展实现。
#[derive(Clone)]
pub struct AgentClient {
  /// Agent ID (标识符)
  pub agent_id: String,
  /// 基础端点 (e.g., "http://192.168.1.10:8090")
  pub endpoint: String,
  /// HTTP 客户端
  pub client: reqwest::Client,
  /// 默认请求超时
  pub timeout: Duration,
}

impl AgentClient {
  /// 创建新的 Agent 客户端
  pub fn new(agent_id: String, endpoint: String, timeout: Option<Duration>) -> Self {
    // 确保 endpoint 包含协议
    let full_endpoint = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
      endpoint
    } else {
      format!("http://{}", endpoint)
    };

    let timeout = timeout.unwrap_or(Duration::from_secs(60));

    Self {
      agent_id,
      endpoint: full_endpoint,
      client: reqwest::Client::builder()
        .timeout(timeout * 5) // 总超时通常设得较长
        .build()
        // reqwest 构建一般不会失败，除非 TLS 配置严重错误
        .unwrap_or_else(|e| panic!("无法创建 reqwest 客户端: {}", e)),
      timeout,
    }
  }

  /// 检查 Agent 健康状态
  pub async fn health_check(&self) -> bool {
    let url = format!("{}/health", self.endpoint);
    match tokio::time::timeout(Duration::from_secs(5), self.client.get(&url).send()).await {
      Ok(Ok(response)) => response.status().is_success(),
      Ok(Err(e)) => {
        warn!("Agent {} 健康检查失败: {}", self.agent_id, e);
        false
      }
      Err(_) => {
        warn!("Agent {} 健康检查超时", self.agent_id);
        false
      }
    }
  }

  /// 获取 Agent 信息
  pub async fn get_info(&self) -> Result<AgentInfo, AgentClientError> {
    self.get::<AgentInfo>("/api/v1/info").await
  }

  /// 通用 GET 请求辅助函数
  pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, AgentClientError> {
    let url = format!("{}{}", self.endpoint, path);
    // 默认简单重试逻辑可以在这里实现，或者留给调用者
    // 这里实现一个简单的单次请求
    let response = self
      .client
      .get(&url)
      .timeout(self.timeout)
      .send()
      .await
      .map_err(|e| AgentClientError::ConnectionError(e.to_string()))?;

    if !response.status().is_success() {
      return Err(AgentClientError::Other(format!(
        "Agent 返回错误状态: {}",
        response.status()
      )));
    }

    response
      .json::<T>()
      .await
      .map_err(|e| AgentClientError::Other(format!("解析响应失败: {}", e)))
  }

  /// 带查询参数的 GET 请求辅助函数
  pub async fn get_with_query<T: DeserializeOwned, Q: serde::Serialize>(
    &self,
    path: &str,
    query: &Q,
  ) -> Result<T, AgentClientError> {
    let url = format!("{}{}", self.endpoint, path);
    let response = self
      .client
      .get(&url)
      .query(query)
      .timeout(self.timeout)
      .send()
      .await
      .map_err(|e| AgentClientError::ConnectionError(e.to_string()))?;

    if !response.status().is_success() {
      return Err(AgentClientError::Other(format!(
        "Agent 返回错误状态: {}",
        response.status()
      )));
    }

    response
      .json::<T>()
      .await
      .map_err(|e| AgentClientError::Other(format!("解析响应失败: {}", e)))
  }

  /// 通用 POST 请求辅助函数
  pub async fn post<B: serde::Serialize, T: DeserializeOwned>(
    &self,
    path: &str,
    body: &B,
  ) -> Result<T, AgentClientError> {
    let url = format!("{}{}", self.endpoint, path);
    let response = self
      .client
      .post(&url)
      .json(body)
      .timeout(self.timeout)
      .send()
      .await
      .map_err(|e| AgentClientError::ConnectionError(e.to_string()))?;

    if !response.status().is_success() {
      return Err(AgentClientError::Other(format!(
        "Agent 返回错误状态: {}",
        response.status()
      )));
    }

    response
      .json::<T>()
      .await
      .map_err(|e| AgentClientError::Other(format!("解析响应失败: {}", e)))
  }

  /// 获取原始响应流（用于下载文件等）
  pub async fn get_raw(&self, path: &str) -> Result<reqwest::Response, AgentClientError> {
    let url = format!("{}{}", self.endpoint, path);
    let response = self
      .client
      .get(&url)
      .timeout(self.timeout)
      .send()
      .await
      .map_err(|e| AgentClientError::ConnectionError(e.to_string()))?;

    if !response.status().is_success() {
      return Err(AgentClientError::Other(format!(
        "Agent 返回错误状态: {}",
        response.status()
      )));
    }

    Ok(response)
  }

  /// 带查询参数获取原始响应流
  pub async fn get_raw_with_query<Q: serde::Serialize>(
    &self,
    path: &str,
    query: &Q,
  ) -> Result<reqwest::Response, AgentClientError> {
    let url = format!("{}{}", self.endpoint, path);
    let response = self
      .client
      .get(&url)
      .query(query)
      .timeout(self.timeout)
      .send()
      .await
      .map_err(|e| AgentClientError::ConnectionError(e.to_string()))?;

    if !response.status().is_success() {
      return Err(AgentClientError::Other(format!(
        "Agent 返回错误状态: {}",
        response.status()
      )));
    }

    Ok(response)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agent_client_new_with_http() {
    let client = AgentClient::new(
      "test-agent".to_string(),
      "http://localhost:8080".to_string(),
      None,
    );
    assert_eq!(client.agent_id, "test-agent");
    assert_eq!(client.endpoint, "http://localhost:8080");
    assert_eq!(client.timeout, Duration::from_secs(60));
  }

  #[test]
  fn test_agent_client_new_without_protocol() {
    let client = AgentClient::new(
      "test-agent".to_string(),
      "localhost:8080".to_string(),
      None,
    );
    assert_eq!(client.endpoint, "http://localhost:8080");
  }

  #[test]
  fn test_agent_client_new_with_https() {
    let client = AgentClient::new(
      "test-agent".to_string(),
      "https://localhost:8080".to_string(),
      None,
    );
    assert_eq!(client.endpoint, "https://localhost:8080");
  }

  #[test]
  fn test_agent_client_new_with_custom_timeout() {
    let client = AgentClient::new(
      "test-agent".to_string(),
      "localhost:8080".to_string(),
      Some(Duration::from_secs(30)),
    );
    assert_eq!(client.timeout, Duration::from_secs(30));
  }

  #[test]
  fn test_agent_client_clone() {
    let client = AgentClient::new(
      "test-agent".to_string(),
      "localhost:8080".to_string(),
      None,
    );
    let cloned = client.clone();
    assert_eq!(client.agent_id, cloned.agent_id);
    assert_eq!(client.endpoint, cloned.endpoint);
  }

  #[test]
  fn test_agent_client_error_display() {
    let err = AgentClientError::ConnectionError("test error".to_string());
    assert!(err.to_string().contains("test error"));

    let err = AgentClientError::AgentUnavailable("agent-01".to_string());
    assert!(err.to_string().contains("agent-01"));

    let err = AgentClientError::Timeout;
    assert!(err.to_string().contains("超时"));

    let err = AgentClientError::Other("other error".to_string());
    assert!(err.to_string().contains("other error"));
  }

  #[test]
  fn test_agent_client_error_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let err: AgentClientError = io_err.into();
    assert!(matches!(err, AgentClientError::Io(_)));
  }
}
