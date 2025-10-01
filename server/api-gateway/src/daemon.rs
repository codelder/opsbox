//! Unix 守护进程相关功能

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// 默认 PID 文件路径
pub fn default_pid_file() -> PathBuf {
  let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
  let dir = PathBuf::from(home).join(".opsbox");
  let _ = fs::create_dir_all(&dir);
  dir.join("opsbox.pid")
}

/// 默认日志文件路径
pub fn default_log_file() -> PathBuf {
  let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
  let dir = PathBuf::from(home).join(".opsbox");
  let _ = fs::create_dir_all(&dir);
  dir.join("opsbox.log")
}

/// 确保父目录存在
pub fn ensure_parent_dir(path: &Path) {
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }
}

/// 解析 PID 文件路径（处理 ~ 前缀）
pub fn resolve_pid_path(opt: &Option<PathBuf>) -> PathBuf {
  if let Some(p) = opt {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("~/") {
      let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
      return PathBuf::from(home).join(stripped);
    }
    p.clone()
  } else {
    default_pid_file()
  }
}

#[cfg(unix)]
fn signal_name(force: bool) -> &'static str {
  if force { "SIGKILL" } else { "SIGTERM" }
}

#[cfg(unix)]
fn send_signal_to_process(pid: Pid, sig: Signal) -> io::Result<()> {
  signal::kill(pid, sig).map_err(|e| io::Error::other(format!("发送信号失败: {}", e)))
}

#[cfg(unix)]
fn check_process_alive(pid: Pid) -> bool {
  // 发送信号 0 来检查进程是否存活
  signal::kill(pid, None).is_ok()
}

/// 停止守护进程（Unix）
#[cfg(unix)]
pub fn stop_daemon(pid_path: PathBuf, force: bool) -> io::Result<()> {
  let txt = fs::read_to_string(&pid_path)?;
  let pid_num: i32 = txt
    .trim()
    .parse()
    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "PID 文件内容无效"))?;
  let pid = Pid::from_raw(pid_num);

  // 发送信号
  let signal = if force { Signal::SIGKILL } else { Signal::SIGTERM };
  send_signal_to_process(pid, signal)?;

  log::info!("已发送 {} 到进程 {}", signal_name(force), pid_num);

  // 等待最多 5 秒确认进程退出
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
  while std::time::Instant::now() < deadline {
    if !check_process_alive(pid) {
      log::info!("进程 {} 已退出", pid_num);
      break;
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
  }

  // 移除 PID 文件
  let _ = fs::remove_file(&pid_path);
  Ok(())
}

/// 启动守护进程（Unix）
#[cfg(unix)]
pub fn start_daemon(pid_path: PathBuf) -> io::Result<()> {
  use daemonize::Daemonize;

  // 保持当前工作目录
  let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
  ensure_parent_dir(&pid_path);

  // 准备日志文件
  let log_path = default_log_file();
  let _ = fs::create_dir_all(log_path.parent().unwrap_or(Path::new(".")));
  let stdout = fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
  let stderr = fs::OpenOptions::new().create(true).append(true).open(&log_path)?;

  log::info!("准备后台运行，PID 文件: {:?}, 日志文件: {:?}", pid_path, log_path);

  let daemon = Daemonize::new()
    .pid_file(pid_path.clone())
    .working_directory(cwd)
    .stdout(stdout)
    .stderr(stderr);

  daemon
    .start()
    .map_err(|e| io::Error::other(format!("后台运行失败: {}", e)))?;

  Ok(())
}
