//! 任务、动作与设备状态机（报告 06 章与 10 章迁移原则）。
//!
//! 报告要求：任何硬件动作必须有明确「成功 / 失败 / 超时 / 未知」状态，
//! 不能只返回「命令已发送」。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Pending,
    Running,
    Succeeded,
    Failed,
    Timeout,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionOutcome {
    Success,
    Failed,
    Timeout,
    Unknown,
}

/// 设备动作审计记录。敏感字段（密码 / Token / 短信正文 / 完整 AT 参数）不进入这里。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceActionRecord {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub permission: String,
    pub at: i64,
    pub outcome: ActionOutcome,
    pub duration_ms: u64,
    pub detail: String,
}

/// AT 命令优先级（报告 6.2）。
/// 同一 Modem 只允许一个命令占用串口；短信期间暂停低优先级轮询。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtPriority {
    Background = 0,
    Interactive = 1,
    Sms = 2,
    Critical = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbMode {
    Ecm,
    Ncm,
    Rndis,
    Unknown,
}

impl Default for UsbMode {
    fn default() -> Self {
        Self::Unknown
    }
}

impl UsbMode {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "ecm" | "cdc-ecm" | "cdc_ecm" => Self::Ecm,
            "ncm" | "cdc-ncm" | "cdc_ncm" => Self::Ncm,
            "rndis" => Self::Rndis,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ecm => "ecm",
            Self::Ncm => "ncm",
            Self::Rndis => "rndis",
            Self::Unknown => "unknown",
        }
    }
}

/// USB 热切换状态机：切换过程不可并发重入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbSwitchPhase {
    Idle,
    Preparing,
    Applying,
    Verifying,
    RollingBack,
    Settled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbSwitchState {
    pub phase: UsbSwitchPhase,
    pub from: UsbMode,
    pub to: UsbMode,
    pub permanent: bool,
    pub started_at: i64,
    pub last_error: Option<String>,
}

impl Default for UsbSwitchPhase {
    fn default() -> Self {
        Self::Idle
    }
}

impl Default for UsbSwitchState {
    fn default() -> Self {
        Self {
            phase: UsbSwitchPhase::Idle,
            from: UsbMode::Unknown,
            to: UsbMode::Unknown,
            permanent: false,
            started_at: 0,
            last_error: None,
        }
    }
}

/// 网络配置按接口 / 配置域串行化，避免同时修改同一接口导致竞态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkConfigDomain {
    Airplane,
    DataConnection,
    Roaming,
    Apn,
    Netif,
    Bands,
    Cell,
    Ipv6Proxy,
    Rathole,
}
