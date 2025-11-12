//! OpsBox 主程序入口

use clap::Parser;
use mimalloc::MiMalloc;

// ⚠️ 重要：必须显式引用可选依赖，否则 inventory 机制在 release 模式下不生效
// 原因：Rust linker 会移除未被引用的 crate，导致 inventory::submit! 不被执行
#[cfg(feature = "logseek")]
extern crate logseek;

#[cfg(feature = "agent-manager")]
extern crate agent_manager;

// 模块声明
mod config;
mod daemon;
mod logging;
mod network;
mod server;

#[cfg(windows)]
mod daemon_windows;

use config::{AppConfig, Commands};

#[cfg(windows)]
const SERVICE_NAME: &str = "OpsBoxServer";

#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
static SERVICE_CONFIG: OnceLock<AppConfig> = OnceLock::new();

// 全局内存分配器：mimalloc
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
  // 解析命令行参数
  let config = AppConfig::parse();

  // 处理 Windows 服务相关命令（优先处理）
  #[cfg(windows)]
  {
    if config.install_service {
      handle_install_service(&config);
      return;
    }
    if config.uninstall_service {
      handle_uninstall_service(&config);
      return;
    }
    if config.start_service {
      handle_start_service(&config);
      return;
    }
    if config.stop_service {
      handle_stop_service(&config);
      return;
    }
    if config.service_mode {
      run_as_windows_service(config);
      return;
    }
  }

  // 处理 stop 子命令（优先处理）
  if let Some(Commands::Stop { pid_file, force }) = &config.cmd {
    handle_stop_command(pid_file, *force);
    return;
  }

  // 处理守护进程模式（在日志初始化之前，避免重复初始化）
  handle_daemon_mode(&config);

  // 初始化日志系统
  logging::init(&config);

  // 初始化网络环境
  network::init_network_env();

  log::info!("OpsBox 启动中...");
  log::debug!("配置: {:?}", config);

  // 获取监听地址
  let addr = config.get_addr().expect("无效的监听地址");

  // 初始化数据库
  let db_url = config.get_database_url();
  log::info!("数据库路径: {}", db_url);

  // 设置模块配置环境变量（模块将从环境变量读取配置）
  setup_module_env_vars(&config);

  // 创建 Tokio 运行时并启动服务器（使用 Tokio 默认工作线程）
  let rt = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .expect("创建 Tokio 运行时失败");

  rt.block_on(async_main(addr, db_url));
}

/// 异步主逻辑
async fn async_main(addr: std::net::SocketAddr, db_url: String) {
  // 初始化数据库连接池
  let db_config = opsbox_core::DatabaseConfig::new(db_url, 10, 30);

  let db_pool = opsbox_core::init_pool(&db_config)
    .await
    .expect("数据库连接池初始化失败");

  log::info!("数据库连接池初始化成功");

  // ✅ 自动发现所有已注册的模块
  let modules = opsbox_core::get_all_modules();
  log::info!("发现 {} 个模块", modules.len());

  // 配置各模块（从环境变量读取配置）
  for module in &modules {
    log::info!("配置模块: {}", module.name());
    module.configure();
  }

  // 初始化各模块的数据库 schema
  for module in &modules {
    log::info!("初始化模块数据库: {}", module.name());
    module.init_schema(&db_pool).await.unwrap_or_else(|e| {
      log::error!("模块 {} 数据库初始化失败: {}", module.name(), e);
      std::process::exit(1);
    });
  }

  log::info!("所有模块初始化完成");

  // 启动 HTTP 服务器
  server::run(addr, db_pool, modules).await;
}

/// 处理 stop 命令
fn handle_stop_command(pid_file: &Option<std::path::PathBuf>, force: bool) {
  #[cfg(unix)]
  {
    let path = daemon::resolve_pid_path(pid_file);
    match daemon::stop_daemon(path.clone(), force) {
      Ok(()) => {
        let signal = if force { "SIGKILL" } else { "SIGTERM" };
        eprintln!("已发送 {}，服务停止流程已触发", signal);
      }
      Err(e) => {
        eprintln!("停止失败：{}", e);
        std::process::exit(2);
      }
    }
  }
  #[cfg(all(not(unix), not(windows)))]
  {
    let _ = (pid_file, force); // 避免未使用变量警告
    eprintln!("当前操作系统不支持内置 stop 命令");
    std::process::exit(2);
  }
  #[cfg(windows)]
  {
    let _ = (pid_file, force); // 避免未使用变量警告
    eprintln!("在 Windows 上，请使用 --stop-service 或 sc stop 命令停止服务");
    std::process::exit(2);
  }
}

/// 处理守护进程模式（在日志初始化之前调用）
fn handle_daemon_mode(config: &AppConfig) {
  #[cfg(unix)]
  let mut need_daemon = config.daemon;
  #[cfg(not(unix))]
  let need_daemon = config.daemon;

  #[cfg(unix)]
  let mut pid_file = None;

  #[cfg(unix)]
  if let Some(Commands::Start { daemon, pid_file: pf }) = &config.cmd {
    need_daemon = *daemon;
    pid_file = pf.clone();
  }

  if !need_daemon {
    return;
  }

  #[cfg(unix)]
  {
    let pid_path = daemon::resolve_pid_path(&pid_file);

    if let Err(e) = daemon::start_daemon(pid_path) {
      eprintln!("后台运行失败：{}", e);
      std::process::exit(1);
    }

    eprintln!("守护进程已启动");
  }

  #[cfg(not(unix))]
  {
    eprintln!("当前操作系统不支持内置后台运行");
    std::process::exit(2);
  }
}

