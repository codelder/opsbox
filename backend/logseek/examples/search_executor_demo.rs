//! SearchExecutor 可复用性演示
//!
//! 此示例展示 SearchExecutor 如何在不同场景下复用：
//! 1. HTTP 路由层（Web API）
//! 2. CLI 工具（命令行搜索）
//! 3. 定时任务（后台批处理）
//!
//! 运行方式：
//! ```bash
//! cargo run --example search_executor_demo
//! ```

use logseek::service::search::SearchEvent;
use logseek::service::search_executor::{SearchExecutor, SearchExecutorConfig};
use opsbox_core::SqlitePool;

/// 场景 1: HTTP 路由层使用
///
/// 在 Web API 中使用 SearchExecutor 处理搜索请求
async fn http_route_usage(pool: SqlitePool, query: &str) {
  println!("=== 场景 1: HTTP 路由层使用 ===");

  // 创建搜索执行器
  let config = SearchExecutorConfig::default();
  let executor = SearchExecutor::new(pool, config);

  // 执行搜索
  match executor.search(query, "test-sid".to_string(), 3, None).await {
    Ok(mut rx) => {


      // 消费搜索结果并转换为 NDJSON 流
      while let Some(event) = rx.recv().await {
        match event {
          SearchEvent::Success(result) => {
            println!("✓ 找到匹配: {} ({} 行)", result.path, result.lines.len());
          }
          SearchEvent::Complete { source, elapsed_ms } => {
            println!("✓ 数据源完成: {} (耗时 {}ms)", source, elapsed_ms);
          }
          SearchEvent::Error { source, message, .. } => {
            println!("✗ 数据源错误: {} - {}", source, message);
          }
        }
      }
    }
    Err(e) => {
      println!("搜索失败: {}", e);
    }
  }
}

/// 场景 2: CLI 工具使用
///
/// 在命令行工具中使用 SearchExecutor 进行交互式搜索
async fn cli_tool_usage(pool: SqlitePool, query: &str) {
  println!("\n=== 场景 2: CLI 工具使用 ===");

  // 创建搜索执行器（可以使用不同的配置）
  let config = SearchExecutorConfig {
    io_max_concurrency: 20, // CLI 工具可以使用更高的并发
    stream_channel_capacity: 256,
  };
  let executor = SearchExecutor::new(pool, config);

  // 执行搜索
  match executor.search(query, "test-sid".to_string(), 5, None).await {
    Ok(mut rx) => {
      let mut total_matches = 0;

      // 实时显示搜索结果
      while let Some(event) = rx.recv().await {
        if let SearchEvent::Success(result) = event {
          total_matches += result.lines.len();
          println!("  📄 {}: {} 行匹配", result.path, result.lines.len());
        }
      }

      println!("\n总共找到 {} 行匹配", total_matches);
    }
    Err(e) => {
      eprintln!("搜索失败: {}", e);
    }
  }
}

/// 场景 3: 定时任务使用
///
/// 在后台定时任务中使用 SearchExecutor 进行批量搜索
async fn scheduled_task_usage(pool: SqlitePool, queries: Vec<&str>) {
  println!("\n=== 场景 3: 定时任务使用 ===");

  // 创建搜索执行器（可以复用同一个实例）
  let config = SearchExecutorConfig::default();
  let executor = SearchExecutor::new(pool, config);

  // 批量执行多个搜索任务
  for query in queries {
    println!("\n处理查询: {}", query);

    match executor.search(query, "test-sid".to_string(), 2, None).await {
      Ok(mut rx) => {
        let mut result_count = 0;

        // 收集结果用于后续处理（如发送告警、生成报告等）
        while let Some(event) = rx.recv().await {
          if let SearchEvent::Success(_) = event {
            result_count += 1;
          }
        }

        println!("  ✓ 查询完成: 结果数={}", result_count);
      }
      Err(e) => {
        println!("  ✗ 查询失败: {}", e);
      }
    }
  }
}

#[tokio::main]
async fn main() {
  // 创建内存数据库用于演示
  let pool = SqlitePool::connect(":memory:").await.expect("无法创建数据库连接");

  // 初始化数据库 schema
  // 注意：实际使用时需要调用 logseek::init_schema(&pool).await

  let test_query = "error app:myapp";

  // 演示不同场景下的使用
  http_route_usage(pool.clone(), test_query).await;
  cli_tool_usage(pool.clone(), test_query).await;
  scheduled_task_usage(pool.clone(), vec!["error", "warning", "critical"]).await;

  println!("\n=== 演示完成 ===");
  println!("SearchExecutor 可以在以下场景复用：");
  println!("  ✓ HTTP API 路由层");
  println!("  ✓ CLI 命令行工具");
  println!("  ✓ 后台定时任务");
  println!("  ✓ 其他需要搜索功能的场景");
}
