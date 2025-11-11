//! Windows 服务相关功能（Agent）

#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use windows_service::{
  service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType},
  service_control_handler::{self, ServiceControlHandlerResult},
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
  main_fn: impl FnOnce(std::sync::Arc<tokio::sync::Notify>) -> Result<(), Box<dyn std::error::Error>> + Send + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
  // 在服务启动的早期阶段初始化基本日志（使用 stderr，因为服务可能没有控制台）
  // 这样即使后续初始化失败，也能看到错误信息
  let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
    .target(env_logger::Target::Stderr)
    .try_init();

  let service_name = OsString::from(service_name);
  let shutdown_notify = std::sync::Arc::new(tokio::sync::Notify::new());

  log::info!("开始注册 Windows 服务控制处理器...");

  // 注册服务控制处理器
  // 注意：这只能在服务已经被服务控制管理器启动时调用
  let status_handle = match service_control_handler::register(service_name.as_os_str(), service_control_handler) {
    Ok(handle) => {
      log::info!("服务控制处理器注册成功");
      handle
    }
    Err(e) => {
      let error_msg = format!(
        "注册服务控制处理器失败: {}. 请确保服务已正确安装并通过 'sc start' 启动，而不是直接运行 --service-mode",
        e
      );
      log::error!("{}", error_msg);
      eprintln!("{}", error_msg);
      return Err(error_msg.into());
    }
  };

  log::info!("设置服务状态为启动中...");

  // 先设置为启动中状态
  status_handle.set_service_status(ServiceStatus {
    service_type: ServiceType::OWN_PROCESS,
    current_state: ServiceState::StartPending,
    controls_accepted: ServiceControlAccept::STOP,
    exit_code: ServiceExitCode::Win32(0),
    checkpoint: 1,
    wait_hint: std::time::Duration::from_secs(30),
    process_id: None,
  })?;

  log::info!("启动主逻辑线程...");

  // 使用 Arc 来共享状态，以便主线程可以检查启动是否成功
  let startup_result = std::sync::Arc::new(std::sync::Mutex::new(None::<Box<dyn std::error::Error + Send + Sync>>));
  let startup_result_clone = startup_result.clone();
  let shutdown = shutdown_notify.clone();
  let status_handle_clone = status_handle; // ServiceStatusHandle 实现了 Copy

  // 启动主逻辑（在新线程中运行 Tokio 运行时）
  let main_thread = std::thread::spawn(move || {
    log::info!("主逻辑线程已启动，开始执行初始化...");
    if let Err(e) = main_fn(shutdown) {
      let error_msg = format!("服务运行错误: {}", e);
      log::error!("{}", error_msg);
      eprintln!("{}", error_msg);
      *startup_result_clone.lock().unwrap() = Some(format!("{}", e).into());
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
    } else {
      log::info!("主逻辑正常退出");
    }
  });

  // 等待主线程启动（最多等待 30 秒）
  // 如果主线程还在运行且没有错误，就认为启动成功
  log::info!("等待主逻辑初始化完成...");
  let start_time = std::time::Instant::now();
  let timeout = std::time::Duration::from_secs(30);
  let min_wait_time = std::time::Duration::from_millis(500); // 缩短最小等待时间，尽快报告 Running
  let mut checkpoint = 1;
  let last_status_update = std::sync::Arc::new(std::sync::Mutex::new(std::time::Instant::now()));

  loop {
    // 检查是否有错误
    if let Some(err) = startup_result.lock().unwrap().take() {
      log::error!("主逻辑初始化失败: {}", err);
      // 设置服务状态为已停止（错误）
      status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(1),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
      })?;
      return Err(err);
    }

    // 检查主线程是否还在运行
    if main_thread.is_finished() {
      // 再次检查是否有错误（可能在检查 is_finished 之后才设置）
      if let Some(err) = startup_result.lock().unwrap().take() {
        log::error!("主逻辑线程退出，错误: {}", err);
        status_handle.set_service_status(ServiceStatus {
          service_type: ServiceType::OWN_PROCESS,
          current_state: ServiceState::Stopped,
          controls_accepted: ServiceControlAccept::empty(),
          exit_code: ServiceExitCode::Win32(1),
          checkpoint: 0,
          wait_hint: std::time::Duration::default(),
          process_id: None,
        })?;
        return Err(err);
      }
      log::error!("主逻辑线程意外退出（无错误信息）");
      status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(1),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
      })?;
      return Err("主逻辑线程意外退出".into());
    }

    // 定期更新服务状态，告诉 Windows 服务还在启动中（每 2 秒更新一次）
    let elapsed = start_time.elapsed();
    let should_update_status = {
      let mut last_update = last_status_update.lock().unwrap();
      if last_update.elapsed() >= std::time::Duration::from_secs(2) {
        *last_update = std::time::Instant::now();
        true
      } else {
        false
      }
    };

    if should_update_status && elapsed < timeout {
      checkpoint += 1;
      let remaining = timeout.as_secs().saturating_sub(elapsed.as_secs());
      let _ = status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint,
        wait_hint: std::time::Duration::from_secs(remaining.max(1)),
        process_id: None,
      });
    }

    // 如果主线程还在运行，且已经等待了足够的时间，认为启动成功
    if elapsed >= min_wait_time && !main_thread.is_finished() {
      log::info!("主逻辑初始化成功（线程运行中），设置服务状态为运行中");
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
      log::info!("Windows 服务已成功启动并运行");
      break;
    }

    if elapsed > timeout {
      log::error!("主逻辑初始化超时（{} 秒）", timeout.as_secs());
      // 设置服务状态为已停止（超时）
      status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(1),
        checkpoint: 0,
        wait_hint: std::time::Duration::default(),
        process_id: None,
      })?;
      return Err("服务启动超时".into());
    }

    std::thread::sleep(std::time::Duration::from_millis(100));
  }

  // 等待停止信号
  log::info!("服务运行中，等待停止信号...");
  while !SERVICE_STOPPING.load(Ordering::SeqCst) {
    std::thread::sleep(std::time::Duration::from_millis(100));
  }

  log::info!("收到停止信号，开始停止 Windows 服务...");
  shutdown_notify.notify_waiters();

  // 等待主线程完成（最多等待 10 秒）
  let shutdown_timeout = std::time::Duration::from_secs(10);
  let shutdown_start = std::time::Instant::now();
  while shutdown_start.elapsed() < shutdown_timeout && !main_thread.is_finished() {
    std::thread::sleep(std::time::Duration::from_millis(100));
  }

  if !main_thread.is_finished() {
    log::warn!("主线程在 {} 秒内未完成关闭", shutdown_timeout.as_secs());
  }

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

  // 先检查服务是否已存在，如果存在则先删除
  println!("检查服务 '{}' 是否已存在...", service_name);
  let check_output = Command::new("sc").args(&["query", service_name]).output();

  if let Ok(output) = check_output {
    if output.status.success() {
      println!("服务 '{}' 已存在，先尝试停止并删除...", service_name);

      // 尝试停止服务（忽略错误，因为服务可能已经停止）
      let _ = Command::new("sc").args(&["stop", service_name]).output();

      // 等待服务停止（最多等待 5 秒）
      for _ in 0..50 {
        let status_output = Command::new("sc").args(&["query", service_name]).output();

        if let Ok(so) = status_output {
          let status_text = String::from_utf8_lossy(&so.stdout);
          if status_text.contains("STOPPED") || !so.status.success() {
            break;
          }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
      }

      // 删除服务
      let delete_output = Command::new("sc").args(&["delete", service_name]).output()?;

      if !delete_output.status.success() {
        let error_msg = String::from_utf8_lossy(&delete_output.stderr);
        // 如果错误是"指定的服务不存在"或"指定的服务已被标记为删除"，可以继续
        if !error_msg.contains("1060") && !error_msg.contains("1072") {
          println!("警告: 删除旧服务时出现错误: {}", error_msg);
        }
      } else {
        println!("旧服务已删除");
      }

      // 等待服务完全删除（最多等待 3 秒）
      // 错误 1072 表示服务正在被删除，需要等待
      for _ in 0..30 {
        let check_output = Command::new("sc").args(&["query", service_name]).output();

        if let Ok(co) = check_output {
          if !co.status.success() {
            // 服务已不存在，可以继续安装
            break;
          }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
      }
    }
  }

  // binPath 需要用引号括起来，并且需要包含 --service-mode 参数
  let bin_path_with_args = format!("\"{}\" --service-mode", bin_path);

  println!("正在安装服务 '{}'...", service_name);
  let output = Command::new("sc")
    .args(&[
      "create",
      service_name,
      &format!("binPath={}", bin_path_with_args),
      &format!("DisplayName={}", display_name),
      "start=auto",
    ])
    .output()?;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let error_msg = if !stderr.is_empty() {
      stderr.to_string()
    } else {
      stdout.to_string()
    };

    // 如果错误是 1072（服务正在被删除），提供更友好的提示
    if error_msg.contains("1072") {
      return Err(
        format!(
          "安装服务失败: 服务正在被删除中，请等待几秒钟后重试。如果问题持续，请运行 'sc delete {}' 手动删除服务。",
          service_name
        )
        .into(),
      );
    }

    return Err(format!("安装服务失败: {}", error_msg).into());
  }

  // 安装成功后，设置服务描述（非致命失败）
  // 说明：使用 `sc description` 设置服务描述，便于在“服务”管理器中识别
  let description = "OpsBox Agent：运维工具箱远程代理";
  let desc_output = Command::new("sc")
    .args(&["description", service_name, description])
    .output();
  match desc_output {
    Ok(o) if o.status.success() => {
      println!("已设置服务描述");
    }
    Ok(o) => {
      let msg = String::from_utf8_lossy(&o.stderr);
      println!("警告: 设置服务描述失败: {}", msg);
    }
    Err(e) => {
      println!("警告: 设置服务描述时发生错误: {}", e);
    }
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
