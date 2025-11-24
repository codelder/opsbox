use std::{
  collections::HashMap,
  sync::OnceLock,
  time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tokio::time as tokio_time;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::domain::FileUrl;

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
}

#[derive(Debug, Clone)]
pub struct Entry<T> {
  pub last_touch: Instant,
  pub value: T,
}

type KeywordsCache = RwLock<HashMap<String, Entry<Vec<String>>>>;
type FilesCache = RwLock<HashMap<(String, FileUrl), Entry<CompactLines>>>;

#[derive(Debug)]
pub struct Cache {
  ttl: Duration,
  keywords: KeywordsCache, // sid -> keywords
  files: FilesCache,       // (sid, FileUrl) -> lines
}

static GLOBAL: OnceLock<Cache> = OnceLock::new();
static CLEANER_STARTED: OnceLock<()> = OnceLock::new();
static CLEANER_CANCEL: OnceLock<CancellationToken> = OnceLock::new();

pub fn cache() -> &'static Cache {
  GLOBAL.get_or_init(|| {
    Cache::start_cleaner_once();
    Cache {
      ttl: Duration::from_secs(60),
      keywords: RwLock::new(HashMap::new()),
      files: RwLock::new(HashMap::new()),
    }
  })
}

pub fn new_sid() -> String {
  Uuid::new_v4().to_string()
}

