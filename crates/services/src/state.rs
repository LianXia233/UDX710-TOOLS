//! 应用装配：把所有服务组合成 `AppServices`，供 API 层以 `State` 注入。
//!
//! - D-Bus 连接失败不阻止启动（旧版降级行为兼容）；
//! - 只启动一个 AT worker（单资源执行器）；
//! - 周期性维护任务使用 `poll_loop`（跳过堆积 tick），并响应关闭信号；
//! - 所有后台任务都受 `TaskRegistry` 上限约束。

use crate::auth::AuthService;
use crate::config::AppConfig;
use crate::crypto::new_id;
use crate::modem::ModemService;
use crate::network::NetworkService;
use crate::ota::OtaService;
use crate::plugin::PluginService;
use crate::proxy::ProxyService;
use crate::sms::SmsService;
use crate::tasks::{poll_loop, AtBackend, AtQueue, TaskRegistry, at_worker};
use crate::usb::UsbService;
use crate::watchdog::{WatchdogState, spawn_feeder, spawn_monitor};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use udx_domain::error::AppResult;
use udx_domain::models::{AuditEntry, AuditSink, HealthStatus};
use udx_domain::permissions::Principal;
use udx_domain::state::ActionOutcome;
use udx_platform_linux::{DbusClient, ShellExecutor};
use udx_storage::{now_ts, Db};

/// 实时指标快照。
///
/// `/api/ws` 原本每个连接每 2 秒自己去读 /proc、写一次 SQLite、再打好几次
/// D-Bus —— 多开一个页面就翻一倍负载，写库还会与其它请求抢锁，表现为
/// 推送变慢、超时、连接被浏览器判死。改为「单个后台任务采集 + 所有连接
/// 读同一份内存快照」，开销与客户端数量无关。
#[derive(Clone, Default)]
pub struct MetricsSnapshot {
    pub cpu_usage: f64,
    pub mem_used: u64,
    pub mem_total: u64,
    pub temperature: Option<f64>,
    pub traffic_total: u64,
    pub traffic_rate: f64,
    pub rsrp: Option<i64>,
    pub updated_ts: i64,
}

pub struct AppServices {
    pub config: AppConfig,
    pub db: Db,
    pub auth: Arc<AuthService>,
    pub shell: Arc<ShellExecutor>,
    pub modem: Arc<ModemService>,
    pub sms: Arc<SmsService>,
    pub network: Arc<NetworkService>,
    pub usb: Arc<UsbService>,
    pub ota: Arc<OtaService>,
    pub plugin: Arc<PluginService>,
    pub proxy: Arc<ProxyService>,
    pub audit: Arc<dyn AuditSink>,
    pub tasks: TaskRegistry,
    pub dbus_ok: Arc<AtomicBool>,
    pub started_at: Instant,
    pub shutdown: broadcast::Sender<()>,
    /// 看门狗状态：feeder 在 runtime 内喂狗，独立 OS 线程监测假死。
    pub watchdog: Arc<WatchdogState>,
    /// 由后台任务周期性刷新，供 WS 推送与页面复用。
    pub metrics: Arc<RwLock<MetricsSnapshot>>,
}

impl AppServices {
    pub async fn new(config: AppConfig) -> AppResult<Arc<Self>> {
        let db = Db::open(&config.database)?;
        db.health().await?;
        let audit: Arc<dyn AuditSink> = Arc::new(db.clone());

        let auth = Arc::new(AuthService::new(db.clone(), config.auth.clone()));
        auth.ensure_default_admin().await?;

        let dbus = match DbusClient::connect(Duration::from_secs(config.dbus.call_timeout_secs)).await
        {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(target: "dbus", "D-Bus 不可用，相关功能降级: {}", e.public_message());
                None
            }
        };
        let dbus_ok = Arc::new(AtomicBool::new(dbus.is_some()));

        let (at, channels) = AtQueue::new((config.limits.at_queue_capacity / 4).max(1));
        let at = Arc::new(at);
        let backend = Arc::new(match &dbus {
            Some(c) => AtBackend::Ofono(c.clone()),
            None => AtBackend::Unavailable(
                "未检测到 oFono D-Bus 服务；AT 与 Modem 功能需在真机上验证".to_string(),
            ),
        });
        let (shutdown, _) = broadcast::channel(1);
        tokio::spawn(at_worker(channels, backend));

        // 看门狗：feeder 在 runtime 内喂狗；monitor 是独立 OS 线程，仅持 Weak，
        // AppServices 释放后自行退出，不会在测试 / 正常停机时误判假死。
        let watchdog = WatchdogState::new(&config.watchdog);
        let _wd_feeder = spawn_feeder(
            watchdog.clone(),
            Duration::from_secs(config.watchdog.heartbeat_interval_secs.max(1)),
            shutdown.subscribe(),
        );
        let _wd_monitor = spawn_monitor(
            std::sync::Arc::downgrade(&watchdog),
            Duration::from_secs(config.watchdog.check_interval_secs.max(1)),
        );

        let shell = Arc::new(ShellExecutor::new(config.shell_policy(), audit.clone()));
        let modem = Arc::new(ModemService::new(db.clone(), dbus, at.clone()));
        let sms = Arc::new(SmsService::new(db.clone(), at.clone()));
        let network = Arc::new(NetworkService::new(db.clone()));
        let usb = Arc::new(UsbService::new(db.clone()));
        let ota = Arc::new(OtaService::new(
            db.clone(),
            config.ota.clone(),
            "packages",
        ));
        let plugin = Arc::new(PluginService::new(db.clone()));
        let proxy = Arc::new(ProxyService::new(db.clone()));

