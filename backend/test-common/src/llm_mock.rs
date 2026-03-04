//! LLM Mock服务器
//!
//! 提供模拟LLM服务器(Ollama/OpenAI)的工具，用于集成测试

use crate::TestError;
use axum::{Json, Router, routing::post};
use serde_json::{Value, json};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// LLM模拟服务器配置
#[derive(Clone)]
pub struct MockLlmConfig {
  /// 服务器监听地址
  pub address: String,
  /// 服务器监听端口
  pub port: u16,
  /// 模拟的模型名称
  pub model: String,
  /// 预设的回答内容
  pub preset_response: String,
}

impl Default for MockLlmConfig {
  fn default() -> Self {
    Self {
      address: "127.0.0.1".to_string(),
      port: 0, // 0表示随机可用端口
      model: "mock-model".to_string(),
      preset_response: "Mock response from LLM".to_string(),
    }
  }
}

/// LLM模拟服务器实例
pub struct MockLlmServer {
  /// 服务器任务句柄
  pub task: JoinHandle<()>,
  /// 服务器地址
  pub address: SocketAddr,
  /// 服务器配置
  pub config: MockLlmConfig,
}

impl MockLlmServer {
  /// 启动模拟LLM服务器
  pub async fn start(config: MockLlmConfig) -> Result<Self, TestError> {
    let address = format!("{}:{}", config.address, config.port);

    let app = Router::new()
      // Ollama 聊天接口
      .route(
        "/api/chat",
        post({
          let config = config.clone();
          move |Json(payload): Json<Value>| {
            let config = config.clone();
            async move {
              // 验证模型是否匹配（可选）
              let model = payload.get("model").and_then(|v| v.as_str()).unwrap_or("unknown");

              // 构造 Ollama 格式的响应
              let response_content = if let Some(format) = payload.get("format") {
                if format.as_str() == Some("json") {
                  // 如果请求 JSON 格式，返回 JSON 内容
                  // 这里假设 preset_response 可以是 JSON，或者是普通文本
                  // 简单起见，我们包装 preset_response
                  format!(
                    "{{\"think\": \"thinking...\", \"answer\": \"{}\"}}",
                    config.preset_response
                  )
                } else {
                  config.preset_response.clone()
                }
              } else {
                config.preset_response.clone()
              };

              Json(json!({
                  "model": model,
                  "created_at": chrono::Utc::now().to_rfc3339(),
                  "message": {
                      "role": "assistant",
                      "content": response_content
                  },
                  "done": true,
                  "total_duration": 100,
                  "load_duration": 10,
                  "prompt_eval_count": 10,
                  "prompt_eval_duration": 10,
                  "eval_count": 10,
                  "eval_duration": 10
              }))
            }
          }
        }),
      )
      // OpenAI 聊天接口
      .route(
        "/v1/chat/completions",
        post({
          let config = config.clone();
          move |Json(payload): Json<Value>| {
            let config = config.clone();
            async move {
              let model = payload.get("model").and_then(|v| v.as_str()).unwrap_or("unknown");

              Json(json!({
                  "id": "chatcmpl-123",
                  "object": "chat.completion",
                  "created": chrono::Utc::now().timestamp(),
                  "model": model,
                  "choices": [
                      {
                          "index": 0,
                          "message": {
                              "role": "assistant",
                              "content": config.preset_response
                          },
                          "finish_reason": "stop"
                      }
                  ],
                  "usage": {
                      "prompt_tokens": 10,
                      "completion_tokens": 10,
                      "total_tokens": 20
                  }
              }))
            }
          }
        }),
      );

    let listener = TcpListener::bind(&address)
      .await
      .map_err(|e| TestError::Network(format!("绑定端口失败: {}", e)))?;

    let bound_address = listener
      .local_addr()
      .map_err(|e| TestError::Network(format!("获取本地地址失败: {}", e)))?;

    let task = tokio::spawn(async move {
      axum::serve(listener, app.into_make_service())
        .await
        .expect("模拟LLM服务器启动失败");
    });

    // 等待服务器启动
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    Ok(Self {
      task,
      address: bound_address,
      // 更新配置中的端口为实际绑定端口
      config: MockLlmConfig {
        port: bound_address.port(),
        ..config
      },
    })
  }

  /// 获取服务器基础URL (http://127.0.0.1:port)
  pub fn base_url(&self) -> String {
    format!("http://{}", self.address)
  }

  /// 停止服务器
  pub async fn stop(self) -> Result<(), TestError> {
    self.task.abort();
    match self.task.await {
      Ok(_) => Ok(()),
      Err(e) if e.is_cancelled() => Ok(()),
      Err(e) => Err(TestError::Other(format!("停止服务器失败: {}", e))),
    }
  }
}
