//! 归档文件测试工具
//!
//! 提供创建各种格式的测试归档文件：
//! - tar归档 (.tar)
//! - tar.gz压缩归档 (.tar.gz, .tgz)
//! - zip归档 (.zip)
//! - 嵌套归档结构
//! - 自定义内容归档

use crate::TestError;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::fs;
use tokio_util::compat::TokioAsyncReadCompatExt;

/// 归档生成器
pub struct ArchiveGenerator {
  /// 临时工作目录
  temp_dir: TempDir,
  /// 创建的归档文件列表
  created_archives: Vec<PathBuf>,
}

impl ArchiveGenerator {
  /// 创建新的归档生成器
  pub fn new() -> Result<Self, TestError> {
    let temp_dir = TempDir::new()?;
    Ok(Self {
      temp_dir,
      created_archives: Vec::new(),
    })
  }

  /// 获取临时目录路径
  pub fn temp_dir(&self) -> &Path {
    self.temp_dir.path()
  }

  /// 创建简单的tar归档
  pub async fn create_tar_archive(&mut self, filename: &str) -> Result<PathBuf, TestError> {
    let archive_path = self.temp_dir().join(filename);

    // 创建测试文件内容
    let files = vec![
      (
        "logs/app1.log",
        "2024-01-01 INFO Starting application\n2024-01-01 ERROR Test error in tar\n",
      ),
      (
        "logs/app2.log",
        "2024-01-02 DEBUG Processing request\n2024-01-02 INFO Request completed\n",
      ),
      ("config/settings.json", r#"{"debug": true, "log_level": "info"}"#),
    ];

    // 使用tokio-tar创建归档
    let file = tokio::fs::File::create(&archive_path).await?;
    let mut builder = tokio_tar::Builder::new(file);

    for (path, content) in files {
      let temp_file_path = self.temp_dir().join(format!("temp_{}", path.replace("/", "_")));
      fs::write(&temp_file_path, content).await?;

      // 添加文件到归档
      let mut file = tokio::fs::File::open(&temp_file_path).await?;
      builder
        .append_file(path, &mut file)
        .await
        .map_err(|e| TestError::Archive(e.to_string()))?;

      // 清理临时文件
      fs::remove_file(temp_file_path).await?;
    }

    builder.finish().await.map_err(|e| TestError::Archive(e.to_string()))?;
    self.created_archives.push(archive_path.clone());
    Ok(archive_path)
  }

  /// 创建tar.gz压缩归档
  pub async fn create_tar_gz_archive(&mut self, filename: &str) -> Result<PathBuf, TestError> {
    let _tar_path = self.temp_dir().join("temp.tar");
    let gz_path = self.temp_dir().join(filename);

    // 先创建tar归档
    let tar_archive = self.create_tar_archive("temp.tar").await?;

    // 使用async-compression进行gzip压缩
    use async_compression::tokio::write::GzipEncoder;
    use tokio::io::AsyncWriteExt;

    let content = fs::read(&tar_archive).await?;
    let mut encoder = GzipEncoder::new(tokio::fs::File::create(&gz_path).await?);
    encoder.write_all(&content).await?;
    encoder.shutdown().await?;

    // 清理临时tar文件
    fs::remove_file(tar_archive).await?;
    self.created_archives.push(gz_path.clone());
    Ok(gz_path)
  }

  /// 创建zip归档
  pub async fn create_zip_archive(&mut self, filename: &str) -> Result<PathBuf, TestError> {
    let archive_path = self.temp_dir().join(filename);

    // 创建测试文件内容
    let files = vec![
      (
        "logs/app1.log",
        "2024-01-01 INFO Starting application\n2024-01-01 ERROR Test error in zip\n",
      ),
      (
        "logs/app2.log",
        "2024-01-02 DEBUG Processing request\n2024-01-02 INFO Request completed\n",
      ),
      ("config/settings.json", r#"{"debug": true, "log_level": "info"}"#),
    ];

    // 使用async_zip创建归档
    use async_zip::ZipEntryBuilder;
    use async_zip::tokio::write::ZipFileWriter as TokioZipFileWriter;

    let file = tokio::fs::File::create(&archive_path).await?;
    let compat_file = file.compat();
    let mut writer = TokioZipFileWriter::new(compat_file);

    for (path, content) in files {
      let entry_builder = ZipEntryBuilder::new(path.into(), async_zip::Compression::Stored);
      writer.write_entry_whole(entry_builder, content.as_bytes()).await?;
    }

    writer.close().await?;
    self.created_archives.push(archive_path.clone());
    Ok(archive_path)
  }

  /// 创建嵌套归档（归档中包含归档）
  pub async fn create_nested_archive(&mut self, filename: &str) -> Result<PathBuf, TestError> {
    let outer_archive_path = self.temp_dir().join(filename);

    // 先创建一个内层zip归档
    let inner_zip = self.create_zip_archive("inner.zip").await?;
    let inner_zip_content = fs::read(&inner_zip).await?;

    // 创建外层tar归档，包含内层zip
    let file = tokio::fs::File::create(&outer_archive_path).await?;
    let mut builder = tokio_tar::Builder::new(file);

    // 创建临时文件存放zip内容
    let temp_zip_path = self.temp_dir().join("temp_inner.zip");
    fs::write(&temp_zip_path, &inner_zip_content).await?;

    // 添加zip文件到tar归档
    let mut zip_file = tokio::fs::File::open(&temp_zip_path).await?;
    builder
      .append_file("archive/inner.zip", &mut zip_file)
      .await
      .map_err(|e| TestError::Archive(e.to_string()))?;

    // 添加一些普通文件
    let log_content = "2024-01-01 INFO Nested archive test\n";
    let temp_log_path = self.temp_dir().join("temp.log");
    fs::write(&temp_log_path, log_content).await?;

    let mut log_file = tokio::fs::File::open(&temp_log_path).await?;
    builder
      .append_file("logs/outer.log", &mut log_file)
      .await
      .map_err(|e| TestError::Archive(e.to_string()))?;

    builder.finish().await.map_err(|e| TestError::Archive(e.to_string()))?;

    // 清理临时文件
    fs::remove_file(temp_zip_path).await?;
    fs::remove_file(temp_log_path).await?;
    fs::remove_file(inner_zip).await?;

    self.created_archives.push(outer_archive_path.clone());
    Ok(outer_archive_path)
  }

  /// 创建包含指定文件的归档
  pub async fn create_custom_archive(
    &mut self,
    filename: &str,
    files: Vec<(String, String)>,
    format: ArchiveFormat,
  ) -> Result<PathBuf, TestError> {
    match format {
      ArchiveFormat::Tar => self.create_custom_tar_archive(filename, files).await,
      ArchiveFormat::TarGz => self.create_custom_tar_gz_archive(filename, files).await,
      ArchiveFormat::Zip => self.create_custom_zip_archive(filename, files).await,
    }
  }

  /// 创建自定义内容的tar归档
  async fn create_custom_tar_archive(
    &mut self,
    filename: &str,
    files: Vec<(String, String)>,
  ) -> Result<PathBuf, TestError> {
    let archive_path = self.temp_dir().join(filename);

    let file = tokio::fs::File::create(&archive_path).await?;
    let mut builder = tokio_tar::Builder::new(file);

    for (path, content) in files {
      let temp_file_path = self.temp_dir().join(format!("temp_{}", path.replace("/", "_")));
      fs::write(&temp_file_path, content).await?;

      let mut file = tokio::fs::File::open(&temp_file_path).await?;
      builder
        .append_file(path, &mut file)
        .await
        .map_err(|e| TestError::Archive(e.to_string()))?;

      fs::remove_file(temp_file_path).await?;
    }

    builder.finish().await.map_err(|e| TestError::Archive(e.to_string()))?;
    self.created_archives.push(archive_path.clone());
    Ok(archive_path)
  }

  /// 创建自定义内容的tar.gz归档
  async fn create_custom_tar_gz_archive(
    &mut self,
    filename: &str,
    files: Vec<(String, String)>,
  ) -> Result<PathBuf, TestError> {
    let _tar_path = self.temp_dir().join("temp.tar");

    // 先创建tar归档
    let tar_archive = self.create_custom_tar_archive("temp.tar", files).await?;

    // 压缩为gzip
    use async_compression::tokio::write::GzipEncoder;
    use tokio::io::AsyncWriteExt;

    let content = fs::read(&tar_archive).await?;
    let gz_path = self.temp_dir().join(filename);
    let mut encoder = GzipEncoder::new(tokio::fs::File::create(&gz_path).await?);
    encoder.write_all(&content).await?;
    encoder.shutdown().await?;

    // 清理临时文件
    fs::remove_file(tar_archive).await?;
    self.created_archives.push(gz_path.clone());
    Ok(gz_path)
  }

  /// 创建自定义内容的zip归档
  async fn create_custom_zip_archive(
    &mut self,
    filename: &str,
    files: Vec<(String, String)>,
  ) -> Result<PathBuf, TestError> {
    let archive_path = self.temp_dir().join(filename);

    use async_zip::ZipEntryBuilder;
    use async_zip::tokio::write::ZipFileWriter as TokioZipFileWriter;

    let file = tokio::fs::File::create(&archive_path).await?;
    let compat_file = file.compat();
    let mut writer = TokioZipFileWriter::new(compat_file);

    for (path, content) in files {
      let entry_builder = ZipEntryBuilder::new(path.into(), async_zip::Compression::Stored);
      writer.write_entry_whole(entry_builder, content.as_bytes()).await?;
    }

    writer.close().await?;
    self.created_archives.push(archive_path.clone());
    Ok(archive_path)
  }

  /// 清理所有创建的归档文件
  pub fn cleanup(self) -> Result<(), TestError> {
    // TempDir在drop时会自动清理
    Ok(())
  }
}

/// 归档格式枚举
#[derive(Debug, Clone, Copy)]
pub enum ArchiveFormat {
  Tar,
  TarGz,
  Zip,
}

impl ArchiveFormat {
  /// 获取文件扩展名
  pub fn extension(&self) -> &'static str {
    match self {
      ArchiveFormat::Tar => ".tar",
      ArchiveFormat::TarGz => ".tar.gz",
      ArchiveFormat::Zip => ".zip",
    }
  }
}

/// 预定义的测试归档内容
pub mod test_content {
  /// 基础日志文件内容
  pub const APP1_LOG: &str = "2024-01-01 INFO Starting application\n2024-01-01 ERROR Test error\n";
  pub const APP2_LOG: &str = "2024-01-02 DEBUG Processing request\n2024-01-02 INFO Request completed\n";
  pub const APP3_LOG: &str = "2024-01-03 INFO System started\n2024-01-03 ERROR Database connection timeout\n";

