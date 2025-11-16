//! 大模型（LLM）客户端模块
//!
//! 提供统一的聊天接口，兼容 Ollama 和 OpenAI。
//! - 通过 `LlmClient` trait 定义抽象能力
//! - 提供 `OllamaClient` 与 `OpenAIClient` 实现
//! - 支持从环境变量快速构建客户端
//! - 包含详细的调试日志用于查看原始输出
//!
//! 环境变量（建议）：
//! - LLM_PROVIDER=ollama|openai
//! - OLLAMA_BASE_URL（默认：http://127.0.0.1:11434）
//! - OLLAMA_MODEL（默认：qwen3:8b）
//! - OPENAI_BASE_URL（默认：https://api.openai.com）
//! - OPENAI_API_KEY（必填，当 provider=openai 时）
//! - OPENAI_MODEL（默认：gpt-4o-mini）
use crate::error::AppError;
use async_trait::async_trait;
use tracing::{debug, info, warn};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const INTERNAL_THINK_PROMPT: &str = "你是一名助手。请严格仅输出一个 JSON 对象：{\"think\": string, \"answer\": string}。\n- think：简明思考过程（可中文）。\n- answer：给用户的最终答案（中文为主）。\n不要输出除 JSON 以外的任何多余文本。";

/// 聊天角色
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")] // 序列化为 system/user/assistant
pub enum Role {
  System,
  User,
  Assistant,
}

/// 聊天消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
  /// 角色：system/user/assistant
  pub role: Role,
  /// 文本内容
  pub content: String,
}

/// 聊天请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
  /// 对话消息列表（按顺序）
  pub messages: Vec<ChatMessage>,
  /// 模型名（可覆盖配置中的默认模型）
  #[serde(skip_serializing_if = "Option::is_none")]
  pub model: Option<String>,
  /// 温度（采样随机性）
  #[serde(skip_serializing_if = "Option::is_none")]
  pub temperature: Option<f32>,
  /// 最大生成 token 数
  #[serde(skip_serializing_if = "Option::is_none")]
  pub max_tokens: Option<u32>,
  /// 是否需要将“思考过程”和“最终回答”分离并以 JSON 返回（think + answer）
  #[serde(default)]
  pub separate_think: bool,
  /// 内置系统提示的注入模式（仅当 separate_think=true 生效）
  /// 默认 prepend：内置提示在前，用户提示在后
  #[serde(default = "default_injection_mode")]
  pub injection_mode: InjectionMode,
}

/// 聊天响应（简化版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
  /// 最终回复文本
  pub content: String,
  /// 可选的思考过程（当请求 `separate_think=true` 时尝试填充）
  #[serde(skip_serializing_if = "Option::is_none")]
  pub think: Option<String>,
  /// 实际使用的模型
  pub model: String,
  /// 结束原因（如 stop）
  #[serde(skip_serializing_if = "Option::is_none")]
  pub finish_reason: Option<String>,
}

/// LLM 客户端抽象
#[async_trait]
pub trait LlmClient: Send + Sync {
  /// 同步（非流式）聊天调用
  async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AppError>;
}

/// 系统提示注入模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InjectionMode {
  /// 内置提示在最前面，用户系统提示随后（默认）
  Prepend,
  /// 用户系统提示在前，内置提示追加在最后
  Append,
  /// 不注入内置提示，使用用户自定义系统提示（需自行约束 JSON）
  Replace,
  /// 不注入任何提示（既不内置也不强制用户提供）
  None,
}

fn default_injection_mode() -> InjectionMode {
  InjectionMode::Prepend
}

/// 统一的客户端动态类型
pub type DynLlmClient = Arc<dyn LlmClient>;

/// 提供方枚举
#[derive(Debug, Clone)]
pub enum LlmProvider {
  Ollama(OllamaConfig),
  OpenAI(OpenAIConfig),
}

/// Ollama 配置
#[derive(Debug, Clone)]
pub struct OllamaConfig {
  /// 基础地址，例如 http://127.0.0.1:11434
  pub base_url: String,
  /// 默认模型名，例如 qwen3:8b
  pub model: String,
  /// 请求超时时间（秒）
  pub timeout_secs: u64,
}

