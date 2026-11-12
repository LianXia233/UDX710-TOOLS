//! 网络接口、流量、充电、定时重启、APN 与系统信息服务。

use crate::crypto::new_id;
use serde_json::{json, Value};
use udx_domain::error::{bad_request, AppError, AppResult};
use udx_platform_linux::{
    interface_stats, list_interfaces, system_info, CommandRunner, FixedCommand, Sysfs,
};
use udx_storage::{Db, Entity};

pub struct NetworkService {
    db: Db,
    runner: CommandRunner,
    sysfs: Sysfs,
}

impl NetworkService {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            runner: CommandRunner::default(),
            sysfs: Sysfs::default(),
        }
    }

    pub async fn system_info(&self, version: &str) -> AppResult<Value> {
        let info = system_info(version).await?;
        serde_json::to_value(info)
            .map_err(|e| AppError::Internal(format!("系统信息序列化失败: {e}")))
    }

    // ---------------- 网络接口 ----------------

    pub async fn netif_list(&self) -> AppResult<Value> {
        let mut list = list_interfaces().await?;
        let monitored: Vec<String> = self
            .db
            .config_get("netif.monitored")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        for iface in &mut list {
            iface.monitored = monitored.contains(&iface.name);
        }
        Ok(json!({"interfaces": list}))
    }

    pub async fn netif_stats(&self, payload: &Value) -> AppResult<Value> {
        let name = payload
            .get("interface")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        validate_ifname(name)?;
        let info = interface_stats(name).await?;
        serde_json::to_value(info)
            .map_err(|e| AppError::Internal(format!("接口统计序列化失败: {e}")))
    }

    pub async fn netif_monitor(&self, payload: &Value) -> AppResult<Value> {
        let name = payload
            .get("interface")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        validate_ifname(name)?;
        let enabled = payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        let mut list: Vec<String> = self
            .db
            .config_get("netif.monitored")
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        if enabled {
            if !list.contains(&name.to_string()) {
                list.push(name.to_string());
            }
        } else {
            list.retain(|n| n != name);
        }
        self.db
            .config_set("netif.monitored", &json!(list))
            .await?;
        Ok(json!({"status": "ok", "interface": name, "enabled": enabled}))
    }

    // ---------------- 流量 ----------------

    pub async fn traffic_total(&self) -> AppResult<Value> {
        let output = self
            .runner
            .run(FixedCommand::CatReadOnly, &["/proc/net/dev".to_string()])
            .await;
        let (rx, tx) = match output {
            Ok(o) => parse_netdev_totals(&o.stdout),
            Err(_) => (0, 0),
        };
        let cfg = self.traffic_config().await?;
        Ok(json!({
            "Total": rx + tx,
            "rx": rx,
            "tx": tx,
            "switch": cfg.get("switch").cloned().unwrap_or(json!(0)),
            "much": cfg.get("much").cloned().unwrap_or(json!(0)),
        }))
    }

    pub async fn traffic_config(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("traffic.config")
            .await?
            .unwrap_or(json!({"switch": 0, "much": 0})))
    }

    pub async fn traffic_set(&self, payload: &Value) -> AppResult<Value> {
        let switch = payload.get("switch").and_then(|v| v.as_u64()).unwrap_or(0);
        let much = payload.get("much").and_then(|v| v.as_u64()).unwrap_or(0);
        if much > 1024u64.saturating_mul(1024 * 1024 * 1024) {
            return Err(bad_request("流量限额超出范围"));
        }
        // 空对象表示清除统计（旧版行为兼容）。
        if payload.as_object().map(|m| m.is_empty()).unwrap_or(false) {
            self.db.config_set("traffic.config", &json!({"switch": 0, "much": 0})).await?;
            return Ok(json!({"status": "ok", "cleared": true}));
        }
        self.db
            .config_set("traffic.config", &json!({"switch": switch, "much": much}))
            .await?;
        Ok(json!({"status": "ok"}))
    }

    // ---------------- 充电 ----------------

    pub async fn charge_config(&self) -> AppResult<Value> {
        let mut cfg = self
            .db
            .config_get("charge.config")
            .await?
            .unwrap_or(json!({"enabled": false, "startThreshold": 20, "stopThreshold": 80}));
        if let Ok(v) = self.sysfs.read_u32("class/power_supply/battery/capacity").await {
            cfg["battery_level"] = json!(v);
        }
        Ok(cfg)
    }

    pub async fn charge_set(&self, payload: &Value) -> AppResult<Value> {
        let start = payload.get("startThreshold").and_then(|v| v.as_u64()).unwrap_or(20);
        let stop = payload.get("stopThreshold").and_then(|v| v.as_u64()).unwrap_or(80);
        if start > 100 || stop > 100 || start >= stop {
            return Err(bad_request("充电阈值非法"));
        }
        self.db.config_set("charge.config", payload).await?;
        Ok(json!({"status": "ok"}))
    }

    pub async fn charge_on(&self) -> AppResult<Value> {
        self.charge_switch(true).await
    }

    pub async fn charge_off(&self) -> AppResult<Value> {
        self.charge_switch(false).await
    }

    async fn charge_switch(&self, on: bool) -> AppResult<Value> {
        // sysfs 路径由白名单根限定，值仅为 0/1，不经过 shell。
        let result = self
            .sysfs
            .write("class/power_supply/battery/charging_enabled", if on { "1" } else { "0" })
            .await;
        Ok(json!({
            "status": "ok",
            "action": if on { "charge_on" } else { "charge_off" },
            "outcome": if result.is_ok() { "success" } else { "unknown" },
            "device_verified": false,
        }))
    }

    // ---------------- 定时重启 ----------------

    pub async fn reboot_config(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("reboot.schedule")
            .await?
            .unwrap_or(json!({"days": [], "hour": 0, "minute": 0, "enabled": false})))
    }

    pub async fn reboot_set(&self, payload: &Value) -> AppResult<Value> {
        let hour = payload.get("hour").and_then(|v| v.as_u64()).unwrap_or(0);
        let minute = payload.get("minute").and_then(|v| v.as_u64()).unwrap_or(0);
        if hour > 23 || minute > 59 {
            return Err(bad_request("定时时间非法"));
        }
        self.db.config_set("reboot.schedule", payload).await?;
        Ok(json!({"status": "ok"}))
    }

    pub async fn reboot_clear(&self) -> AppResult<Value> {
        self.db.config_delete("reboot.schedule").await?;
        Ok(json!({"status": "ok", "cleared": true}))
    }

    // ---------------- APN ----------------

    pub async fn apn_config(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("apn.config")
            .await?
            .unwrap_or(json!({"apn": "", "username": "", "password": "", "auth_type": "auto", "ip_type": "ipv4v6"})))
    }

    pub async fn apn_set(&self, payload: &Value) -> AppResult<Value> {
        let apn = payload.get("apn").and_then(|v| v.as_str()).unwrap_or("");
        if apn.len() > 128 || apn.chars().any(|c| c.is_control()) {
            return Err(bad_request("APN 非法"));
        }
        self.db.config_set("apn.config", payload).await?;
        Ok(json!({"status": "ok"}))
    }

    pub async fn apn_templates_list(&self) -> AppResult<Value> {
        Ok(json!({"templates": self.db.entity_list(Entity::ApnTemplate).await?}))
    }

    pub async fn apn_template_create(&self, payload: &Value) -> AppResult<Value> {
        let id = new_id();
        self.db
            .entity_upsert(Entity::ApnTemplate, &id, payload)
            .await?;
        Ok(json!({"status": "ok", "id": id}))
    }

    pub async fn apn_template_update(&self, id: &str, payload: &Value) -> AppResult<Value> {
        validate_id(id)?;
        self.db
            .entity_upsert(Entity::ApnTemplate, id, payload)
            .await?;
        Ok(json!({"status": "ok", "id": id}))
    }

    pub async fn apn_template_delete(&self, id: &str) -> AppResult<Value> {
        validate_id(id)?;
        let n = self.db.entity_delete(Entity::ApnTemplate, id).await?;
        Ok(json!({"status": "ok", "deleted": n}))
    }

    pub async fn apn_apply(&self) -> AppResult<Value> {
        Ok(json!({
            "status": "ok",
            "outcome": "unknown",
            "device_verified": false,
            "note": "APN 应用需设备侧 oFono 上下文确认"
        }))
    }

    pub async fn apn_clear(&self) -> AppResult<Value> {
        self.db.config_delete("apn.config").await?;
        Ok(json!({"status": "ok", "cleared": true}))
    }

    // ---------------- 手机壳模式 ----------------

    pub async fn phone_case_get(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("phone_case")
            .await?
            .unwrap_or(json!({"enabled": false})))
    }

    pub async fn phone_case_set(&self, payload: &Value) -> AppResult<Value> {
        self.db.config_set("phone_case", payload).await?;
        Ok(json!({"status": "ok"}))
    }
}

