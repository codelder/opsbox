//! Explorer Integration Tests - Local Files
//!
//! Tests for local file system browsing functionality using ORL protocol.

use explorer::service::ExplorerService;
use opsbox_test_common::database::TestDatabase;
use tempfile::TempDir;
use tokio::fs;
use std::fs::File;
use tar::Builder;
use flate2::Compression;
use flate2::write::GzEncoder;

#[cfg(feature = "agent-manager")]
use opsbox_test_common::agent_mock;

/// Convert a path to ORL-compatible format (forward slashes)
/// On Windows, paths like `C:\Users\...` become `C:/Users/...`
/// On Unix, paths remain unchanged
fn path_to_orl<P: AsRef<std::path::Path>>(path: P) -> String {
  path.as_ref().to_string_lossy().replace('\\', "/")
}

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
    let orl = format!("orl://local{}", path_to_orl(test_dir.path()));

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
    let orl = format!("orl://local{}", path_to_orl(test_dir.path()));

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

    let orl = format!("orl://local{}", path_to_orl(&restricted_dir));

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

// ============================================================================
// Archive Navigation Tests
// ============================================================================

/// Helper function to create a test tar archive with log files
fn create_test_tar_archive(dir: &std::path::Path) -> std::path::PathBuf {
    let archive_path = dir.join("test.tar");

    // Create test files (blocking)
    let file1 = dir.join("file1.log");
    let file2 = dir.join("file2.log");
    std::fs::write(&file1, "log content 1\n").expect("Failed to create file1");
    std::fs::write(&file2, "log content 2\n").expect("Failed to create file2");

    // Create tar archive (blocking)
    let file = File::create(&archive_path).expect("Failed to create tar file");
    let mut builder = Builder::new(file);
    builder
        .append_path_with_name(&file1, "logs/file1.log")
        .expect("Failed to add file1 to tar");
    builder
        .append_path_with_name(&file2, "logs/file2.log")
        .expect("Failed to add file2 to tar");
    builder.finish().expect("Failed to finish tar");

    // Cleanup temp files (blocking)
    std::fs::remove_file(&file1).ok();
    std::fs::remove_file(&file2).ok();

    archive_path
}

/// Helper function to create a test tar.gz archive
fn create_test_tar_gz_archive(dir: &std::path::Path) -> std::path::PathBuf {
    use std::io::Read;

    // First create tar (blocking)
    let tar_path = create_test_tar_archive(dir);
    let gz_path = dir.join("test.tar.gz");

    // Compress to gz (blocking)
    let mut input = File::open(&tar_path).expect("Failed to read tar");
    let mut tar_data = Vec::new();
    input.read_to_end(&mut tar_data).expect("Failed to read tar data");

    let file = File::create(&gz_path).expect("Failed to create gz file");
    let mut encoder = GzEncoder::new(file, Compression::default());
    std::io::Write::write_all(&mut encoder, &tar_data).expect("Failed to compress");
    encoder.finish().expect("Failed to finish compression");

    std::fs::remove_file(&tar_path).ok();
    gz_path
}

/// Helper function to test downloading entire archive files
/// Used to verify that archives can be downloaded as-is without extraction
async fn assert_download_entire_archive(
    create_archive_fn: fn(&std::path::Path) -> std::path::PathBuf,
    expected_filename_contains: &str,
) {
    use tokio::io::AsyncReadExt;

    // Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    // Create test archive using provided function
    let test_dir = TempDir::new().expect("Failed to create test dir");
    let archive_path = create_archive_fn(test_dir.path());

    // Read original archive bytes for comparison
    let original_bytes = tokio::fs::read(&archive_path)
        .await
        .expect("Failed to read original archive");

    // Build ORL for archive WITHOUT entry parameter (download entire file)
    let orl = format!("orl://local{}", path_to_orl(&archive_path));

    // Execute: Download entire archive file
    let result = service.download(&orl).await;

    // Assert: Should succeed (not fail with "Entry '/' not found in archive")
    assert!(
        result.is_ok(),
        "Download entire archive should succeed, but got error: {:?}",
        result.err()
    );

    let (filename, size, mut stream) = result.unwrap();

    // Verify filename is the archive name
    assert!(
        filename.contains(expected_filename_contains),
        "Filename should contain '{}', got: {}",
        expected_filename_contains,
        filename
    );

    // Verify size matches original
    assert_eq!(
        size,
        Some(original_bytes.len() as u64),
        "Downloaded size should match original archive size"
    );

    // Read downloaded content
    let mut downloaded_content = Vec::new();
    stream
        .read_to_end(&mut downloaded_content)
        .await
        .expect("Failed to read downloaded content");

    // Verify content matches original archive (byte-for-byte)
    assert_eq!(
        downloaded_content, original_bytes,
        "Downloaded content should match original archive bytes"
    );
}

