//! Linux / UDX710 平台适配层。
//!
//! 分层职责（报告 03 章 Infrastructure）：
//! 封装 D-Bus、系统进程、sysfs、网络接口与路径安全，向上层只暴露有类型的接口。
//!
//! 两条命令通道严格分离：
//! - `command`：**设备内部动作**，命令 ID 白名单 + 固定 argv，绝不拼接用户输入；
//! - `shell`：**Web 任意 Shell**（用户要求保留），独立高风险管理员通道，
//!   带认证、权限、并发、超时、输出上限与审计约束。

pub mod command;
pub mod dbus;
pub mod netif;
pub mod path;
pub mod shell;
pub mod sysfs;

pub use command::{validate_arg, CommandOutput, CommandRunner, FixedCommand, DEFAULT_MAX_OUTPUT};
pub use dbus::{DbusClient, OFONO_SERVICE};
pub use netif::{interface_stats, ipv6_map, list_interfaces, system_info};
pub use path::{atomic_write, resolve_under, safe_component, MAX_COMPONENT_LEN};
pub use shell::{
    redact_sensitive, InteractiveShell, ShellExecutor, ShellPolicy, ShellReader, ShellWriter,
    DEFAULT_MAX_COMMAND_LEN, DEFAULT_SESSION_OUTPUT_CAP,
};
pub use sysfs::{is_allowed_absolute, Sysfs, ALLOWED_ROOTS};
