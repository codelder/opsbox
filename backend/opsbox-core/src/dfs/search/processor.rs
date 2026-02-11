//! Entry Stream Processor - 条目流处理器
//!
//! 消费 EntryStream，调用 ContentProcessor 处理内容。
//! 支持并发处理、预读优化、取消检测和路径过滤。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::{StreamExt, stream::FuturesUnordered};
use tokio::io::AsyncRead;
use tracing::{trace, warn};

use crate::fs::{EntryMeta, EntrySource, EntryStream, PrefixedReader};

use super::types::{preload_entry, ContentProcessor, PreloadResult, ProcessedContent};

/// 统一条目流处理器：消费 EntryStream，调用 ContentProcessor 处理内容
pub struct EntryStreamProcessor<P: ContentProcessor> {
    processor: Arc<P>,
    content_timeout: Duration,
    /// 额外路径过滤器列表（AND 逻辑）
    extra_path_filters: Vec<PathFilter>,
    cancel_token: Option<Arc<tokio_util::sync::CancellationToken>>,
    /// 基础路径（用于相对路径过滤）
    base_path: Option<PathBuf>,
    /// 预读阈值
    preload_threshold: usize,
}

/// 路径过滤器（简化版，从 logseek 复制）
#[derive(Clone, Default)]
pub struct PathFilter {
    /// 包含模式
    pub include: Option<globset::GlobSet>,
    /// 排除模式
    pub exclude: Option<globset::GlobSet>,
}

impl PathFilter {
    /// 检查路径是否被允许
    pub fn is_allowed(&self, path: &str) -> bool {
        // 先检查排除
        if let Some(ref exclude) = self.exclude {
            if exclude.is_match(path) {
                return false;
            }
        }
        // 再检查包含
        if let Some(ref include) = self.include {
            if !include.is_match(path) {
                return false;
            }
        }
        true
    }
}

