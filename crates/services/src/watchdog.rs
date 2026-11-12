//! 后端看门狗：防止服务「死掉或假死」。
//!
//! 结构：
//!  - `feeder`：Tokio runtime 内的周期任务，按固定间隔写入心跳时间戳；
//!  - `monitor`：**独立 OS 线程**，不依赖 Tokio 调度。若 runtime 卡死，feeder 也停摆，
//!    monitor 在 `stall_after_secs` 后检出并以非 0 退出码结束进程；
//!  - 进程退出由 systemd `Restart=on-failure` / OpenWrt procd `respawn` 重新拉起，
//!    这就是「自愈」闭环。前端另有 `web/js/core/watchdog.js` 负责 UI 假死与后端失联。
//!
//! 为什么不走优雅关闭：runtime 已假死时 `shutdown` 广播、`axum::graceful_shutdown`
//! 都无法被调度执行，唯一可靠路径是让进程退出、交给外部 supervisor 重启。
//!
//! 状态通过 `/api/health` 的 `watchdog` 字段暴露（不写日志、不含任何凭证）。

use crate::config::WatchdogConfig;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 退出码：借用 EX_TEMPFAIL(75) 语义，明确表达「临时故障，请 supervisor 重试」。
pub const EXIT_RESTART: i32 = 75;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone, Serialize)]
pub struct WatchdogStatus {
    pub enabled: bool,
    pub stalled: bool,
    pub tripped: bool,
    pub stopped: bool,
    pub action: String,
    pub age_ms: u64,
    pub stall_after_ms: u64,
    pub heartbeats: u64,
    pub monitor: &'static str,
    pub exit_code: i32,
}

pub struct WatchdogState {
    last_beat_ms: AtomicU64,
    heartbeats: AtomicU64,
    tripped: AtomicBool,
    stopped: AtomicBool,
    enabled: bool,
    stall_ms: u64,
    action: String,
    exit_on_stall: bool,
}

impl WatchdogState {
    pub fn new(cfg: &WatchdogConfig) -> Arc<Self> {
        let stall_ms = cfg.stall_after_secs.saturating_mul(1000).max(1000);
        Arc::new(Self {
            // 启动时先喂一次，保证有完整的 stall_ms 宽限期
            last_beat_ms: AtomicU64::new(now_ms()),
            heartbeats: AtomicU64::new(1),
            tripped: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            enabled: cfg.enabled,
            stall_ms,
            action: cfg.action.clone(),
            exit_on_stall: cfg.should_exit(),
        })
    }

    pub fn beat(&self) {
        self.beat_at(now_ms());
    }

    /// 可注入时间的喂狗，供单测确定性地验证超时判定（避免真实时钟漂移）。
    pub fn beat_at(&self, now: u64) {
        self.last_beat_ms.store(now, Ordering::Relaxed);
        self.heartbeats.fetch_add(1, Ordering::Relaxed);
    }

    pub fn age_ms_at(&self, now: u64) -> u64 {
        now.saturating_sub(self.last_beat_ms.load(Ordering::Relaxed))
    }

    /// 纯逻辑判定，便于单测注入时间而不用真等 30 秒。
    pub fn is_stalled_at(&self, now: u64) -> bool {
        self.enabled && self.age_ms_at(now) > self.stall_ms
    }

    /// 只触发一次，避免 monitor 反复写日志 / 反复 exit。
    pub fn trip(&self) -> bool {
        self.tripped
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    pub fn should_exit(&self) -> bool {
        self.exit_on_stall
    }

    /// 服务优雅关闭时调用，monitor 线程随即退出，避免「正常停机被误判为假死」。
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }

    pub fn stall_ms(&self) -> u64 {
        self.stall_ms
    }

    pub fn status_at(&self, now: u64) -> WatchdogStatus {
        let age = self.age_ms_at(now);
        WatchdogStatus {
            enabled: self.enabled,
            stalled: self.is_stalled_at(now) && !self.is_stopped(),
            tripped: self.tripped.load(Ordering::SeqCst),
            stopped: self.is_stopped(),
            action: self.action.clone(),
            age_ms: age,
            stall_after_ms: self.stall_ms,
            heartbeats: self.heartbeats.load(Ordering::Relaxed),
            monitor: "os-thread",
            exit_code: EXIT_RESTART,
        }
    }

    pub fn status(&self) -> WatchdogStatus {
        self.status_at(now_ms())
    }
}

