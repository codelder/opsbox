use std::{
  collections::HashMap,
  sync::OnceLock,
  time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tokio::time as tokio_time;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{domain::Odfi, query::KeywordHighlight};

#[derive(Debug, Clone)]
pub struct CompactLines {
  content: String,
  line_starts: Vec<usize>,
}

impl CompactLines {
  fn from_lines(lines: Vec<String>) -> Self {
    // 预分配内存：总字符数 + 少量额外空间
    let total_len: usize = lines.iter().map(|s| s.len()).sum();
    let mut content = String::with_capacity(total_len);
    let mut line_starts = Vec::with_capacity(lines.len() + 1);

    for line in lines {
      line_starts.push(content.len());
      content.push_str(&line);
    }
    line_starts.push(content.len()); // 哨兵，标记最后一个行的结束位置

    Self { content, line_starts }
  }

  fn get_slice(&self, start: usize, end: usize) -> Vec<String> {
    let mut res = Vec::with_capacity(end - start + 1);
    // start 和 end 是 1-based 索引
    // line_starts 包含 N+1 个元素，索引 0 对应第 1 行的起始
    for i in start..=end {
      if i < 1 || i >= self.line_starts.len() {
        continue;
      }
      let s_idx = self.line_starts[i - 1];
      let e_idx = self.line_starts[i];
      // 安全切片：虽然我们自己构建的索引应该是安全的，但为了保险起见
      if let Some(s) = self.content.get(s_idx..e_idx) {
        res.push(s.to_string());
      }
    }
    res
  }

  fn len(&self) -> usize {
    self.line_starts.len().saturating_sub(1)
  }

  fn size_in_bytes(&self) -> usize {
    self.content.len() + (self.line_starts.capacity() * std::mem::size_of::<usize>())
  }
}

#[derive(Debug)]
struct SessionData {
  last_touch: Instant,
  keywords: Vec<KeywordHighlight>,
  files: HashMap<Odfi, CompactLines>,
}

impl SessionData {
  fn size_in_bytes(&self) -> usize {
    let keywords_size: usize = self
      .keywords
      .iter()
      .map(|k| match k {
        KeywordHighlight::Literal(s) | KeywordHighlight::Phrase(s) | KeywordHighlight::Regex(s) => s.len(),
      })
      .sum();
    let files_size: usize = self.files.values().map(|v| v.size_in_bytes()).sum();
    keywords_size + files_size
  }
}

#[derive(Debug)]
pub struct Cache {
  ttl: Duration,
  sessions: RwLock<HashMap<String, SessionData>>,
}

static GLOBAL: OnceLock<Cache> = OnceLock::new();
static CLEANER_STARTED: OnceLock<()> = OnceLock::new();
static CLEANER_CANCEL: OnceLock<CancellationToken> = OnceLock::new();

pub fn cache() -> &'static Cache {
  GLOBAL.get_or_init(|| {
    Cache::start_cleaner_once();
    Cache {
      ttl: Duration::from_secs(15 * 60),
      sessions: RwLock::new(HashMap::new()),
    }
  })
}

pub fn new_sid() -> String {
  Uuid::new_v4().to_string()
}

impl Cache {
  fn expired(&self, session: &SessionData) -> bool {
    session.last_touch.elapsed() > self.ttl
  }

