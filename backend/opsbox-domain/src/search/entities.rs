//! Search 上下文实体
//!
//! SearchSession 聚合根和相关实体定义。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::value_objects::ParsedQuery;

/// 搜索会话 ID
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SearchSessionId(String);

impl SearchSessionId {
    /// 创建新的搜索会话 ID
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// 从字符串创建搜索会话 ID
    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    /// 获取内部字符串
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SearchSessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SearchSessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 搜索状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchState {
    /// 等待中
    Pending,
    /// 运行中
    Running,
    /// 已完成
    Completed,
    /// 已取消
    Cancelled,
    /// 错误
    Failed,
}

/// 搜索结果条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultEntry {
    /// 文件路径
    pub file_path: String,
    /// 行号
    pub line_number: u64,
    /// 匹配的行内容
    pub line_content: String,
    /// 匹配的位置（字节偏移）
    pub match_offset: Option<u64>,
    /// 匹配的长度
    pub match_length: Option<u64>,
}

/// 搜索统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStatistics {
    /// 已扫描的文件数量
    pub files_scanned: u64,
    /// 已扫描的字节数
    pub bytes_scanned: u64,
    /// 匹配的文件数量
    pub files_matched: u64,
    /// 匹配的行数
    pub lines_matched: u64,
    /// 开始时间
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    /// 结束时间
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl SearchStatistics {
    /// 创建新的搜索统计
    pub fn new() -> Self {
        Self {
            files_scanned: 0,
            bytes_scanned: 0,
            files_matched: 0,
            lines_matched: 0,
            start_time: Some(chrono::Utc::now()),
            end_time: None,
        }
    }

    /// 计算持续时间
    pub fn duration(&self) -> Option<Duration> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => {
                let duration = end.signed_duration_since(start);
                Some(Duration::from_secs(duration.num_seconds().max(0) as u64))
            }
            _ => None,
        }
    }
}

impl Default for SearchStatistics {
    fn default() -> Self {
        Self::new()
    }
}

/// 搜索源
///
/// 表示要搜索的文件或目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchSource {
    /// 源 ID（用于标识唯一的搜索源）
    pub id: String,
    /// 源描述（用于调试和日志）
    pub description: String,
    /// 源类型
    pub source_type: SearchSourceType,
}

/// 搜索源类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchSourceType {
    /// 本地目录
    LocalDir { path: String, recursive: bool },
    /// 本地文件列表
    LocalFiles { paths: Vec<String> },
    /// 归档文件
    Archive { path: String, entry: Option<String> },
    /// Agent 目录
    AgentDir { agent_id: String, path: String, recursive: bool },
    /// S3 目录
    S3Dir { profile: String, path: String, recursive: bool },
}

/// 搜索结果集合
///
/// 存储搜索结果，支持分页和流式访问。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    /// 结果条目
    entries: Vec<SearchResultEntry>,
    /// 最大结果数限制
    max_results: Option<usize>,
}

impl SearchResults {
    /// 创建新的搜索结果集合
    pub fn new(max_results: Option<usize>) -> Self {
        Self {
            entries: Vec::new(),
            max_results,
        }
    }

    /// 添加结果条目
    pub fn add(&mut self, entry: SearchResultEntry) -> bool {
        if let Some(max) = self.max_results {
            if self.entries.len() >= max {
                return false;
            }
        }
        self.entries.push(entry);
        true
    }

    /// 获取所有结果
    pub fn entries(&self) -> &[SearchResultEntry] {
        &self.entries
    }

    /// 获取结果数量
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 清空结果
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 分页获取结果
    pub fn paginate(&self, offset: usize, limit: usize) -> &[SearchResultEntry] {
        let start = offset.min(self.entries.len());
        let end = (offset + limit).min(self.entries.len());
        &self.entries[start..end]
    }
}

