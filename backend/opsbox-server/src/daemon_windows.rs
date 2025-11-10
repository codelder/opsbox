//! Windows 服务相关功能

#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use tokio::sync::Notify;
#[cfg(windows)]
use windows_service::{
  service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
  service_control_handler::{self, ServiceControlHandlerResult},
  service_dispatcher,
};

#[cfg(windows)]
static SERVICE_STOPPING: AtomicBool = AtomicBool::new(false);

/// Windows 服务控制处理器
#[cfg(windows)]
fn service_control_handler(control_event: ServiceControl) -> ServiceControlHandlerResult {
  match control_event {
    ServiceControl::Stop => {
      SERVICE_STOPPING.store(true, Ordering::SeqCst);
      log::info!("收到 Windows 服务停止请求");
      ServiceControlHandlerResult::NoError
    }
    ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
    _ => ServiceControlHandlerResult::NotImplemented,
  }
}

/// 启动 Windows 服务
#[cfg(windows)]
pub fn run_as_service(
  service_name: &str,
  main_fn: impl FnOnce(Arc<Notify>) -> Result<(), Box<dyn std::error::Error>> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
  let service_name = OsString::from(service_name);
  let shutdown_notify = Arc::new(Notify::new());

  // 注册服务控制处理器
  let status_handle = service_control_handler::register(service_name.as_os_str(), service_control_handler)?;

  // 设置服务状态为运行中
  status_handle.set_service_status(ServiceStatus {
    service_type: ServiceType::OWN_PROCESS,
    current_state: ServiceState::Running,
    controls_accepted: ServiceControlAccept::STOP,
    exit_code: ServiceExitCode::Win32(0),
    checkpoint: 0,
    wait_hint: std::time::Duration::default(),
    process_id: None,
  })?;

  log::info!("Windows 服务已启动");

  // 启动主逻辑（在新线程中运行 Tokio 运行时）
  let shutdown = shutdown_notify.clone();
  let status_handle_clone = status_handle.clone();
  std::thread::spawn(move || {
    if let Err(e) = main_fn(shutdown) {
      log::error!("服务运行错误: {}", e);
      // 设置服务状态为已停止（错误）
      let _ = status_handle_clone.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(1),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
      });
    }
  });

  // 等待停止信号
  while !SERVICE_STOPPING.load(Ordering::SeqCst) {
    std::thread::sleep(std::time::Duration::from_millis(100));
  }

  log::info!("开始停止 Windows 服务...");
  shutdown_notify.notify_waiters();

  // 等待一段时间让服务优雅关闭
  std::thread::sleep(std::time::Duration::from_secs(2));

  // 设置服务状态为已停止
  status_handle.set_service_status(ServiceStatus {
    service_type: ServiceType::OWN_PROCESS,
    current_state: ServiceState::Stopped,
    controls_accepted: ServiceControlAccept::empty(),
    exit_code: ServiceExitCode::Win32(0),
    checkpoint: 0,
    wait_hint: std::time::Duration::default(),
    process_id: None,
  })?;

  log::info!("Windows 服务已停止");
  Ok(())
}

/// 安装 Windows 服务
#[cfg(windows)]
pub fn install_service(
  service_name: &str,
  display_name: &str,
  bin_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
  use std::process::Command;

  let bin_path = bin_path.replace('/', "\\"); // 确保使用 Windows 路径分隔符

  let output = Command::new("sc")
    .args(&[
      "create",
      service_name,
      &format!("binPath= {}", bin_path),
      &format!("DisplayName= {}", display_name),
      "start= auto",
    ])
    .output()?;

  if !output.status.success() {
    let error = String::from_utf8_lossy(&output.stderr);
    return Err(format!("安装服务失败: {}", error).into());
  }

  println!("Windows 服务 '{}' 安装成功", service_name);
  Ok(())
}

/// 卸载 Windows 服务
#[cfg(windows)]
pub fn uninstall_service(service_name: &str) -> Result<(), Box<dyn std::error::Error>> {
  use std::process::Command;

  // 先停止服务
  let _ = Command::new("sc").args(&["stop", service_name]).output();

  // 等待服务停止
  std::thread::sleep(std::time::Duration::from_secs(2));

  // 删除服务
  let output = Command::new("sc").args(&["delete", service_name]).output()?;

  if !output.status.success() {
    let error = String::from_utf8_lossy(&output.stderr);
    return Err(format!("卸载服务失败: {}", error).into());
  }

  println!("Windows 服务 '{}' 卸载成功", service_name);
  Ok(())
}

/// 启动 Windows 服务（通过 sc 命令）
#[cfg(windows)]
pub fn start_service(service_name: &str) -> Result<(), Box<dyn std::error::Error>> {
  use std::process::Command;

  let output = Command::new("sc").args(&["start", service_name]).output()?;

  if !output.status.success() {
    let error = String::from_utf8_lossy(&output.stderr);
    return Err(format!("启动服务失败: {}", error).into());
  }

  println!("Windows 服务 '{}' 启动成功", service_name);
  Ok(())
}

/// 停止 Windows 服务（通过 sc 命令）
#[cfg(windows)]
pub fn stop_service(service_name: &str) -> Result<(), Box<dyn std::error::Error>> {
  use std::process::Command;

  let output = Command::new("sc").args(&["stop", service_name]).output()?;

  if !output.status.success() {
    let error = String::from_utf8_lossy(&output.stderr);
    return Err(format!("停止服务失败: {}", error).into());
  }

  println!("Windows 服务 '{}' 停止成功", service_name);
  Ok(())
}
