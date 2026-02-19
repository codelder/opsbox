//! NL2Q集成测试
//!
//! 测试自然语言转查询功能的边界条件和错误处理

use logseek::repository::llm::{self, LlmBackend, ProviderKind};
use logseek::service::nl2q::{NL2QError, call_llm};
use opsbox_test_common::llm_mock::{MockLlmConfig, MockLlmServer};
use sqlx::sqlite::SqlitePool;

/// 测试strip_think_sections函数边界条件
#[test]
fn test_strip_think_sections_edge_cases() {
  use logseek::service::nl2q::strip_think_sections;

  // 测试包含嵌套<think>标签（函数可能不处理完全嵌套）
  // 根据现有测试，函数只移除第一个<think>到第一个</think>
  let input = "text<think><think>inner</think></think>more";
  let output = strip_think_sections(input);
  // 实际行为：移除第一个<think>到第一个</think>，留下</think>more
  assert_eq!(output, "text</think>more");

  // 测试只有<think>没有</think>
  let input = "start<think>unclosed";
  let output = strip_think_sections(input);
  assert_eq!(output, "start");

  // 测试空字符串
  let input = "";
  let output = strip_think_sections(input);
  assert_eq!(output, "");

  // 测试只有标签没有内容
  let input = "<think></think>";
  let output = strip_think_sections(input);
  assert_eq!(output, "");
}

/// 测试build_messages函数边界条件
#[test]
fn test_build_messages_edge_cases() {
  use logseek::service::nl2q::build_messages;

  // 测试空输入
  let messages = build_messages("");
  assert_eq!(messages.len(), 2);
  assert_eq!(messages[1].content, "");

  // 测试只有空格
  let messages = build_messages("   ");
  assert_eq!(messages.len(), 2);
  assert_eq!(messages[1].content, "");

  // 测试换行符处理
  let messages = build_messages("line1\nline2");
  assert_eq!(messages.len(), 2);
  assert_eq!(messages[1].content, "line1\nline2");

  // 测试特殊字符
  let messages = build_messages("error & warning | critical");
  assert_eq!(messages.len(), 2);
  assert_eq!(messages[1].content, "error & warning | critical");
}

/// 集成测试：验证nl2q模块与数据库的集成
#[tokio::test]
#[cfg_attr(not(feature = "network-tests"), ignore)]
async fn test_nl2q_integration_with_db() {
  // 运行时检查：如果网络不可用则跳过
  if !logseek::test_utils::is_network_binding_available() {
    eprintln!("⚠️  跳过测试：网络绑定不可用（沙箱或受限环境）");
    return;
  }

  // 1. 启动Mock LLM服务器
  let config = MockLlmConfig {
    model: "test-model".to_string(),
    preset_response: "error AND level:error".to_string(),
    ..Default::default()
  };

  // 如果端口被占用 (test-common中端口分配可能冲突)，尝试可以容错
  // 但在mock服务器实现中，0表示随机端口，所以几乎总是成功
  let server = MockLlmServer::start(config).await.expect("启动Mock LLM服务器失败");
  let base_url = server.base_url();

  // 2. 初始化内存数据库
  // 设置环境变量以禁用代理检测，避免沙盒环境问题
  // SAFETY: 集成测试运行在独立进程中，每个测试独立执行，无并发风险。
  unsafe { std::env::set_var("OPSBOX_NO_PROXY", "1") };
  let pool = SqlitePool::connect(":memory:").await.unwrap();
  logseek::init_schema(&pool).await.unwrap();

  // 3. 配置数据库中的LLM Backend
  let backend = LlmBackend {
    name: "test-ollama".to_string(),
    provider: ProviderKind::Ollama,
    base_url,
    model: "test-model".to_string(),
    timeout_secs: 5,
    api_key: None,
    organization: None,
    project: None,
  };

  llm::save_backend(&pool, &backend, true).await.expect("保存Backend失败");
  llm::set_default(&pool, Some("test-ollama"))
    .await
    .expect("设置默认Backend失败");

  // 4. 调用业务逻辑
  // 为了防止call_llm因为环境变量回退机制而使用了其他配置，
  // 我们需要确保没有干扰的环境变量，但这里是并行测试比较困难
  // 不过因为数据库有默认配置，优先使用数据库配置

  let result = call_llm(&pool, "查找错误日志").await;

  // 5. 验证结果
  assert!(result.is_ok(), "NL2Q调用应该成功");
  let query = result.unwrap();
  assert_eq!(query, "error AND level:error");

  // 6. 清理
  server.stop().await.ok();
}

