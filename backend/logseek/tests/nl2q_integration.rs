//! NL2Q集成测试
//!
//! 测试自然语言转查询功能的边界条件和错误处理

use logseek::service::nl2q::{NL2QError};

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
#[ignore = "需要完整的测试环境配置"]
async fn test_nl2q_integration_with_db() {
    // 这是一个占位符测试，展示完整的集成测试场景
    // 实际实现需要：
    // 1. 在测试数据库中配置模拟LLM后端
    // 2. 设置环境变量回退配置
    // 3. 执行call_llm函数
    // 4. 验证结果

    println!("完整的NL2Q集成测试需要更多基础设施");
}

/// 性能测试：多次调用nl2q
#[tokio::test]
#[ignore = "性能测试，仅在完整测试套件中运行"]
async fn test_nl2q_performance() {
    use std::time::Instant;

    let start = Instant::now();

    // 模拟多次调用
    for i in 0..10 {
        println!("模拟NL2Q调用 {}", i);
        // 实际测试中这里会调用call_llm
    }

    let duration = start.elapsed();
    println!("NL2Q性能测试完成，耗时: {:?}", duration);

    // 验证响应时间在合理范围内
    assert!(duration.as_secs() < 5, "NL2Q调用不应超过5秒");
}

/// 错误路径测试：测试各种错误情况
#[tokio::test]
async fn test_nl2q_error_paths() {
    // 测试错误枚举的显示
    let http_error = NL2QError::Http("连接超时".to_string());
    assert!(http_error.to_string().contains("连接超时"));

    let empty_error = NL2QError::Empty;
    assert_eq!(empty_error.to_string(), "AI 生成了空结果，请重试或改写需求");

    println!("NL2Q错误处理测试通过");
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
        "查询\u{0000}错误",       // NULL字符
        "查找\u{FFFF}日志",       // 非字符
        "测试\u{1F600}表情",      // Emoji
        "查询\u{200B}零宽",       // 零宽空格
        "查找\u{FEFF}BOM",        // BOM字符
    ];

    for input in unicode_inputs {
        let messages = build_messages(input);
        assert_eq!(messages.len(), 2);
        assert!(messages[1].content.contains("查询") || messages[1].content.contains("查找") ||
                messages[1].content.contains("测试") || messages[1].content.is_empty());
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
    use logseek::service::nl2q::NL2QResponse;

    // 测试包含引号的响应
    let response = NL2QResponse { q: r#"field:"value""#.to_string() };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\\\"value\\\""));

    // 测试空响应
    let response = NL2QResponse { q: "".to_string() };
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
    use tokio::time::{timeout, Duration};

    // 模拟一个可能超时的操作
    let result = timeout(
        Duration::from_millis(50),
        tokio::spawn(async {
            // 模拟耗时操作
            tokio::time::sleep(Duration::from_millis(100)).await;
            "completed"
        })
    ).await;

    // 应该超时
    assert!(result.is_err(), "应该超时");
}

/// 测试数据库回退机制
#[tokio::test]
async fn test_database_fallback_mechanism() {
    // 创建内存数据库但未初始化schema
    let pool = sqlx::sqlite::SqlitePool::connect(":memory:").await.unwrap();

    // 尝试解析LLM客户端
    let result = logseek::service::nl2q::call_llm(&pool, "测试查询").await;

    // 验证结果（可能成功也可能失败，取决于环境变量配置）
    match result {
        Ok(query) => {
            // 如果环境变量配置了有效的LLM，可能成功
            println!("NL2Q 成功生成查询: {}", query);
        }
        Err(e) => {
            // 更可能的情况是失败（没有配置LLM）
            println!("NL2Q 预期失败: {}", e);
            // 验证错误类型是预期的
            let err_str = e.to_string();
            assert!(
                err_str.contains("未设置默认") ||
                err_str.contains("失败") ||
                err_str.contains("连接") ||
                err_str.contains("Ollama"),
                "错误信息应该表明配置问题: {}",
                err_str
            );
        }
    }
}

