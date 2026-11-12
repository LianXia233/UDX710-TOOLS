//! 业务服务层（报告 03 章 Application / Domain 边界）。
//!
//! Handler 不直接拼接 Shell 字符串、不直接操作 sysfs、不直接写 SQL；
//! 所有设备动作都经过这里的有类型服务接口，并带权限、超时、状态机与审计。

pub mod auth;
pub mod config;
pub mod crypto;
pub mod modem;
pub mod network;
pub mod ota;
pub mod plugin;
pub mod proxy;
pub mod sms;
pub mod state;
pub mod tasks;
pub mod usb;
pub mod watchdog;

pub use auth::AuthService;
pub use config::{AppConfig, AuthConfig, LimitsConfig, OtaConfig, ShellConfig, WatchdogConfig};
pub use modem::ModemService;
pub use network::NetworkService;
pub use ota::OtaService;
pub use plugin::PluginService;
pub use proxy::ProxyService;
pub use sms::{validate_webhook_url, SmsService};
pub use state::AppServices;
pub use tasks::{AtBackend, AtQueue, ExclusiveLock, TaskRegistry};
pub use usb::UsbService;