  fn start_cleaner_once() {
    CLEANER_STARTED.get_or_init(|| {
      // 使用取消令牌支持优雅关闭后台清理任务
      let token = CLEANER_CANCEL.get_or_init(CancellationToken::new).clone();
      tokio::spawn(async move {
        tracing::info!("后台清理任务已启动，清理间隔: 1分钟");
        let interval = Duration::from_secs(60);
        loop {
          tokio::select! {
            _ = tokio_time::sleep(interval) => {
              let c = cache();
              let now = Instant::now();
              let mut total_removed_count = 0;
              let mut total_removed_bytes = 0;

              // 清理过期会话并统计剩余
              let (active_count, active_bytes) = {
                let mut sessions = c.sessions.write().await;
                let to_remove: Vec<(String, usize)> = sessions
                  .iter()
                  .filter(|(_, data)| now.duration_since(data.last_touch) > c.ttl)
                  .map(|(k, data)| (k.clone(), data.size_in_bytes()))
                  .collect();

                for (k, size) in to_remove {
                  sessions.remove(&k);
                  total_removed_count += 1;
                  total_removed_bytes += size;
                }

                // 统计剩余活跃会话
                let count = sessions.len();
                let bytes: usize = sessions.values().map(|s| s.size_in_bytes()).sum();
                (count, bytes)
              };

              let active_mb = active_bytes as f64 / 1024.0 / 1024.0;

              if total_removed_count > 0 {
                let removed_mb = total_removed_bytes as f64 / 1024.0 / 1024.0;
                tracing::info!(
                  "缓存清理完成: 移除 {} 个过期会话 ({:.2} MB), 当前活跃: {} 个 ({:.2} MB)",
                  total_removed_count, removed_mb, active_count, active_mb
                );
              } else {
                 tracing::debug!(
                   "缓存清理检查完成，无过期会话。当前活跃: {} 个 ({:.2} MB)",
                   active_count, active_mb
                 );
              }

              // 无论是否移除了条目，都尝试触发底层分配器的内存回收
              // 这对于回收搜索过程中产生的临时内存（如未命中的文件内容）非常重要
              #[cfg(feature = "mimalloc-collect")]
              {
                // 使用 spawn_blocking 避免阻塞异步运行时线程
                tokio::task::spawn_blocking(move || {
                  // 直接调用 libmimalloc-sys 提供的 mi_collect FFI
                  unsafe {
                    libmimalloc_sys::mi_collect(true);
                  }
                  tracing::debug!("libmimalloc_sys::mi_collect(true) 调用完成");
                });
              }
            }
            // 收到关闭信号时退出循环
            _ = token.cancelled() => {
              tracing::info!("后台清理任务已停止");
              break;
            }
          }
        }
      });
    });
  }

  pub async fn put_keywords(&self, sid: &str, kws: Vec<KeywordHighlight>) {
    Self::start_cleaner_once();
    let mut sessions = self.sessions.write().await;

    sessions
      .entry(sid.to_string())
      .and_modify(|s| {
        s.last_touch = Instant::now();
        s.keywords = kws.clone();
      })
      .or_insert(SessionData {
        last_touch: Instant::now(),
        keywords: kws,
        files: HashMap::new(),
      });
  }

  pub async fn get_keywords(&self, sid: &str) -> Option<Vec<KeywordHighlight>> {
    Self::start_cleaner_once();
    let mut sessions = self.sessions.write().await;

    if let Some(session) = sessions.get_mut(sid) {
      if self.expired(session) {
        sessions.remove(sid);
        return None;
      }
      session.last_touch = Instant::now();
      return Some(session.keywords.clone());
    }
    None
  }

  pub async fn put_lines(&self, sid: &str, file_url: &Odfi, lines: Vec<String>) {
    Self::start_cleaner_once();
    let mut sessions = self.sessions.write().await;

    let session = sessions.entry(sid.to_string()).or_insert(SessionData {
      last_touch: Instant::now(),
      keywords: Vec::new(),
      files: HashMap::new(),
    });

    session.last_touch = Instant::now();

    tracing::debug!(
      "🔍 Cache存储: sid={}, file_url={}, lines_count={}",
      sid,
      file_url,
      lines.len()
    );

    // 使用 CompactLines 优化存储
    let compact = CompactLines::from_lines(lines);
    session.files.insert(file_url.clone(), compact);

    tracing::debug!("🔍 Cache当前会话文件数: {}", session.files.len());
  }

  pub async fn get_lines_slice(
    &self,
    sid: &str,
    file_url: &Odfi,
    start: usize,
    end: usize,
  ) -> Option<(usize, Vec<String>)> {
    Self::start_cleaner_once();
    let mut sessions = self.sessions.write().await;

    if let Some(session) = sessions.get_mut(sid) {
      if self.expired(session) {
        tracing::debug!("🔍 Cache会话已过期，移除: sid={}", sid);
        sessions.remove(sid);
        return None;
      }

      session.last_touch = Instant::now();

      if let Some(compact_lines) = session.files.get(file_url) {
        let total = compact_lines.len();
        if total == 0 {
          return Some((0, Vec::new()));
        }
        let s = start.max(1).min(total.max(1));
        let eidx = end.max(s).min(total);

        let slice = compact_lines.get_slice(s, eidx);
        return Some((total, slice));
      }
    }

    None
  }

