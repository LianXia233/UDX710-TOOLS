//! Rathole 内网穿透与 IPv6 端口转发服务。
//!
//! 配置与规则落在 SQLite（参数化查询），运行态落在 config 表；
//! 日志只保留最近 300 条（报告 4.7 内存管理）。所有「启动/停止」返回
//! 明确状态，不假装设备侧进程已成功运行。

use crate::crypto::new_id;
use serde_json::{json, Value};
use udx_domain::error::{bad_request, AppResult};
use udx_storage::{now_ts, Db, Entity};

#[derive(Clone)]
pub struct ProxyService {
    db: Db,
}

impl ProxyService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    // ---------------- Rathole ----------------

    pub async fn rathole_config(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("rathole.config")
            .await?
            .unwrap_or(json!({"enabled": false, "server": "", "token": ""})))
    }

    pub async fn rathole_config_set(&self, payload: &Value) -> AppResult<Value> {
        self.db.config_set("rathole.config", payload).await?;
        Ok(json!({"status": "ok"}))
    }

    pub async fn rathole_services(&self) -> AppResult<Value> {
        Ok(json!({"services": self.db.entity_list(Entity::RatholeService).await?}))
    }

    pub async fn rathole_service_add(&self, payload: &Value) -> AppResult<Value> {
        let id = new_id();
        self.db
            .entity_upsert(Entity::RatholeService, &id, payload)
            .await?;
        Ok(json!({"status": "ok", "id": id}))
    }

    pub async fn rathole_service_update(&self, id: &str, payload: &Value) -> AppResult<Value> {
        validate_id(id)?;
        self.db
            .entity_upsert(Entity::RatholeService, id, payload)
            .await?;
        Ok(json!({"status": "ok", "id": id}))
    }

    pub async fn rathole_service_delete(&self, id: &str) -> AppResult<Value> {
        validate_id(id)?;
        let n = self.db.entity_delete(Entity::RatholeService, id).await?;
        Ok(json!({"status": "ok", "deleted": n}))
    }

    pub async fn rathole_action(&self, action: &str) -> AppResult<Value> {
        let running = !matches!(action, "stop");
        self.db
            .config_set("rathole.runtime", &json!({"running": running, "since": now_ts()}))
            .await?;
        self.push_log("rathole.logs", action).await?;
        Ok(json!({
            "status": "ok",
            "action": action,
            "outcome": "unknown",
            "device_verified": false,
            "note": "运行态已记录；真机需验证 rathole 进程"
        }))
    }

    pub async fn rathole_status(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("rathole.runtime")
            .await?
            .unwrap_or(json!({"running": false})))
    }

    pub async fn rathole_logs(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("rathole.logs")
            .await?
            .unwrap_or(json!({"logs": []})))
    }

    pub async fn rathole_server_config(&self) -> AppResult<Value> {
        let cfg = self.rathole_config().await?;
        Ok(json!({"server_config": cfg, "note": "服务端配置需按实际拓扑生成"}))
    }

    pub async fn rathole_autostart(&self, payload: &Value) -> AppResult<Value> {
        self.db.config_set("rathole.autostart", payload).await?;
        Ok(json!({"status": "ok"}))
    }

    // ---------------- IPv6 Proxy ----------------

    /// IPv6 推送配置。
    ///
    /// 前端读的是 `send_enabled` / `webhook_url` / `send_interval`，而后端原先
    /// 只给 `enabled` / `webhook`，导致「启用定时推送」开关恒 off、URL 恒空、
    /// 间隔恒 30。这里按前端契约输出，同时保留旧键以兼容既有配置。
    pub async fn ipv6_config(&self) -> AppResult<Value> {
        let raw = self
            .db
            .config_get("ipv6_proxy.config")
            .await?
            .unwrap_or_else(|| json!({}));
        let send_enabled = raw
            .get("send_enabled")
            .or_else(|| raw.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let webhook_url = raw
            .get("webhook_url")
            .or_else(|| raw.get("webhook"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let send_interval = raw
            .get("send_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
            .max(1);
        Ok(json!({
            "send_enabled": send_enabled,
            "webhook_url": webhook_url,
            "send_interval": send_interval,
            // 旧键保留，避免老调用方失效。
            "enabled": send_enabled,
            "webhook": webhook_url,
        }))
    }

    pub async fn ipv6_config_set(&self, payload: &Value) -> AppResult<Value> {
        // 只覆盖前端明确给出的键，不再整份替换配置对象。
        let current = self.ipv6_config().await?;
        let send_enabled = payload
            .get("send_enabled")
            .or_else(|| payload.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| current["send_enabled"].as_bool().unwrap_or(false));
        let webhook_url = payload
            .get("webhook_url")
            .or_else(|| payload.get("webhook"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                current["webhook_url"]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            });
        // Webhook 只允许 http(s)，避免被写成 file:// 或 javascript:。
        if !webhook_url.is_empty()
            && !(webhook_url.starts_with("http://") || webhook_url.starts_with("https://"))
        {
            return Err(bad_request("Webhook 必须是 http(s) 地址"));
        }
        let send_interval = payload
            .get("send_interval")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| {
                current["send_interval"].as_u64().unwrap_or(30)
            })
            .max(1);

        self.db
            .config_set(
                "ipv6_proxy.config",
                &json!({
                    "send_enabled": send_enabled,
                    "webhook_url": webhook_url,
                    "send_interval": send_interval,
                    "enabled": send_enabled,
                    "webhook": webhook_url,
                }),
            )
            .await?;
        Ok(json!({"status": "ok"}))
    }

    pub async fn ipv6_rules(&self) -> AppResult<Value> {
        Ok(json!({"rules": self.db.entity_list(Entity::Ipv6Rule).await?}))
    }

    pub async fn ipv6_rule_add(&self, payload: &Value) -> AppResult<Value> {
        let id = new_id();
        self.db.entity_upsert(Entity::Ipv6Rule, &id, payload).await?;
        Ok(json!({"status": "ok", "id": id}))
    }

    pub async fn ipv6_rule_update(&self, id: &str, payload: &Value) -> AppResult<Value> {
        validate_id(id)?;
        self.db.entity_upsert(Entity::Ipv6Rule, id, payload).await?;
        Ok(json!({"status": "ok", "id": id}))
    }

    pub async fn ipv6_rule_delete(&self, id: &str) -> AppResult<Value> {
        validate_id(id)?;
        let n = self.db.entity_delete(Entity::Ipv6Rule, id).await?;
        Ok(json!({"status": "ok", "deleted": n}))
    }

    pub async fn ipv6_action(&self, action: &str) -> AppResult<Value> {
        let running = !matches!(action, "stop");
        self.db
            .config_set("ipv6_proxy.runtime", &json!({"running": running, "since": now_ts()}))
            .await?;
        self.push_log("ipv6_proxy.send-logs", action).await?;
        Ok(json!({
            "status": "ok",
            "action": action,
            "outcome": "unknown",
            "device_verified": false
        }))
    }

    /// IPv6 转发运行状态。
    ///
    /// 前端「运行状态」卡需要 `running` 与 `ipv6`（本机 IPv6 地址）两项，
    /// 原实现只返回 `running`，地址栏永远是破折号。
    pub async fn ipv6_status(&self) -> AppResult<Value> {
        let cfg = self
            .db
            .config_get("ipv6_proxy.runtime")
            .await?
            .unwrap_or_else(|| json!({}));
        let running = cfg
            .get("running")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let address = read_global_ipv6();
        Ok(json!({
            "running": running,
            "ipv6": address.clone().unwrap_or_default(),
            "address": address,
            "since": cfg.get("since").cloned().unwrap_or(Value::Null),
            // 启停目前只维护状态位，未托管真实转发进程，如实标注，
            // 避免把「已启动」误解为「转发已生效」。
            "implemented": false,
            "note": if running {
                "状态位已置为运行，但当前版本未托管实际转发进程"
            } else {
                "未启动"
            },
        }))
    }

    pub async fn ipv6_send_logs(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("ipv6_proxy.send-logs")
            .await?
            .unwrap_or(json!({"logs": []})))
    }

    async fn push_log(&self, key: &str, action: &str) -> AppResult<()> {
        let mut logs = self
            .db
            .config_get(key)
            .await?
            .unwrap_or(json!({"logs": []}));
        if let Some(arr) = logs.get_mut("logs").and_then(|v| v.as_array_mut()) {
            arr.push(json!({"at": now_ts(), "action": action}));
            if arr.len() > 300 {
                let drain = arr.len() - 300;
                arr.drain(0..drain);
            }
        }
        self.db.config_set(key, &logs).await
    }
}

fn validate_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(bad_request("ID 非法"));
    }
    Ok(())
}

/// 读取本机第一个全局 IPv6 地址。
///
/// `/proc/net/if_inet6` 每行：16 字节十六进制地址、ifindex、前缀长度、
/// scope（`00` = 全局）、flags、接口名。跳过链路本地（fe80）与回环，
/// 只返回可对外转发的地址。
fn read_global_ipv6() -> Option<String> {
    let text = std::fs::read_to_string("/proc/net/if_inet6").ok()?;
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let hex = cols[0];
        let scope = cols[3];
        let ifname = cols[5];
        if hex.len() != 32 || scope != "00" || hex.starts_with("fe80") || ifname == "lo" {
            continue;
        }
        let groups: Vec<&str> = (0..8).map(|i| &hex[i * 4..i * 4 + 4]).collect();
        return Some(groups.join(":"));
    }
    None
}
