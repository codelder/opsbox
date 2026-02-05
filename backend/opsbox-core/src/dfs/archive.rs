//! Archive 模块 - 归档容器概念
//!
//! 定义了 ArchiveType 和 ArchiveContext

use super::path::ResourcePath;

/// 归档类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveType {
    /// TAR 归档
    Tar,
    /// GZIP 压缩的 TAR
    TarGz,
    /// .tgz 扩展名的 TAR+GZ
    Tgz,
    /// ZIP 归档
    Zip,
    /// 单独的 GZIP 文件
    Gz,
}

impl ArchiveType {
    /// 从文件扩展名识别归档类型
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            ".tar" => Some(ArchiveType::Tar),
            ".tar.gz" => Some(ArchiveType::TarGz),
            ".tgz" => Some(ArchiveType::Tgz),
            ".zip" => Some(ArchiveType::Zip),
            ".gz" => Some(ArchiveType::Gz),
            _ => None,
        }
    }

    /// 获取归档类型的扩展名
    pub fn extension(&self) -> &'static str {
        match self {
            ArchiveType::Tar => ".tar",
            ArchiveType::TarGz => ".tar.gz",
            ArchiveType::Tgz => ".tgz",
            ArchiveType::Zip => ".zip",
            ArchiveType::Gz => ".gz",
        }
    }
}

/// 归档上下文
///
/// 表示资源位于归档文件内的上下文信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveContext {
    /// 归档内的路径
    pub inner_path: ResourcePath,
    /// 归档类型
    pub archive_type: Option<ArchiveType>,
}

impl ArchiveContext {
    /// 创建新的归档上下文
    pub fn new(inner_path: ResourcePath, archive_type: Option<ArchiveType>) -> Self {
        Self {
            inner_path,
            archive_type,
        }
    }

    /// 从路径字符串创建归档上下文
    pub fn from_path_str(inner_path: &str, archive_type: Option<ArchiveType>) -> Self {
        Self {
            inner_path: ResourcePath::from_str(inner_path),
            archive_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_archive_type_from_extension() {
        assert_eq!(ArchiveType::from_extension(".tar"), Some(ArchiveType::Tar));
        assert_eq!(ArchiveType::from_extension(".tar.gz"), Some(ArchiveType::TarGz));
        assert_eq!(ArchiveType::from_extension(".tgz"), Some(ArchiveType::Tgz));
        assert_eq!(ArchiveType::from_extension(".zip"), Some(ArchiveType::Zip));
        assert_eq!(ArchiveType::from_extension(".gz"), Some(ArchiveType::Gz));
        assert_eq!(ArchiveType::from_extension(".txt"), None);
    }

    #[test]
    fn test_archive_type_extension() {
        assert_eq!(ArchiveType::Tar.extension(), ".tar");
        assert_eq!(ArchiveType::TarGz.extension(), ".tar.gz");
        assert_eq!(ArchiveType::Tgz.extension(), ".tgz");
        assert_eq!(ArchiveType::Zip.extension(), ".zip");
        assert_eq!(ArchiveType::Gz.extension(), ".gz");
    }

    #[test]
    fn test_archive_context_new() {
        let inner_path = ResourcePath::from_str("logs/app.log");
        let ctx = ArchiveContext::new(inner_path.clone(), Some(ArchiveType::Tar));
        assert_eq!(ctx.inner_path, inner_path);
        assert_eq!(ctx.archive_type, Some(ArchiveType::Tar));
    }

    #[test]
    fn test_archive_context_from_path_str() {
        let ctx = ArchiveContext::from_path_str("data/file.txt", Some(ArchiveType::Zip));
        assert_eq!(ctx.inner_path.to_string(), "data/file.txt");
        assert_eq!(ctx.archive_type, Some(ArchiveType::Zip));
    }
}