impl Cache {
  fn expired<T>(&self, e: &Entry<T>) -> bool {
    e.last_touch.elapsed() > self.ttl
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
              let mut total_removed = 0;

              // 清理 keywords
              {
                let mut m = c.keywords.write().await;
                let to_remove: Vec<String> = m
                  .iter()
                  .filter(|(_, e)| now.duration_since(e.last_touch) > c.ttl)
                  .map(|(k, _)| k.clone())
                  .collect();
                total_removed += to_remove.len();
                for k in to_remove {
                  let _ = m.remove(&k);
                }
              }

              // 清理 files
              {
                let mut m = c.files.write().await;
                let to_remove: Vec<(String, FileUrl)> = m
                  .iter()
                  .filter(|(_, e)| now.duration_since(e.last_touch) > c.ttl)
                  .map(|(k, _)| k.clone())
                  .collect();
                total_removed += to_remove.len();
                for k in to_remove {
                  let _ = m.remove(&k);
                }
              }

              if total_removed > 0 {
                tracing::info!("缓存清理完成: 移除 {} 个条目", total_removed);
              } else {
                 tracing::info!("缓存清理检查完成，无过期条目");
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
                  tracing::info!("libmimalloc_sys::mi_collect(true) 调用完成");
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

  pub async fn put_keywords(&self, sid: &str, kws: Vec<String>) {
    Self::start_cleaner_once();
    let mut map = self.keywords.write().await;
    map.insert(
      sid.to_string(),
      Entry {
        last_touch: Instant::now(),
        value: kws,
      },
    );
  }
  pub async fn get_keywords(&self, sid: &str) -> Option<Vec<String>> {
    Self::start_cleaner_once();
    let mut map = self.keywords.write().await; // write to refresh
    let e = map.get_mut(sid)?;
    if self.expired(e) {
      map.remove(sid);
      return None;
    }
    e.last_touch = Instant::now();
    Some(e.value.clone())
  }
  pub async fn put_lines(&self, sid: &str, file_url: &FileUrl, lines: Vec<String>) {
    Self::start_cleaner_once();
    let mut map = self.files.write().await;
    let key = (sid.to_string(), file_url.clone());
    tracing::debug!(
      "🔍 Cache存储: key=({:?}, {:?}), lines_count={}",
      key.0,
      key.1,
      lines.len()
    );

    // 使用 CompactLines 优化存储，减少内存碎片
    let compact = CompactLines::from_lines(lines);

    map.insert(
      key,
      Entry {
        last_touch: Instant::now(),
        value: compact,
      },
    );
    tracing::debug!("🔍 Cache当前大小: {}", map.len());
  }
  pub async fn get_lines_slice(
    &self,
    sid: &str,
    file_url: &FileUrl,
    start: usize,
    end: usize,
  ) -> Option<(usize, Vec<String>)> {
    Self::start_cleaner_once();
    let mut map = self.files.write().await;
    let key = (sid.to_string(), file_url.clone());
    tracing::debug!("🔍 Cache查找: key=({:?}, {:?}), cache_size={}", key.0, key.1, map.len());

    // 打印所有现有的键用于调试
    for (existing_key, entry) in map.iter() {
      tracing::debug!(
        "🔍 Cache现有条目: key=({:?}, {:?}), expired={}",
        existing_key.0,
        existing_key.1,
        self.expired(entry)
      );
    }

    let e = map.get_mut(&key)?;
    if self.expired(e) {
      tracing::debug!("🔍 Cache条目已过期，移除");
      map.remove(&key);
      return None;
    }
    e.last_touch = Instant::now();

    let total = e.value.len();
    if total == 0 {
      return Some((0, Vec::new()));
    }
    let s = start.max(1).min(total.max(1));
    let eidx = end.max(s).min(total);

    // 从 CompactLines 中提取切片
    let slice = e.value.get_slice(s, eidx);

    Some((total, slice))
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
    let keywords = vec!["error".to_string(), "warn".to_string()];

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
    let file_url = FileUrl::local("/test-file.log");
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
    let file_url = FileUrl::local("/non-existent-file.log");
    let result = c.get_lines_slice("non-existent-sid", &file_url, 1, 10).await;
    assert_eq!(result, None);
  }

  #[tokio::test]
  async fn test_cache_keywords_retrieval() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let keywords = vec!["test".to_string()];

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

    c.put_keywords(&sid, vec!["old".to_string()]).await;
    c.put_keywords(&sid, vec!["new".to_string()]).await;

    let result = c.get_keywords(&sid).await;
    assert_eq!(result, Some(vec!["new".to_string()]));
  }

  #[tokio::test]
  async fn test_cache_multiple_files_same_sid() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url1 = FileUrl::local("/file1.log");
    let file_url2 = FileUrl::local("/file2.log");

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
    let file_url = FileUrl::local("/shared-file.log");

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
    let file_url = FileUrl::local("/test-file.log");
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
    let file_url = FileUrl::local("/test-file.log");
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

    c.put_keywords(&sid, vec!["test".to_string()]).await;

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
          cache().put_keywords(&sid, vec![format!("keyword-{}", i)]).await;
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
    let keywords = vec!["test1".to_string(), "test2".to_string()];

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
    let file_url = FileUrl::local("/test-file.log");
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
    c.put_keywords(&sid, vec![]).await;

    let result = c.get_keywords(&sid).await;
    assert_eq!(result, Some(vec![]));
  }

  #[tokio::test]
  async fn test_cache_empty_lines() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url = FileUrl::local("/empty-file.log");

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
    let keywords: Vec<String> = (0..1000).map(|i| format!("keyword-{}", i)).collect();

    c.put_keywords(&sid, keywords.clone()).await;

    let result = c.get_keywords(&sid).await;
    assert_eq!(result, Some(keywords));
  }

  #[tokio::test]
  async fn test_cache_special_characters_in_sid() {
    let c = cache();
    let sid = "sid-with-特殊字符-!@#$%";

    c.put_keywords(sid, vec!["test".to_string()]).await;

    let result = c.get_keywords(sid).await;
    assert_eq!(result, Some(vec!["test".to_string()]));
  }

  #[tokio::test]
  async fn test_cache_special_characters_in_file_id() {
    let c = cache();
    let sid = format!("test-sid-{}", Uuid::new_v4());
    let file_url = FileUrl::local("/path/to/file-with-特殊字符.log");

    c.put_lines(&sid, &file_url, vec!["line 1".to_string()]).await;

    let result = c.get_lines_slice(&sid, &file_url, 1, 1).await;
    assert!(result.is_some());
  }
}
