//! 归档检测对齐测试
//!
//! 验证 LogSeek 与 Explorer 的归档检测关键语义：
//! - open_read 失败时不误判归档
//! - 无归档扩展名但 magic bytes 命中时，能够生成带 ?entry= 的 ORL

use flate2::Compression;
use flate2::write::GzEncoder;
use opsbox_core::dfs::archive::{ArchiveType, detect_archive_type};
use opsbox_core::dfs::{ArchiveContext, Endpoint, LocalFileSystem, Resource, ResourcePath, build_orl_from_resource};
use std::io::Write;

fn create_test_tar_gz_with_file(path_in_archive: &str, content: &[u8]) -> Vec<u8> {
  let mut tar_buf = Vec::new();
  {
    let mut tar_builder = tar::Builder::new(&mut tar_buf);
    let mut header = tar::Header::new_gnu();
    header.set_size(content.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar_builder
      .append_data(&mut header, path_in_archive, content)
      .expect("append tar entry");
    tar_builder.finish().expect("finish tar");
  }

  let mut gz = GzEncoder::new(Vec::new(), Compression::default());
  gz.write_all(&tar_buf).expect("write gzip");
  gz.finish().expect("finish gzip")
}

#[tokio::test]
async fn test_directory_not_misdetected_as_archive() {
  let temp_dir = tempfile::tempdir().expect("tempdir");

  // 使用同一基准：root=temp_dir, path="" 指向 root 本身
  let resource = Resource {
    endpoint: Endpoint::local_fs(),
    primary_path: ResourcePath::parse(""),
    archive_context: None,
    filter_glob: None,
  };
  let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).expect("local fs");

  let result = detect_archive_type(&fs, &resource).await;
  assert!(result.is_none(), "目录不应被误判为归档");
}

#[tokio::test]
async fn test_magic_bytes_detection_orl_has_entry() {
  let temp_dir = tempfile::tempdir().expect("tempdir");
  let archive_path = temp_dir.path().join("secret.data");
  let tar_gz_content = create_test_tar_gz_with_file("internal/app.log", b"test log content");
  std::fs::write(&archive_path, &tar_gz_content).expect("write archive");

  // 路径相对 root，保证 LocalFileSystem 路径解析一致
  let resource = Resource {
    endpoint: Endpoint::local_fs(),
    primary_path: ResourcePath::parse("secret.data"),
    archive_context: None,
    filter_glob: None,
  };
  let fs = LocalFileSystem::new(temp_dir.path().to_path_buf()).expect("local fs");

  let archive_type = detect_archive_type(&fs, &resource).await;
  assert_eq!(archive_type, Some(ArchiveType::TarGz));

  // 模拟 search_executor 分发前写回 archive_context + 搜索结果 entry
  let resource_with_context = Resource {
    archive_context: Some(ArchiveContext::new(
      ResourcePath::parse("internal/app.log"),
      archive_type,
    )),
    ..resource
  };
  let result_orl = build_orl_from_resource(&resource_with_context);

  assert!(
    result_orl.contains("?entry="),
    "ORL 应包含 ?entry= 参数: {}",
    result_orl
  );
  assert!(
    result_orl.contains("entry=internal%2Fapp%2Elog"),
    "ORL entry 参数应包含正确编码路径: {}",
    result_orl
  );
}
