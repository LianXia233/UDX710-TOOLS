//! 认证主体与细粒度权限（报告 08 章「安全默认值」）。
//!
//! 权限名与插件协议 `permissions.json` 使用同一套点分字符串，
//! 便于「安装时声明 + 调用时校验」双重检查。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 角色。旧版只有单一管理员密码；重构后默认账号仍为 Admin，
/// 因此既有客户端行为不变，但可按需新增只读/操作账号。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Operator,
    Viewer,
}

impl Role {
    pub fn parse(s: &str) -> Self {
        match s {
            "operator" => Self::Operator,
            "viewer" => Self::Viewer,
            _ => Self::Admin,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::Viewer => "viewer",
        }
    }
}

/// 全部权限常量。新增权限必须同时登记到 `ALL`，否则视为未知权限并被拒绝。
pub mod perms {
    // ---- 只读 ----
    pub const DEVICE_STATUS_READ: &str = "device.status.read";
    pub const MODEM_INFO_READ: &str = "modem.info.read";
    pub const MODEM_AT_READ: &str = "modem.at.read";
    pub const SMS_READ: &str = "sms.read";
    pub const NETWORK_READ: &str = "network.read";
    pub const TRAFFIC_READ: &str = "traffic.read";
    pub const SYSTEM_READ: &str = "system.read";
    pub const PLUGIN_READ: &str = "plugin.read";
    pub const PLUGIN_STORAGE_READ: &str = "plugin.storage.read";
    pub const SCRIPT_READ: &str = "script.read";
    pub const CONFIG_READ: &str = "config.read";
    pub const LOG_READ: &str = "log.read";
    pub const UPDATE_READ: &str = "update.read";
    pub const SECURITY_READ: &str = "security.read";

    // ---- 常规写 ----
    pub const DEVICE_CONTROL: &str = "device.control";
    pub const MODEM_AT_WRITE: &str = "modem.at.write";
    pub const MODEM_CONFIG_WRITE: &str = "modem.config.write";
    pub const SMS_SEND: &str = "sms.send";
    pub const SMS_DELETE: &str = "sms.delete";
    pub const SMS_CONFIG_WRITE: &str = "sms.config.write";
    pub const NETWORK_WRITE: &str = "network.write";
    pub const APN_WRITE: &str = "apn.write";
    pub const USB_SWITCH: &str = "usb.switch";
    pub const BAND_WRITE: &str = "band.write";
    pub const CELL_WRITE: &str = "cell.write";
    pub const CHARGE_WRITE: &str = "charge.write";
    pub const TRAFFIC_WRITE: &str = "traffic.write";
    pub const SYSTEM_TIME_WRITE: &str = "system.time.write";
    pub const REBOOT_SCHEDULE_WRITE: &str = "reboot.schedule.write";
    pub const PLUGIN_WRITE: &str = "plugin.write";
    pub const PLUGIN_STORAGE_WRITE: &str = "plugin.storage.write";
    pub const SCRIPT_WRITE: &str = "script.write";
    pub const CONFIG_WRITE: &str = "config.write";
    pub const WEBHOOK_WRITE: &str = "webhook.write";
    pub const IPV6_PROXY_WRITE: &str = "ipv6_proxy.write";
    pub const RATHOLE_WRITE: &str = "rathole.write";
    pub const SECURITY_WRITE: &str = "security.write";

    // ---- 高风险（默认仅 Admin）----
    pub const SHELL_EXECUTE: &str = "shell.execute";
    pub const TERMINAL_SESSION: &str = "terminal.session";
    pub const FACTORY_RESET: &str = "factory.reset";
    pub const OTA_INSTALL: &str = "ota.install";

    pub const ALL: &[&str] = &[
        DEVICE_STATUS_READ, MODEM_INFO_READ, MODEM_AT_READ, SMS_READ,
        NETWORK_READ, TRAFFIC_READ, SYSTEM_READ, PLUGIN_READ,
        PLUGIN_STORAGE_READ, SCRIPT_READ, CONFIG_READ, LOG_READ,
        UPDATE_READ, SECURITY_READ, DEVICE_CONTROL, MODEM_AT_WRITE,
        MODEM_CONFIG_WRITE, SMS_SEND, SMS_DELETE, SMS_CONFIG_WRITE,
        NETWORK_WRITE, APN_WRITE, USB_SWITCH, BAND_WRITE, CELL_WRITE,
        CHARGE_WRITE, TRAFFIC_WRITE, SYSTEM_TIME_WRITE,
        REBOOT_SCHEDULE_WRITE, PLUGIN_WRITE, PLUGIN_STORAGE_WRITE,
        SCRIPT_WRITE, CONFIG_WRITE, WEBHOOK_WRITE, IPV6_PROXY_WRITE,
        RATHOLE_WRITE, SECURITY_WRITE, SHELL_EXECUTE, TERMINAL_SESSION,
        FACTORY_RESET, OTA_INSTALL,
    ];

    /// 高风险权限：即便 Operator 也不默认授予，必须 Admin 且显式配置。
    pub const HIGH_RISK: &[&str] = &[
        SHELL_EXECUTE, TERMINAL_SESSION, FACTORY_RESET, OTA_INSTALL,
        DEVICE_CONTROL,
    ];

    pub fn is_known(s: &str) -> bool {
        ALL.contains(&s)
    }

    pub fn is_device_write(s: &str) -> bool {
        matches!(
            s,
            DEVICE_CONTROL
                | MODEM_AT_WRITE
                | MODEM_CONFIG_WRITE
                | SMS_SEND
                | SMS_DELETE
                | NETWORK_WRITE
                | APN_WRITE
                | USB_SWITCH
                | BAND_WRITE
                | CELL_WRITE
                | CHARGE_WRITE
                | IPV6_PROXY_WRITE
                | RATHOLE_WRITE
                | SHELL_EXECUTE
                | TERMINAL_SESSION
                | FACTORY_RESET
                | OTA_INSTALL
        )
    }