        // 实时指标采集：只跑一个后台任务，所有 WS 连接读同一份内存快照。
        // 每个连接各自采集时，一次页面刷新就可能叠加出多倍的 /proc 读取、
        // SQLite 写入与 D-Bus 调用，造成推送延迟甚至连接被判死。
        let metrics = Arc::new(RwLock::new(MetricsSnapshot::default()));
        {
            let snap = metrics.clone();
            let net = network.clone();
            let modem = modem.clone();
            let mut sd = shutdown.subscribe();
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(3));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut last_total: u64 = 0;
                let mut last_at = Instant::now();
                loop {
                    tokio::select! {
                        _ = ticker.tick() => {}
                        _ = sd.recv() => break,
                    }
                    let mut next = MetricsSnapshot::default();
                    if let Ok(info) = net.system_info(env!("CARGO_PKG_VERSION")).await {
                        next.cpu_usage = info
                            .get("cpu_usage")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        next.mem_used = info.get("mem_used").and_then(|v| v.as_u64()).unwrap_or(0);
                        next.mem_total =
                            info.get("mem_total").and_then(|v| v.as_u64()).unwrap_or(0);
                        next.temperature = info.get("temperature").and_then(|v| v.as_f64());
                    }
                    if let Ok(t) = net.traffic_total().await {
                        let total = t.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                        let now = Instant::now();
                        let dt = now.duration_since(last_at).as_secs_f64().max(0.001);
                        // total 回落说明设备重启，速率归零重新计。
                        next.traffic_rate = if last_total == 0 || total < last_total {
                            0.0
                        } else {
                            (total - last_total) as f64 / dt
                        };
                        next.traffic_total = total;
                        last_total = total;
                        last_at = now;
                    }
                    if let Ok(b) = modem.current_band().await {
                        next.rsrp = b.get("rsrp").and_then(|v| v.as_i64());
                    }
                    next.updated_ts = now_ts();
                    *snap.write().await = next;
                }
            });
        }

        // 周期性维护：清理过期 Token；跳过堆积 tick，避免重入。
        let db_maint = db.clone();
        let maintenance = move || {
            let db = db_maint.clone();
            async move {
                let _ = db.token_purge(now_ts()).await;
            }
        };
        tokio::spawn(poll_loop(
            Duration::from_secs(300),
            shutdown.subscribe(),
            maintenance,
        ));

        Ok(Arc::new(Self {
            config,
            db,
            auth,
            shell,
            modem,
            sms,
            network,
            usb,
            ota,
            plugin,
            proxy,
            audit,
            tasks: TaskRegistry::new(64),
            dbus_ok,
            started_at: Instant::now(),
            shutdown,
            watchdog,
            metrics,
        }))
    }

    /// 健康检查。
    ///
    /// `dbus_ok` 原本只表示「D-Bus 客户端对象创建成功」，与 oFono 是否可用、
    /// 是否托管了 Modem 完全无关，导致面板在模组无数据时仍报 `ok`。
    /// 这里改为三级判定，并用实时探测结果刷新缓存供 WS 推送复用。
    pub async fn health(&self) -> AppResult<HealthStatus> {
        let dbus_ok = self.dbus_ok.load(Ordering::Relaxed);
        let ofono_ok = self.modem.dbus_available().await;
        let modem_ready = ofono_ok && self.modem.modem_count().await > 0;
        // 缓存 oFono 可用性，供 /api/ws 的低成本推送复用。
        self.dbus_ok.store(ofono_ok, Ordering::Relaxed);

        let status = if !dbus_ok {
            "degraded"
        } else if !ofono_ok {
            "degraded"
        } else if !modem_ready {
            "degraded"
        } else {
            "ok"
        };

        Ok(HealthStatus {
            status,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.started_at.elapsed().as_secs(),
            db_ok: self.db.health().await.is_ok(),
            dbus_ok,
            ofono_ok,
            modem_ready,
            active_tasks: self.tasks.active(),
        })
    }

    /// 统一设备动作审计。敏感字段由调用方保证不传入。
    pub fn record(
        &self,
        principal: &Principal,
        action: &str,
        permission: &str,
        outcome: ActionOutcome,
        detail: impl Into<String>,
        duration_ms: u64,
    ) {
        self.audit.record(AuditEntry {
            id: new_id(),
            at: now_ts(),
            actor: principal.subject.clone(),
            action: action.to_string(),
            permission: permission.to_string(),
            outcome,
            detail: detail.into(),
            duration_ms,
        });
    }

    pub fn record_system(&self, action: &str, permission: &str, detail: impl Into<String>) {
        self.audit.record(AuditEntry {
            id: new_id(),
            at: now_ts(),
            actor: "system".to_string(),
            action: action.to_string(),
            permission: permission.to_string(),
            outcome: ActionOutcome::Success,
            detail: detail.into(),
            duration_ms: 0,
        });
    }

    pub fn shutdown(&self) {
        // 先通知 monitor 停止，再广播让 feeder / 轮询任务退出。
        self.watchdog.stop();
        let _ = self.shutdown.send(());
    }

    /// 供插件渲染器读取的受限数据上下文（只读，不含敏感字段）。
    pub async fn plugin_context(&self) -> AppResult<Value> {
        Ok(serde_json::json!({
            "modem": self.modem.status().await?,
            "device": {
                "version": env!("CARGO_PKG_VERSION"),
                "dbus_ok": self.dbus_ok.load(Ordering::Relaxed),
            }
        }))
    }
}
