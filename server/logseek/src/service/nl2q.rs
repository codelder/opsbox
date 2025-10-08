use log::{debug, error, info, warn};
use ollama_rs::Ollama;
use ollama_rs::generation::completion::request::GenerationRequest;
use serde::{Deserialize, Serialize};
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

fn build_prompt(user_nl: &str) -> String {
  // 严格约束输出，仅允许输出一行最终 q 字符串；禁止输出 <think> 思考内容
  format!(
    "你是一名日志检索查询串生成器。请严格遵循以下文档把用户的自然语言需求转换为本站使用的查询字符串（q）。\n\n=== 文档开始 ===\n{}\n=== 文档结束 ===\n\n要求：\n- 仅输出一行最终 q 字符串，不要任何解释或多余符号\n- 不要用引号包裹整个表达式\n- 若用户给出多个同义词，用大写 OR 连接\n- 若是“同一行出现 A 与 B”，用正则 /(A.*B|B.*A)/\n- 不要输出任何 <think> 或 </think> 内容\n\n用户需求：{}\n输出：",
    QUICK_GUIDE,
    user_nl.trim()
  )
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

pub async fn call_ollama(nl: &str) -> Result<String, NL2QError> {
  info!("NL2Q请求: '{}'", nl);

  let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://127.0.0.1".to_string());
  let port: u16 = std::env::var("OLLAMA_PORT")
    .ok()
    .and_then(|s| s.parse().ok())
    .unwrap_or(11434);
  let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen3:8b".to_string());

  debug!("Ollama配置: host={}, port={}, model={}", host, port, model);

  let prompt = build_prompt(nl);
  debug!("构建的提示词长度: {} 字符", prompt.len());

  let client = Ollama::new(host, port);
  let req = GenerationRequest::new(model, prompt);

  debug!("向Ollama发送请求");
  let start = std::time::Instant::now();
  let resp = client.generate(req).await.map_err(|e| {
    error!("Ollama请求失败: {}", e);
    NL2QError::Http(e.to_string())
  })?;

  let duration = start.elapsed();
  info!("Ollama响应耗时: {:?}", duration);

  let mut q = resp.response.trim().to_string();
  debug!("Ollama原始输出: '{}'", &q);

  // 先移除 <think> 思考片段，再做其它清理
  let before_strip = q.clone();
  q = strip_think_sections(&q).trim().to_string();
  if q != before_strip {
    debug!("移除<think>片段后: '{}'", &q);
  }

  // 容错清理——去除围绕的代码块/引号，仅保留单行
  if q.starts_with("```") && q.ends_with("```") {
    debug!("移除代码块围栏");
    // 去掉围栏
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