impl OllamaConfig {
  /// 从环境变量读取配置
  pub fn from_env() -> Self {
    let base_url = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen3:8b".to_string());
    let timeout_secs = std::env::var("OLLAMA_TIMEOUT_SECS")
      .ok()
      .and_then(|s| s.parse::<u64>().ok())
      .unwrap_or(60);

    Self {
      base_url,
      model,
      timeout_secs,
    }
  }
}

/// OpenAI 配置
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
  /// 基础地址，例如 https://api.openai.com 或自建兼容网关
  pub base_url: String,
  /// API Key（必填）
  pub api_key: String,
  /// 默认模型名，例如 gpt-4o-mini
  pub model: String,
  /// 请求超时时间（秒）
  pub timeout_secs: u64,
  /// 可选：组织 ID
  pub organization: Option<String>,
  /// 可选：项目 ID（如 OpenAI Projects）
  pub project: Option<String>,
}

impl OpenAIConfig {
  /// 从环境变量读取配置
  pub fn from_env() -> Result<Self, AppError> {
    let base_url = std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com".to_string());
    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| AppError::config("缺少 OPENAI_API_KEY 环境变量"))?;
    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
    let timeout_secs = std::env::var("OPENAI_TIMEOUT_SECS")
      .ok()
      .and_then(|s| s.parse::<u64>().ok())
      .unwrap_or(60);
    let organization = std::env::var("OPENAI_ORG").ok();
    let project = std::env::var("OPENAI_PROJECT").ok();

    Ok(Self {
      base_url,
      api_key,
      model,
      timeout_secs,
      organization,
      project,
    })
  }
}

/// 从环境变量构建统一客户端
pub fn build_llm_from_env() -> Result<DynLlmClient, AppError> {
  let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "ollama".to_string());
  match provider.to_ascii_lowercase().as_str() {
    "ollama" => {
      let cfg = OllamaConfig::from_env();
      Ok(build_ollama_client(cfg))
    }
    "openai" => {
      let cfg = OpenAIConfig::from_env()?;
      Ok(build_openai_client(cfg))
    }
    other => Err(AppError::config(format!("不支持的 LLM_PROVIDER: {}", other))),
  }
}

/// 构建 Ollama 客户端
pub fn build_ollama_client(cfg: OllamaConfig) -> DynLlmClient {
  Arc::new(OllamaClient::new(cfg))
}

/// 构建 OpenAI 客户端
pub fn build_openai_client(cfg: OpenAIConfig) -> DynLlmClient {
  Arc::new(OpenAIClient::new(cfg))
}

/// Ollama 客户端实现
struct OllamaClient {
  http: reqwest::Client,
  cfg: OllamaConfig,
  base_chat_url: Url,
}

impl OllamaClient {
  fn new(cfg: OllamaConfig) -> Self {
    let timeout = Duration::from_secs(cfg.timeout_secs);
    let http = reqwest::Client::builder()
      .timeout(timeout)
      .build()
      .expect("创建 HTTP 客户端失败");

    let mut base = Url::parse(&cfg.base_url).unwrap_or_else(|_| Url::parse("http://127.0.0.1:11434").unwrap());
    // /api/chat
    base.set_path("/api/chat");

    Self {
      http,
      cfg,
      base_chat_url: base,
    }
  }
}

#[derive(Serialize)]
struct OllamaChatReq<'a> {
  model: &'a str,
  messages: &'a [ChatMessage],
  #[serde(default)]
  stream: bool,
  /// 当需要 JSON 输出时，设置为 "json" （Ollama 将尝试返回机器可解析的 JSON）
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

#[derive(Deserialize)]
struct OllamaMessageResp {
  content: String,
  #[allow(dead_code)]
  role: Option<String>,
}

#[derive(Deserialize)]
struct OllamaChatResp {
  message: OllamaMessageResp,
  model: String,
  #[allow(dead_code)]
  done: bool,
}

