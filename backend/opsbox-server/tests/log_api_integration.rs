//! Server 日志 API 集成测试
//!
//! 测试日志配置 API 的完整功能：
//! - 获取日志配置
//! - 更新日志级别
//! - 更新日志保留数量
//! - 参数验证

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use opsbox_core::{
    database::{init_pool, DatabaseConfig},
    logging::{repository::LogConfigRepository, LogLevel},
};
use serde_json::json;
use std::sync::Once;
use tempfile::TempDir;
use tower::ServiceExt; // for `oneshot`

// 确保日志系统只初始化一次
static INIT_LOGGING: Once = Once::new();

/// 初始化测试日志系统（只执行一次）
fn init_test_logging() {
    INIT_LOGGING.call_once(|| {
        let temp_dir = TempDir::new().expect("创建临时日志目录失败");
        let log_dir = temp_dir.path().join("logs");
        std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

        let log_config = opsbox_core::logging::LogConfig {
            level: LogLevel::Info,
            log_dir: log_dir.clone(),
            retention_count: 7,
            enable_console: true,
            enable_file: false,
            file_prefix: "test-server".to_string(),
        };
        let reload_handle = opsbox_core::logging::init(log_config).expect("初始化日志系统失败");
        opsbox_server::server::set_log_reload_handle(reload_handle);
        opsbox_server::server::set_log_dir(log_dir);

        // 防止 temp_dir 被 drop
        std::mem::forget(temp_dir);
    });
}

/// 创建测试数据库
async fn create_test_pool() -> (opsbox_core::SqlitePool, TempDir) {
    let temp_dir = TempDir::new().expect("创建临时目录失败");
    let db_path = temp_dir.path().join("test.db");

    let config = DatabaseConfig::new(format!("sqlite://{}", db_path.display()), 5, 30);

    let pool = init_pool(&config).await.expect("初始化数据库失败");

    // 初始化 logging schema
    opsbox_core::logging::run_migration(&pool)
        .await
        .expect("初始化 logging schema 失败");

    (pool, temp_dir)
}

/// 创建测试路由
fn create_test_router(
    pool: opsbox_core::SqlitePool,
    log_dir: std::path::PathBuf,
) -> axum::Router {
    opsbox_server::log_routes::create_log_routes(pool, log_dir)
}

#[tokio::test]
async fn test_get_log_config_success() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 初始化默认配置
    let repo = LogConfigRepository::new(pool.clone());
    repo.update_level("server", LogLevel::Info)
        .await
        .expect("初始化日志级别失败");
    repo.update_retention("server", 7)
        .await
        .expect("初始化保留数量失败");

    // 创建路由
    let app = create_test_router(pool, log_dir.clone());

    // 发送请求
    let request = Request::builder()
        .uri("/api/v1/log/config")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // 验证响应
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["level"], "info");
    assert_eq!(json["retention_count"], 7);
    assert_eq!(json["log_dir"], log_dir.to_str().unwrap());
}

#[tokio::test]
async fn test_update_log_level_success() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 初始化日志系统（全局只执行一次）
    init_test_logging();

    // 创建路由
    let app = create_test_router(pool.clone(), log_dir);

    // 测试更新为 DEBUG 级别
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/level")
        .header("content-type", "application/json")
        .body(Body::from(json!({"level": "debug"}).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();

    // 验证响应
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["message"].as_str().unwrap().contains("debug"));

    // 验证数据库已更新
    let repo = LogConfigRepository::new(pool);
    let config = repo.get("server").await.unwrap();
    assert_eq!(config.level, "debug");
}

#[tokio::test]
async fn test_update_log_level_all_levels() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 初始化日志系统（全局只执行一次）
    init_test_logging();

    // 创建路由
    let app = create_test_router(pool.clone(), log_dir);

    // 测试所有日志级别
    let levels = vec!["error", "warn", "info", "debug", "trace"];

    for level in levels {
        let request = Request::builder()
            .method("PUT")
            .uri("/api/v1/log/level")
            .header("content-type", "application/json")
            .body(Body::from(json!({"level": level}).to_string()))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "更新日志级别 {} 失败",
            level
        );
    }
}