    /// 角色默认权限。Admin 拥有全部；Operator 不含高风险项；Viewer 只读。
    pub fn for_role(role: super::Role) -> Vec<&'static str> {
        match role {
            super::Role::Admin => ALL.to_vec(),
            super::Role::Operator => ALL
                .iter()
                .copied()
                .filter(|p| !HIGH_RISK.contains(p))
                .collect(),
            super::Role::Viewer => ALL
                .iter()
                .copied()
                .filter(|p| p.ends_with(".read"))
                .collect(),
        }
    }
}

/// 已认证请求主体。由 Token 校验后注入请求扩展，Handler 不得自行伪造。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub subject: String,
    pub role: Role,
    pub token_id: String,
    pub expires_at: i64,
    #[serde(default)]
    pub permissions: HashSet<String>,
}

impl Principal {
    pub fn new(subject: impl Into<String>, role: Role, token_id: impl Into<String>, expires_at: i64) -> Self {
        let permissions = perms::for_role(role).into_iter().map(String::from).collect();
        Self {
            subject: subject.into(),
            role,
            token_id: token_id.into(),
            expires_at,
            permissions,
        }
    }

    pub fn can(&self, permission: &str) -> bool {
        perms::is_known(permission) && self.permissions.contains(permission)
    }

    pub fn is_expired(&self, now: i64) -> bool {
        self.expires_at <= now
    }
}

/// 把 HTTP 方法与路径映射到所需权限。
/// 返回 `None` 表示该写路径没有专属权限（仍要求认证）。
pub fn permission_for(method: &str, path: &str) -> Option<&'static str> {
    let m = method.to_ascii_uppercase();
    let write = matches!(m.as_str(), "POST" | "PUT" | "DELETE" | "PATCH");
    if !write {
        return None;
    }

    // 高风险精确匹配优先
    if path == "/api/device_control" {
        return Some(perms::DEVICE_CONTROL);
    }
    if path == "/api/shell" {
        return Some(perms::SHELL_EXECUTE);
    }
    if path.starts_with("/api/terminal") {
        return Some(perms::TERMINAL_SESSION);
    }
    if path.ends_with("/factory-reset") || path == "/api/factory-reset" {
        return Some(perms::FACTORY_RESET);
    }
    if path == "/api/update/install" || path == "/api/update/extract" {
        return Some(perms::OTA_INSTALL);
    }
    if path.starts_with("/api/auth/password") {
        return Some(perms::SECURITY_WRITE);
    }

    if path.starts_with("/api/sms/send") {
        return Some(perms::SMS_SEND);
    }
    if path.starts_with("/api/sms/") {
        return Some(if m == "DELETE" { perms::SMS_DELETE } else { perms::SMS_CONFIG_WRITE });
    }
    if path == "/api/at" {
        return Some(perms::MODEM_AT_WRITE);
    }
    if path.starts_with("/api/usb") {
        return Some(perms::USB_SWITCH);
    }
    if path.starts_with("/api/apn") {
        return Some(perms::APN_WRITE);
    }
    if path.contains("lock_bands") || path.starts_with("/api/bands") {
        return Some(perms::BAND_WRITE);
    }
    if path.contains("cell") {
        return Some(perms::CELL_WRITE);
    }
    if path.starts_with("/api/charge") {
        return Some(perms::CHARGE_WRITE);
    }
    if path.starts_with("/api/get/set") || path.starts_with("/api/set/total") {
        return Some(perms::TRAFFIC_WRITE);
    }
    if path.starts_with("/api/plugins/storage") {
        return Some(perms::PLUGIN_STORAGE_WRITE);
    }
    if path.starts_with("/api/plugins") {
        return Some(perms::PLUGIN_WRITE);
    }
    if path.starts_with("/api/scripts") {
        return Some(perms::SCRIPT_WRITE);
    }
    if path.starts_with("/api/security") {
        return Some(perms::SECURITY_WRITE);
    }
    if path.starts_with("/api/ipv6-proxy") {
        return Some(perms::IPV6_PROXY_WRITE);
    }
    if path.starts_with("/api/rathole") {
        return Some(perms::RATHOLE_WRITE);
    }
    if path.starts_with("/api/netif") || path.starts_with("/api/data")
        || path.starts_with("/api/roaming") || path == "/api/set_network"
        || path == "/api/switch" || path == "/api/airplane_mode"
    {
        return Some(perms::NETWORK_WRITE);
    }

    Some(perms::CONFIG_WRITE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_has_shell_but_operator_does_not() {
        let admin = Principal::new("admin", Role::Admin, "t1", i64::MAX);
        let operator = Principal::new("op", Role::Operator, "t2", i64::MAX);
        assert!(admin.can(perms::SHELL_EXECUTE));
        assert!(!operator.can(perms::SHELL_EXECUTE));
        assert!(operator.can(perms::NETWORK_WRITE));
    }

    #[test]
    fn unknown_permission_is_always_denied() {
        let admin = Principal::new("admin", Role::Admin, "t1", i64::MAX);
        assert!(!admin.can("plugin.arbitrary.js"));
    }

    #[test]
    fn device_control_requires_explicit_permission() {
        assert_eq!(
            permission_for("POST", "/api/device_control"),
            Some(perms::DEVICE_CONTROL)
        );
        assert_eq!(
            permission_for("POST", "/api/shell"),
            Some(perms::SHELL_EXECUTE)
        );
    }
}