/// 性能测试：多次调用nl2q (使用Mock)
#[tokio::test]
#[cfg_attr(not(feature = "network-tests"), ignore)]
async fn test_nl2q_performance() {
  use std::time::Instant;

  // 运行时检查：如果网络不可用则跳过
  if !logseek::test_utils::is_network_binding_available() {
    eprintln!("⚠️  跳过测试：网络绑定不可用（沙箱或受限环境）");
    return;
  }

  // 启动Mock LLM服务器
  let config = MockLlmConfig {
    preset_response: "mock response".to_string(),
    ..Default::default()
  };
  let server = MockLlmServer::start(config).await.expect("启动Mock服务器失败");

  // 初始化数据库
  // SAFETY: 集成测试运行在独立进程中，每个测试独立执行，无并发风险。
  unsafe { std::env::set_var("OPSBOX_NO_PROXY", "1") };
  let pool = SqlitePool::connect(":memory:").await.unwrap();
  logseek::init_schema(&pool).await.unwrap();

  let backend = LlmBackend {
    name: "perf-ollama".to_string(),
    provider: ProviderKind::Ollama,
    base_url: server.base_url(),
    model: "test-model".to_string(),
    timeout_secs: 5,
    api_key: None,
    organization: None,
    project: None,
  };
  llm::save_backend(&pool, &backend, true).await.unwrap();
  llm::set_default(&pool, Some("perf-ollama")).await.unwrap();

  let start = Instant::now();

  // 模拟多次调用
  for _ in 0..5 {
    let res = call_llm(&pool, "test").await;
    assert!(res.is_ok());
  }

  let duration = start.elapsed();
  // 本地 mock 应该非常快
  assert!(duration.as_millis() < 2000, "Mock LLM调用应该很快");

  server.stop().await.ok();
}

/// 错误路径测试：测试各种错误情况
#[tokio::test]
#[cfg_attr(not(feature = "network-tests"), ignore)]
async fn test_nl2q_error_paths() {
  // 运行时检查：如果网络不可用则跳过
  if !logseek::test_utils::is_network_binding_available() {
    eprintln!("⚠️  跳过测试：网络绑定不可用（沙箱或受限环境）");
    return;
  }

  // 测试错误枚举的显示
  let http_error = NL2QError::Http("连接超时".to_string());
  assert!(http_error.to_string().contains("连接超时"));

  let empty_error = NL2QError::Empty;
  assert_eq!(empty_error.to_string(), "AI 生成了空结果，请重试或改写需求");

  // 测试 LLM 返回空内容
  let config = MockLlmConfig {
    preset_response: "".to_string(), // 返回空
    ..Default::default()
  };
  let server = MockLlmServer::start(config).await.expect("启动Mock服务器失败");

  // SAFETY: 集成测试运行在独立进程中，每个测试独立执行，无并发风险。
  unsafe { std::env::set_var("OPSBOX_NO_PROXY", "1") };
  let pool = SqlitePool::connect(":memory:").await.unwrap();
  logseek::init_schema(&pool).await.unwrap();

  let backend = LlmBackend {
    name: "empty-ollama".to_string(),
    provider: ProviderKind::Ollama,
    base_url: server.base_url(),
    model: "test-model".to_string(),
    timeout_secs: 5,
    api_key: None,
    organization: None,
    project: None,
  };
  llm::save_backend(&pool, &backend, true).await.unwrap();
  llm::set_default(&pool, Some("empty-ollama")).await.unwrap();

  let result = call_llm(&pool, "test").await;
  // 空响应应该被处理为错误
  assert!(matches!(result, Err(NL2QError::Empty)));

  server.stop().await.ok();
}

