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

use config::{AppConfig, Commands};

// 全局内存分配器：mimalloc
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
  // 解析命令行参数
  let config = AppConfig::parse();

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
  #[cfg(not(unix))]
  {
    eprintln!("当前操作系统不支持内置 stop 命令");
    std::process::exit(2);
  }
}

/// 处理守护进程模式（在日志初始化之前调用）
fn handle_daemon_mode(config: &AppConfig) {
  let mut need_daemon = config.daemon;
  let mut pid_file = None;

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
