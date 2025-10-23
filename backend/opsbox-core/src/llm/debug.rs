//! Ollama 调试工具
//!
//! 这个模块提供了调试 Ollama 原始输出的功能

use crate::error::AppError;
use crate::llm::ChatMessage;
use reqwest::Url;
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
struct OllamaChatReq<'a> {
  model: &'a str,
  messages: &'a [ChatMessage],
  #[serde(default)]
  stream: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  format: Option<&'a str>,
  #[serde(skip_serializing_if = "Option::is_none")]
  options: Option<OllamaOptions<'a>>,
}

#[derive(Serialize)]
struct OllamaOptions<'a> {
  #[serde(skip_serializing_if = "Option::is_none")]
  temperature: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  num_predict: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  stop: Option<&'a [&'a str]>,
}

/// 调试 Ollama 原始输出
///
/// # 参数
/// - `base_url`: Ollama 服务的基础 URL
/// - `model`: 要使用的模型名称
/// - `messages`: 聊天消息列表
/// - `enable_json_format`: 是否启用 JSON 格式输出
///
/// # 返回
/// 返回原始的响应文本
pub async fn debug_ollama_raw_output(
  base_url: &str,
  model: &str,
  messages: Vec<ChatMessage>,
  enable_json_format: bool,
) -> Result<String, AppError> {
  let timeout = Duration::from_secs(60);
  let http = reqwest::Client::builder()
    .timeout(timeout)
    .build()
    .expect("创建 HTTP 客户端失败");

  let mut url = Url::parse(base_url).unwrap_or_else(|_| Url::parse("http://127.0.0.1:11434").unwrap());
  url.set_path("/api/chat");

  let body = OllamaChatReq {
    model,
    messages: &messages,
    stream: false,
    format: enable_json_format.then_some("json"),
    options: Some(OllamaOptions {
      temperature: Some(0.7),
      num_predict: Some(1000),
      stop: None,
    }),
  };

  println!("=== Ollama 调试请求 ===");
  println!("URL: {}", url);
  println!(
    "请求体: {}",
    serde_json::to_string_pretty(&body).unwrap_or_else(|_| "序列化失败".to_string())
  );
  println!("========================");

  let resp = http
    .post(url)
    .json(&body)
    .send()
    .await
    .map_err(|e| AppError::external_service(format!("Ollama 请求失败: {}", e)))?;

  println!("=== Ollama 响应状态 ===");
  println!("状态码: {}", resp.status());
  println!("========================");

  let response_text = resp
    .text()
    .await
    .map_err(|e| AppError::external_service(format!("读取响应失败: {}", e)))?;

  println!("=== Ollama 原始响应 ===");
  println!("{}", response_text);
  println!("========================");

  Ok(response_text)
}

/// 快速测试 Ollama 连接
pub async fn test_ollama_connection(base_url: &str) -> Result<(), AppError> {
  let timeout = Duration::from_secs(10);
  let http = reqwest::Client::builder()
    .timeout(timeout)
    .build()
    .expect("创建 HTTP 客户端失败");

  let mut url = Url::parse(base_url).unwrap_or_else(|_| Url::parse("http://127.0.0.1:11434").unwrap());
  url.set_path("/api/tags");

  println!("测试 Ollama 连接: {}", url);

  let resp = http
    .get(url)
    .send()
    .await
    .map_err(|e| AppError::external_service(format!("连接测试失败: {}", e)))?;

  if resp.status().is_success() {
    let text = resp.text().await.unwrap_or_default();
    println!("✓ Ollama 连接成功");
    println!("可用模型: {}", text);
    Ok(())
  } else {
    Err(AppError::external_service(format!(
      "Ollama 连接失败，状态码: {}",
      resp.status()
    )))
  }
}