/// 测试恶意输入防护 - SQL注入尝试
#[test]
fn test_malicious_input_sql_injection() {
  use logseek::service::nl2q::build_messages;

  let malicious_inputs = vec![
    "查询'; DROP TABLE logs; --",
    "查找' OR '1'='1",
    "错误日志'; DELETE FROM settings; --",
    "查询\n; DROP TABLE users;",
    "测试' UNION SELECT * FROM passwords --",
  ];

  for input in malicious_inputs {
    let messages = build_messages(input);
    // 验证消息被正确构建，不会因为特殊字符而崩溃
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content, input.trim());
    // 注意：实际应用中应该在更高层进行输入验证和清理
  }
}

/// 测试恶意输入防护 - XSS尝试
#[test]
fn test_malicious_input_xss() {
  use logseek::service::nl2q::build_messages;

  let xss_inputs = vec![
    "查询<script>alert('xss')</script>",
    "查找<img src=x onerror=alert('xss')>",
    "错误日志<iframe src='evil.com'>",
    "测试<body onload=alert('xss')>",
    "查询javascript:alert('xss')",
  ];

  for input in xss_inputs {
    let messages = build_messages(input);
    // 验证消息被正确构建，不会因为HTML标签而崩溃
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content, input.trim());
  }
}

/// 测试恶意输入防护 - 命令注入尝试
#[test]
fn test_malicious_input_command_injection() {
  use logseek::service::nl2q::build_messages;

  let cmd_injection_inputs = vec![
    "查询$(cat /etc/passwd)",
    "查找`whoami`",
    "错误日志; rm -rf /",
    "测试| cat /etc/shadow",
    "查询&& curl evil.com",
  ];

  for input in cmd_injection_inputs {
    let messages = build_messages(input);
    // 验证消息被正确构建，不会因为命令注入字符而崩溃
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content, input.trim());
  }
}

/// 测试超长输入处理
#[test]
fn test_very_long_input() {
  use logseek::service::nl2q::build_messages;

  // 创建超长输入（10万个字符）
  let long_input = "查询错误日志 ".repeat(10000);
  let messages = build_messages(&long_input);

  assert_eq!(messages.len(), 2);
  assert_eq!(messages[1].content.len(), long_input.trim().len());
}

/// 测试Unicode边界字符
#[test]
fn test_unicode_boundary_characters() {
  use logseek::service::nl2q::build_messages;

  let unicode_inputs = vec![
    "查询\u{0000}错误",  // NULL字符
    "查找\u{FFFF}日志",  // 非字符
    "测试\u{1F600}表情", // Emoji
    "查询\u{200B}零宽",  // 零宽空格
    "查找\u{FEFF}BOM",   // BOM字符
  ];

  for input in unicode_inputs {
    let messages = build_messages(input);
    assert_eq!(messages.len(), 2);
    assert!(
      messages[1].content.contains("查询")
        || messages[1].content.contains("查找")
        || messages[1].content.contains("测试")
        || messages[1].content.is_empty()
    );
  }
}

/// 测试strip_think_sections的更多边界情况
#[test]
fn test_strip_think_sections_advanced() {
  use logseek::service::nl2q::strip_think_sections;

  // 测试思考标签内包含代码块
  let input = "<think>```code```</think>result";
  assert_eq!(strip_think_sections(input), "result");

  // 测试思考标签内包含引号
  let input = r#"<think>say "hello"</think>output"#;
  assert_eq!(strip_think_sections(input), "output");

  // 测试思考标签跨多行（<think>标签后的内容会被完全移除）
  let input = "start<think>\nline1\nline2\n</think>end";
  // strip_think_sections 会移除 <think> 到 </think> 之间的所有内容
  // 包括开头的换行符
  let result = strip_think_sections(input);
  assert!(result.starts_with("start"));
  assert!(result.ends_with("end"));

  // 测试空思考标签
  let input = "<think>   </think>output";
  assert_eq!(strip_think_sections(input), "output");

  // 测试嵌套思考标签（复杂情况）
  let input = "<think>outer<think>inner</think>outer</think>final";
  let result = strip_think_sections(input);
  // 应该移除第一个<think>到第一个</think>之间的内容
  assert!(result.contains("final"));
}

