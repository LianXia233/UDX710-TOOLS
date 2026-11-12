//! 应用配置。
//!
//! 优先级：命令行端口 > 环境变量 > 配置文件 > 默认值。
//! 配置只影响策略（超时、限额、开关），绝不把外部输入拼进命令或 SQL。

use serde::{Deserialize, Serialize};
use udx_platform_linux::ShellPolicy;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub port: u16,
    pub database: String,
    /// 静态资源目录。release 构建使用 rust-embed 内嵌，此目录仅用于开发模式。
    pub web_dir: String,
    pub auth: AuthConfig,
    pub shell: ShellConfig,
    pub ota: OtaConfig,
    pub limits: LimitsConfig,
    pub dbus: DbusConfig,
    /// 前后端看门狗：独立 OS 线程监测 Tokio runtime 心跳，假死时交由 supervisor 重启。
    pub watchdog: WatchdogConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: 6677,
            database: "6677.db".to_string(),
            web_dir: "web".to_string(),
            auth: AuthConfig::default(),
            shell: ShellConfig::default(),
            ota: OtaConfig::default(),
            limits: LimitsConfig::default(),
            dbus: DbusConfig::default(),
            watchdog: WatchdogConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn from_toml(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn shell_policy(&self) -> ShellPolicy {
        self.shell.policy()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AuthConfig {
    /// 兼容旧版默认密码；首次登录后应立即修改。
    pub default_password: String,
    pub token_ttl_secs: i64,
    pub login_window_secs: i64,
    pub login_max_failures: u32,
    pub min_password_len: usize,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            default_password: "admin".to_string(),
            token_ttl_secs: 24 * 3600,
            login_window_secs: 300,
            login_max_failures: 5,
            min_password_len: 6,
        }
    }
}

/// Web 任意 Shell 的策略（用户明确要求保留，默认启用）。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ShellConfig {
    pub enabled: bool,
    pub max_command_len: usize,
    pub timeout_secs: u64,
    pub max_output_bytes: usize,
    pub max_concurrent: usize,
    pub session_timeout_secs: u64,
    pub session_output_cap: usize,
    pub audit_command: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        let base = ShellPolicy::default();
        Self {
            enabled: base.enabled,
            max_command_len: base.max_command_len,
            timeout_secs: base.timeout.as_secs(),
            max_output_bytes: base.max_output_bytes,
            max_concurrent: base.max_concurrent,
            session_timeout_secs: base.session_timeout.as_secs(),
            session_output_cap: base.session_output_cap,
            audit_command: base.audit_command,
        }
    }
}

impl ShellConfig {
    pub fn policy(&self) -> ShellPolicy {
        ShellPolicy {
            enabled: self.enabled,
            max_command_len: self.max_command_len,
            timeout: std::time::Duration::from_secs(self.timeout_secs),
            max_output_bytes: self.max_output_bytes,
            max_concurrent: self.max_concurrent,
            session_timeout: std::time::Duration::from_secs(self.session_timeout_secs),
            session_output_cap: self.session_output_cap,
            audit_command: self.audit_command,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct OtaConfig {
    /// 恢复 TLS 证书验证（报告 08 章：禁止 -k / --no-check-certificate）。
    pub verify_tls: bool,
    /// 允许下载更新包的主机白名单，空表示不限制（仍需 TLS 与摘要校验）。
    pub allowed_hosts: Vec<String>,
    pub max_package_bytes: usize,
    pub require_signature: bool,
    pub download_timeout_secs: u64,
}

impl Default for OtaConfig {
    fn default() -> Self {
        Self {
            verify_tls: true,
            allowed_hosts: Vec::new(),
            max_package_bytes: 64 * 1024 * 1024,
            require_signature: true,
            download_timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LimitsConfig {
    pub request_timeout_secs: u64,
    pub max_body_bytes: usize,
    pub rate_limit_per_minute: u32,
    pub poll_interval_secs: u64,
    pub at_queue_capacity: usize,
    pub worker_threads: usize,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            request_timeout_secs: 15,
            max_body_bytes: 2 * 1024 * 1024,
            rate_limit_per_minute: 120,
            poll_interval_secs: 5,
            at_queue_capacity: 32,
            // 报告 5.3：不未经测量强制单线程，通过 worker_threads 上限控制资源。
            worker_threads: 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DbusConfig {
    pub call_timeout_secs: u64,
    pub reconnect_backoff_secs: u64,
    pub max_reconnect_attempts: u32,
}

impl Default for DbusConfig {
    fn default() -> Self {
        Self {
            call_timeout_secs: 3,
            reconnect_backoff_secs: 5,
            max_reconnect_attempts: 3,
        }
    }
}

/// 看门狗配置： feeder 在 Tokio runtime 内喂狗，monitor 是独立 OS 线程。
/// runtime 真卡死时 feeder 一并停摆，monitor 才能检出——这正是设计意图。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct WatchdogConfig {
    pub enabled: bool,
    pub heartbeat_interval_secs: u64,
    /// 超过该时长未收到心跳即判定 runtime 假死。
    pub stall_after_secs: u64,
    pub check_interval_secs: u64,
    /// "exit"：以非 0 退出码结束进程，由 systemd / procd respawn 拉起；
    /// "log"：只记录告警与 /api/health 状态，不结束进程（无 supervisor 时用）。
    pub action: String,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            heartbeat_interval_secs: 2,
            stall_after_secs: 30,
            check_interval_secs: 1,
            action: "exit".to_string(),
        }
    }
}

impl WatchdogConfig {
    pub fn should_exit(&self) -> bool {
        self.action.eq_ignore_ascii_case("exit")
    }
}
