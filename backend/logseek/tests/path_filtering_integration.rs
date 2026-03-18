use logseek::repository::planners;
use logseek::service::search::SearchEvent;
use logseek::service::search_executor::SearchExecutor;
use logseek::service::search_executor::SearchExecutorConfig;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::PathBuf;
use std::str::FromStr;

/// 创建测试用的内存数据库连接池
async fn create_test_pool() -> SqlitePool {
  let connect_options = SqliteConnectOptions::from_str("sqlite::memory:")
    .unwrap()
    .create_if_missing(true);

  let pool = SqlitePoolOptions::new()
    .max_connections(1)
    .connect_with(connect_options)
    .await
    .expect("Failed to create test pool");

  logseek::init_schema(&pool).await.expect("Failed to initialize schema");

  pool
}

/// Helper to escape paths for Starlark string literals
fn escape_path_for_starlark(path: &std::path::Path) -> String {
  path.to_string_lossy().replace("\\", "/")
}

/// 跨平台路径匹配：检查路径是否包含指定的部分（支持 / 和 \ 分隔符）
fn path_contains(path: &str, pattern: &str) -> bool {
  // 将两种分隔符都标准化为正斜杠进行比较
  let normalized_path = path.replace('\\', "/");
  normalized_path.contains(pattern)
}

async fn collect_search_results(executor: &SearchExecutor, query: &str) -> Vec<String> {
  let result = executor.search(query, "test-sid".to_string(), 0, None).await;
  match result {
    Ok(mut rx) => {
      let mut paths = Vec::new();
      while let Some(event) = rx.recv().await {
        if let SearchEvent::Success(res) = event {
          paths.push(res.path);
        }
      }
      paths
    }
    Err(_) => Vec::new(),
  }
}

