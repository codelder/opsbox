//! Unix 守护进程相关功能

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::path::{Path, PathBuf};

#[cfg(unix)]
use nix::sys::signal::{self, Signal};
#[cfg(unix)]
use nix::unistd::Pid;

/// 获取用户主目录（跨平台）
#[cfg(unix)]
fn get_user_home() -> String {
  std::env::var("HOME").unwrap_or_else(|_| ".".into())
}

/// 默认 PID 文件路径
#[cfg(unix)]
pub fn default_pid_file() -> PathBuf {
  let home = get_user_home();
  let dir = PathBuf::from(home).join(".opsbox");
  let _ = fs::create_dir_all(&dir);
  dir.join("opsbox.pid")
}

/// 确保父目录存在
#[cfg(unix)]
pub fn ensure_parent_dir(path: &Path) {
  if let Some(parent) = path.parent() {
    let _ = fs::create_dir_all(parent);
  }
}

/// 解析 PID 文件路径（处理 ~ 前缀）
#[cfg(unix)]
pub fn resolve_pid_path(opt: &Option<PathBuf>) -> PathBuf {
  if let Some(p) = opt {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("~/") {
      let home = get_user_home();
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

  tracing::info!("已发送 {} 到进程 {}", signal_name(force), pid_num);

  // 等待最多 5 秒确认进程退出
  let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
  let mut exited = false;
  while std::time::Instant::now() < deadline {
    if !check_process_alive(pid) {
      tracing::info!("进程 {} 已退出", pid_num);
      exited = true;
      break;
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
  }

  // 如未退出且非强制，尝试升级为 SIGKILL 再等 2 秒
  if !exited && !force {
    tracing::warn!("进程 {} 未在超时时间内退出，升级为 SIGKILL", pid_num);
    send_signal_to_process(pid, Signal::SIGKILL)?;
    let deadline2 = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline2 {
      if !check_process_alive(pid) {
        tracing::info!("进程 {} 已被 SIGKILL 终止", pid_num);
        exited = true;
        break;
      }
      std::thread::sleep(std::time::Duration::from_millis(100));
    }
  }

  // 仅在确认退出时移除 PID 文件
  if exited {
    let _ = fs::remove_file(&pid_path);
  } else {
    tracing::warn!("进程 {} 仍在运行，未移除 PID 文件", pid_num);
  }
  Ok(())
}

/// 启动守护进程（Unix）
#[cfg(unix)]
pub fn start_daemon(pid_path: PathBuf) -> io::Result<()> {
  use daemonize::Daemonize;

  // 保持当前工作目录
  let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
  ensure_parent_dir(&pid_path);

  // 守护进程输出重定向：不再写入 ~/.opsbox/opsbox.log，直接丢弃
  let dev_null = PathBuf::from("/dev/null");
  let stdout = fs::OpenOptions::new().create(true).append(true).open(&dev_null)?;
  let stderr = fs::OpenOptions::new().create(true).append(true).open(&dev_null)?;

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

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::Mutex;

  /// Mutex to serialize tests that modify the HOME environment variable.
  /// Prevents race conditions when `cargo test` runs tests in parallel.
  #[cfg(unix)]
  static HOME_LOCK: Mutex<()> = Mutex::new(());

  /// Test get_user_home returns HOME environment variable
  #[test]
  #[cfg(unix)]
  fn test_get_user_home() {
    let _guard = HOME_LOCK.lock().unwrap();
    // SAFETY: 单元测试中设置 HOME 环境变量，由 HOME_LOCK 保证串行访问。
    unsafe {
      std::env::set_var("HOME", "/home/testuser");
    }
    assert_eq!(get_user_home(), "/home/testuser");
  }

  /// Test get_user_home falls back to current directory
  #[test]
  #[cfg(unix)]
  fn test_get_user_home_fallback() {
    let _guard = HOME_LOCK.lock().unwrap();
    // SAFETY: 单元测试中修改环境变量，由 HOME_LOCK 保证串行访问。
    unsafe {
      std::env::remove_var("HOME");
    }
    assert_eq!(get_user_home(), ".");
  }

  /// Test resolve_pid_path with tilde expansion
  #[test]
  #[cfg(unix)]
  fn test_resolve_pid_path_with_tilde() {
    let _guard = HOME_LOCK.lock().unwrap();
    // SAFETY: 单元测试中设置 HOME 环境变量，由 HOME_LOCK 保证串行访问。
    unsafe {
      std::env::set_var("HOME", "/home/testuser");
    }
    let path = resolve_pid_path(&Some(PathBuf::from("~/custom.pid")));
    assert_eq!(path, PathBuf::from("/home/testuser/custom.pid"));
  }

  /// Test resolve_pid_path without tilde
  #[test]
  #[cfg(unix)]
  fn test_resolve_pid_path_absolute() {
    let path = resolve_pid_path(&Some(PathBuf::from("/var/run/opsbox.pid")));
    assert_eq!(path, PathBuf::from("/var/run/opsbox.pid"));
  }

  /// Test resolve_pid_path returns default when None
  #[test]
  #[cfg(unix)]
  fn test_resolve_pid_path_default() {
    let _guard = HOME_LOCK.lock().unwrap();
    // SAFETY: 单元测试中设置 HOME 环境变量，由 HOME_LOCK 保证串行访问。
    unsafe {
      std::env::set_var("HOME", "/home/testuser");
    }
    let path = resolve_pid_path(&None);
    assert!(path.to_string_lossy().contains(".opsbox"));
    assert!(path.to_string_lossy().contains("opsbox.pid"));
  }

  /// Test ensure_parent_dir creates directory
  #[test]
  #[cfg(unix)]
  fn test_ensure_parent_dir() {
    let temp_dir = std::env::temp_dir();
    let test_path = temp_dir.join("test_opsbox_dir").join("test.pid");

    ensure_parent_dir(&test_path);

    assert!(test_path.parent().unwrap().exists());

    // Cleanup
    let _ = fs::remove_dir_all(test_path.parent().unwrap());
  }

  /// Test signal_name function
  #[test]
  #[cfg(unix)]
  fn test_signal_name() {
    assert_eq!(signal_name(false), "SIGTERM");
    assert_eq!(signal_name(true), "SIGKILL");
  }
}
