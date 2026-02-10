//! 测试 Starlark Planner 对 ORL 字符串格式的解析能力
//!
//! 这个测试文件用于验证 Starlark 脚本能够正确输出 ORL 格式的来源列表。
//! 由于 DFS 迁移，plan_with_starlark_with_script 现在返回 Vec<String>，
//! 测试中使用 OrlParser::parse() 解析字符串。

use logseek::domain::source_planner::plan_with_starlark_with_script;
use opsbox_core::dfs::{OrlParser, Location};
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

  assert!(matches!(resource.endpoint.location, Location::Local));
  assert_eq!(resource.primary_path.to_string(), "/var/log/syslog");
}

#[tokio::test]
async fn test_orl_parse_local_with_glob() {
  // 测试带 glob 的 ORL（glob 参数在 query string 中，由 OrlParser 内部处理）
  let resource = OrlParser::parse("orl://local/var/log?glob=*.log").expect("Should parse local ORL with glob");

  assert!(matches!(resource.endpoint.location, Location::Local));
  assert_eq!(resource.primary_path.to_string(), "/var/log");
  // 注意：glob 参数被 OrlParser 解析但 Resource 结构体不直接存储它
  // glob 信息在搜索时通过 SearchRequest.path_includes 使用
}

#[tokio::test]
async fn test_orl_parse_local_archive() {
  // 测试归档路径（通过扩展名自动识别）
  let resource = OrlParser::parse("orl://local/var/log/app.tar.gz").expect("Should parse archive ORL");

  assert!(matches!(resource.endpoint.location, Location::Local));
  assert_eq!(resource.primary_path.to_string(), "/var/log/app.tar.gz");
  assert!(resource.archive_context.is_some());
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
  let resource = OrlParser::parse("orl://myprofile@s3/mybucket/path/to/file.tar.gz").expect("Should parse S3 ORL");

  assert!(matches!(resource.endpoint.location, Location::Cloud));
  assert_eq!(resource.endpoint.identity, "myprofile");
  assert_eq!(resource.primary_path.to_string(), "/mybucket/path/to/file.tar.gz");
  assert!(resource.archive_context.is_some());
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

  // 解析返回的 ORL 字符串
  let orl_str = &result.sources[0];
  let resource = OrlParser::parse(orl_str).expect("Should parse ORL string");

  assert!(matches!(resource.endpoint.location, Location::Local));
  assert_eq!(resource.primary_path.to_string(), "/var/log");
  // glob 参数在 ORL 字符串中，用于搜索时过滤

  println!("✓ Local ORL parsed: {}", orl_str);
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

  // 解析返回的 ORL 字符串
  let orl_str = &result.sources[0];
  let resource = OrlParser::parse(orl_str).expect("Should parse ORL string");

  assert!(matches!(resource.endpoint.location, Location::Remote { .. }));
  assert_eq!(resource.endpoint.identity, "my-agent-id");
  assert_eq!(resource.primary_path.to_string(), "/data/logs");
  // glob 参数在 ORL 字符串中

  println!("✓ Agent ORL parsed: {}", orl_str);
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

  // 解析返回的 ORL 字符串
  let orl_str = &result.sources[0];
  let resource = OrlParser::parse(orl_str).expect("Should parse ORL string");

  assert!(matches!(resource.endpoint.location, Location::Cloud));
  assert_eq!(resource.endpoint.identity, "myprofile");
  assert_eq!(resource.primary_path.to_string(), "/mybucket/logs/app.tar.gz");
  assert!(resource.archive_context.is_some());

  println!("✓ S3 ORL parsed: {}", orl_str);
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
  let r0 = OrlParser::parse(&result.sources[0]).expect("Should parse ORL");
  let r1 = OrlParser::parse(&result.sources[1]).expect("Should parse ORL");
  let r2 = OrlParser::parse(&result.sources[2]).expect("Should parse ORL");

  assert!(matches!(r0.endpoint.location, Location::Local));
  assert!(matches!(r1.endpoint.location, Location::Remote { .. }));
  assert!(matches!(r2.endpoint.location, Location::Cloud));

  println!("✓ Mixed sources parsed successfully");
  for (i, orl_str) in result.sources.iter().enumerate() {
    let r = OrlParser::parse(orl_str).expect("Should parse");
    println!("  Source[{}]: {} (type={:?})", i, orl_str, r.endpoint.location);
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

  let orl_str = &result.sources[0];
  let resource = OrlParser::parse(orl_str).expect("Should parse ORL string");

  assert_eq!(resource.primary_path.to_string(), "/tmp/test/logs");
  // glob 参数在 ORL 字符串中

  println!("✓ f-string interpolation works: {}", orl_str);
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

  let result = plan_with_starlark_with_script(&pool, Some("e2e_local"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1);

  let resource = OrlParser::parse(&result.sources[0]).expect("Should parse ORL");
  assert!(matches!(resource.endpoint.location, Location::Local));
  assert_eq!(resource.primary_path.to_string(), abs_root);

  println!("✓ E2E local script format works");
}

#[tokio::test]
async fn test_e2e_agent_script_format() {
  let pool = create_test_pool().await;

  // 精确复制 E2E 测试中的 Agent 脚本格式
  let agent_id = "test-agent-001";
  let test_logs_dir = "/data/test_logs";
  let script = format!(
    r#"
SOURCES = ["orl://{agent_id}@agent{test_logs_dir}?glob=*.log"]
"#,
    agent_id = agent_id,
    test_logs_dir = test_logs_dir
  );

  logseek::repository::planners::upsert_script(&pool, "e2e_agent", &script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("e2e_agent"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1);

  let resource = OrlParser::parse(&result.sources[0]).expect("Should parse ORL");
  assert!(matches!(resource.endpoint.location, Location::Remote { .. }));
  assert_eq!(resource.endpoint.identity, agent_id);
  assert_eq!(resource.primary_path.to_string(), test_logs_dir);

  println!("✓ E2E agent script format works");
}

#[tokio::test]
async fn test_e2e_s3_script_format() {
  let pool = create_test_pool().await;

  // 精确复制 E2E 测试中的 S3 脚本格式
  let profile = "test-profile";
  let bucket = "test-bucket";
  let key = "logs/archive.tar.gz";
  let script = format!(
    r#"
SOURCES = ["orl://{profile}@s3/{bucket}/{key}?glob=*.log"]
"#,
    profile = profile,
    bucket = bucket,
    key = key
  );

  logseek::repository::planners::upsert_script(&pool, "e2e_s3", &script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("e2e_s3"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1);

  let resource = OrlParser::parse(&result.sources[0]).expect("Should parse ORL");
  assert!(matches!(resource.endpoint.location, Location::Cloud));
  assert_eq!(resource.endpoint.identity, profile);
  assert_eq!(resource.primary_path.to_string(), format!("/{}/{}", bucket, key));
  assert!(resource.archive_context.is_some());

  println!("✓ E2E S3 script format works");
}

#[tokio::test]
async fn test_starlark_with_entry_path() {
  let pool = create_test_pool().await;

  // 测试带 entry 参数的归档 ORL
  let script = r#"
SOURCES = ["orl://local/tmp/archive.tar.gz?entry=internal/service.log"]
"#;

  logseek::repository::planners::upsert_script(&pool, "test_entry", script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("test_entry"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1);

  let resource = OrlParser::parse(&result.sources[0]).expect("Should parse ORL");
  assert!(resource.archive_context.is_some());
  let archive_ctx = resource.archive_context.as_ref().unwrap();
  assert_eq!(archive_ctx.inner_path.to_string(), "internal/service.log");

  println!("✓ ORL with entry path works");
}

#[tokio::test]
async fn test_starlark_with_context_variables() {
  let pool = create_test_pool().await;

  // 测试使用上下文变量构建 ORL
  let script = r#"
# 使用注入的 DATES 变量
if len(DATES) > 0:
    date_str = DATES[0]["yyyymmdd"]
    SOURCES = [f"orl://local/var/log/app.{date_str}.log"]
else:
    SOURCES = []
"#;

  logseek::repository::planners::upsert_script(&pool, "test_context", script)
    .await
    .expect("Should save script");

  let result = plan_with_starlark_with_script(&pool, Some("test_context"), "test query", None)
    .await
    .expect("Should plan successfully");

  assert_eq!(result.sources.len(), 1);

  let resource = OrlParser::parse(&result.sources[0]).expect("Should parse ORL");
  let path = resource.primary_path.to_string();
  assert!(path.starts_with("/var/log/app."));
  assert!(path.ends_with(".log"));
  // 日期格式应该是 YYYYMMDD（8位数字）
  let date_part = path.strip_prefix("/var/log/app.").unwrap().strip_suffix(".log").unwrap();
  assert_eq!(date_part.len(), 8);
  assert!(date_part.chars().all(|c| c.is_ascii_digit()));

  println!("✓ Context variables work: path={}", path);
}