fn validate_ifname(name: &str) -> AppResult<()> {
    if name.is_empty() || name.len() > 32 {
        return Err(bad_request("接口名为空或过长"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | ':'))
    {
        return Err(bad_request("接口名包含非法字符"));
    }
    Ok(())
}

fn validate_id(id: &str) -> AppResult<()> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(bad_request("ID 非法"));
    }
    Ok(())
}

fn parse_netdev_totals(text: &str) -> (u64, u64) {
    let mut rx = 0u64;
    let mut tx = 0u64;
    for line in text.lines().skip(2) {
        let Some((_, values)) = line.split_once(':') else { continue };
        let nums: Vec<u64> = values
            .split_whitespace()
            .filter_map(|v| v.parse::<u64>().ok())
            .collect();
        if nums.len() >= 10 {
            rx = rx.saturating_add(nums[0]);
            tx = tx.saturating_add(nums[8]);
        }
    }
    (rx, tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_net_dev() {
        let t = "Inter-|   Receive\n face |bytes\n  eth0: 100 1 0 0 0 0 0 0 200 0 0 0 0 0 0 0\n";
        assert_eq!(parse_netdev_totals(t), (100, 200));
    }

    #[test]
    fn rejects_bad_ifname() {
        assert!(validate_ifname("eth0").is_ok());
        assert!(validate_ifname("eth0;reboot").is_err());
    }
}
