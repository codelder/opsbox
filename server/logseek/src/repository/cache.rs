use std::{
  collections::HashMap,
  sync::OnceLock,
  time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tokio::time as tokio_time;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Entry<T> {
  pub last_touch: Instant,
  pub value: T,
}

type KeywordsCache = RwLock<HashMap<String, Entry<Vec<String>>>>;
type FilesCache = RwLock<HashMap<(String, String), Entry<Vec<String>>>>;

#[derive(Debug)]
pub struct Cache {
  ttl: Duration,
  keywords: KeywordsCache, // sid -> keywords
  files: FilesCache,       // (sid, file_id) -> lines
}

static GLOBAL: OnceLock<Cache> = OnceLock::new();
static CLEANER_STARTED: OnceLock<()> = OnceLock::new();
static CLEANER_CANCEL: OnceLock<CancellationToken> = OnceLock::new();

pub fn cache() -> &'static Cache {
  GLOBAL.get_or_init(|| Cache {
    ttl: Duration::from_secs(2 * 60),
    keywords: RwLock::new(HashMap::new()),
    files: RwLock::new(HashMap::new()),
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
        let interval = Duration::from_secs(60);
        loop {
          tokio::select! {
            _ = tokio_time::sleep(interval) => {
              let c = cache();
              let now = Instant::now();
              // 清理 keywords
              {
                let mut m = c.keywords.write().await;
                let to_remove: Vec<String> = m
                  .iter()
                  .filter(|(_, e)| now.duration_since(e.last_touch) > c.ttl)
                  .map(|(k, _)| k.clone())
                  .collect();
                for k in to_remove {
                  let _ = m.remove(&k);
                }
              }
              // 清理 files
              {
                let mut m = c.files.write().await;
                let to_remove: Vec<(String, String)> = m
                  .iter()
                  .filter(|(_, e)| now.duration_since(e.last_touch) > c.ttl)
                  .map(|(k, _)| k.clone())
                  .collect();
                for k in to_remove {
                  let _ = m.remove(&k);
                }
              }
            }
            // 收到关闭信号时退出循环
            _ = token.cancelled() => {
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
  pub async fn put_lines(&self, sid: &str, file_id: &str, lines: Vec<String>) {
    Self::start_cleaner_once();
    let mut map = self.files.write().await;
    map.insert(
      (sid.to_string(), file_id.to_string()),
      Entry {
        last_touch: Instant::now(),
        value: lines,
      },
    );
  }
  pub async fn get_lines_slice(
    &self,
    sid: &str,
    file_id: &str,
    start: usize,
    end: usize,
  ) -> Option<(usize, Vec<String>)> {
    Self::start_cleaner_once();
    let mut map = self.files.write().await;
    let key = (sid.to_string(), file_id.to_string());
    let e = map.get_mut(&key)?;
    if self.expired(e) {
      map.remove(&key);
      return None;
    }
    e.last_touch = Instant::now();
    let total = e.value.len();
    let s = start.max(1).min(total.max(1));
    let eidx = end.max(s).min(total);
    let slice = e.value[(s - 1)..eidx].to_vec();
    Some((total, slice))
  }

  /// 停止后台清理任务（用于优雅关闭）
  pub fn stop_cleaner() {
    if let Some(token) = CLEANER_CANCEL.get() {
      token.cancel();
    }
  }
}
