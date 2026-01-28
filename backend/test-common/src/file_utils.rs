//! 文件测试工具
//!
//! 提供创建测试文件、目录等工具

use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::fs;
use crate::TestError;

/// 测试文件生成器
pub struct TestFileGenerator {
    /// 临时目录
    temp_dir: TempDir,
    /// 创建的文件列表
    created_files: Vec<PathBuf>,
}

impl TestFileGenerator {
    /// 创建新的测试文件生成器
    pub fn new() -> Result<Self, TestError> {
        let temp_dir = TempDir::new()?;
        Ok(Self {
            temp_dir,
            created_files: Vec::new(),
        })
    }

    /// 获取临时目录路径
    pub fn temp_dir(&self) -> &Path {
        self.temp_dir.path()
    }

    /// 创建测试日志文件（从现有测试代码提取）
    pub async fn create_log_files(&mut self) -> Result<(), TestError> {
        let dir = self.temp_dir();

        // 创建多个测试日志文件
        let app1_content = "2024-01-01 INFO Starting application\n\
                            2024-01-01 ERROR Connection failed\n\
                            2024-01-01 WARN Retrying connection\n\
                            2024-01-01 INFO Connection established\n";

        let app2_content = "2024-01-02 DEBUG Processing request\n\
                            2024-01-02 ERROR Invalid input data\n\
                            2024-01-02 INFO Request completed\n";

        let app3_content = "2024-01-03 INFO System started\n\
                            2024-01-03 ERROR Database connection timeout\n\
                            2024-01-03 ERROR Failed to initialize service\n\
                            2024-01-03 WARN Falling back to default config\n";

        fs::write(dir.join("app1.log"), app1_content).await?;
        fs::write(dir.join("app2.log"), app2_content).await?;
        fs::write(dir.join("app3.log"), app3_content).await?;

        self.created_files.extend([
            dir.join("app1.log"),
            dir.join("app2.log"),
            dir.join("app3.log"),
        ]);

        Ok(())
    }

    /// 创建指定内容的文件
    pub async fn create_file(&mut self, filename: &str, content: &str) -> Result<PathBuf, TestError> {
        let path = self.temp_dir().join(filename);
        fs::write(&path, content).await?;
        self.created_files.push(path.clone());
        Ok(path)
    }

    /// 创建包含特殊字符的文件（用于边界测试）
    pub async fn create_file_with_special_chars(&mut self, filename: &str) -> Result<PathBuf, TestError> {
        let content = r#"特殊字符测试文件:
- UTF-8边界字符: 𝄞 (U+1D11E) 🎉
- 控制字符: \x00\x01\x02
- 换行符: \n\r\n
- 制表符: \t\t
- 中文字符: 中文测试
- 混合编码测试
- 超长行: "Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat."
"#;

        self.create_file(filename, content).await
    }

    /// 创建大文件（用于性能测试）
    pub async fn create_large_file(&mut self, filename: &str, size_mb: usize) -> Result<PathBuf, TestError> {
        let path = self.temp_dir().join(filename);

        // 创建重复的内容块以快速生成大文件
        let chunk = "This is a test line for performance testing. ".repeat(100);
        let chunk_size = chunk.len();
        let target_size = size_mb * 1024 * 1024;

        let mut file = tokio::fs::File::create(&path).await?;
        let mut written = 0;

        while written < target_size {
            let to_write = std::cmp::min(chunk_size, target_size - written);
            let content = &chunk.as_bytes()[..to_write];
            tokio::io::AsyncWriteExt::write_all(&mut file, content).await?;
            written += to_write;
        }

        self.created_files.push(path.clone());
        Ok(path)
    }

    /// 清理所有创建的文件
    pub fn cleanup(self) -> Result<(), TestError> {
        // TempDir在drop时会自动清理
        Ok(())
    }
}

/// 创建测试目录结构
pub async fn create_test_directory_structure(base_dir: &Path) -> Result<(), TestError> {
    // 创建嵌套目录结构
    let dirs = vec![
        "logs/app1",
        "logs/app2",
        "logs/app3/archive",
        "config",
        "data/temp",
        "data/cache",
    ];

    for dir in dirs {
        fs::create_dir_all(base_dir.join(dir)).await?;
    }

    // 创建一些测试文件
    let files = vec![
        ("logs/app1/error.log", "2024-01-01 ERROR Test error 1\n2024-01-01 ERROR Test error 2\n"),
        ("logs/app2/debug.log", "2024-01-01 DEBUG Debug message 1\n2024-01-01 DEBUG Debug message 2\n"),
        ("logs/app3/archive/old.log", "2023-12-31 INFO Old log entry\n"),
        ("config/settings.json", r#"{"debug": true, "level": "info"}"#),
    ];

    for (path, content) in files {
        fs::write(base_dir.join(path), content).await?;
    }

    Ok(())
}

/// 获取文件大小（以MB为单位）
pub fn get_file_size_mb(path: &Path) -> Result<f64, TestError> {
    let metadata = std::fs::metadata(path)?;
    Ok(metadata.len() as f64 / (1024.0 * 1024.0))
}