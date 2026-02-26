//! Explorer Integration Tests - Local Files
//!
//! Tests for local file system browsing functionality using ORL protocol.

use explorer::service::ExplorerService;
use opsbox_core::database::{DatabaseConfig, init_pool};
use tempfile::TempDir;
use tokio::fs;

async fn create_test_pool() -> (opsbox_core::SqlitePool, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("test.db");

    let config = DatabaseConfig::new(
        format!("sqlite://{}", db_path.display()),
        5,
        30
    );

    let pool = init_pool(&config).await.expect("Failed to init pool");
    (pool, temp_dir)
}

#[tokio::test]
async fn test_list_local_directory_with_files() {
    // Setup: Create test pool and temp directory
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    // Create test files
    let test_dir = TempDir::new().expect("Failed to create test dir");
    fs::write(test_dir.path().join("file1.txt"), "content1").await.unwrap();
    fs::write(test_dir.path().join("file2.log"), "content2").await.unwrap();

    // Build ORL for local directory
    let orl = format!("orl://local{}", test_dir.path().display());

    // Execute: List directory
    let result = service.list(&orl).await;

    // Assert: Should succeed with 2 files
    assert!(result.is_ok(), "List should succeed");
    let items = result.unwrap();
    assert_eq!(items.len(), 2, "Should have 2 files");

    // Verify file names
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"file1.txt"), "Should contain file1.txt");
    assert!(names.contains(&"file2.log"), "Should contain file2.log");
}

#[tokio::test]
async fn test_list_local_empty_directory() {
    // Setup
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    let test_dir = TempDir::new().expect("Failed to create test dir");
    let orl = format!("orl://local{}", test_dir.path().display());

    // Execute
    let result = service.list(&orl).await;

    // Assert
    assert!(result.is_ok(), "List should succeed");
    let items = result.unwrap();
    assert_eq!(items.len(), 0, "Empty directory should have 0 items");
}

#[cfg(unix)]
#[tokio::test]
async fn test_list_local_with_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    // Setup
    let (pool, _temp_dir) = create_test_pool().await;
    let service = ExplorerService::new(pool);

    let test_dir = TempDir::new().expect("Failed to create test dir");

    // Create a subdirectory with no read permissions
    let restricted_dir = test_dir.path().join("restricted");
    fs::create_dir(&restricted_dir).await.unwrap();
    fs::set_permissions(&restricted_dir, PermissionsExt::from_mode(0o000))
        .await
        .unwrap();

    let orl = format!("orl://local{}", restricted_dir.display());

    // Execute
    let result = service.list(&orl).await;

    // Cleanup: Restore permissions before temp dir cleanup
    fs::set_permissions(&restricted_dir, PermissionsExt::from_mode(0o755))
        .await
        .ok();

    // Assert: Should fail with permission error
    assert!(result.is_err(), "Should fail with permission denied");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.to_lowercase().contains("permission") ||
        err_msg.to_lowercase().contains("denied") ||
        err_msg.to_lowercase().contains("access"),
        "Error should mention permission: {}",
        err_msg
    );
}