#[async_trait]
impl LlmClient for OllamaClient {
  async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AppError> {
    let model = req.model.as_deref().unwrap_or(&self.cfg.model);

    // 当需要结构化输出时，根据注入模式合成消息
    let mut messages = req.messages;
    if req.separate_think {
      let sys = ChatMessage {
        role: Role::System,
        content: INTERNAL_THINK_PROMPT.to_string(),
      };
      match req.injection_mode {
        InjectionMode::Prepend => {
          let mut new_msgs = Vec::with_capacity(messages.len() + 1);
          new_msgs.push(sys);
          new_msgs.extend(messages);
          messages = new_msgs;
        }
        InjectionMode::Append => {
          messages.push(sys);
        }
        InjectionMode::Replace | InjectionMode::None => {
          // 不注入内置提示
        }
      }
    }

    let body = OllamaChatReq {
      model,
      messages: &messages,
      stream: false,
      format: req.separate_think.then_some("json"),
      options: Some(OllamaOptions {
        temperature: req.temperature,
        num_predict: req.max_tokens,
        stop: None,
      }),
    };

    // 调试：输出请求详情
    debug!("Ollama 请求 URL: {}", self.base_chat_url);
    debug!(
      "Ollama 请求体: {}",
      serde_json::to_string_pretty(&body).unwrap_or_else(|_| "序列化失败".to_string())
    );

    let resp = self
      .http
      .post(self.base_chat_url.clone())
      .json(&body)
      .send()
      .await
      .map_err(|e| AppError::external_service(format!("Ollama 请求失败: {}", e)))?;

    // 调试：输出响应状态
    debug!("Ollama 响应状态: {}", resp.status());

    if !resp.status().is_success() {
      let status = resp.status();
      let text = resp.text().await.unwrap_or_default();
      warn!("Ollama 响应错误（{}）: {}", status, text);
      return Err(AppError::external_service(format!(
        "Ollama 响应错误（{}）: {}",
        status, text
      )));
    }

    // 获取原始响应文本用于调试
    let response_text = resp
      .text()
      .await
      .map_err(|e| AppError::external_service(format!("读取 Ollama 响应失败: {}", e)))?;

    // 调试：输出原始响应
    info!("Ollama 原始响应: {}", response_text);

    let data: OllamaChatResp = serde_json::from_str(&response_text)
      .map_err(|e| AppError::external_service(format!("解析 Ollama 响应失败: {}，原始响应: {}", e, response_text)))?;

    // 解析结构化 JSON（若启用）
    if req.separate_think {
      #[derive(Deserialize)]
      struct Structured {
        think: String,
        answer: String,
      }
      if let Ok(parsed) = serde_json::from_str::<Structured>(&data.message.content) {
        return Ok(ChatResponse {
          content: parsed.answer,
          think: Some(parsed.think),
          model: data.model,
          finish_reason: Some("stop".to_string()),
        });
      }
      // 若解析失败，回退为普通文本
      tracing::warn!("Ollama 未返回期望的 JSON，回退为纯文本");
    }

    Ok(ChatResponse {
      content: data.message.content,
      think: None,
      model: data.model,
      finish_reason: Some("stop".to_string()),
    })
  }
}

/// OpenAI 客户端实现
struct OpenAIClient {
  http: reqwest::Client,
  cfg: OpenAIConfig,
  chat_url: Url,
}

impl OpenAIClient {
  fn new(cfg: OpenAIConfig) -> Self {
    let timeout = Duration::from_secs(cfg.timeout_secs);
    let http = reqwest::Client::builder()
      .timeout(timeout)
      .build()
      .expect("创建 HTTP 客户端失败");

    // 构造聊天 URL：在保留 base_url 现有路径前缀的基础上追加 chat/completions
    // 规则：
    // - 若 base_url 无路径（/ 或空），生成 /v1/chat/completions
    // - 若已有路径（如 /v1 或 /compatible-mode/v1），生成 {existing}/chat/completions
    let mut base = Url::parse(&cfg.base_url).unwrap_or_else(|_| Url::parse("https://api.openai.com").unwrap());
    let existing = base.path().trim_end_matches('/');
    let chat_path = if existing.is_empty() || existing == "/" {
      "/v1/chat/completions".to_string()
    } else {
      format!("{}/chat/completions", existing)
    };
    base.set_path(&chat_path);

    Self {
      http,
      cfg,
      chat_url: base,
    }
  }
}

#[derive(Serialize)]
struct OpenAIChatReq<'a> {
  model: &'a str,
  messages: &'a [ChatMessage],
  #[serde(skip_serializing_if = "Option::is_none")]
  temperature: Option<f32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  max_tokens: Option<u32>,
  /// 当需要 JSON 输出时，使用 OpenAI 的 response_format 保证返回 JSON 对象
  #[serde(skip_serializing_if = "Option::is_none")]
  response_format: Option<OpenAIResponseFormat>,
}

