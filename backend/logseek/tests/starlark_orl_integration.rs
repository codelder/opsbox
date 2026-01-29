//! 测试 Starlark Planner 对 ORL 字符串格式的解析能力
//!
//! 这个测试文件用于调试 E2E 测试失败的问题，通过单元测试验证
//! Starlark 脚本能够正确输出 ORL 格式的来源列表。

use logseek::domain::source_planner::plan_with_starlark_with_script;
use opsbox_core::odfs::orl::{EndpointType, ORL, TargetType};
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
  let orl = ORL::parse("orl://local/var/log/syslog").expect("Should parse local ORL");

  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Local);
  assert_eq!(orl.path(), "/var/log/syslog");
  assert!(orl.filter_glob().is_none());
  assert!(orl.entry_path().is_none());
  assert_eq!(orl.target_type(), TargetType::Dir);
}

#[tokio::test]
async fn test_orl_parse_local_with_glob() {
  // 测试带 glob 的 ORL
  let orl = ORL::parse("orl://local/var/log?glob=*.log").expect("Should parse local ORL with glob");

  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Local);
  assert_eq!(orl.path(), "/var/log");
  assert_eq!(orl.filter_glob().unwrap().as_ref(), "*.log");
  assert_eq!(orl.target_type(), TargetType::Dir);
}

#[tokio::test]
async fn test_orl_parse_local_archive() {
  // 测试归档路径（通过扩展名自动识别）
  let orl = ORL::parse("orl://local/var/log/app.tar.gz").expect("Should parse archive ORL");

  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Local);
  assert_eq!(orl.path(), "/var/log/app.tar.gz");
  assert_eq!(orl.target_type(), TargetType::Archive);
}

#[tokio::test]
async fn test_orl_parse_agent_with_id() {
  // 测试 Agent ORL (新格式: id@agent)
  let orl = ORL::parse("orl://agent-123@agent/data/logs").expect("Should parse agent ORL");

  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Agent);
  assert_eq!(orl.endpoint_id(), Some("agent-123"));
  assert_eq!(orl.effective_id().as_ref(), "agent-123");
  assert_eq!(orl.path(), "/data/logs");
}

#[tokio::test]
async fn test_orl_parse_s3_with_profile() {
  // 测试 S3 ORL (新格式: profile@s3)
  let orl = ORL::parse("orl://myprofile@s3/mybucket/path/to/file.tar.gz").expect("Should parse S3 ORL");

  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::S3);
  assert_eq!(orl.endpoint_id(), Some("myprofile"));
  assert_eq!(orl.effective_id().as_ref(), "myprofile");
  assert_eq!(orl.path(), "/mybucket/path/to/file.tar.gz");
  assert_eq!(orl.target_type(), TargetType::Archive);
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

  let orl = &result.sources[0];
  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Local);
  assert_eq!(orl.path(), "/var/log");
  assert_eq!(orl.filter_glob().unwrap().as_ref(), "*.log");

  println!("✓ Local ORL parsed: {}", orl);
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

  let orl = &result.sources[0];
  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Agent);
  assert_eq!(orl.effective_id().as_ref(), "my-agent-id");
  assert_eq!(orl.path(), "/data/logs");
  assert_eq!(orl.filter_glob().unwrap().as_ref(), "*.log");

  println!("✓ Agent ORL parsed: {}", orl);
}

#[tokio::test]
async fn test_starlark_returns_orl_strings_s3() {
  let pool = create_test_pool().await;

  // 使用 E2E 测试相同的格式
  let script = r#"
SOURCES = ["orl://myprofile@s3/mybucket/logs/app.tar.gz?glob=*.log"]
"#;

  logseek::repository::planners::upsert_script(&pool, "test_s3", script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("test_s3"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1, "Should return 1 source");

  let orl = &result.sources[0];
  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::S3);
  assert_eq!(orl.effective_id().as_ref(), "myprofile");
  assert_eq!(orl.path(), "/mybucket/logs/app.tar.gz");
  assert_eq!(orl.filter_glob().unwrap().as_ref(), "*.log");
  assert_eq!(orl.target_type(), TargetType::Archive);

  println!("✓ S3 ORL parsed: {}", orl);
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
  assert_eq!(result.sources[0].endpoint_type().unwrap(), EndpointType::Local);
  assert_eq!(result.sources[1].endpoint_type().unwrap(), EndpointType::Agent);
  assert_eq!(result.sources[2].endpoint_type().unwrap(), EndpointType::S3);

  println!("✓ Mixed sources parsed successfully");
  for (i, orl) in result.sources.iter().enumerate() {
    println!("  Source[{}]: {} (type={:?})", i, orl, orl.endpoint_type());
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

  let orl = &result.sources[0];
  assert_eq!(orl.path(), "/tmp/test/logs");
  assert_eq!(orl.filter_glob().unwrap().as_ref(), "*.log");

  println!("✓ f-string interpolation works: {}", orl);
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

  let orl = &result.sources[0];
  println!("Parsed ORL: {}", orl);
  println!("  endpoint_type: {:?}", orl.endpoint_type());
  println!("  path: {}", orl.path());
  println!("  glob: {:?}", orl.filter_glob());
  println!("  target_type: {:?}", orl.target_type());

  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Local);
  assert_eq!(orl.path(), abs_root);
  assert_eq!(orl.filter_glob().unwrap().as_ref(), "*.log");

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

  let orl = &result.sources[0];
  println!("Parsed ORL: {}", orl);
  println!("  endpoint_type: {:?}", orl.endpoint_type());
  println!("  effective_id: {}", orl.effective_id());
  println!("  path: {}", orl.path());
  println!("  glob: {:?}", orl.filter_glob());

  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::Agent);
  assert_eq!(orl.effective_id().as_ref(), agent_id);
  assert_eq!(orl.path(), test_logs_dir);

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
SOURCES = ["orl://{}@s3/{}/{}?glob=*.log"]
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

  let orl = &result.sources[0];
  println!("Parsed ORL: {}", orl);
  println!("  endpoint_type: {:?}", orl.endpoint_type());
  println!("  effective_id: {}", orl.effective_id());
  println!("  path: {}", orl.path());
  println!("  glob: {:?}", orl.filter_glob());
  println!("  target_type: {:?}", orl.target_type());

  assert_eq!(orl.endpoint_type().unwrap(), EndpointType::S3);
  assert_eq!(orl.effective_id().as_ref(), profile);
  // 路径应该是 /bucket/key
  assert_eq!(orl.path(), format!("/{}/{}", bucket, key));
  assert_eq!(orl.target_type(), TargetType::Archive);

  println!("✓ E2E S3 script format works correctly");
}
