//! OpsBox 主程序入口

use clap::Parser;
use mimalloc::MiMalloc;

// 模块声明
mod config;
mod daemon;
mod logging;
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

    // 初始化日志系统
    logging::init(&config);
    logging::init_network_env();

    log::info!("OpsBox 启动中...");
    log::debug!("配置: {:?}", config);

    // 获取监听地址
    let addr = config.get_addr().expect("无效的监听地址");

    // 处理守护进程模式
    handle_daemon_mode(&config);

    // 初始化数据库
    let db_url = config.get_database_url();
    log::info!("数据库路径: {}", db_url);

    // 配置 logseek 模块性能参数
    configure_logseek_module(&config);

    // 创建 Tokio 运行时并启动服务器
    let worker_threads = config.get_worker_threads();
    log::info!("Tokio 工作线程数: {}", worker_threads);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .expect("创建 Tokio 运行时失败");

    rt.block_on(async_main(addr, db_url));
}

/// 异步主逻辑
async fn async_main(addr: std::net::SocketAddr, db_url: String) {
    // 初始化数据库连接池
    let db_config = opsbox_core::DatabaseConfig {
        url: db_url,
        max_connections: 10,
        connect_timeout: 30,
    };

    let db_pool = opsbox_core::init_pool(&db_config)
        .await
        .expect("数据库连接池初始化失败");

    log::info!("数据库连接池初始化成功");

    // 初始化各模块的数据库 schema
    logseek::init_schema(&db_pool)
        .await
        .expect("LogSeek 模块数据库初始化失败");

    log::info!("模块数据库初始化完成");

    // 启动 HTTP 服务器
    server::run(addr, db_pool).await;
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

/// 处理守护进程模式
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
        // 守护进程启动成功后，日志需要重新初始化
        logging::init(config);
    }

    #[cfg(not(unix))]
    {
        eprintln!("当前操作系统不支持内置后台运行");
        std::process::exit(2);
    }
}

/// 配置 LogSeek 模块参数
fn configure_logseek_module(config: &AppConfig) {
    let tuning = logseek::tuning::Tuning {
        s3_max_concurrency: config.get_s3_max_concurrency(),
        cpu_concurrency: config.get_cpu_concurrency(),
        stream_ch_cap: config.get_stream_ch_cap(),
        minio_timeout_sec: config.get_minio_timeout_sec(),
        minio_max_attempts: config.get_minio_max_attempts(),
    };

    log::debug!("LogSeek 模块配置: {:?}", tuning);
    logseek::tuning::set(tuning);
}