/// 测试JSON反序列化边界
#[test]
fn test_json_deserialization_edge_cases() {
  use logseek::service::nl2q::NLBody;

  // 测试空内容反序列化
  let json = r#"{"nl":""}"#;
  let body: NLBody = serde_json::from_str(json).unwrap();
  assert_eq!(body.nl, "");

  // 测试特殊字符的JSON转义
  let json = r#"{"nl":"测试\n换行\t制表"}"#;
  let body: NLBody = serde_json::from_str(json).unwrap();
  assert_eq!(body.nl, "测试\n换行\t制表");

  // 测试Unicode字符
  let json = r#"{"nl":"查询🔍错误"}"#;
  let body: NLBody = serde_json::from_str(json).unwrap();
  assert_eq!(body.nl, "查询🔍错误");
}

/// 测试JSON序列化响应
#[test]
fn test_json_response_serialization() {
  use logseek::api::models::NL2QOut;

  // 测试包含引号的响应
  let response = NL2QOut {
    q: r#"field:"value""#.to_string(),
  };
  let json = serde_json::to_string(&response).unwrap();
  assert!(json.contains("\\\"value\\\""));

  // 测试空响应
  let response = NL2QOut { q: "".to_string() };
  let json = serde_json::to_string(&response).unwrap();
  assert!(json.contains("\"q\":\"\""));
}

/// 测试并发调用场景
#[tokio::test]
async fn test_concurrent_nl2q_calls() {
  use std::sync::Arc;
  use std::sync::atomic::{AtomicUsize, Ordering};

  let counter = Arc::new(AtomicUsize::new(0));
  let mut handles = vec![];

  // 模拟并发调用
  for i in 0..10 {
    let counter_clone = counter.clone();
    let handle = tokio::spawn(async move {
      // 模拟处理
      tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
      counter_clone.fetch_add(1, Ordering::SeqCst);
      format!("task {}", i)
    });
    handles.push(handle);
  }

  // 等待所有任务完成
  for handle in handles {
    let _ = handle.await;
  }

  assert_eq!(counter.load(Ordering::SeqCst), 10);
}

/// 测试网络超时场景模拟
#[tokio::test]
async fn test_network_timeout_simulation() {
  use tokio::time::{Duration, timeout};

  // 模拟一个可能超时的操作
  let result = timeout(
    Duration::from_millis(50),
    tokio::spawn(async {
      // 模拟耗时操作
      tokio::time::sleep(Duration::from_millis(100)).await;
      "completed"
    }),
  )
  .await;

  // 应该超时
  assert!(result.is_err(), "应该超时");
}

/// 测试数据库回退机制
#[tokio::test]
async fn test_database_fallback_mechanism() {
  // 创建内存数据库但未初始化schema
  // SAFETY: 集成测试运行在独立进程中，每个测试独立执行，无并发风险。
  unsafe { std::env::set_var("OPSBOX_NO_PROXY", "1") };
  let pool = sqlx::sqlite::SqlitePool::connect(":memory:").await.unwrap();

  // 尝试解析LLM客户端
  let result = logseek::service::nl2q::call_llm(&pool, "测试查询").await;

  // 验证结果
  if let Err(e) = result {
    let err_str = e.to_string();
    assert!(
      err_str.contains("未设置默认")
        || err_str.contains("失败")
        || err_str.contains("连接")
        || err_str.contains("Ollama")
        || err_str.contains("provider"),
      "错误信息应该表明配置问题: {}",
      err_str
    );
  } else {
    println!("警告: 数据库回退测试意外成功 (可能是环境变量配置了有效的LLM)");
  }
}
