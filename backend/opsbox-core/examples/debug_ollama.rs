use opsbox_core::llm::debug::{debug_ollama_raw_output, test_ollama_connection};
use opsbox_core::llm::{ChatMessage, Role};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("=== Ollama 调试工具 ===\n");

  // 测试连接
  println!("1. 测试 Ollama 连接...");
  match test_ollama_connection("http://127.0.0.1:11434").await {
    Ok(_) => println!("✓ 连接测试成功\n"),
    Err(e) => {
      eprintln!("✗ 连接测试失败: {}", e);
      return Err(e.into());
    }
  }

  // 测试普通输出
  println!("2. 测试普通输出...");
  let messages = vec![ChatMessage {
    role: Role::User,
    content: "请简单介绍一下你自己，不超过50字。".to_string(),
  }];

  match debug_ollama_raw_output("http://127.0.0.1:11434", "qwen3:8b", messages, false).await {
    Ok(_) => println!("✓ 普通输出测试完成\n"),
    Err(e) => {
      eprintln!("✗ 普通输出测试失败: {}", e);
    }
  }

  // 测试 JSON 格式输出
  println!("3. 测试 JSON 格式输出...");
  let json_messages = vec![
    ChatMessage {
      role: Role::System,
      content: "你是一个助手。请输出 JSON 格式：{\"name\": \"你的名字\", \"description\": \"简短描述\"}".to_string(),
    },
    ChatMessage {
      role: Role::User,
      content: "请按照要求输出 JSON。".to_string(),
    },
  ];

  match debug_ollama_raw_output("http://127.0.0.1:11434", "qwen3:8b", json_messages, true).await {
    Ok(_) => println!("✓ JSON 输出测试完成\n"),
    Err(e) => {
      eprintln!("✗ JSON 输出测试失败: {}", e);
    }
  }

  println!("调试完成！");
  Ok(())
}