  /// 配置文件内容
  pub const SETTINGS_JSON: &str = r#"{"debug": true, "log_level": "info"}"#;

  /// 获取标准测试文件集合
  pub fn standard_files() -> Vec<(String, String)> {
    vec![
      ("logs/app1.log".to_string(), APP1_LOG.to_string()),
      ("logs/app2.log".to_string(), APP2_LOG.to_string()),
      ("logs/app3.log".to_string(), APP3_LOG.to_string()),
      ("config/settings.json".to_string(), SETTINGS_JSON.to_string()),
    ]
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokio::fs;

  #[test]
  fn test_archive_format_extension() {
    // 测试ArchiveFormat的扩展名方法
    assert_eq!(ArchiveFormat::Tar.extension(), ".tar");
    assert_eq!(ArchiveFormat::TarGz.extension(), ".tar.gz");
    assert_eq!(ArchiveFormat::Zip.extension(), ".zip");
  }

  #[test]
  fn test_archive_format_debug() {
    // 测试ArchiveFormat的Debug实现
    let format = ArchiveFormat::Tar;
    let debug_output = format!("{:?}", format);
    assert!(debug_output.contains("Tar"));
  }

  #[test]
  fn test_test_content_constants() {
    // 测试预定义内容常量
    assert!(test_content::APP1_LOG.contains("Starting application"));
    assert!(test_content::APP2_LOG.contains("Processing request"));
    assert!(test_content::APP3_LOG.contains("System started"));
    assert!(test_content::SETTINGS_JSON.contains("debug"));
  }

  #[test]
  fn test_test_content_standard_files() {
    // 测试标准文件集合生成
    let files = test_content::standard_files();
    assert_eq!(files.len(), 4);
    assert!(files.iter().any(|(path, _)| path == "logs/app1.log"));
    assert!(files.iter().any(|(path, _)| path == "config/settings.json"));
  }

  #[tokio::test]
  async fn test_archive_generator_new() {
    // 测试ArchiveGenerator创建
    let generator = ArchiveGenerator::new();
    assert!(generator.is_ok());

    let generator = generator.unwrap();
    assert!(generator.temp_dir().exists());
    assert!(generator.created_archives.is_empty());
  }

  #[tokio::test]
  async fn test_archive_generator_temp_dir() {
    // 测试temp_dir方法
    let generator = ArchiveGenerator::new().unwrap();
    let temp_dir = generator.temp_dir();
    assert!(temp_dir.exists());
    assert!(temp_dir.is_dir());
  }

  #[tokio::test]
  async fn test_archive_generator_create_tar_archive() {
    // 测试创建tar归档
    let mut generator = ArchiveGenerator::new().unwrap();
    let result = generator.create_tar_archive("test.tar").await;

    assert!(result.is_ok());
    let archive_path = result.unwrap();
    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().ends_with(".tar"));

    // 检查文件是否被添加到创建列表中
    assert_eq!(generator.created_archives.len(), 1);
    assert!(generator.created_archives.contains(&archive_path));
  }

