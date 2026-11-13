//! 领域模型。
//!
//! 兼容性策略：旧版 Handler 返回的 JSON 字段各异，模型一律使用
//! `#[serde(default)]` + `#[serde(flatten)] extra`，确保新增字段不破坏老客户端，
//! 未知字段也不会在读写往返中丢失。

use crate::state::{ActionOutcome, TaskState, UsbMode, UsbSwitchPhase};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// 成功响应的通用外壳；旧端点仍可原样返回自己的 JSON。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEnvelope<T> {
    pub code: u16,
    pub data: T,
}

impl<T> ApiEnvelope<T> {
    pub fn ok(data: T) -> Self {
        Self { code: 0, data }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: &'static str,
    pub version: String,
    pub uptime_secs: u64,
    pub db_ok: bool,
    /// 系统总线是否可连（只反映传输层，不代表 oFono 可用）。
    pub dbus_ok: bool,
    /// oFono 是否已在总线上注册 `org.ofono`。
    pub ofono_ok: bool,
    /// oFono 是否真的托管了 Modem 对象。为 false 时所有模组数据都不可用，
    /// 前端据此区分「设备没数据」与「面板坏了」。
    pub modem_ready: bool,
    pub active_tasks: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemInfo {
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub uptime_secs: u64,
    #[serde(default)]
    pub cpu_usage: f64,
    #[serde(default)]
    pub mem_total_kb: u64,
    #[serde(default)]
    pub mem_used_kb: u64,
    #[serde(default)]
    pub temperature_c: Option<f64>,
    #[serde(default)]
    pub kernel: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModemStatus {
    #[serde(default)]
    pub present: bool,
    #[serde(default)]
    pub powered: bool,
    #[serde(default)]
    pub online: bool,
    #[serde(default)]
    pub imei: String,
    #[serde(default)]
    pub iccid: String,
    #[serde(default)]
    pub operator: String,
    #[serde(default)]
    pub sim_slot: u8,
    #[serde(default)]
    pub network_type: String,
    #[serde(default)]
    pub band: String,
    #[serde(default)]
    pub arfcn: String,
    #[serde(default)]
    pub pci: String,
    #[serde(default)]
    pub rsrp: Option<i32>,
    #[serde(default)]
    pub rsrq: Option<i32>,
    #[serde(default)]
    pub sinr: Option<i32>,
    #[serde(default)]
    pub signal_strength: Option<i32>,
    #[serde(default)]
    pub modem_path: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BandInfo {
    #[serde(default)]
    pub bands: Vec<String>,
    #[serde(default)]
    pub locked: Vec<String>,
    #[serde(default)]
    pub current: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CellInfo {
    #[serde(default)]
    pub cells: Vec<Value>,
    #[serde(default)]
    pub locked: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SmsMessage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub sender: String,
    #[serde(default)]
    pub recipient: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub direction: String,
    #[serde(default)]
    pub status: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SmsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_delete: bool,
    #[serde(default)]
    pub sca: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default)]
    pub template: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrafficStats {
    #[serde(default)]
    pub total_rx: u64,
    #[serde(default)]
    pub total_tx: u64,
    #[serde(default)]
    pub limit_enabled: bool,
    #[serde(default)]
    pub limit_bytes: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetifInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub rx_bytes: u64,
    #[serde(default)]
    pub tx_bytes: u64,
    #[serde(default)]
    pub monitored: bool,
    /// 硬件地址，读 `/sys/class/net/<name>/address`；不可读时为空串。
    #[serde(default)]
    pub mac: String,
    /// IPv4 地址，来自 `getifaddrs`；无地址时为空串。
    #[serde(default)]
    pub ipv4: String,
    /// 由 `/sys/class/net/<name>/flags` 的 IFF_UP 位判定，比 operstate 更准
    ///（点对点接口常为 unknown 但实际可用）。
    #[serde(default)]
    pub up: bool,
    #[serde(default)]
    pub mtu: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChargeStatus {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub start_threshold: u8,
    #[serde(default)]
    pub stop_threshold: u8,
    #[serde(default)]
    pub battery_level: Option<u8>,
    #[serde(default)]
    pub charging: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApnConfig {
    #[serde(default)]
    pub apn: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub auth_type: String,
    #[serde(default)]
    pub ip_type: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApnTemplate {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub config: ApnConfig,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsbModeStatus {
    #[serde(default)]
    pub mode: UsbMode,
    #[serde(default)]
    pub permanent: bool,
    #[serde(default)]
    pub phase: UsbSwitchPhase,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RebootSchedule {
    #[serde(default)]
    pub days: Vec<String>,
    #[serde(default)]
    pub hour: u8,
    #[serde(default)]
    pub minute: u8,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RatholeService {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ipv6Rule {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityStatus {
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub locked_until: i64,
    #[serde(default)]
    pub failed_attempts: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateInfo {
    #[serde(default)]
    pub current_version: String,
    #[serde(default)]
    pub available_version: String,
    #[serde(default)]
    pub update_available: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_error: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdatePackage {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub changelog: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptRecord {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigRecord {
    pub key: String,
    pub value: Value,
    pub updated_at: i64,
}

// ---------------------------------------------------------------------------
// 插件协议（报告 07 章）：声明 / 界面 / 权限 / 数据四部分，无任意 JS 执行。
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub min_api_version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginOption {
    pub label: String,
    pub value: String,
}

/// 宿主渲染器可解释的节点类型集合，白名单之外一律拒绝。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginViewNode {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub bind: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub children: Vec<PluginViewNode>,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub options: Vec<PluginOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginUiSpec {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub view: PluginViewNode,
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginPermissionDoc {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub max_storage_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub enabled: bool,
    pub installed_at: i64,
    #[serde(default)]
    pub ui: Option<PluginUiSpec>,
    #[serde(default)]
    pub permissions: HashSet<String>,
}

pub const SUPPORTED_VIEW_TYPES: &[&str] = &[
    "column", "row", "text", "status", "badge", "button", "table", "input",
    "select", "divider",
];

/// 宿主注册动作白名单：插件 action 只能引用这些名字。
pub const SUPPORTED_PLUGIN_ACTIONS: &[&str] = &[
    "device.status.refresh",
    "modem.info.refresh",
    "sms.list.refresh",
    "traffic.stats.refresh",
    "network.status.refresh",
    "config.save",
    "toast.show",
];

// ---------------------------------------------------------------------------
// Web Shell（用户明确要求保留）与审计
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShellResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub at: i64,
    pub actor: String,
    pub action: String,
    pub permission: String,
    pub outcome: ActionOutcome,
    pub detail: String,
    pub duration_ms: u64,
}

/// 审计落盘接口。实现方必须异步落库，不得在请求路径上同步阻塞。
pub trait AuditSink: Send + Sync {
    fn record(&self, entry: AuditEntry);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub token_hash: String,
    pub token_id: String,
    pub subject: String,
    pub role: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub revoked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub subject: String,
    pub password_salt: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub name: String,
    pub state: TaskState,
    pub started_at: i64,
    pub last_error: Option<String>,
}

impl Default for TaskState {
    fn default() -> Self {
        TaskState::Pending
    }
}