/// 取消令牌
///
/// 用于取消正在运行的搜索。
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    /// 创建新的取消令牌
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 取消操作
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// 检查是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// 创建子令牌（共享取消状态）
    pub fn child(&self) -> Self {
        Self {
            cancelled: Arc::clone(&self.cancelled),
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

/// 搜索会话（聚合根）
///
/// 管理搜索的完整生命周期，包括状态、结果和统计信息。
pub struct SearchSession {
    /// 会话 ID
    pub id: SearchSessionId,
    /// 解析后的查询
    pub query: ParsedQuery,
    /// 搜索源列表
    pub sources: Vec<SearchSource>,
    /// 搜索状态
    state: SearchState,
    /// 搜索结果
    results: SearchResults,
    /// 取消令牌
    cancel_token: CancellationToken,
    /// 统计信息
    stats: SearchStatistics,
    /// 最大结果数
    max_results: Option<usize>,
}

impl SearchSession {
    /// 创建新的搜索会话
    pub fn new(
        query: ParsedQuery,
        sources: Vec<SearchSource>,
        max_results: Option<usize>,
    ) -> Self {
        Self {
            id: SearchSessionId::new(),
            query,
            sources,
            state: SearchState::Pending,
            results: SearchResults::new(max_results),
            cancel_token: CancellationToken::new(),
            stats: SearchStatistics::new(),
            max_results,
        }
    }

    /// 启动搜索
    pub fn start(&mut self) {
        self.state = SearchState::Running;
        self.stats.start_time = Some(chrono::Utc::now());
    }

    /// 完成搜索
    pub fn complete(&mut self) {
        self.state = SearchState::Completed;
        self.stats.end_time = Some(chrono::Utc::now());
    }

    /// 取消搜索
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// 标记搜索为失败
    pub fn fail(&mut self) {
        self.state = SearchState::Failed;
        self.stats.end_time = Some(chrono::Utc::now());
    }

    /// 添加搜索结果
    pub fn add_result(&mut self, entry: SearchResultEntry) -> bool {
        self.results.add(entry)
    }

    /// 更新统计信息
    pub fn update_stats<F>(&mut self, f: F)
    where
        F: FnOnce(&mut SearchStatistics),
    {
        f(&mut self.stats);
    }

    /// 获取搜索状态
    pub fn state(&self) -> SearchState {
        self.state
    }

    /// 获取搜索结果
    pub fn results(&self) -> &SearchResults {
        &self.results
    }

    /// 获取取消令牌
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel_token
    }

    /// 获取统计信息
    pub fn stats(&self) -> &SearchStatistics {
        &self.stats
    }

    /// 检查是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// 获取最大结果数
    pub fn max_results(&self) -> Option<usize> {
        self.max_results
    }

    /// 获取搜索源数量
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// 检查是否可以接受更多结果
    pub fn can_accept_more_results(&self) -> bool {
        if let Some(max) = self.max_results {
            self.results.len() < max
        } else {
            true
        }
    }
}

/// 搜索会话注册表
///
/// 管理所有活跃的搜索会话。
pub struct SearchSessionRegistry {
    sessions: HashMap<String, SearchSession>,
}

impl SearchSessionRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// 注册搜索会话
    pub fn register(&mut self, session: SearchSession) {
        let id = session.id.as_str().to_string();
        self.sessions.insert(id, session);
    }

    /// 获取搜索会话
    pub fn get(&self, id: &str) -> Option<&SearchSession> {
        self.sessions.get(id)
    }

    /// 获取可变搜索会话
    pub fn get_mut(&mut self, id: &str) -> Option<&mut SearchSession> {
        self.sessions.get_mut(id)
    }

    /// 移除搜索会话
    pub fn remove(&mut self, id: &str) -> Option<SearchSession> {
        self.sessions.remove(id)
    }

    /// 取消搜索会话
    pub fn cancel(&self, id: &str) -> bool {
        self.sessions.get(id).map(|s| {
            s.cancel();
            true
        }).unwrap_or(false)
    }

    /// 获取所有活跃会话的 ID
    pub fn active_session_ids(&self) -> Vec<String> {
        self.sessions.values()
            .filter(|s| s.state() == SearchState::Running)
            .map(|s| s.id.as_str().to_string())
            .collect()
    }

    /// 清理已完成的会话
    pub fn cleanup_completed(&mut self) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|_, s| {
            matches!(s.state(), SearchState::Pending | SearchState::Running)
        });
        before - self.sessions.len()
    }
}

