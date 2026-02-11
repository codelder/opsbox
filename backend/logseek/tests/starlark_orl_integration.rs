//! 测试 Starlark Planner 对 ORL 字符串格式的解析能力
//!
//! 这个测试文件用于调试 E2E 测试失败的问题，通过单元测试验证
//! Starlark 脚本能够正确输出 ORL 格式的来源列表。

use logseek::domain::source_planner::plan_with_starlark_with_script;
use opsbox_core::dfs::{Location, OrlParser};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

/// 创建测试用的内存数据库连接池
async fn create_test_pool() -> opsbox_core::SqlitePool {
  let connect_options = SqliteConnectOptions::from_str("sqlite::memory:")
    .unwrap()
    .create_if_missing(true);

  let pool = SqlitePoolOptions::new()
    .max_connections(1)
    .connect_with(connect_options)
    .await
    .expect("Failed to create test pool");

  // 初始化 schema
  logseek::init_schema(&pool).await.expect("Failed to initialize schema");

  pool
}

#[tokio::test]
async fn test_orl_parse_local_path() {
  // 测试基本 ORL 解析
  let resource = OrlParser::parse("orl://local/var/log/syslog").expect("Should parse local ORL");

  assert_eq!(resource.endpoint.location, Location::Local);
  assert_eq!(resource.primary_path.to_string(), "/var/log/syslog");
  assert!(resource.archive_context.is_none());
}

#[tokio::test]
async fn test_orl_parse_local_with_glob() {
  // 测试带 glob 的 ORL (在 DFS 中，glob 通过 filter_glob 参数处理)
  let resource = OrlParser::parse("orl://local/var/log?glob=*.log").expect("Should parse local ORL with glob");

  assert_eq!(resource.endpoint.location, Location::Local);
  assert_eq!(resource.primary_path.to_string(), "/var/log");
}

#[tokio::test]
async fn test_orl_parse_local_archive() {
  // 测试归档路径（DFS 需要显式的 ?entry= 参数）
  let resource = OrlParser::parse("orl://local/var/log/app.tar.gz?entry=inner.log").expect("Should parse archive ORL");

  assert_eq!(resource.endpoint.location, Location::Local);
  assert_eq!(resource.primary_path.to_string(), "/var/log/app.tar.gz");
  assert!(resource.archive_context.is_some(), "Should have archive context");
  if let Some(ctx) = &resource.archive_context {
    assert_eq!(ctx.inner_path.to_string(), "inner.log");
  }
}

#[tokio::test]
async fn test_orl_parse_agent_with_id() {
  // 测试 Agent ORL (新格式: id@agent)
  let resource = OrlParser::parse("orl://agent-123@agent/data/logs").expect("Should parse agent ORL");

  assert!(matches!(resource.endpoint.location, Location::Remote { .. }));
  assert_eq!(resource.endpoint.identity, "agent-123");
  assert_eq!(resource.primary_path.to_string(), "/data/logs");
}

#[tokio::test]
async fn test_orl_parse_s3_with_profile() {
  // 测试 S3 ORL (新格式: profile@s3)
  let resource = OrlParser::parse("orl://myprofile@s3/mybucket/path/to/file.tar.gz?entry=inner.log").expect("Should parse S3 ORL");

  assert_eq!(resource.endpoint.location, Location::Cloud);
  assert_eq!(resource.endpoint.identity, "myprofile");
  assert_eq!(resource.primary_path.to_string(), "/mybucket/path/to/file.tar.gz");
  assert!(resource.archive_context.is_some(), "Should have archive context");
  if let Some(ctx) = &resource.archive_context {
    assert_eq!(ctx.inner_path.to_string(), "inner.log");
  }
}

