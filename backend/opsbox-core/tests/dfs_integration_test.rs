//! DFS Integration Tests - Cross-system combinations
//!
//! Tests for ORL parsing and archive detection across different storage backends

use opsbox_core::dfs::{endpoint::Location, orl_parser::OrlParser};
use std::fs::File;
use std::io::Write;
use tar::Builder;
use tempfile::TempDir;
use zip::write::FileOptions;

/// Helper to escape paths for ORL URLs (forward slashes)
fn escape_path_for_orl(path: &std::path::Path) -> String {
  path.to_string_lossy().replace('\\', "/")
}

/// Helper function to create a test tar archive
fn create_test_tar_archive(dir: &std::path::Path) -> std::path::PathBuf {
  let archive_path = dir.join("test.tar");

  // Create test files
  let file1 = dir.join("file1.log");
  let file2 = dir.join("file2.log");
  std::fs::write(&file1, "log content 1\n").expect("Failed to create file1");
  std::fs::write(&file2, "log content 2\n").expect("Failed to create file2");

  // Create tar archive
  let file = File::create(&archive_path).expect("Failed to create tar file");
  let mut builder = Builder::new(file);
  builder
    .append_path_with_name(&file1, "logs/file1.log")
    .expect("Failed to add file1");
  builder
    .append_path_with_name(&file2, "logs/file2.log")
    .expect("Failed to add file2");
  builder.finish().expect("Failed to finish tar");

  // Cleanup
  std::fs::remove_file(&file1).ok();
  std::fs::remove_file(&file2).ok();

  archive_path
}

/// Helper function to create a test zip archive
fn create_test_zip_archive(dir: &std::path::Path) -> std::path::PathBuf {
  let archive_path = dir.join("test.zip");

  let file = File::create(&archive_path).expect("Failed to create zip file");
  let mut zip = zip::ZipWriter::new(file);

  zip
    .start_file::<_, ()>("logs/file1.log", FileOptions::default())
    .expect("Failed to start file");
  zip.write_all(b"log content 1\n").expect("Failed to write content");

  zip
    .start_file::<_, ()>("logs/file2.log", FileOptions::default())
    .expect("Failed to start file");
  zip.write_all(b"log content 2\n").expect("Failed to write content");

  zip.finish().expect("Failed to finish zip");

  archive_path
}

#[test]
fn test_local_archive_tar_read() {
  // Setup
  let temp_dir = TempDir::new().expect("Failed to create temp dir");
  let archive_path = create_test_tar_archive(temp_dir.path());

  // Build ORL for archive entry
  let orl = format!("orl://local/{}?entry=logs/file1.log", escape_path_for_orl(&archive_path));

  // Parse ORL
  let resource = OrlParser::parse(&orl).expect("Should parse ORL");

  // Verify: Archive context detected
  assert!(resource.archive_context.is_some(), "Should detect archive");

  // Verify: Correct path (compare with forward slashes for cross-platform)
  assert_eq!(
    resource.primary_path.to_string().replace('\\', "/"),
    escape_path_for_orl(&archive_path),
    "Primary path should match archive path"
  );

  // Verify: Entry path
  let archive_ctx = resource.archive_context.as_ref().unwrap();
  assert_eq!(
    archive_ctx.inner_path.to_string(),
    "logs/file1.log",
    "Entry path should match"
  );
}

#[test]
fn test_local_archive_zip_read() {
  // Setup
  let temp_dir = TempDir::new().expect("Failed to create temp dir");
  let archive_path = create_test_zip_archive(temp_dir.path());

  // Build ORL for zip entry
  let orl = format!("orl://local/{}?entry=logs/file1.log", escape_path_for_orl(&archive_path));

  // Parse ORL
  let resource = OrlParser::parse(&orl).expect("Should parse ORL");

  // Verify: Archive detected
  assert!(resource.archive_context.is_some(), "Should detect zip archive");
}

#[test]
fn test_s3_archive_tar_read() {
  // Test ORL parsing for S3 archive (no actual S3 connection)
  let orl = "orl://myprofile@s3/mybucket/logs/2024/10/data.tar.gz?entry=internal/service.log";

  // Parse ORL
  let resource = OrlParser::parse(orl).expect("Should parse S3 archive ORL");

  // Verify: Cloud location
  assert_eq!(resource.endpoint.location, Location::Cloud, "Should be Cloud");

  // Verify: Profile
  assert_eq!(resource.endpoint.identity, "myprofile", "Profile should match");

  // Verify: Path
  assert_eq!(
    resource.primary_path.to_string(),
    "/mybucket/logs/2024/10/data.tar.gz",
    "Path should match"
  );

  // Verify: Archive entry
  assert!(resource.archive_context.is_some(), "Should detect archive");
  let archive_ctx = resource.archive_context.as_ref().unwrap();
  assert_eq!(
    archive_ctx.inner_path.to_string(),
    "internal/service.log",
    "Entry path should match"
  );
}

#[test]
fn test_agent_archive_tar_read() {
  // Test ORL parsing for Agent archive
  let orl = "orl://web-01@agent.192.168.1.100:4001/var/log/nginx/access.tar.gz?entry=internal/access.log";

  // Parse ORL
  let resource = OrlParser::parse(orl).expect("Should parse Agent archive ORL");

  // Verify: Remote location
  assert!(
    matches!(resource.endpoint.location, Location::Remote { .. }),
    "Should be Remote"
  );

  // Verify: Agent ID
  assert_eq!(resource.endpoint.identity, "web-01", "Agent ID should match");

  // Verify: Path
  assert_eq!(
    resource.primary_path.to_string(),
    "/var/log/nginx/access.tar.gz",
    "Path should match"
  );

  // Verify: Archive entry
  assert!(resource.archive_context.is_some(), "Should detect archive");
}

#[test]
fn test_orl_archive_parsing() {
  // Test various archive ORL formats

  // Test 1: tar.gz with entry
  let orl1 = "orl://local/var/log/archive.tar.gz?entry=logs/app.log";
  let r1 = OrlParser::parse(orl1).expect("Should parse tar.gz");
  assert!(r1.archive_context.is_some());

  // Test 2: zip with entry
  let orl2 = "orl://local/var/log/archive.zip?entry=data.csv";
  let r2 = OrlParser::parse(orl2).expect("Should parse zip");
  assert!(r2.archive_context.is_some());

  // Test 3: Nested path in entry
  let orl3 = "orl://local/data.tar?entry=a/b/c/file.txt";
  let r3 = OrlParser::parse(orl3).expect("Should parse nested entry");
  assert_eq!(
    r3.archive_context.as_ref().unwrap().inner_path.to_string(),
    "a/b/c/file.txt"
  );

  // Test 4: No entry parameter (should not have archive context)
  let orl4 = "orl://local/var/log/archive.tar.gz";
  let r4 = OrlParser::parse(orl4).expect("Should parse without entry");
  // Note: May or may not have archive context depending on implementation
  // This tests that ORL parsing is flexible
  assert!(
    r4.archive_context.is_none(),
    "Without entry param, should not have archive context"
  );

  // Test 5: Special characters in entry path
  let orl5 = "orl://local/archive.tar?entry=logs/app%20(1).log";
  let r5 = OrlParser::parse(orl5).expect("Should parse special chars");
  assert!(r5.archive_context.is_some());
  // Verify URL decoding
  assert_eq!(
    r5.archive_context.as_ref().unwrap().inner_path.to_string(),
    "logs/app (1).log"
  );
}