  #[tokio::test]
  async fn test_archive_generator_create_tar_gz_archive() {
    // 测试创建tar.gz归档
    let mut generator = ArchiveGenerator::new().unwrap();
    let result = generator.create_tar_gz_archive("test.tar.gz").await;

    assert!(result.is_ok());
    let archive_path = result.unwrap();
    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().ends_with(".tar.gz"));

    // 文件大小应该大于0
    let metadata = fs::metadata(&archive_path).await.unwrap();
    assert!(metadata.len() > 0);
  }

  #[tokio::test]
  async fn test_archive_generator_create_zip_archive() {
    // 测试创建zip归档
    let mut generator = ArchiveGenerator::new().unwrap();
    let result = generator.create_zip_archive("test.zip").await;

    assert!(result.is_ok());
    let archive_path = result.unwrap();
    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().ends_with(".zip"));
  }

  #[tokio::test]
  async fn test_archive_generator_create_nested_archive() {
    // 测试创建嵌套归档
    let mut generator = ArchiveGenerator::new().unwrap();
    let result = generator.create_nested_archive("nested.tar").await;

    assert!(result.is_ok());
    let archive_path = result.unwrap();
    assert!(archive_path.exists());
    assert!(archive_path.to_string_lossy().contains("nested"));
  }

  #[tokio::test]
  async fn test_archive_generator_create_custom_archive_tar() {
    // 测试创建自定义tar归档
    let mut generator = ArchiveGenerator::new().unwrap();
    let files = vec![
      ("custom/file1.txt".to_string(), "Content 1".to_string()),
      ("custom/file2.txt".to_string(), "Content 2".to_string()),
    ];

    let result = generator
      .create_custom_archive("custom.tar", files, ArchiveFormat::Tar)
      .await;

    assert!(result.is_ok());
    let archive_path = result.unwrap();
    assert!(archive_path.exists());
  }

  #[tokio::test]
  async fn test_archive_generator_create_custom_archive_zip() {
    // 测试创建自定义zip归档
    let mut generator = ArchiveGenerator::new().unwrap();
    let files = vec![("zip/file1.txt".to_string(), "ZIP Content 1".to_string())];

    let result = generator
      .create_custom_archive("custom.zip", files, ArchiveFormat::Zip)
      .await;

    assert!(result.is_ok());
    let archive_path = result.unwrap();
    assert!(archive_path.exists());
  }

  #[tokio::test]
  async fn test_archive_generator_cleanup() {
    // 测试清理方法
    let generator = ArchiveGenerator::new().unwrap();
    let result = generator.cleanup();
    assert!(result.is_ok());
  }

  #[test]
  fn test_archive_format_clone() {
    // 测试ArchiveFormat的Clone实现
    let format1 = ArchiveFormat::Tar;
    let format2 = format1.clone();
    match format2 {
      ArchiveFormat::Tar => assert!(true),
      _ => panic!("Expected Tar"),
    }
  }

  #[test]
  fn test_archive_format_copy() {
    // 测试ArchiveFormat的Copy语义
    let format1 = ArchiveFormat::Zip;
    let format2 = format1; // Copy
    match format2 {
      ArchiveFormat::Zip => assert!(true),
      _ => panic!("Expected Zip"),
    }
    // format1应该仍然可用（因为是Copy）
    match format1 {
      ArchiveFormat::Zip => assert!(true),
      _ => panic!("Expected Zip"),
    }
  }
}