/// 在 Tokio runtime 内周期喂狗；收到关闭广播后退出，不阻止进程优雅退出。
pub fn spawn_feeder(
    state: Arc<WatchdogState>,
    period: Duration,
    mut shutdown: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        state.beat();
        let mut ticker = tokio::time::interval(period.max(Duration::from_millis(200)));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => state.beat(),
                _ = shutdown.recv() => break,
            }
        }
    })
}

/// 独立 OS 线程，不进入 Tokio 调度器。
///
/// 刻意持有 `Weak`：当 `AppServices` 被释放（例如集成测试结束、runtime 销毁）时，
/// monitor 无法 upgrade 便自行退出，绝不会在测试进程里因「心跳停止」误杀进程。
pub fn spawn_monitor(
    state: Weak<WatchdogState>,
    check_interval: Duration,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("udx-watchdog".to_string())
        .spawn(move || {
            let period = check_interval.max(Duration::from_millis(200));
            let mut announced = false;
            loop {
                std::thread::sleep(period);
                let Some(state) = state.upgrade() else {
                    // 服务对象已释放：正常退出，不视为假死
                    break;
                };
                if state.is_stopped() || !state.enabled {
                    break;
                }
                if !announced {
                    announced = true;
                    tracing::info!(
                        target: "watchdog",
                        "后端看门狗已启动：心跳超时 {} ms，动作={}",
                        state.stall_ms(),
                        if state.should_exit() { "exit+respawn" } else { "log-only" }
                    );
                }
                let now = now_ms();
                if !state.is_stalled_at(now) {
                    continue;
                }
                if !state.trip() {
                    continue;
                }
                let age = state.age_ms_at(now);
                tracing::error!(
                    target: "watchdog",
                    "Tokio runtime 已 {} ms 无心跳（阈值 {} ms），判定为假死",
                    age,
                    state.stall_ms()
                );
                if state.should_exit() {
                    // 假死时无法优雅关闭；以非 0 退出码交给 systemd / procd 重启。
                    std::process::exit(EXIT_RESTART);
                }
                tracing::warn!(
                    target: "watchdog",
                    "action=log：仅告警不退出；请配置外部 supervisor 并将 action 改为 exit 以启用自愈"
                );
            }
        })
        .expect("启动看门狗监控线程失败")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(action: &str) -> WatchdogConfig {
        WatchdogConfig {
            enabled: true,
            stall_after_secs: 10,
            heartbeat_interval_secs: 1,
            check_interval_secs: 1,
            action: action.to_string(),
        }
    }

    #[test]
    fn fresh_state_is_not_stalled_and_has_grace() {
        let s = WatchdogState::new(&cfg("exit"));
        let now = now_ms();
        assert!(!s.is_stalled_at(now));
        assert_eq!(s.status_at(now).heartbeats, 1);
        assert_eq!(s.stall_ms(), 10_000);
        assert!(s.should_exit());
    }

    #[test]
    fn stall_is_detected_only_after_threshold_and_beat_resets_it() {
        let s = WatchdogState::new(&cfg("exit"));
        let t0 = now_ms();
        assert!(!s.is_stalled_at(t0 + 9_999));
        assert!(s.is_stalled_at(t0 + 10_001));
        s.beat_at(t0 + 10_001);
        assert!(!s.is_stalled_at(t0 + 10_001));
        assert!(!s.is_stalled_at(t0 + 20_001)); // 恰好等于阈值，不算超时
        assert!(s.is_stalled_at(t0 + 20_002));
        assert_eq!(s.status_at(t0 + 20_002).heartbeats, 2);
    }

    #[test]
    fn disabled_watchdog_never_stalls() {
        let mut c = cfg("exit");
        c.enabled = false;
        let s = WatchdogState::new(&c);
        assert!(!s.is_stalled_at(now_ms() + 600_000));
        assert!(!s.status().stalled);
    }

    #[test]
    fn trip_fires_only_once() {
        let s = WatchdogState::new(&cfg("log"));
        assert!(s.trip());
        assert!(!s.trip());
        assert!(s.status().tripped);
        assert!(!s.should_exit());
    }

    #[test]
    fn status_exposes_monitor_and_exit_code() {
        let s = WatchdogState::new(&cfg("exit"));
        let st = s.status();
        assert!(st.enabled);
        assert_eq!(st.monitor, "os-thread");
        assert_eq!(st.exit_code, EXIT_RESTART);
        assert_eq!(st.action, "exit");
        assert_eq!(st.stall_after_ms, 10_000);
    }
}
