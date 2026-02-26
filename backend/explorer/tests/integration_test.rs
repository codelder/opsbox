//! Explorer Integration Tests - Local Files
//!
//! Tests for local file system browsing functionality using ORL protocol.

use explorer::service::ExplorerService;
use opsbox_test_common::database::TestDatabase;
use tempfile::TempDir;
use tokio::fs;

#[cfg(feature = "agent-manager")]
use opsbox_test_common::agent_mock;

#[tokio::test]
async fn test_list_local_directory_with_files() {
    // Setup: Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    // Create test files
    let test_dir = TempDir::new().expect("Failed to create test directory");
    fs::write(test_dir.path().join("file1.txt"), "content1")
        .await
        .expect("Failed to create test file file1.txt");
    fs::write(test_dir.path().join("file2.log"), "content2")
        .await
        .expect("Failed to create test file file2.log");

    // Build ORL for local directory
    let orl = format!("orl://local{}", test_dir.path().display());

    // Execute: List directory
    let result = service.list(&orl).await;

    // Assert: Should succeed with 2 files
    assert!(result.is_ok(), "List should succeed");
    let items = result.expect("Failed to unwrap list result");
    assert_eq!(items.len(), 2, "Should have 2 files");

    // Verify file names
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"file1.txt"), "Should contain file1.txt");
    assert!(names.contains(&"file2.log"), "Should contain file2.log");
}

#[tokio::test]
async fn test_list_local_empty_directory() {
    // Setup: Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    let test_dir = TempDir::new().expect("Failed to create test directory");
    let orl = format!("orl://local{}", test_dir.path().display());

    // Execute
    let result = service.list(&orl).await;

    // Assert
    assert!(result.is_ok(), "List should succeed");
    let items = result.expect("Failed to unwrap list result");
    assert_eq!(items.len(), 0, "Empty directory should have 0 items");
}

#[cfg(unix)]
#[tokio::test]
async fn test_list_local_with_permission_denied() {
    use std::os::unix::fs::PermissionsExt;

    // Setup: Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    let test_dir = TempDir::new().expect("Failed to create test directory");

    // Create a subdirectory with no read permissions
    let restricted_dir = test_dir.path().join("restricted");
    fs::create_dir(&restricted_dir)
        .await
        .expect("Failed to create restricted directory");
    fs::set_permissions(&restricted_dir, PermissionsExt::from_mode(0o000))
        .await
        .expect("Failed to set restricted permissions");

    let orl = format!("orl://local{}", restricted_dir.display());

    // Execute
    let result = service.list(&orl).await;

    // Cleanup: Restore permissions before temp dir cleanup
    fs::set_permissions(&restricted_dir, PermissionsExt::from_mode(0o755))
        .await
        .ok();

    // Assert: Should fail with permission error
    assert!(result.is_err(), "Should fail with permission denied");
    let err_msg = result.expect_err("Should have error for permission denied");
    assert!(
        err_msg.to_lowercase().contains("permission") ||
        err_msg.to_lowercase().contains("denied") ||
        err_msg.to_lowercase().contains("access"),
        "Error should mention permission: {}",
        err_msg
    );
}

// ============================================================================
// Agent File Browsing Tests
// ============================================================================

#[cfg(feature = "agent-manager")]
#[tokio::test]
async fn test_list_agent_files_success() {
    // Setup: Find available port and start mock agent server
    let port = agent_mock::find_available_port(
        opsbox_test_common::constants::AGENT_PORT_START,
        opsbox_test_common::constants::AGENT_PORT_END,
    )
    .expect("Failed to find available port");
    let mock_server = agent_mock::start_mock_agent_server(port)
        .await
        .expect("Failed to start mock agent");

    // Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    // Register mock agent (ORL format: agent-id@agent.host:port)
    let orl = format!("orl://test-agent@agent.127.0.0.1:{}/logs", port);

    // Execute: List agent files
    let result = service.list(&orl).await;

    // Cleanup first (so cleanup happens even if assertion fails)
    mock_server.stop().await.ok();

    // Assert: Verify result structure
    assert!(
        result.is_ok(),
        "List should succeed: {:?}",
        result.err()
    );

    let items = result.unwrap();
    // Verify we got a valid Vec (even if empty from mock)
    // items.len() always >= 0, so we just verify we can iterate
    // If mock returns data, verify structure
    for item in &items {
        assert!(!item.name.is_empty(), "Item name should not be empty");
    }
}

#[cfg(feature = "agent-manager")]
#[tokio::test]
async fn test_list_agent_with_offline_agent() {
    // Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    // Use non-existent agent (offline)
    let orl = "orl://offline-agent@agent.127.0.0.1:9999/logs";

    // Execute
    let result = service.list(&orl).await;

    // Assert: Should fail with connection error
    assert!(result.is_err(), "Should fail for offline agent");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.to_lowercase().contains("connection") ||
        err_msg.to_lowercase().contains("timeout") ||
        err_msg.to_lowercase().contains("unreachable") ||
        err_msg.to_lowercase().contains("failed"),
        "Error should indicate connection issue: {}",
        err_msg
    );
}

#[cfg(feature = "agent-manager")]
#[tokio::test]
async fn test_list_agent_with_network_error() {
    // Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    // Use invalid port (network error)
    let orl = "orl://error-agent@agent.127.0.0.1:1/logs";

    // Execute
    let result = service.list(&orl).await;

    // Assert: Should fail with network error
    assert!(result.is_err(), "Should fail with network error");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.to_lowercase().contains("connection") ||
        err_msg.to_lowercase().contains("refused") ||
        err_msg.to_lowercase().contains("error"),
        "Error should indicate network issue: {}",
        err_msg
    );
}
