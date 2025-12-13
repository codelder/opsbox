use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, trace, warn};
// 使用新的 LLM 客户端
use crate::repository::llm::{self, ProviderKind};
use opsbox_core::SqlitePool;
use opsbox_core::{
  ChatMessage, ChatRequest, DynLlmClient, InjectionMode, OllamaConfig, OpenAIConfig, Role, build_llm_from_env,
  build_ollama_client, build_openai_client,
};

// 将快速指南在编译期内嵌，使用基于 crate 根目录的绝对路径，避免相对路径失效
const QUICK_GUIDE: &str = include_str!(concat!(
  env!("CARGO_MANIFEST_DIR"),
  "/../../docs/guides/query-syntax.md"
));

#[derive(Debug, Clone, Deserialize)]
pub struct NLBody {
  /// 自然语言需求文本
  pub nl: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NL2QResponse {
  /// 生成的查询字符串（q）
  pub q: String,
}

#[derive(thiserror::Error, Debug)]
pub enum NL2QError {
  #[error("Ollama服务未就绪或连接失败: {0}")]
  Http(String),
  #[error("AI 生成了空结果，请重试或改写需求")]
  Empty,
}

fn build_messages(user_nl: &str) -> Vec<ChatMessage> {
  // 将整份规范作为 system 消息提供，保持与客户端无关
  let system = ChatMessage {
    role: Role::System,
    content: QUICK_GUIDE.to_string(),
  };
  let user = ChatMessage {
    role: Role::User,
    content: user_nl.trim().to_string(),
  };
  vec![system, user]
}

// 移除大模型推理模型产生的 <think> 思考片段，保留真实输出
fn strip_think_sections(input: &str) -> String {
  let mut out = input.to_string();
  while let Some(start) = out.find("<think>") {
    if let Some(end_rel) = out[start..].find("</think>") {
      let end = start + end_rel + "</think>".len();
      out.replace_range(start..end, "");
    } else {
      // 若没有闭合标签，直接移除从 <think> 开始至末尾
      out.replace_range(start.., "");
      break;
    }
  }
  out
}

pub async fn call_llm(pool: &SqlitePool, nl: &str) -> Result<String, NL2QError> {
  info!("NL2Q请求: '{}'", nl);

  let messages = build_messages(nl);
  debug!("消息数: {}（system + user）", messages.len());

  // 构建统一 LLM 客户端（优先使用数据库中的默认配置；不存在则回退到环境变量）
  let client = match resolve_llm_client(pool).await {
    Ok(c) => c,
    Err(e) => {
      warn!("使用数据库默认 LLM 失败，回退到环境变量：{}", e);
      build_llm_from_env().map_err(|e| {
        error!("LLM 客户端初始化失败: {}", e);
        NL2QError::Http(e.to_string())
      })?
    }
  };

  let req = ChatRequest {
    messages,
    model: None,
    temperature: Some(0.2),
    max_tokens: None, // 默认不限制长度，由提供方按模型策略决定
    // 使用结构化输出，并用 replace 避免与文档自身的系统提示冲突
    separate_think: true,
    injection_mode: InjectionMode::Replace,
  };

  debug!("发送 ChatRequest（separate_think=true, injection=Replace）");
  let start = std::time::Instant::now();
  let resp = client.chat(req).await.map_err(|e| {
    error!("LLM 调用失败: {}", e);
    NL2QError::Http(e.to_string())
  })?;
  let duration = start.elapsed();
  info!("LLM 响应耗时: {:?}，模型: {}", duration, resp.model);

  let mut q = resp.content.trim().to_string();
  trace!("LLM 内容输出: '{}'", &q);
  trace!("LLM 响应详情: {:?}", resp);

  // 兜底清理：移除 <think> 片段、去掉代码块，仅取首行
  // 注意：不要移除双引号！双引号是查询语法的一部分（精确查找）
  let before_strip = q.clone();
  q = strip_think_sections(&q).trim().to_string();
  if q != before_strip {
    trace!("移除<think>片段后: '{}'", &q);
  }
  if q.starts_with("```") && q.ends_with("```") {
    debug!("移除代码块围栏");
    let inner = q.trim_start_matches("```\n").trim_end_matches("\n```");
    q = inner.trim().to_string();
  }
  // 已移除自动删除引号的逻辑：双引号是查询语法的一部分（用于精确查找），必须保留
  if let Some(nl_pos) = q.find('\n') {
    debug!("截取第一行内容");
    q = q[..nl_pos].trim().to_string();
  }

  if q.is_empty() {
    warn!("AI生成了空结果");
    return Err(NL2QError::Empty);
  }

  info!("NL2Q生成成功: '{}'", &q);
  Ok(q)
}

/// 解析默认 LLM 客户端（从数据库），失败则返回错误
async fn resolve_llm_client(pool: &SqlitePool) -> Result<DynLlmClient, NL2QError> {
  let default = llm::get_default(pool)
    .await
    .map_err(|e| NL2QError::Http(format!("加载默认 LLM 失败: {}", e)))?;

  let name = default.ok_or_else(|| NL2QError::Http("未设置默认大模型".to_string()))?;
  let backend = llm::get_backend(pool, &name)
    .await
    .map_err(|e| NL2QError::Http(format!("读取 LLM 配置失败: {}", e)))?
    .ok_or_else(|| NL2QError::Http("默认大模型不存在".to_string()))?;

  match backend.provider {
    ProviderKind::Ollama => {
      let cfg = OllamaConfig {
        base_url: backend.base_url,
        model: backend.model,
        timeout_secs: backend.timeout_secs as u64,
      };
      Ok(build_ollama_client(cfg))
    }
    ProviderKind::OpenAI => {
      let api_key = backend
        .api_key
        .ok_or_else(|| NL2QError::Http("OpenAI 配置缺少 API Key".to_string()))?;
      let cfg = OpenAIConfig {
        base_url: backend.base_url,
        api_key,
        model: backend.model,
        timeout_secs: backend.timeout_secs as u64,
        organization: backend.organization,
        project: backend.project,
      };
      Ok(build_openai_client(cfg))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_build_messages() {
    let nl = "查找错误日志";
    let messages = build_messages(nl);

    // 应该有 2 条消息：system + user
    assert_eq!(messages.len(), 2);

    // 第一条是 system 消息
    assert!(matches!(messages[0].role, Role::System));
    assert!(messages[0].content.contains("查询字符串规范"));

    // 第二条是 user 消息
    assert!(matches!(messages[1].role, Role::User));
    assert_eq!(messages[1].content, "查找错误日志");
  }

  #[test]
  fn test_build_messages_with_whitespace() {
    let nl = "  查找错误日志  \n";
    let messages = build_messages(nl);

    // user 消息应该被 trim
    assert_eq!(messages[1].content, "查找错误日志");
  }

  #[test]
  fn test_strip_think_sections_no_think() {
    let input = "error AND warning";
    let output = strip_think_sections(input);
    assert_eq!(output, "error AND warning");
  }

  #[test]
  fn test_strip_think_sections_single_think() {
    let input = "<think>这是思考过程</think>error AND warning";
    let output = strip_think_sections(input);
    assert_eq!(output, "error AND warning");
  }

  #[test]
  fn test_strip_think_sections_multiple_think() {
    let input = "<think>思考1</think>error<think>思考2</think> AND warning";
    let output = strip_think_sections(input);
    assert_eq!(output, "error AND warning");
  }

  #[test]
  fn test_strip_think_sections_unclosed_think() {
    let input = "error AND warning<think>未闭合的思考";
    let output = strip_think_sections(input);
    assert_eq!(output, "error AND warning");
  }

  #[test]
  fn test_strip_think_sections_nested_think() {
    let input = "<think>外层<think>内层</think>外层</think>result";
    let output = strip_think_sections(input);
    // 应该移除第一个 <think>...</think> 对
    assert!(output.contains("result"));
  }

  #[test]
  fn test_strip_think_sections_empty_think() {
    let input = "<think></think>error";
    let output = strip_think_sections(input);
    assert_eq!(output, "error");
  }

  #[test]
  fn test_nl2q_error_display() {
    let err = NL2QError::Http("连接失败".to_string());
    assert_eq!(err.to_string(), "Ollama服务未就绪或连接失败: 连接失败");

    let err = NL2QError::Empty;
    assert_eq!(err.to_string(), "AI 生成了空结果，请重试或改写需求");
  }

  #[test]
  fn test_nl_body_deserialization() {
    let json = r#"{"nl": "查找错误日志"}"#;
    let body: NLBody = serde_json::from_str(json).unwrap();
    assert_eq!(body.nl, "查找错误日志");
  }

  #[test]
  fn test_nl2q_response_serialization() {
    let response = NL2QResponse {
      q: "error AND warning".to_string(),
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("error AND warning"));
  }

  #[test]
  fn test_quick_guide_loaded() {
    // 验证 QUICK_GUIDE 常量已正确加载
    assert!(QUICK_GUIDE.contains("查询字符串规范"));
  }

  #[test]
  fn test_build_messages_preserves_quotes() {
    let nl = r#"查找包含 "exact phrase" 的日志"#;
    let messages = build_messages(nl);
    assert!(messages[1].content.contains(r#""exact phrase""#));
  }

  #[test]
  fn test_strip_think_sections_preserves_content() {
    let input = "before<think>思考</think>middle<think>更多思考</think>after";
    let output = strip_think_sections(input);
    assert_eq!(output, "beforemiddleafter");
  }

  #[test]
  fn test_strip_think_sections_with_newlines() {
    let input = "result\n<think>\n思考过程\n</think>\nmore";
    let output = strip_think_sections(input);
    assert_eq!(output, "result\n\nmore");
  }

  #[test]
  fn test_nl_body_clone() {
    let body = NLBody { nl: "test".to_string() };
    let cloned = body.clone();
    assert_eq!(body.nl, cloned.nl);
  }

  #[test]
  fn test_nl2q_response_clone() {
    let response = NL2QResponse {
      q: "test query".to_string(),
    };
    let cloned = response.clone();
    assert_eq!(response.q, cloned.q);
  }

  #[test]
  fn test_nl2q_error_debug() {
    let err = NL2QError::Http("test".to_string());
    let debug_str = format!("{:?}", err);
    assert!(debug_str.contains("Http"));
    assert!(debug_str.contains("test"));
  }

  #[test]
  fn test_build_messages_empty_nl() {
    let nl = "";
    let messages = build_messages(nl);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content, "");
  }

  #[test]
  fn test_strip_think_sections_only_think() {
    let input = "<think>只有思考</think>";
    let output = strip_think_sections(input);
    assert_eq!(output, "");
  }

  #[test]
  fn test_strip_think_sections_multiple_unclosed() {
    let input = "result<think>思考1<think>思考2";
    let output = strip_think_sections(input);
    assert_eq!(output, "result");
  }
}
