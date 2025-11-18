//! 优雅关闭功能
//!
//! 处理系统信号和优雅关闭逻辑

use tracing::info;

/// 等待关闭信号
#[cfg(unix)]
pub async fn shutdown_signal() {
  use tokio::signal::unix::{SignalKind, signal};

  let mut sigterm = signal(SignalKind::terminate()).expect("无法监听 SIGTERM");
  let mut sigint = signal(SignalKind::interrupt()).expect("无法监听 SIGINT");

  let signal_name = tokio::select! {
    _ = sigterm.recv() => "SIGTERM",
    _ = sigint.recv() => "SIGINT (Ctrl-C)",
  };

  info!("收到关闭信号 [{}]，开始优雅关闭...", signal_name);
}

/// 等待关闭信号 (Windows)
#[cfg(not(unix))]
pub async fn shutdown_signal() {
  tokio::signal::ctrl_c().await.expect("无法监听 Ctrl-C 信号");
  info!("收到关闭信号 [Ctrl-C]，开始优雅关闭...");
}
