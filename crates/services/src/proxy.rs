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

    pub async fn ipv6_config(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("ipv6_proxy.config")
            .await?
            .unwrap_or(json!({"enabled": false, "webhook": ""})))
    }

    pub async fn ipv6_config_set(&self, payload: &Value) -> AppResult<Value> {
        self.db.config_set("ipv6_proxy.config", payload).await?;
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

    pub async fn ipv6_status(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("ipv6_proxy.runtime")
            .await?
            .unwrap_or(json!({"running": false})))
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
