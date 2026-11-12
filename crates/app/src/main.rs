//! UDX710-TOOLS 启动入口。
//!
//! - 端口优先级：命令行参数 > `UDX710_PORT`（历史别名 `UDX_PORT`）> `config.toml` > 默认 6677；
//! - Tokio runtime 使用配置中的 `worker_threads` 上限（报告 5.3：
//!   不未经测量强制单线程，也不默认放任多线程无限增长）；
//! - D-Bus / oFono 不可用时以降级模式启动，行为与旧版一致；
//! - 优雅关闭：SIGINT / SIGTERM 触发广播信号，后台轮询任务随之退出。

use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use udx_api::{build_router, AppState};
use udx_services::{AppConfig, AppServices};

fn main() -> Result<()> {
    let config = load_config()?;
    let workers = config.limits.worker_threads.max(1);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()
        .context("构建 Tokio runtime 失败")?;
    runtime.block_on(run(config))
}

async fn run(mut config: AppConfig) -> Result<()> {
    init_tracing();
    apply_port_overrides(&mut config);

    let version = env!("CARGO_PKG_VERSION");
    tracing::info!(target: "startup", "UDX710-TOOLS v{version} 启动中");

    let services = AppServices::new(config)
        .await
        .context("初始化服务失败")?;

    if services.config.shell.enabled {
        tracing::warn!(
            target: "security",
            "Web 任意 Shell 按用户要求保留：仅限管理员诊断，所有调用将被审计；\
             可通过 config.toml [shell] enabled=false 关闭"
        );
    } else {
        tracing::info!(target: "security", "Web 任意 Shell 已在配置中禁用");
    }

    let port = services.config.port;
    let state = AppState::new(services.clone());
    let app = build_router(state);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("无法监听 {addr}"))?;
    tracing::info!(target: "startup", "HTTP 服务已监听 http://{addr}");

    let shutdown_services = services.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move { shutdown_signal(shutdown_services).await })
    .await
    .context("HTTP 服务异常退出")?;

    services.shutdown();
    tracing::info!(target: "startup", "服务已优雅关闭");
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,udx_api=info,udx_services=info,audit=info"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_target(true))
        .try_init();
}

fn load_config() -> Result<AppConfig> {
    let path = std::env::var("UDX_CONFIG").unwrap_or_else(|_| "config.toml".to_string());
    match std::fs::read_to_string(&path) {
        Ok(text) => match AppConfig::from_toml(&text) {
            Ok(cfg) => {
                tracing::info!(target: "startup", "已加载配置 {path}");
                Ok(cfg)
            }
            Err(e) => Err(anyhow::anyhow!("配置文件 {path} 解析失败: {e}")),
        },
        Err(_) => Ok(AppConfig::default()),
    }
}

fn apply_port_overrides(config: &mut AppConfig) {
    // 兼容旧版 `./server 80`
    if let Some(arg) = std::env::args().nth(1) {
        if let Ok(p) = arg.parse::<u16>() {
            config.port = p;
            return;
        }
    }
    // packaging/start.sh 与 config.example.toml 声明的权威来源是 UDX710_PORT；
    // 保留 UDX_PORT 作为历史别名，避免旧部署脚本静默失效。
    for key in ["UDX710_PORT", "UDX_PORT"] {
        if let Ok(v) = std::env::var(key) {
            if let Ok(p) = v.parse::<u16>() {
                config.port = p;
                return;
            }
        }
    }
}

async fn shutdown_signal(services: Arc<AppServices>) {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = signal(SignalKind::terminate()).expect("注册 SIGTERM 失败");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    services.shutdown();
}