/// 设置模块配置环境变量
///
/// 将命令行参数转换为环境变量，供各模块在 configure() 中读取
fn setup_module_env_vars(config: &AppConfig) {
  unsafe {
    // LogSeek 模块配置（仅保留 S3 相关参数）
    std::env::set_var(
      "LOGSEEK_S3_MAX_CONCURRENCY",
      config.get_s3_max_concurrency().to_string(),
    );
    std::env::set_var("LOGSEEK_S3_TIMEOUT_SEC", config.get_s3_timeout_sec().to_string());
    std::env::set_var("LOGSEEK_S3_MAX_RETRIES", config.get_s3_max_retries().to_string());
  }

  log::debug!("模块配置环境变量已设置");
}

/// Windows 服务相关处理函数
#[cfg(windows)]
fn handle_install_service(_config: &AppConfig) {
  use daemon_windows::install_service;
  use std::env;

  let service_name = SERVICE_NAME;
  let display_name = "OpsBox Server";

  // 获取当前可执行文件路径
  let exe_path = env::current_exe()
    .expect("无法获取当前可执行文件路径")
    .to_string_lossy()
    .to_string();

  if let Err(e) = install_service(service_name, display_name, &exe_path) {
    eprintln!("安装 Windows 服务失败: {}", e);
    std::process::exit(1);
  }

  println!("Windows 服务安装成功！");
  println!("使用以下命令管理服务：");
  println!("  启动服务: sc start {}", service_name);
  println!("  停止服务: sc stop {}", service_name);
  println!("  查看状态: sc query {}", service_name);
}

#[cfg(windows)]
fn handle_uninstall_service(_config: &AppConfig) {
  use daemon_windows::uninstall_service;

  let service_name = SERVICE_NAME;

  if let Err(e) = uninstall_service(service_name) {
    eprintln!("卸载 Windows 服务失败: {}", e);
    std::process::exit(1);
  }
}

#[cfg(windows)]
fn handle_start_service(_config: &AppConfig) {
  use daemon_windows::start_service;

  let service_name = SERVICE_NAME;

  if let Err(e) = start_service(service_name) {
    eprintln!("启动 Windows 服务失败: {}", e);
    std::process::exit(1);
  }
}

#[cfg(windows)]
fn handle_stop_service(_config: &AppConfig) {
  use daemon_windows::stop_service;

  let service_name = SERVICE_NAME;

  if let Err(e) = stop_service(service_name) {
    eprintln!("停止 Windows 服务失败: {}", e);
    std::process::exit(1);
  }
}

/// 以 Windows 服务模式运行
#[cfg(windows)]
fn run_as_windows_service(config: AppConfig) {
  use daemon_windows::run_as_service;
  use windows_service::define_windows_service;
  use windows_service::service_dispatcher;

  // 将配置存入全局 OnceLock，供服务主入口读取
  let _ = SERVICE_CONFIG.set(config.clone());

  // 生成符合 SCM 要求的 FFI 入口，并委托到本地 service_main
  define_windows_service!(ffi_service_main, service_main);

  fn service_main(_: Vec<std::ffi::OsString>) {
    // 从全局取出配置
    let cfg = SERVICE_CONFIG.get().expect("服务配置未初始化").clone();

    if let Err(e) = run_as_service(SERVICE_NAME, move |shutdown| {
      // 初始化日志系统
      logging::init(&cfg);

      // 初始化网络环境
      network::init_network_env();

      log::info!("OpsBox Windows 服务启动中...");
      log::debug!("配置: {:?}", cfg);

      // 获取监听地址
      let addr = cfg.get_addr().expect("无效的监听地址");

      // 初始化数据库
      let db_url = cfg.get_database_url();
      log::info!("数据库路径: {}", db_url);

      // 设置模块配置环境变量
      setup_module_env_vars(&cfg);

      // 创建 Tokio 运行时
      let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("创建 Tokio 运行时失败");

      // 在运行时中执行异步主逻辑
      let shutdown_clone = shutdown.clone();
      rt.block_on(async {
        // 监听关闭信号
        tokio::spawn(async move {
          shutdown_clone.notified().await;
          log::info!("收到停止信号，开始优雅关闭...");
        });

        async_main(addr, db_url).await;
      });

      Ok(())
    }) {
      eprintln!("Windows 服务运行失败: {}", e);
    }
  }

  // 通过服务调度器启动，确保在 SCM 上下文中运行
  if let Err(e) = service_dispatcher::start(SERVICE_NAME, ffi_service_main) {
    eprintln!("启动 Windows 服务调度器失败: {}", e);
    std::process::exit(1);
  }
}
