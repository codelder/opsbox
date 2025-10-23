#!/usr/bin/env rust-script
//! 调试 Ollama 原始输出的测试脚本

use opsbox_core::llm::{debug_ollama_raw_output, ChatMessage, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 设置日志级别为 debug 以查看详细输出
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    println!("开始调试 Ollama 原始输出...\n");

    // 测试消息
    let messages = vec![
        ChatMessage {
            role: Role::System,
            content: "你是一个有用的助手。".to_string(),
        },
        ChatMessage {
            role: Role::User,
            content: "请简单介绍一下你自己。".to_string(),
        },
    ];

    // 从环境变量获取配置，或使用默认值
    let base_url = std::env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
    let model = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen3:8b".to_string());

    println!("使用配置:");
    println!("  基础 URL: {}", base_url);
    println!("  模型: {}", model);
    println!();

    // 调用调试函数
    match debug_ollama_raw_output(&base_url, &model, messages).await {
        Ok(response) => {
            println!("调试完成！原始响应已输出到控制台。");
            
            // 尝试解析响应
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response) {
                println!("\n=== 解析后的 JSON 结构 ===");
                println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
                println!("==========================");
            } else {
                println!("\n响应不是有效的 JSON 格式");
            }
        }
        Err(e) => {
            eprintln!("调试失败: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
