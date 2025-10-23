#!/usr/bin/env rust-script
//! 调试 Ollama 原始输出的简单测试

use opsbox_core::llm::{debug_ollama_raw_output, ChatMessage, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Ollama 原始输出调试工具 ===\n");

    // 创建测试消息
    let messages = vec![
        ChatMessage {
            role: Role::User,
            content: "你好，请介绍一下你自己。".to_string(),
        },
    ];

    // 使用默认配置
    let base_url = "http://127.0.0.1:11434";
    let model = "qwen3:8b";

    println!("配置信息:");
    println!("  URL: {}", base_url);
    println!("  模型: {}", model);
    println!();

    // 执行调试
    match debug_ollama_raw_output(base_url, model, messages).await {
        Ok(_) => println!("\n调试完成！"),
        Err(e) => {
            eprintln!("调试失败: {}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
