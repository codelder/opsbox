//! 测试 SearchExecutor 端到端流程
//!
//! 验证 SearchExecutor 能够正确处理 ORL 来源并返回搜索结果

use logseek::repository::planners;
use logseek::service::search::SearchEvent;
use logseek::service::search_executor::{SearchExecutor, SearchExecutorConfig};
use opsbox_core::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::fs::File;
use std::io::Write;
use std::str::FromStr;
use tempfile::tempdir;

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

#[tokio::test]
async fn test_search_with_orl_local_source() {
    let pool = create_test_pool().await;

    // 创建临时目录和测试文件
    let temp_dir = tempdir().unwrap();
    let log_file = temp_dir.path().join("test.log");
    let mut file = File::create(&log_file).unwrap();
    writeln!(file, "2025-01-01 INFO This is a test log line UNIQUE_MARKER_123").unwrap();
    drop(file);

    let abs_path = temp_dir.path().to_string_lossy().to_string();

    // 使用 ORL 格式的脚本
    let script = format!(
        r#"
SOURCES = ["orl://local{}?glob=*.log"]
"#,
        abs_path
    );

    println!("Script:\n{}", script);
    println!("Temp dir: {}", abs_path);
    println!("Log file exists: {}", log_file.exists());

    planners::upsert_script(&pool, "test_orl_local", &script)
        .await
        .unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 执行搜索
    let result = executor.search("app:test_orl_local UNIQUE_MARKER_123", 1, None).await;

    match &result {
        Ok(_) => println!("Search initiated successfully"),
        Err(e) => println!("Search failed: {:?}", e),
    }

    assert!(result.is_ok(), "Search should succeed: {:?}", result.err());

    let (mut rx, sid) = result.unwrap();
    println!("Search SID: {}", sid);

    // 收集结果
    let mut success_count = 0;
    let mut error_count = 0;
    let mut complete_count = 0;

    while let Some(event) = rx.recv().await {
        match &event {
            SearchEvent::Success(res) => {
                success_count += 1;
                println!(
                    "SUCCESS: path={}, lines_count={}, merged={:?}",
                    res.path,
                    res.lines.len(),
                    res.merged
                );
            }
            SearchEvent::Error {
                source,
                message,
                recoverable,
            } => {
                error_count += 1;
                println!(
                    "ERROR: source={}, message={}, recoverable={}",
                    source, message, recoverable
                );
            }
            SearchEvent::Complete { source, elapsed_ms } => {
                complete_count += 1;
                println!("COMPLETE: source={}, elapsed={}ms", source, elapsed_ms);
            }
        }
    }

    println!(
        "\nResults: success={}, error={}, complete={}",
        success_count, error_count, complete_count
    );

    // 应该有至少 1 个成功结果
    assert!(
        success_count >= 1,
        "Should have at least 1 success result, got {} (errors: {})",
        success_count,
        error_count
    );
}



#[tokio::test]
async fn test_search_with_relative_glob_pattern() {
    let pool = create_test_pool().await;

    // 创建临时目录结构
    // root/
    //   sub/
    //     target.log   <-- should match */*.log
    //   root.log       <-- should NOT match */*.log
    let temp_dir = tempdir().unwrap();
    let root = temp_dir.path();

    let sub_dir = root.join("sub");
    std::fs::create_dir_all(&sub_dir).unwrap();

    // 创建 root.log（不应匹配 */*.log）
    let root_log = root.join("root.log");
    let mut f = File::create(&root_log).unwrap();
    writeln!(f, "ROOT_LOG_CONTENT should not match").unwrap();
    drop(f);

    // 创建 sub/target.log（应匹配 */*.log）
    let target_log = sub_dir.join("target.log");
    let mut f = File::create(&target_log).unwrap();
    writeln!(f, "TARGET_LOG_CONTENT_789 should match").unwrap();
    drop(f);

    let abs_root = root.to_string_lossy().to_string();

    // 使用相对 glob 模式
    let script = format!(
        r#"
SOURCES = ["orl://local{}?glob=*/*.log"]
"#,
        abs_root
    );

    println!("Script:\n{}", script);
    println!("Root dir: {}", abs_root);
    println!("root.log exists: {}", root_log.exists());
    println!("sub/target.log exists: {}", target_log.exists());

    planners::upsert_script(&pool, "test_relative_glob", &script)
        .await
        .unwrap();

    let config = SearchExecutorConfig::default();
    let executor = SearchExecutor::new(pool, config);

    // 搜索只应该匹配 sub/target.log 中的内容
    let result = executor
        .search("app:test_relative_glob TARGET_LOG_CONTENT_789", 1, None)
        .await;

    assert!(result.is_ok(), "Search should succeed: {:?}", result.err());

    let (mut rx, _sid) = result.unwrap();

    let mut success_count = 0;
    let mut matched_paths = Vec::new();

    while let Some(event) = rx.recv().await {
        match event {
            SearchEvent::Success(res) => {
                success_count += 1;
                matched_paths.push(res.path.clone());
                println!("SUCCESS: path={}", res.path);
            }
            SearchEvent::Error { source, message, .. } => {
                println!("ERROR: source={}, message={}", source, message);
            }
            SearchEvent::Complete { source, elapsed_ms } => {
                println!("COMPLETE: source={}, elapsed={}ms", source, elapsed_ms);
            }
        }
    }

    println!("\nMatched paths: {:?}", matched_paths);

    assert_eq!(
        success_count, 1,
        "Should match exactly 1 file (sub/target.log), got {} matches: {:?}",
        success_count, matched_paths
    );

    // 验证匹配的是 sub/target.log，而不是 root.log
    assert!(
        matched_paths[0].contains("target.log"),
        "Should match target.log, but matched: {}",
        matched_paths[0]
    );
}

#[tokio::test]
async fn test_create_entry_stream_with_orl() {
    use logseek::service::entry_stream::create_entry_stream;
    use opsbox_core::odfs::orl::ORL;

    let pool = create_test_pool().await;

    // 创建临时文件
    let temp_dir = tempdir().unwrap();
    let log_file = temp_dir.path().join("factory_test.log");
    let mut file = File::create(&log_file).unwrap();
    writeln!(file, "Factory test content").unwrap();
    drop(file);

    let abs_path = temp_dir.path().to_string_lossy().to_string();

    // 创建 ORL
    let orl_str = format!("orl://local{}?glob=*.log", abs_path);
    let orl = ORL::parse(&orl_str).expect("Should parse ORL");

    println!("Testing create_entry_stream with ORL: {}", orl);
    println!("  endpoint_type: {:?}", orl.endpoint_type());
    println!("  path: {}", orl.path());
    println!("  target_type: {:?}", orl.target_type());

    // 使用 create_entry_stream 创建流
    let stream_result = create_entry_stream(&pool, &orl).await;

    match &stream_result {
        Ok(_) => println!("EntryStream created successfully"),
        Err(e) => println!("EntryStream creation failed: {}", e),
    }

    assert!(
        stream_result.is_ok(),
        "create_entry_stream should create stream: {:?}",
        stream_result.err()
    );

    // 验证能读取条目
    let mut stream = stream_result.unwrap();
    let mut entry_count = 0;

    while let Ok(Some((meta, _reader))) = stream.next_entry().await {
        entry_count += 1;
        println!("Entry: path={}, source={:?}", meta.path, meta.source);
    }

    println!("Total entries: {}", entry_count);

    assert!(
        entry_count >= 1,
        "Should have at least 1 entry, got {}",
        entry_count
    );
}