#[tokio::test]
async fn test_update_log_level_invalid() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 初始化日志系统（全局只执行一次）
    init_test_logging();

    // 创建路由
    let app = create_test_router(pool, log_dir);

    // 测试无效的日志级别
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/level")
        .header("content-type", "application/json")
        .body(Body::from(json!({"level": "invalid"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // 验证返回 400 错误
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["error"].as_str().unwrap().contains("无效的日志级别"));
}

#[tokio::test]
async fn test_update_log_retention_success() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 创建路由
    let app = create_test_router(pool.clone(), log_dir);

    // 测试更新保留数量
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/retention")
        .header("content-type", "application/json")
        .body(Body::from(json!({"retention_count": 30}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // 验证响应
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["message"].as_str().unwrap().contains("30"));

    // 验证数据库已更新
    let repo = LogConfigRepository::new(pool);
    let config = repo.get("server").await.unwrap();
    assert_eq!(config.retention_count, 30);
}

#[tokio::test]
async fn test_update_log_retention_boundary_values() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 创建路由
    let app = create_test_router(pool.clone(), log_dir);

    // 测试边界值：最小值 1
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/retention")
        .header("content-type", "application/json")
        .body(Body::from(json!({"retention_count": 1}).to_string()))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 测试边界值：最大值 365
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/retention")
        .header("content-type", "application/json")
        .body(Body::from(json!({"retention_count": 365}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 验证数据库已更新为 365
    let repo = LogConfigRepository::new(pool);
    let config = repo.get("server").await.unwrap();
    assert_eq!(config.retention_count, 365);
}

#[tokio::test]
async fn test_update_log_retention_invalid_zero() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 创建路由
    let app = create_test_router(pool, log_dir);

    // 测试无效值：0
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/retention")
        .header("content-type", "application/json")
        .body(Body::from(json!({"retention_count": 0}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // 验证返回 400 错误
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("保留数量必须在 1-365 之间"));
}

#[tokio::test]
async fn test_update_log_retention_invalid_too_large() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 创建路由
    let app = create_test_router(pool, log_dir);

    // 测试无效值：超过 365
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/retention")
        .header("content-type", "application/json")
        .body(Body::from(json!({"retention_count": 366}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // 验证返回 400 错误
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("保留数量必须在 1-365 之间"));
}

#[tokio::test]
async fn test_concurrent_updates() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 初始化日志系统（全局只执行一次）
    init_test_logging();

    // 创建路由
    let app = create_test_router(pool.clone(), log_dir);

    // 并发更新日志级别
    let mut handles = vec![];
    let levels = vec!["debug", "info", "warn", "error", "trace"];

    for level in levels {
        let app_clone = app.clone();
        let level_str = level.to_string();

        let handle = tokio::spawn(async move {
            let request = Request::builder()
                .method("PUT")
                .uri("/api/v1/log/level")
                .header("content-type", "application/json")
                .body(Body::from(json!({"level": level_str}).to_string()))
                .unwrap();

            app_clone.oneshot(request).await.unwrap()
        });

        handles.push(handle);
    }

    // 等待所有请求完成
    let results = futures::future::join_all(handles).await;

    // 验证所有请求都成功
    for result in results {
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    // 验证最终状态一致
    let repo = LogConfigRepository::new(pool);
    let config = repo.get("server").await.unwrap();
    // 最终级别应该是某个有效级别
    let valid_levels = vec!["error", "warn", "info", "debug", "trace"];
    assert!(
        valid_levels.contains(&config.level.as_str()),
        "最终日志级别应该是有效的级别，实际为: {}",
        config.level
    );
}

#[tokio::test]
async fn test_malformed_json_requests() {
    // 创建测试环境
    let (pool, temp_dir) = create_test_pool().await;
    let log_dir = temp_dir.path().join("logs");
    std::fs::create_dir_all(&log_dir).expect("创建日志目录失败");

    // 创建路由
    let app = create_test_router(pool, log_dir);

    // 测试格式错误的 JSON
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/level")
        .header("content-type", "application/json")
        .body(Body::from("{invalid json"))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();

    // 验证返回错误（400 或 422）
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY
    );

    // 测试缺少必需字段
    let request = Request::builder()
        .method("PUT")
        .uri("/api/v1/log/level")
        .header("content-type", "application/json")
        .body(Body::from(json!({"wrong_field": "debug"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    // 验证返回错误
    assert!(
        response.status() == StatusCode::BAD_REQUEST
            || response.status() == StatusCode::UNPROCESSABLE_ENTITY
    );
}