impl<P: ContentProcessor + 'static> EntryStreamProcessor<P> {
    /// 创建新的条目流处理器
    pub fn new(processor: Arc<P>) -> Self {
        Self {
            processor,
            content_timeout: Duration::from_secs(60),
            extra_path_filters: Vec::new(),
            cancel_token: None,
            base_path: None,
            preload_threshold: 120 * 1024 * 1024, // 120MB
        }
    }

    /// 设置取消令牌
    pub fn with_cancel_token(mut self, token: Arc<tokio_util::sync::CancellationToken>) -> Self {
        self.cancel_token = Some(token);
        self
    }

    /// 设置基础路径（用于相对路径过滤）
    pub fn with_base_path(mut self, base_path: impl Into<PathBuf>) -> Self {
        self.base_path = Some(base_path.into());
        self
    }

    /// 添加额外路径过滤器（多个过滤器之间是 AND 关系）
    pub fn with_extra_path_filter(mut self, filter: PathFilter) -> Self {
        self.extra_path_filters.push(filter);
        self
    }

    /// 设置内容处理超时时间
    #[allow(dead_code)]
    pub fn with_content_timeout(mut self, timeout: Duration) -> Self {
        self.content_timeout = timeout;
        self
    }

    /// 设置预读阈值
    #[allow(dead_code)]
    pub fn with_preload_threshold(mut self, threshold: usize) -> Self {
        self.preload_threshold = threshold;
        self
    }

    /// 检查路径是否应该处理
    fn should_process_path(&self, path_str: &str) -> bool {
        for filter in &self.extra_path_filters {
            if !filter.is_allowed(path_str) {
                return false;
            }
        }
        true
    }

    /// 获取并发度
    fn entry_concurrency() -> usize {
        // 优先使用环境变量
        if let Ok(val) = std::env::var("ENTRY_CONCURRENCY") {
            if let Ok(parsed) = val.parse::<usize>() {
                return parsed.clamp(1, 128);
            }
        }

        // 默认值：根据 CPU 核心数动态计算
        let cpu_count = num_cpus::get();
        let default = (cpu_count * 2).clamp(8, 32);
        default.clamp(1, 128)
    }

    /// 并发处理条目
    pub async fn process_stream(
        &mut self,
        entries: &mut dyn EntryStream,
        result_callback: impl Fn(ProcessedContent) + Send + Sync + 'static,
    ) -> Result<(), String> {
        let processor = self.processor.clone();
        let content_timeout = self.content_timeout;
        let mut in_flight: FuturesUnordered<_> = FuturesUnordered::new();
        let max_conc = Self::entry_concurrency();
        let callback = Arc::new(result_callback);

        loop {
            // 检查取消
            if let Some(token) = &self.cancel_token {
                if token.is_cancelled() {
                    break;
                }
            }

            // 如果并发达到上限，先等待一个任务完成
            if in_flight.len() >= max_conc {
                if let Some(handle) = in_flight.next().await {
                    let _ = handle;
                }
                continue;
            }

            // 拉取下一个条目
            let next = entries.next_entry().await.map_err(|e| e.to_string())?;
            let Some((meta, reader)) = next else {
                break;
            };

            // 路径过滤
            let path_str = self.compute_path_string(&meta);

            if !self.should_process_path(&path_str) {
                trace!(
                    "路径不匹配 (extra filters)，跳过: meta.path={} path_str_for_filter={}",
                    &meta.path, &path_str
                );
                continue;
            }

            // 处理归档条目和普通文件
            if meta.is_compressed || meta.source == EntrySource::Tar || meta.source == EntrySource::TarGz {
                // 归档条目：需要考虑共享底层读取器的情况
                self.process_archive_entry(
                    &meta,
                    reader,
                    &processor,
                    content_timeout,
                    &mut in_flight,
                    callback.clone(),
                )
                .await?;
            } else {
                // 普通文件：可以并发处理
                self.process_regular_entry(
                    &meta,
                    reader,
                    &processor,
                    content_timeout,
                    &mut in_flight,
                    callback.clone(),
                )
                .await;
            }
        }

        // 等待所有在途任务完成
        while let Some(handle) = in_flight.next().await {
            let _ = handle;
        }

        Ok(())
    }

    /// 计算用于过滤的路径字符串
    fn compute_path_string(&self, meta: &EntryMeta) -> String {
        if let Some(base) = &self.base_path {
            let path_obj = std::path::Path::new(&meta.path);
            if let Ok(p) = path_obj.strip_prefix(base) {
                // 去除 ./ 前缀
                let mut out = std::path::PathBuf::new();
                let mut leading = true;
                for c in p.components() {
                    match c {
                        std::path::Component::CurDir if leading => continue,
                        _ => {
                            leading = false;
                            out.push(c.as_os_str());
                        }
                    }
                }
                out.to_string_lossy().into_owned()
            } else if let (Ok(canon_path), Ok(canon_base)) =
                (std::fs::canonicalize(path_obj), std::fs::canonicalize(base))
            {
                if let Ok(p) = canon_path.strip_prefix(&canon_base) {
                    let mut out = std::path::PathBuf::new();
                    let mut leading = true;
                    for c in p.components() {
                        match c {
                            std::path::Component::CurDir if leading => continue,
                            _ => {
                                leading = false;
                                out.push(c.as_os_str());
                            }
                        }
                    }
                    out.to_string_lossy().into_owned()
                } else {
                    path_obj.to_string_lossy().into_owned()
                }
            } else {
                path_obj.to_string_lossy().into_owned()
            }
        } else {
            std::path::Path::new(&meta.path)
                .to_string_lossy()
                .into_owned()
        }
    }

    /// 处理归档条目
    async fn process_archive_entry(
        &mut self,
        meta: &EntryMeta,
        mut reader: Box<dyn AsyncRead + Send + Unpin>,
        processor: &Arc<P>,
        content_timeout: Duration,
        in_flight: &mut FuturesUnordered<tokio::task::JoinHandle<()>>,
        callback: Arc<impl Fn(ProcessedContent) + Send + Sync + 'static>,
    ) -> Result<(), String> {
        // 对于归档条目，预读到内存后允许并发处理
        match preload_entry(&mut reader, self.preload_threshold).await {
            Ok(PreloadResult::Complete(content)) => {
                // 小文件完全读取，可以并发处理
                trace!(
                    "归档条目预读成功（完整），允许并发处理: {} ({} bytes)",
                    meta.path,
                    content.len()
                );
                let proc_clone = processor.clone();
                let cb_clone = callback.clone();
                let path = meta.path.clone();
                let container_path = meta.container_path.clone();

                let handle = tokio::spawn(async move {
                    let mut mem_reader: Box<dyn AsyncRead + Send + Unpin> =
                        Box::new(std::io::Cursor::new(content));
                    match tokio::time::timeout(
                        content_timeout,
                        proc_clone.process_content(path.clone(), &mut mem_reader),
                    )
                    .await
                    {
                        Ok(Ok(Some(mut result))) => {
                            result.archive_path = container_path;
                            cb_clone(result);
                        }
                        Ok(Ok(None)) => {}
                        Ok(Err(e)) => {
                            warn!("处理预读条目内容失败: {}", e);
                        }
                        Err(_) => {
                            warn!("处理预读条目超时: {}", path);
                        }
                    }
                });
                in_flight.push(handle);
            }
            Ok(PreloadResult::Partial(prefix)) => {
                // 大文件：已读取部分内容，使用 PrefixedReader 组合
                trace!(
                    "归档条目过大，使用流式处理: {} (已读取 {} bytes)",
                    meta.path,
                    prefix.len()
                );
                // 等待所有并发任务完成
                while let Some(handle) = in_flight.next().await {
                    let _ = handle;
                }

                // 使用 PrefixedReader 组合已读取的部分和剩余的 reader
                let combined_reader = PrefixedReader::new(prefix, reader);
                let container_path = meta.container_path.clone();
                let path = meta.path.clone();
                let proc_clone = processor.clone();
                let cb_clone = callback.clone();

                // 串行处理大文件
                let mut boxed_reader: Box<dyn AsyncRead + Send + Unpin> = Box::new(combined_reader);
                match tokio::time::timeout(content_timeout, proc_clone.process_content(path.clone(), &mut boxed_reader))
                    .await
                {
                    Ok(Ok(Some(mut result))) => {
                        result.archive_path = container_path;
                        cb_clone(result);
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(e)) => {
                        warn!("处理大文件条目内容失败: {}", e);
                    }
                    Err(_) => warn!("处理大文件条目超时: {}", path),
                }
            }
            Err(e) => {
                warn!("归档条目预读失败: {}: {}", meta.path, e);
            }
        }
        Ok(())
    }

    /// 处理普通文件
    async fn process_regular_entry(
        &mut self,
        meta: &EntryMeta,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        processor: &Arc<P>,
        content_timeout: Duration,
        in_flight: &mut FuturesUnordered<tokio::task::JoinHandle<()>>,
        callback: Arc<impl Fn(ProcessedContent) + Send + Sync + 'static>,
    ) {
        let proc_clone = processor.clone();
        let cb_clone = callback.clone();
        let path = meta.path.clone();
        let container_path = meta.container_path.clone();

        let handle = tokio::spawn(async move {
            let mut reader = reader;
            match tokio::time::timeout(content_timeout, proc_clone.process_content(path.clone(), &mut reader))
                .await
            {
                Ok(Ok(Some(mut result))) => {
                    result.archive_path = container_path;
                    cb_clone(result);
                }
                Ok(Ok(None)) => {}
                Ok(Err(e)) => {
                    warn!("处理条目内容失败: {}", e);
                }
                Err(_) => {
                    warn!("处理条目超时: {}", path);
                }
            }
        });
        in_flight.push(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_concurrency() {
        let conc = EntryStreamProcessor::<DummyProcessor>::entry_concurrency();
        assert!(conc >= 1);
        assert!(conc <= 128);
    }

    #[test]
    fn test_path_filter() {
        let mut builder = globset::GlobSetBuilder::new();
        builder.add(globset::Glob::new("*.log").unwrap());
        let include = builder.build().unwrap();

        let filter = PathFilter {
            include: Some(include),
            exclude: None,
        };

        assert!(filter.is_allowed("test.log"));
        assert!(filter.is_allowed("path/to/test.log"));
        assert!(!filter.is_allowed("test.txt"));
    }

    #[test]
    fn test_path_filter_with_exclude() {
        let mut include_builder = globset::GlobSetBuilder::new();
        include_builder.add(globset::Glob::new("*").unwrap());
        let include = include_builder.build().unwrap();

        let mut exclude_builder = globset::GlobSetBuilder::new();
        exclude_builder.add(globset::Glob::new("*.tmp").unwrap());
        let exclude = exclude_builder.build().unwrap();

        let filter = PathFilter {
            include: Some(include),
            exclude: Some(exclude),
        };

        assert!(filter.is_allowed("test.log"));
        assert!(!filter.is_allowed("test.tmp"));
    }

    /// Dummy processor for testing
    struct DummyProcessor;

    #[async_trait::async_trait]
    impl ContentProcessor for DummyProcessor {
        async fn process_content(
            &self,
            _path: String,
            _reader: &mut Box<dyn AsyncRead + Send + Unpin>,
        ) -> std::io::Result<Option<ProcessedContent>> {
            Ok(None)
        }
    }
}