#[tokio::test]
async fn test_navigate_tar_archive() {
    // Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    // Create test tar archive
    let test_dir = TempDir::new().expect("Failed to create test dir");
    let archive_path = create_test_tar_archive(test_dir.path());

    // Build ORL for archive (navigate to logs directory inside archive)
    let orl = format!(
        "orl://local{}?entry=logs",
        path_to_orl(&archive_path)
    );

    // Execute: List archive contents
    let result = service.list(&orl).await;

    // Assert
    assert!(
        result.is_ok(),
        "List archive should succeed: {:?}",
        result.err()
    );
    let items = result.unwrap();

    // Verify we got files from the archive
    assert!(items.len() >= 2, "Should have at least 2 files in archive");

    // Verify file names
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert!(
        names.iter().any(|n| n.contains("file1.log")),
        "Should contain file1.log"
    );
    assert!(
        names.iter().any(|n| n.contains("file2.log")),
        "Should contain file2.log"
    );
}

#[tokio::test]
async fn test_navigate_tar_gz_archive() {
    // Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    // Create test tar.gz archive
    let test_dir = TempDir::new().expect("Failed to create test dir");
    let archive_path = create_test_tar_gz_archive(test_dir.path());

    // Build ORL for compressed archive
    let orl = format!(
        "orl://local{}?entry=logs",
        path_to_orl(&archive_path)
    );

    // Execute: List compressed archive contents
    let result = service.list(&orl).await;

    // Assert
    assert!(
        result.is_ok(),
        "List tar.gz should succeed: {:?}",
        result.err()
    );
    let items = result.unwrap();

    // Verify we got files from the archive
    assert!(items.len() >= 2, "Should have at least 2 files");
}

#[tokio::test]
async fn test_download_archive_entry() {
    use tokio::io::AsyncReadExt;

    // Create test database and service
    let db = TestDatabase::file_based()
        .await
        .expect("Failed to create test database");
    let service = ExplorerService::new(db.pool().clone());

    // Create test tar archive
    let test_dir = TempDir::new().expect("Failed to create test dir");
    let archive_path = create_test_tar_archive(test_dir.path());

    // Build ORL for specific file inside archive
    let orl = format!(
        "orl://local{}?entry=logs/file1.log",
        path_to_orl(&archive_path)
    );

    // Execute: Download archive entry
    let result = service.download(&orl).await;

    // Assert
    assert!(
        result.is_ok(),
        "Download archive entry should succeed: {:?}",
        result.err()
    );

    let (filename, _size, mut stream) = result.unwrap();

    // Verify filename
    assert!(
        filename.contains("file1.log"),
        "Filename should contain file1.log: {}",
        filename
    );

    // Read some content to verify it's accessible
    let mut content = Vec::new();
    stream.read_to_end(&mut content).await.expect("Failed to read content");

    // Verify content is not empty
    assert!(!content.is_empty(), "Downloaded content should not be empty");

    // Verify content matches expected
    let content_str = String::from_utf8_lossy(&content);
    assert_eq!(
        content_str,
        "log content 1\n",
        "Downloaded content should match original test data"
    );
}

/// Test: Download entire archive file (not a specific entry)
///
/// When user wants to download the whole archive file without specifying
/// an entry parameter, the system should return the complete archive file
/// instead of trying to extract an entry.
///
/// Bug scenario: auto_detect_archive() sets archive_context with inner_path="/",
/// then download() tries to extract entry "/" which doesn't exist.
#[tokio::test]
async fn test_download_entire_archive_file() {
    assert_download_entire_archive(create_test_tar_gz_archive, ".tar.gz").await;
}

/// Test: Download entire TAR archive file (local backend)
///
/// Similar to test_download_entire_archive_file but for uncompressed TAR.
/// Verifies the fix works for all archive types.
#[tokio::test]
async fn test_download_entire_tar_file_local() {
    assert_download_entire_archive(create_test_tar_archive, "test.tar").await;
}
