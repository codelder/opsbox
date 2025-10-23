use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
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
  info!("LLM 内容输出: '{}'", &q);
  info!("LLM 内容输出: '{:?}'", resp);

  // 兜底清理：移除 <think> 片段、去掉代码块/外层引号，仅取首行
  let before_strip = q.clone();
  q = strip_think_sections(&q).trim().to_string();
  if q != before_strip {
    debug!("移除<think>片段后: '{}'", &q);
  }
  if q.starts_with("```") && q.ends_with("```") {
    debug!("移除代码块围栏");
    let inner = q.trim_start_matches("```\n").trim_end_matches("\n```");
    q = inner.trim().to_string();
  }
  if (q.starts_with('"') && q.ends_with('"')) || (q.starts_with('\'') && q.ends_with('\'')) {
    debug!("移除引号包围");
    q = q[1..q.len() - 1].to_string();
  }
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