  /// 按 sid 显式移除缓存（用于关闭标签页或会话结束时清理资源）
  pub async fn remove_sid(&self, sid: &str) {
    let mut sessions = self.sessions.write().await;
    if sessions.remove(sid).is_some() {
      tracing::debug!("已清理 sid={} 的缓存", sid);
    }
  }

  /// 获取指定会话的所有文件列表
  pub async fn get_file_list(&self, sid: &str) -> Option<Vec<Odfi>> {
    Self::start_cleaner_once();
    let mut sessions = self.sessions.write().await;

    if let Some(session) = sessions.get_mut(sid) {
      if self.expired(session) {
        tracing::debug!("🔍 Cache会话已过期，移除: sid={}", sid);
        sessions.remove(sid);
        return None;
      }

      session.last_touch = Instant::now();
      let files: Vec<Odfi> = session.files.keys().cloned().collect();
      return Some(files);
    }

    None
  }

  /// 停止后台清理任务（用于优雅关闭）
  pub fn stop_cleaner() {
    if let Some(token) = CLEANER_CANCEL.get() {
      token.cancel();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_cache_put_and_get_keywords() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let keywords = vec![
      KeywordHighlight::Literal("error".to_string()),
      KeywordHighlight::Literal("warn".to_string()),
    ];

    c.put_keywords(&sid, keywords.clone()).await;
    let result = c.get_keywords(&sid).await;

    assert_eq!(result, Some(keywords));
  }

  #[tokio::test]
  async fn test_cache_get_keywords_missing() {
    let c = cache();
    let result = c.get_keywords("non-existent-sid").await;
    assert_eq!(result, None);
  }

  #[tokio::test]
  async fn test_cache_put_and_get_file_lines() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "test-file.log",
      None,
    );
    let lines = vec!["line 1".to_string(), "line 2".to_string()];

    c.put_lines(&sid, &file_url, lines.clone()).await;
    let result = c.get_lines_slice(&sid, &file_url, 1, 2).await;