#[derive(Serialize)]
struct OpenAIResponseFormat {
  #[serde(rename = "type")]
  r#type: &'static str,
}

#[derive(Deserialize)]
struct OpenAIChoice {
  message: ChatMessage,
}

#[derive(Deserialize)]
struct OpenAIChatResp {
  model: String,
  choices: Vec<OpenAIChoice>,
}

#[async_trait]
impl LlmClient for OpenAIClient {
  async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, AppError> {
    let model = req.model.as_deref().unwrap_or(&self.cfg.model);

    // 当需要结构化输出时，根据注入模式合成消息
    let mut messages = req.messages;
    if req.separate_think {
      let sys = ChatMessage {
        role: Role::System,
        content: INTERNAL_THINK_PROMPT.to_string(),
      };
      match req.injection_mode {
        InjectionMode::Prepend => {
          let mut new_msgs = Vec::with_capacity(messages.len() + 1);
          new_msgs.push(sys);
          new_msgs.extend(messages);
          messages = new_msgs;
        }
        InjectionMode::Append => {
          messages.push(sys);
        }
        InjectionMode::Replace | InjectionMode::None => {
          // 不注入内置提示
        }
      }
    }

    let body = OpenAIChatReq {
      model,
      messages: &messages,
      temperature: req.temperature,
      max_tokens: req.max_tokens,
      response_format: req
        .separate_think
        .then_some(OpenAIResponseFormat { r#type: "json_object" }),
    };

    // 调试：输出请求详情
    debug!("OpenAI 请求 URL: {}", self.chat_url);
    debug!(
      "OpenAI 请求体: {}",
      serde_json::to_string_pretty(&body).unwrap_or_else(|_| "序列化失败".to_string())
    );

    let mut req_builder = self.http.post(self.chat_url.clone()).json(&body);
    req_builder = req_builder.bearer_auth(&self.cfg.api_key);
    if let Some(org) = &self.cfg.organization {
      req_builder = req_builder.header("OpenAI-Organization", org);
    }
    if let Some(project) = &self.cfg.project {
      req_builder = req_builder.header("OpenAI-Project", project);
    }

    let resp = req_builder
      .send()
      .await
      .map_err(|e| AppError::external_service(format!("OpenAI 请求失败: {}", e)))?;

    // 调试：输出响应状态
    debug!("OpenAI 响应状态: {}", resp.status());

    if !resp.status().is_success() {
      let status = resp.status();
      let text = resp.text().await.unwrap_or_default();
      warn!("OpenAI 响应错误（{}）: {}", status, text);
      return Err(AppError::external_service(format!(
        "OpenAI 响应错误（{}）: {}",
        status, text
      )));
    }

    // 获取原始响应文本用于调试
    let response_text = resp
      .text()
      .await
      .map_err(|e| AppError::external_service(format!("读取 OpenAI 响应失败: {}", e)))?;

    // 调试：输出原始响应
    info!("OpenAI 原始响应: {}", response_text);

    let data: OpenAIChatResp = serde_json::from_str(&response_text)
      .map_err(|e| AppError::external_service(format!("解析 OpenAI 响应失败: {}，原始响应: {}", e, response_text)))?;

    let content = data
      .choices
      .into_iter()
      .next()
      .ok_or_else(|| AppError::external_service("OpenAI 返回空结果".to_string()))?
      .message
      .content;

    // 若期望结构化输出，尝试将 content 解析为 JSON {think, answer}
    if req.separate_think {
      #[derive(Deserialize)]
      struct Structured {
        think: String,
        answer: String,
      }
      if let Ok(parsed) = serde_json::from_str::<Structured>(&content) {
        return Ok(ChatResponse {
          content: parsed.answer,
          think: Some(parsed.think),
          model: data.model,
          finish_reason: Some("stop".to_string()),
        });
      }
      tracing::warn!("OpenAI 未返回期望的 JSON，回退为纯文本");
    }

    Ok(ChatResponse {
      content,
      think: None,
      model: data.model,
      finish_reason: Some("stop".to_string()),
    })
  }
}
