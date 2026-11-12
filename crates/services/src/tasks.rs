//! Tokio 设备任务模型（报告 06 章）。
//!
//! - AT / 串口：**有界队列 + 单资源执行器 + 优先级 + 每命令超时**；
//! - USB 热切换 / 网络配置：独占锁，冲突明确返回 Busy；
//! - 状态轮询：`interval` + `MissedTickBehavior::Skip`，避免堆积重入；
//! - 所有任务都有取消/关闭信号，禁止无限重试与无界 task。

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot, OwnedSemaphorePermit, Semaphore};
use udx_domain::error::{AppError, AppResult};
use udx_domain::state::AtPriority;
use udx_platform_linux::DbusClient;

/// 每个优先级独立有界通道，总容量 = 4 × cap（默认 8 → 32，见报告 6.2）。
pub const AT_PRIORITY_LEVELS: usize = 4;

pub struct AtJob {
    pub command: String,
    pub timeout: Duration,
    pub reply: oneshot::Sender<AppResult<String>>,
}

pub struct AtChannels {
    pub critical: mpsc::Receiver<AtJob>,
    pub sms: mpsc::Receiver<AtJob>,
    pub interactive: mpsc::Receiver<AtJob>,
    pub background: mpsc::Receiver<AtJob>,
}

#[derive(Clone)]
pub struct AtQueue {
    critical: mpsc::Sender<AtJob>,
    sms: mpsc::Sender<AtJob>,
    interactive: mpsc::Sender<AtJob>,
    background: mpsc::Sender<AtJob>,
}

impl AtQueue {
    /// 返回队列句柄与接收端；调用方负责启动一个（且仅一个）worker。
    pub fn new(capacity_per_priority: usize) -> (Self, AtChannels) {
        let cap = capacity_per_priority.max(1);
        let (critical_tx, critical_rx) = mpsc::channel(cap);
        let (sms_tx, sms_rx) = mpsc::channel(cap);
        let (interactive_tx, interactive_rx) = mpsc::channel(cap);
        let (background_tx, background_rx) = mpsc::channel(cap);
        (
            Self {
                critical: critical_tx,
                sms: sms_tx,
                interactive: interactive_tx,
                background: background_tx,
            },
            AtChannels {
                critical: critical_rx,
                sms: sms_rx,
                interactive: interactive_rx,
                background: background_rx,
            },
        )
    }

    fn sender(&self, priority: AtPriority) -> &mpsc::Sender<AtJob> {
        match priority {
            AtPriority::Critical => &self.critical,
            AtPriority::Sms => &self.sms,
            AtPriority::Interactive => &self.interactive,
            AtPriority::Background => &self.background,
        }
    }

    /// 提交 AT 命令。队列满时明确返回 Busy，不排队堆积、不无限重试。
    pub async fn submit(
        &self,
        command: impl Into<String>,
        priority: AtPriority,
        timeout: Duration,
    ) -> AppResult<String> {
        let (reply, waiter) = oneshot::channel();
        let job = AtJob {
            command: command.into(),
            timeout,
            reply,
        };
        self.sender(priority)
            .try_send(job)
            .map_err(|_| AppError::Busy("AT 命令队列已满".to_string()))?;
        waiter
            .await
            .map_err(|_| AppError::ServiceUnavailable("AT 执行器已停止".to_string()))?
    }
}

/// 设备侧 AT 后端。
///
/// 报告明确要求「不能把 C 函数机械翻译成 async Rust」：真正的串口 AT 通道
/// 属于硬件适配边界，需在真机上按 UDX710 的 tty/驱动完成适配与兼容性测试。
/// 这里提供两个可替换后端：oFono D-Bus 代理与「设备不可用」降级。
pub enum AtBackend {
    Ofono(DbusClient),
    Unavailable(String),
}

impl AtBackend {
    pub async fn execute(&self, command: &str) -> AppResult<String> {
        match self {
            Self::Ofono(dbus) => {
                let paths = dbus.ofono_modem_paths().await?;
                let path = paths.first().cloned().unwrap_or_else(|| "/".to_string());
                let props = dbus.ofono_properties(&path, "org.ofono.Modem").await?;
                serde_json::to_string(&serde_json::json!({
                    "command": command,
                    "modem_path": path,
                    "properties": props,
                }))
                .map_err(|e| AppError::Internal(format!("AT 响应序列化失败: {e}")))
            }
            Self::Unavailable(msg) => Err(AppError::DeviceUnavailable(msg.clone())),
        }
    }
}

/// 单资源执行器：一次只处理一条 AT 命令，保证串口不交错。
/// `biased` 让高优先级队列优先；短信期间低优先级轮询自然被压制。
pub async fn at_worker(mut channels: AtChannels, backend: Arc<AtBackend>) {
    loop {
        let job = tokio::select! {
            biased;
            Some(j) = channels.critical.recv() => j,
            Some(j) = channels.sms.recv() => j,
            Some(j) = channels.interactive.recv() => j,
            Some(j) = channels.background.recv() => j,
            else => break,
        };

        let result = match tokio::time::timeout(job.timeout, backend.execute(&job.command)).await {
            Ok(Ok(text)) => Ok(text),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(AppError::Timeout(format!(
                "AT 命令执行超过 {:?}",
                job.timeout
            ))),
        };
        let _ = job.reply.send(result);
    }
}

/// 独占锁：USB 热切换、网络配置域等不可重入动作使用。
#[derive(Clone)]
pub struct ExclusiveLock(Arc<Semaphore>);

impl Default for ExclusiveLock {
    fn default() -> Self {
        Self::new()
    }
}

impl ExclusiveLock {
    pub fn new() -> Self {
        Self(Arc::new(Semaphore::new(1)))
    }

    pub fn try_lock(&self) -> AppResult<OwnedSemaphorePermit> {
        self.0
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::Busy("设备资源正被占用，请稍后重试".to_string()))
    }
}

/// 有界后台任务计数（健康检查用）。用 Arc 持有计数，避免裸指针与 unsafe。
#[derive(Clone)]
pub struct TaskRegistry {
    active: Arc<std::sync::atomic::AtomicUsize>,
    max: usize,
}

impl Default for TaskRegistry {
    fn default() -> Self {
        Self::new(64)
    }
}

impl TaskRegistry {
    pub fn new(max: usize) -> Self {
        Self {
            active: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            max,
        }
    }

    pub fn try_enter(&self) -> AppResult<TaskGuard> {
        use std::sync::atomic::Ordering;
        let mut current = self.active.load(Ordering::Relaxed);
        loop {
            if current >= self.max {
                return Err(AppError::Busy("后台任务数已达上限".to_string()));
            }
            match self.active.compare_exchange_weak(
                current,
                current + 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    return Ok(TaskGuard {
                        counter: self.active.clone(),
                    })
                }
                Err(v) => current = v,
            }
        }
    }

    pub fn active(&self) -> usize {
        self.active.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// 作用域守卫，Drop 时递减计数。
pub struct TaskGuard {
    counter: Arc<std::sync::atomic::AtomicUsize>,
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.counter
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// 固定周期轮询：跳过堆积 tick，防止「上一轮未结束、下一轮又开始」。
pub async fn poll_loop<F, Fut>(
    period: Duration,
    mut shutdown: broadcast::Receiver<()>,
    task: F,
) where
    F: Fn() -> Fut,
    Fut: Future<Output = ()>,
{
    let mut ticker = tokio::time::interval(period);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = ticker.tick() => task().await,
            _ = shutdown.recv() => break,
        }
    }
}