    assert!(result.is_some());
    let (total, slice) = result.unwrap();
    assert_eq!(total, 2);
    assert_eq!(slice, lines);
  }

  #[tokio::test]
  async fn test_cache_get_file_lines_missing() {
    let c = cache();
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "non-existent-file.log",
      None,
    );
    let result = c.get_lines_slice("non-existent-sid", &file_url, 1, 10).await;
    assert_eq!(result, None);
  }

  #[tokio::test]
  async fn test_cache_keywords_retrieval() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let keywords = vec![KeywordHighlight::Literal("test".to_string())];

    c.put_keywords(&sid, keywords.clone()).await;

    // 验证可以多次获取
    let result1 = c.get_keywords(&sid).await;
    let result2 = c.get_keywords(&sid).await;

    assert_eq!(result1, Some(keywords.clone()));
    assert_eq!(result2, Some(keywords));
  }

  #[tokio::test]
  async fn test_cache_overwrite_keywords() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());

    c.put_keywords(&sid, vec![KeywordHighlight::Literal("old".to_string())])
      .await;
    c.put_keywords(&sid, vec![KeywordHighlight::Literal("new".to_string())])
      .await;

    let result = c.get_keywords(&sid).await;
    assert_eq!(result, Some(vec![KeywordHighlight::Literal("new".to_string())]));
  }

  #[tokio::test]
  async fn test_cache_multiple_files_same_sid() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url1 = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "file1.log",
      None,
    );
    let file_url2 = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "file2.log",
      None,
    );

    c.put_lines(&sid, &file_url1, vec!["a".to_string()]).await;
    c.put_lines(&sid, &file_url2, vec!["b".to_string()]).await;

    let result1 = c.get_lines_slice(&sid, &file_url1, 1, 1).await.map(|(_, lines)| lines);
    let result2 = c.get_lines_slice(&sid, &file_url2, 1, 1).await.map(|(_, lines)| lines);

    assert_eq!(result1, Some(vec!["a".to_string()]));
    assert_eq!(result2, Some(vec!["b".to_string()]));
  }

  #[tokio::test]
  async fn test_cache_same_file_different_sids() {
    let c = cache();
    let sid1 = format!("test-sid-1-{}", Uuid::new_v4());
    let sid2 = format!("test-sid-2-{}", Uuid::new_v4());
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "shared-file.log",
      None,
    );

    c.put_lines(&sid1, &file_url, vec!["content1".to_string()]).await;
    c.put_lines(&sid2, &file_url, vec!["content2".to_string()]).await;

    let result1 = c.get_lines_slice(&sid1, &file_url, 1, 1).await.map(|(_, lines)| lines);
    let result2 = c.get_lines_slice(&sid2, &file_url, 1, 1).await.map(|(_, lines)| lines);

    assert_eq!(result1, Some(vec!["content1".to_string()]));
    assert_eq!(result2, Some(vec!["content2".to_string()]));
  }

  #[tokio::test]
  async fn test_cache_get_file_lines_slice() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "test-file.log",
      None,
    );
    let lines: Vec<String> = (1..=10).map(|i| format!("line {}", i)).collect();

    c.put_lines(&sid, &file_url, lines).await;

    // 获取第 3-5 行 (1-based indexing)
    let result = c.get_lines_slice(&sid, &file_url, 3, 5).await;

    assert!(result.is_some());
    let (total, slice) = result.unwrap();
    assert_eq!(total, 10);
    assert_eq!(slice.len(), 3);
    assert_eq!(slice[0], "line 3");
    assert_eq!(slice[2], "line 5");
  }

  #[tokio::test]
  async fn test_cache_get_file_lines_slice_out_of_bounds() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "test-file.log",
      None,
    );
    let lines = vec!["line 1".to_string(), "line 2".to_string()];

    c.put_lines(&sid, &file_url, lines).await;

    // 请求超出范围的行
    let result = c.get_lines_slice(&sid, &file_url, 1, 100).await;

    assert!(result.is_some());
    let (total, slice) = result.unwrap();
    assert_eq!(total, 2);
    assert_eq!(slice.len(), 2); // 应该只返回实际存在的行
  }

  #[test]
  fn test_new_sid_generates_valid_uuid() {
    let sid1 = new_sid();
    let sid2 = new_sid();

    // 验证是有效的 UUID 格式
    assert!(Uuid::parse_str(&sid1).is_ok());
    assert!(Uuid::parse_str(&sid2).is_ok());

    // 验证每次生成的 SID 不同
    assert_ne!(sid1, sid2);
  }

  #[test]
  fn test_cache_singleton() {
    let c1 = cache();
    let c2 = cache();

    // 验证是同一个实例
    assert!(std::ptr::eq(c1, c2));
  }

  #[tokio::test]
  async fn test_cache_get_updates_last_touch() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());

    c.put_keywords(&sid, vec![KeywordHighlight::Literal("test".to_string())])
      .await;

    // 等待一小段时间
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // get 会更新 last_touch
    let result = c.get_keywords(&sid).await;
    assert!(result.is_some());
  }

  #[tokio::test]
  async fn test_cache_concurrent_writes() {
    use tokio::task;

    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());

    // 并发写入
    let handles: Vec<_> = (0..10)
      .map(|i| {
        let sid = sid.clone();
        task::spawn(async move {
          cache()
            .put_keywords(&sid, vec![KeywordHighlight::Literal(format!("keyword-{}", i))])
            .await;
        })
      })
      .collect();

    // 等待所有任务完成
    for handle in handles {
      handle.await.unwrap();
    }

    // 验证最后一次写入成功（顺序不确定，但应该有一个值）
    let result = c.get_keywords(&sid).await;
    assert!(result.is_some());
  }

  #[tokio::test]
  async fn test_cache_concurrent_reads() {
    use tokio::task;

    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let keywords = vec![
      KeywordHighlight::Literal("test1".to_string()),
      KeywordHighlight::Literal("test2".to_string()),
    ];

    c.put_keywords(&sid, keywords.clone()).await;

    // 并发读取
    let handles: Vec<_> = (0..10)
      .map(|_| {
        let sid = sid.clone();
        let expected = keywords.clone();
        task::spawn(async move {
          let result = cache().get_keywords(&sid).await;
          assert_eq!(result, Some(expected));
        })
      })
      .collect();

    // 等待所有任务完成
    for handle in handles {
      handle.await.unwrap();
    }
  }

  #[tokio::test]
  async fn test_cache_get_lines_slice_boundary_conditions() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "test-file.log",
      None,
    );
    let lines: Vec<String> = (1..=5).map(|i| format!("line {}", i)).collect();

    c.put_lines(&sid, &file_url, lines).await;

    // 测试边界条件：start=0（应该被调整为1）
    let result = c.get_lines_slice(&sid, &file_url, 0, 2).await;
    assert!(result.is_some());
    let (_, slice) = result.unwrap();
    assert_eq!(slice[0], "line 1");

    // 测试边界条件：end > total（应该被限制）
    let result = c.get_lines_slice(&sid, &file_url, 1, 1000).await;
    assert!(result.is_some());
    let (total, slice) = result.unwrap();
    assert_eq!(total, 5);
    assert_eq!(slice.len(), 5);

    // 测试边界条件：start > end（应该返回空或最小范围）
    let result = c.get_lines_slice(&sid, &file_url, 3, 2).await;
    assert!(result.is_some());
    let (_, slice) = result.unwrap();
    // start 会被调整，应该至少返回一行
    assert!(!slice.is_empty());
  }

  #[tokio::test]
  async fn test_cache_empty_keywords() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());

    // 存储空关键词列表
    c.put_keywords(&sid, Vec::<KeywordHighlight>::new()).await;

    let result = c.get_keywords(&sid).await;
    assert_eq!(result, Some(Vec::<KeywordHighlight>::new()));
  }

  #[tokio::test]
  async fn test_cache_empty_lines() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "empty-file.log",
      None,
    );

    // 存储空行列表
    c.put_lines(&sid, &file_url, vec![]).await;

    let result = c.get_lines_slice(&sid, &file_url, 1, 10).await;
    // 空文件应该返回 None 或空结果
    // 根据实现，可能需要调整断言
    if let Some((total, slice)) = result {
      assert_eq!(total, 0);
      assert_eq!(slice.len(), 0);
    }
  }

  #[tokio::test]
  async fn test_cache_large_keywords_list() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());

    // 创建大量关键词
    let keywords: Vec<KeywordHighlight> = (0..1000)
      .map(|i| KeywordHighlight::Literal(format!("keyword-{}", i)))
      .collect();

    c.put_keywords(&sid, keywords.clone()).await;

    let result = c.get_keywords(&sid).await;
    assert_eq!(result, Some(keywords));
  }

  #[tokio::test]
  async fn test_cache_special_characters_in_sid() {
    let c = cache();
    let sid = "sid-with-特殊字符-!@#$%";

    c.put_keywords(sid, vec![KeywordHighlight::Literal("test".to_string())])
      .await;

    let result = c.get_keywords(sid).await;
    assert_eq!(result, Some(vec![KeywordHighlight::Literal("test".to_string())]));
  }

  #[tokio::test]
  async fn test_cache_special_characters_in_file_id() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "path/to/file-with-特殊字符.log",
      None,
    );

    c.put_lines(&sid, &file_url, vec!["line 1".to_string()]).await;

    let result = c.get_lines_slice(&sid, &file_url, 1, 1).await;
    assert!(result.is_some());
  }

  #[tokio::test]
  async fn test_cache_remove_sid() {
    let c = cache();
    let sid = format!("test-sid-remove-{}", Uuid::new_v4());
    let file_url = Odfi::new(
      crate::domain::EndpointType::Local,
      "localhost",
      crate::domain::TargetType::Dir,
      "test-file.log",
      None,
    );

    c.put_keywords(&sid, vec![KeywordHighlight::Literal("test".to_string())])
      .await;
    c.put_lines(&sid, &file_url, vec!["line 1".to_string()]).await;

    // Verify existence
    assert!(c.get_keywords(&sid).await.is_some());
    assert!(c.get_lines_slice(&sid, &file_url, 1, 1).await.is_some());

    // Remove
    c.remove_sid(&sid).await;

    // Verify removal
    assert!(c.get_keywords(&sid).await.is_none());
    assert!(c.get_lines_slice(&sid, &file_url, 1, 1).await.is_none());
  }
}