#[tokio::test]
async fn test_starlark_returns_orl_strings_local() {
  let pool = create_test_pool().await;

  // 注册测试脚本
  let script = r#"
SOURCES = ["orl://local/var/log?glob=*.log"]
"#;

  logseek::repository::planners::upsert_script(&pool, "test_local", script)
    .await
    .expect("Should save script");

  // 执行规划
  let result = plan_with_starlark_with_script(&pool, Some("test_local"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1, "Should return 1 source");

  let resource = &result.sources[0];
  assert_eq!(resource.endpoint.location, Location::Local);
  assert_eq!(resource.primary_path.to_string(), "/var/log");

  println!("✓ Local ORL parsed: {:?}", resource);
}

#[tokio::test]
async fn test_starlark_returns_orl_strings_agent() {
  let pool = create_test_pool().await;

  // 使用 E2E 测试相同的格式
  let script = r#"
SOURCES = ["orl://my-agent-id@agent/data/logs?glob=*.log"]
"#;

  logseek::repository::planners::upsert_script(&pool, "test_agent", script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("test_agent"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1, "Should return 1 source");

  let resource = &result.sources[0];
  assert!(matches!(resource.endpoint.location, Location::Remote { .. }));
  assert_eq!(resource.endpoint.identity, "my-agent-id");
  assert_eq!(resource.primary_path.to_string(), "/data/logs");

  println!("✓ Agent ORL parsed: {:?}", resource);
}

#[tokio::test]
async fn test_starlark_returns_orl_strings_s3() {
  let pool = create_test_pool().await;

  // 使用 E2E 测试相同的格式（需要显式 ?entry= 参数）
  let script = r#"
SOURCES = ["orl://myprofile@s3/mybucket/logs/app.tar.gz?glob=*.log&entry=service.log"]
"#;

  logseek::repository::planners::upsert_script(&pool, "test_s3", script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("test_s3"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1, "Should return 1 source");

  let resource = &result.sources[0];
  assert_eq!(resource.endpoint.location, Location::Cloud);
  assert_eq!(resource.endpoint.identity, "myprofile");
  assert_eq!(resource.primary_path.to_string(), "/mybucket/logs/app.tar.gz");
  assert!(resource.archive_context.is_some(), "Should have archive context");
  if let Some(ctx) = &resource.archive_context {
    assert_eq!(ctx.inner_path.to_string(), "service.log");
  }

  println!("✓ S3 ORL parsed: {:?}", resource);
}

#[tokio::test]
async fn test_starlark_multiple_sources() {
  let pool = create_test_pool().await;

  // 测试多个来源（混合类型）
  let script = r#"
SOURCES = [
    "orl://local/var/log?glob=*.log",
    "orl://agent-1@agent/data/logs?glob=*.log",
    "orl://profile1@s3/bucket1/archive.tar.gz?glob=*.log"
]
"#;

  logseek::repository::planners::upsert_script(&pool, "test_mixed", script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("test_mixed"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 3, "Should return 3 sources");

  // 验证每个来源类型
  assert_eq!(result.sources[0].endpoint.location, Location::Local);
  assert!(matches!(result.sources[1].endpoint.location, Location::Remote { .. }));
  assert_eq!(result.sources[2].endpoint.location, Location::Cloud);

  println!("✓ Mixed sources parsed successfully");
  for (i, resource) in result.sources.iter().enumerate() {
    println!("  Source[{}]: {:?} (location={:?})", i, resource, resource.endpoint.location);
  }
}

#[tokio::test]
async fn test_starlark_with_fstring_interpolation() {
  let pool = create_test_pool().await;

  // 测试 f-string 插值（E2E 测试中使用了模板字符串）
  // 在实际 E2E 中，${absRoot} 会被 JavaScript 模板替换
  // 这里模拟 Starlark 中使用 f-string
  let script = r#"
base_path = "/tmp/test/logs"
SOURCES = [f"orl://local{base_path}?glob=*.log"]
"#;

  logseek::repository::planners::upsert_script(&pool, "test_fstring", script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("test_fstring"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1, "Should return 1 source");

  let resource = &result.sources[0];
  assert_eq!(resource.primary_path.to_string(), "/tmp/test/logs");

  println!("✓ f-string interpolation works: {:?}", resource);
}

#[tokio::test]
async fn test_e2e_local_script_format() {
  let pool = create_test_pool().await;

  // 精确复制 E2E 测试中的脚本格式
  let abs_root = "/tmp/e2e_test_logs";
  let script = format!(
    r#"
SOURCES = ["orl://local{}?glob=*.log"]
"#,
    abs_root
  );

  logseek::repository::planners::upsert_script(&pool, "e2e_local", &script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("e2e_local"), "testquery", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1, "Should return 1 source");

  let resource = &result.sources[0];
  println!("Parsed Resource: {:?}", resource);
  println!("  location: {:?}", resource.endpoint.location);
  println!("  primary_path: {}", resource.primary_path);

  assert_eq!(resource.endpoint.location, Location::Local);
  assert_eq!(resource.primary_path.to_string(), abs_root);

  println!("✓ E2E local script format works correctly");
}

#[tokio::test]
async fn test_e2e_agent_script_format() {
  let pool = create_test_pool().await;

  // 精确复制 E2E 测试中的 Agent 脚本格式
  let agent_id = "e2e-test-agent";
  let test_logs_dir = "/tmp/agent_logs";
  let script = format!(
    r#"
SOURCES = ["orl://{}@agent{}?glob=*.log"]
"#,
    agent_id, test_logs_dir
  );

  logseek::repository::planners::upsert_script(&pool, "e2e_agent", &script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("e2e_agent"), "testquery", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1, "Should return 1 source");

  let resource = &result.sources[0];
  println!("Parsed Resource: {:?}", resource);
  println!("  location: {:?}", resource.endpoint.location);
  println!("  identity: {}", resource.endpoint.identity);
  println!("  primary_path: {}", resource.primary_path);

  assert!(matches!(resource.endpoint.location, Location::Remote { .. }));
  assert_eq!(resource.endpoint.identity, agent_id);
  assert_eq!(resource.primary_path.to_string(), test_logs_dir);

  println!("✓ E2E agent script format works correctly");
}

#[tokio::test]
async fn test_e2e_s3_script_format() {
  let pool = create_test_pool().await;

  // 精确复制 E2E 测试中的 S3 脚本格式
  let profile = "e2e_s3_profile";
  let bucket = "logs-bucket";
  let key = "2025/01/app.tar.gz";
  let script = format!(
    r#"
SOURCES = ["orl://{}@s3/{}/{}?glob=*.log&entry=service.log"]
"#,
    profile, bucket, key
  );

  logseek::repository::planners::upsert_script(&pool, "e2e_s3", &script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("e2e_s3"), "testquery", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1, "Should return 1 source");

  let resource = &result.sources[0];
  println!("Parsed Resource: {:?}", resource);
  assert_eq!(resource.endpoint.location, Location::Cloud);
  assert_eq!(resource.endpoint.identity, profile);
  assert_eq!(resource.primary_path.to_string(), format!("/{}/{}", bucket, key));
  // archive_context should be present when ?entry= is specified
  assert!(resource.archive_context.is_some(), "Should have archive context with ?entry=");
  println!("  location: {:?}", resource.endpoint.location);
  println!("  identity: {}", resource.endpoint.identity);
  println!("  primary_path: {}", resource.primary_path);
  println!("  archive_context: {:?}", resource.archive_context);

  assert_eq!(resource.endpoint.location, Location::Cloud);
  assert_eq!(resource.endpoint.identity, profile);
  // 路径应该是 /bucket/key
  assert_eq!(resource.primary_path.to_string(), format!("/{}/{}", bucket, key));
  assert!(resource.archive_context.is_some(), "Should have archive context");

  println!("✓ E2E S3 script format works correctly");
}