#[tokio::test]
async fn test_path_filtering_combinations() {
  let pool = create_test_pool().await;

  // Create a temporary directory structure
  // root/
  //   src/
  //     main.rs
  //     utils.rs
  //     lib.rs
  //   test/
  //     unit.rs
  //     e2e.rs
  //   docs/
  //     readme.md
  //     api.md
  //   vendor/
  //     dep.rs
  //   config.json
  let temp_dir = tempfile::tempdir().unwrap();
  let root = temp_dir.path();

  std::fs::create_dir(root.join("src")).unwrap();
  std::fs::create_dir(root.join("test")).unwrap();
  std::fs::create_dir(root.join("docs")).unwrap();
  std::fs::create_dir(root.join("vendor")).unwrap();

  let write_file = |path: PathBuf, content: &str| {
    std::fs::write(path, content).unwrap();
  };

  write_file(root.join("src/main.rs"), "fn main() { error }");
  write_file(root.join("src/utils.rs"), "fn utils() { error }");
  write_file(root.join("src/lib.rs"), "fn lib() { error }");
  write_file(root.join("test/unit.rs"), "fn test_unit() { error }");
  write_file(root.join("test/e2e.rs"), "fn test_e2e() { error }");
  write_file(root.join("docs/readme.md"), "# Readme error");
  write_file(root.join("docs/api.md"), "# API error");
  write_file(root.join("vendor/dep.rs"), "fn dep() { error }");
  write_file(root.join("config.json"), "{ \"error\": true }");

  let config = SearchExecutorConfig::default();
  let executor = SearchExecutor::new(pool.clone(), config);

  // ===================================================================================
  // Scenario 1: No ORL Filter
  // ===================================================================================
  let planner_script_no_filter = format!(r#"SOURCES = [ "orl://local/{}" ]"#, escape_path_for_starlark(root));
  planners::upsert_script(&pool, "no_filter", &planner_script_no_filter)
    .await
    .unwrap();
  planners::set_default(&pool, Some("no_filter")).await.unwrap();

  // 1.1 path: (Include) -> path:src/**
  let results = collect_search_results(&executor, "path:src/** error").await;
  assert_eq!(results.len(), 3, "Should match 3 src files");
  assert!(results.iter().all(|p| path_contains(p, "src/")));

  // 1.2 -path: (Exclude) -> -path:vendor/**
  let results = collect_search_results(&executor, "-path:vendor/** error").await;
  // Total 9 files, exclude 1 (vendor/dep.rs) -> 8 expected
  assert_eq!(results.len(), 8);
  assert!(!results.iter().any(|p| path_contains(p, "vendor/")));

  // 1.3 Mixed -> path:src/** -path:**/utils.rs
  let results = collect_search_results(&executor, "path:src/** -path:**/utils.rs error").await;
  // src has 3 files (main, utils, lib). Exclude utils -> 2 expected
  assert_eq!(results.len(), 2);
  assert!(results.iter().any(|p| path_contains(p, "src/main.rs")));
  assert!(results.iter().any(|p| path_contains(p, "src/lib.rs")));
  assert!(!results.iter().any(|p| path_contains(p, "src/utils.rs")));

  // 1.4 Multiple Include -> path:src/** path:test/**
  let results = collect_search_results(&executor, "path:src/** path:test/** error").await;
  // src(3) + test(2) = 5
  assert_eq!(results.len(), 5);
  assert!(results.iter().any(|p| path_contains(p, "src/")));
  assert!(results.iter().any(|p| path_contains(p, "test/")));
  assert!(!results.iter().any(|p| path_contains(p, "docs/")));

  // 1.5 Multiple Exclude -> -path:vendor/** -path:docs/**
  let results = collect_search_results(&executor, "-path:vendor/** -path:docs/** error").await;
  // 9 total - 1 vendor - 2 docs = 6
  assert_eq!(results.len(), 6);
  assert!(!results.iter().any(|p| path_contains(p, "vendor/")));
  assert!(!results.iter().any(|p| path_contains(p, "docs/")));

  // ===================================================================================
  // Scenario 2: With ORL Filter (Base Filter)
  // ===================================================================================
  // Filter: Include **/*.rs
  let planner_script_rs_filter = format!(
    r#"SOURCES = [ "orl://local/{}?glob=**/*.rs" ]"#,
    escape_path_for_starlark(root)
  );
  planners::upsert_script(&pool, "rs_filter", &planner_script_rs_filter)
    .await
    .unwrap();
  planners::set_default(&pool, Some("rs_filter")).await.unwrap();

  // 2.1 Base Filter only
  let results = collect_search_results(&executor, "error").await;
  // src(3) + test(2) + vendor(1) = 6 rs files
  assert_eq!(results.len(), 6);
  assert!(results.iter().all(|p| path_contains(p, ".rs")));
  assert!(!results.iter().any(|p| path_contains(p, ".md")));

  // 2.2 Base Filter + path: (Include) -> Intersection
  // path:src/**. Should match .rs files inside src/.
  let results = collect_search_results(&executor, "path:src/** error").await;
  // src has 3 .rs files.
  assert_eq!(results.len(), 3);
  assert!(results.iter().all(|p| path_contains(p, "src/")));

  // 2.2b Disjoint Intersection: path:docs/** (.md)
  let results = collect_search_results(&executor, "path:docs/** error").await;
  // Base matches .rs, User matches docs(.md). Intersection is empty.
  assert_eq!(results.len(), 0);

  // 2.3 Base + Exclude: -path:vendor/**
  let results = collect_search_results(&executor, "-path:vendor/** error").await;
  // Base: 6 .rs. Exclude vending (dep.rs). Result: 5.
  assert_eq!(results.len(), 5);
  assert!(!results.iter().any(|p| path_contains(p, "vendor/")));

  // 2.4 Complex Combination: Base + Multiple Include + Multiple Exclude
  // Query: "path:src/** path:test/** -path:**/utils.rs -path:**/unit.rs error"
  // Base Filter: **/*.rs
  // Logic:
  //   1. Base matches all .rs files (6 files: src/{main,utils,lib}, test/{unit,e2e}, vendor/dep)
  //   2. User Includes (src/** OR test/**) -> Keeps src/* and test/*. Excludes vendor/dep.rs. (5 files remain)
  //   3. User Excludes (utils.rs OR unit.rs) -> Removes src/utils.rs and test/unit.rs. (3 files remain)
  // Expected: src/main.rs, src/lib.rs, test/e2e.rs
  let results = collect_search_results(
    &executor,
    "path:src/** path:test/** -path:**/utils.rs -path:**/unit.rs error",
  )
  .await;
  assert_eq!(results.len(), 3, "Should match exactly 3 files after complex filtering");
  assert!(results.iter().any(|p| path_contains(p, "src/main.rs")));
  assert!(results.iter().any(|p| path_contains(p, "src/lib.rs")));
  assert!(results.iter().any(|p| path_contains(p, "test/e2e.rs")));
  assert!(!results.iter().any(|p| path_contains(p, "utils.rs")));
  assert!(!results.iter().any(|p| path_contains(p, "unit.rs")));
  assert!(!results.iter().any(|p| path_contains(p, "vendor/")));

  // ===================================================================================
  // Scenario 3: Multiple Sources
  // ===================================================================================
  let planner_script_multi = format!(
    r#"SOURCES = [ "orl://local/{}", "orl://local/{}" ]"#,
    escape_path_for_starlark(&root.join("src")),
    escape_path_for_starlark(&root.join("test"))
  );
  planners::upsert_script(&pool, "multi_source", &planner_script_multi)
    .await
    .unwrap();
  planners::set_default(&pool, Some("multi_source")).await.unwrap();

  // 3.1 Search all (Union of sources)
  let results = collect_search_results(&executor, "error").await;
  // src(3) + test(2) = 5
  assert_eq!(results.len(), 5);

  // 3.2 Filter by file name (since paths are relative to source roots)
  // "main.rs" is in src, "unit.rs" is in test.
  let results = collect_search_results(&executor, "path:main.rs error").await;
  assert_eq!(results.len(), 1);
  assert!(results[0].contains("main.rs"));

  // 3.3 Exclude one file from one source
  let results = collect_search_results(&executor, "-path:unit.rs error").await;
  // 5 total - 1 (unit.rs) = 4
  assert_eq!(results.len(), 4);
  assert!(!results.iter().any(|p| p.contains("unit.rs")));

  // ===================================================================================
  // Scenario 4: Edge Cases & Boundaries
  // ===================================================================================
  // Reset to "root" source planner (Scenario 1 style) so paths include "src/"
  planners::set_default(&pool, Some("no_filter")).await.unwrap();

  // 4.1 Total Exclusion (Self-canceling)
  // path:src/** -path:src/** -> Should be empty
  let results = collect_search_results(&executor, "path:src/** -path:src/** error").await;
  assert_eq!(results.len(), 0, "Include covered by Exclude should return 0 results");

  // 4.2 Overlapping Includes (Subset + Superset)
  // path:src/main.rs path:src/** -> Should be Union (effectively src/**)
  // This verifies that we don't duplicate results or fail on overlapping globs.
  let results = collect_search_results(&executor, "path:src/main.rs path:src/** error").await;
  // Should match all 3 src files
  assert_eq!(results.len(), 3);
  assert!(results.iter().any(|p| path_contains(p, "src/utils.rs"))); // Covered by src/**

  // 4.3 Deep Matching / Exact File Match
  // Exactly matching one deep file (use glob pattern for cross-platform compatibility)
  let results = collect_search_results(&executor, "path:**/lib.rs error").await;
  assert_eq!(results.len(), 1);
  assert!(path_contains(&results[0], "lib.rs"));
}

#[tokio::test]
async fn test_path_filter_uses_relative_path_for_directory_sources() {
  let pool = create_test_pool().await;

  // Create a directory tree where the absolute path contains "dir22",
  // but the relative path under the source root does not.
  let temp_dir = tempfile::tempdir().unwrap();
  let source_root = temp_dir.path().join("dir22");
  let target_dir = source_root.join("home/appadm/logs/msk");
  std::fs::create_dir_all(&target_dir).unwrap();

  let target_file = target_dir.join("nohup-route.log");
  std::fs::write(&target_file, "abcdef\n").unwrap();

  let planner_script = format!(r#"SOURCES = [ "orl://local/{}" ]"#, escape_path_for_starlark(&source_root));
  planners::upsert_script(&pool, "relative_path_filter", &planner_script)
    .await
    .unwrap();
  planners::set_default(&pool, Some("relative_path_filter")).await.unwrap();

  let executor = SearchExecutor::new(pool, SearchExecutorConfig::default());

  // This matches against the relative path "home/appadm/logs/msk/nohup-route.log".
  let results = collect_search_results(&executor, "abcdef path:**/nohup*.log").await;
  assert_eq!(results.len(), 1, "nohup*.log should match the relative path");
  assert!(path_contains(&results[0], "dir22/home/appadm/logs/msk/nohup-route.log"));

  // If filtering used the absolute path, *22* would match "dir22".
  // The current search pipeline strips the source root first, so this should not match.
  let results = collect_search_results(&executor, "abcdef path:**/*22*/**/nohup*.log").await;
  assert_eq!(
    results.len(),
    0,
    "dir22 is outside the relative path used by directory-source filtering"
  );
}