impl Default for SearchSessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::value_objects::QueryExpression;

    #[test]
    fn test_search_session_id_new() {
        let id1 = SearchSessionId::new();
        let id2 = SearchSessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_search_session_id_from_string() {
        let id = SearchSessionId::from_string("test-id".to_string());
        assert_eq!(id.as_str(), "test-id");
    }

    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());

        token.cancel();
        assert!(token.is_cancelled());

        let child = token.child();
        assert!(child.is_cancelled());
    }

    #[test]
    fn test_search_results_add() {
        let mut results = SearchResults::new(Some(2));

        let entry1 = SearchResultEntry {
            file_path: "/test/log1.log".to_string(),
            line_number: 1,
            line_content: "error message".to_string(),
            match_offset: None,
            match_length: None,
        };

        assert!(results.add(entry1.clone()));
        assert_eq!(results.len(), 1);

        let entry2 = SearchResultEntry {
            file_path: "/test/log2.log".to_string(),
            line_number: 1,
            line_content: "error message".to_string(),
            match_offset: None,
            match_length: None,
        };

        assert!(results.add(entry2));
        assert_eq!(results.len(), 2);

        // 超过最大限制
        assert!(!results.add(entry1));
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_results_paginate() {
        let mut results = SearchResults::new(None);

        for i in 0..10 {
            results.add(SearchResultEntry {
                file_path: format!("/test/log{}.log", i),
                line_number: 1,
                line_content: "test".to_string(),
                match_offset: None,
                match_length: None,
            });
        }

        let page = results.paginate(2, 3);
        assert_eq!(page.len(), 3);
        assert_eq!(page[0].file_path, "/test/log2.log");
        assert_eq!(page[2].file_path, "/test/log4.log");
    }

    #[test]
    fn test_search_statistics_new() {
        let stats = SearchStatistics::new();
        assert_eq!(stats.files_scanned, 0);
        assert_eq!(stats.lines_matched, 0);
        assert!(stats.start_time.is_some());
        assert!(stats.end_time.is_none());
    }

    #[test]
    fn test_search_session_new() {
        let query = ParsedQuery::new(
            QueryExpression::literal("error".to_string()),
            "error".to_string(),
        );

        let sources = vec![
            SearchSource {
                id: "source1".to_string(),
                description: "Test source".to_string(),
                source_type: SearchSourceType::LocalDir {
                    path: "/var/log".to_string(),
                    recursive: true,
                },
            },
        ];

        let session = SearchSession::new(query, sources, Some(100));

        assert_eq!(session.state(), SearchState::Pending);
        assert_eq!(session.source_count(), 1);
        assert!(session.can_accept_more_results());
    }

    #[test]
    fn test_search_session_start_complete() {
        let query = ParsedQuery::new(
            QueryExpression::literal("error".to_string()),
            "error".to_string(),
        );

        let mut session = SearchSession::new(query, vec![], None);

        assert_eq!(session.state(), SearchState::Pending);

        session.start();
        assert_eq!(session.state(), SearchState::Running);

        session.complete();
        assert_eq!(session.state(), SearchState::Completed);
        assert!(session.stats().end_time.is_some());
    }

    #[test]
    fn test_search_session_add_result() {
        let query = ParsedQuery::new(
            QueryExpression::literal("error".to_string()),
            "error".to_string(),
        );

        let mut session = SearchSession::new(query, vec![], Some(2));

        let entry = SearchResultEntry {
            file_path: "/test/log.log".to_string(),
            line_number: 1,
            line_content: "error message".to_string(),
            match_offset: None,
            match_length: None,
        };

        assert!(session.add_result(entry.clone()));
        assert_eq!(session.results().len(), 1);

        assert!(session.add_result(entry.clone()));
        assert_eq!(session.results().len(), 2);

        // 超过最大限制
        assert!(!session.add_result(entry));
        assert!(!session.can_accept_more_results());
    }

    #[test]
    fn test_search_session_registry() {
        let mut registry = SearchSessionRegistry::new();

        let query = ParsedQuery::new(
            QueryExpression::literal("error".to_string()),
            "error".to_string(),
        );

        let session = SearchSession::new(query.clone(), vec![], None);
        let id = session.id.as_str().to_string();

        registry.register(session);
        assert!(registry.get(&id).is_some());

        registry.cancel(&id);
        assert!(registry.get(&id).unwrap().is_cancelled());

        let removed = registry.remove(&id);
        assert!(removed.is_some());
        assert!(registry.get(&id).is_none());
    }

    #[test]
    fn test_search_session_registry_cleanup() {
        let mut registry = SearchSessionRegistry::new();

        let query = ParsedQuery::new(
            QueryExpression::literal("error".to_string()),
            "error".to_string(),
        );

        // 添加一个运行中的会话
        let mut running_session = SearchSession::new(query.clone(), vec![], None);
        running_session.start();
        registry.register(running_session);

        // 添加一个已完成的会话
        let mut completed_session = SearchSession::new(query, vec![], None);
        completed_session.complete();
        registry.register(completed_session);

        assert_eq!(registry.active_session_ids().len(), 1);

        let cleaned = registry.cleanup_completed();
        assert_eq!(cleaned, 1);
        assert_eq!(registry.active_session_ids().len(), 1);
    }

    #[test]
    fn test_search_source_types() {
        let local = SearchSource {
            id: "local1".to_string(),
            description: "Local directory".to_string(),
            source_type: SearchSourceType::LocalDir {
                path: "/var/log".to_string(),
                recursive: true,
            },
        };

        let agent = SearchSource {
            id: "agent1".to_string(),
            description: "Agent directory".to_string(),
            source_type: SearchSourceType::AgentDir {
                agent_id: "web-01".to_string(),
                path: "/app/logs".to_string(),
                recursive: false,
            },
        };

        assert_ne!(local, agent);
    }
}
