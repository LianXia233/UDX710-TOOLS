//! 网络接口、流量、充电、定时重启、APN 与系统信息服务。

use crate::crypto::new_id;
use serde_json::{json, Value};
use udx_domain::error::{bad_request, internal, AppError, AppResult};
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
        let mut value = serde_json::to_value(&info)
            .map_err(|e| AppError::Internal(format!("系统信息序列化失败: {e}")))?;

        // ---- 旧字段别名（v2.x 兼容层）----
        // 契约（tests/contract/api_endpoints.json 的 /api/info.response）要求返回
        // uptime / total_ram / free_ram / thermal_temp / machine 等字段，
        // 而内部结构用 uptime_secs / mem_total_kb / mem_used_kb / temperature_c。
        // 这里按契约补齐，保证前端与旧消费方都能取到值，不出现空白格子。
        let obj = value
            .as_object_mut()
            .ok_or_else(|| AppError::Internal("系统信息不是对象".to_string()))?;
        let mem_total_bytes = info.mem_total_kb.saturating_mul(1024);
        let mem_used_bytes = info.mem_used_kb.saturating_mul(1024);
        obj.insert("uptime".to_string(), json!(info.uptime_secs));
        obj.insert("mem_total".to_string(), json!(mem_total_bytes));
        obj.insert("mem_used".to_string(), json!(mem_used_bytes));
        obj.insert("total_ram".to_string(), json!(mem_total_bytes));
        obj.insert(
            "free_ram".to_string(),
            json!(mem_total_bytes.saturating_sub(mem_used_bytes)),
        );
        obj.insert(
            "temperature".to_string(),
            match info.temperature_c {
                Some(t) => json!(t),
                None => Value::Null,
            },
        );
        obj.insert(
            "thermal_temp".to_string(),
            match info.temperature_c {
                Some(t) => json!(t),
                None => Value::Null,
            },
        );
        obj.insert("arch".to_string(), json!(std::env::consts::ARCH));
        obj.insert("machine".to_string(), json!(std::env::consts::ARCH));
        obj.insert("sysname".to_string(), json!("Linux"));
        obj.insert("release".to_string(), json!(info.kernel));
        // modem.js 会读取这些键：无数据源时显式给空串，
        // 避免前端走 undefined 分支显示异常；绝不伪造设备标识。
        for key in ["model", "modem_model", "imei", "iccid", "imsi", "carrier", "serial", "firmware"] {
            obj.entry(key.to_string()).or_insert_with(|| json!(""));
        }
        Ok(value)
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
        // 旧字段别名：前端读取 total（小写）；Total 为 v2.x 原始键名，两者同值。
        // today / month 无历史累计表，返回 0 而非缺字段，避免前端显示 undefined。
        Ok(json!({
            "Total": rx + tx,
            "total": rx + tx,
            "rx": rx,
            "tx": tx,
            "today": 0,
            "month": 0,
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

        // 电池实时状态：仅当内核 power_supply 暴露对应节点时才写入，
        // 缺失时保持字段为空字符串，前端据此显示「—」而不是 undefined。
        let mut level: Option<u32> = None;
        if let Ok(v) = self.sysfs.read_u32("class/power_supply/battery/capacity").await {
            level = Some(v);
            cfg["battery_level"] = json!(v);
            // 旧字段别名：前端仪表盘读 level / capacity，内部用 battery_level。
            cfg["level"] = json!(v);
            cfg["capacity"] = json!(v);
        }
        if let Ok(v) = self.sysfs.read_u32("class/power_supply/battery/voltage_now").await {
            // 内核多为微伏，换算成伏并保留两位，避免前端显示巨大数值。
            cfg["voltage"] = json!((v as f64 / 1_000_000.0 * 100.0).round() / 100.0);
        }
        if let Ok(v) = self.sysfs.read_u32("class/power_supply/battery/current_now").await {
            // 内核多为微安；负值表示放电，保留符号并按 mA 输出。
            cfg["current"] = json!((v as f64 / 1000.0 * 10.0).round() / 10.0);
        }
        if let Ok(v) = self.sysfs.read_u32("class/power_supply/battery/temp").await {
            // 内核以 0.1℃ 为单位。
            cfg["temperature"] = json!((v as f64 / 10.0 * 10.0).round() / 10.0);
        }
        if let Ok(v) = self.sysfs.read("class/power_supply/battery/status").await {
            let st = v.trim().to_string();
            cfg["status"] = json!(st);
            // charging 供前端兼容判断。
            cfg["charging"] = json!(st.eq_ignore_ascii_case("Charging"));
        }
        if let Ok(v) = self.sysfs.read("class/power_supply/battery/health").await {
            cfg["health"] = json!(v.trim());
        }
        if level.is_none() {
            // 无电池节点（本机为模组形态）：显式给出空值，避免前端出现 undefined。
            for key in ["level", "capacity", "voltage", "current", "temperature", "status", "health"] {
                if !cfg.get(key).map(|v| !v.is_null()).unwrap_or(false) {
                    cfg[key] = json!("");
                }
            }
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
        let stored = self
            .db
            .config_get("reboot.schedule")
            .await?
            .unwrap_or(json!({"days": [], "hour": 0, "minute": 0, "enabled": false}));
        Ok(Self::reboot_normalize(&stored))
    }

    /// 归一化定时重启配置：
    /// 兼容旧字段 day（单值，'*' 或 0-6）与 days（数组），
    /// 并保证四个键（days/enabled/hour/minute）齐全，避免前端拿到残缺对象。
    fn reboot_normalize(cfg: &Value) -> Value {
        let mut days: Vec<u64> = cfg
            .get("days")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_u64())
                    .filter(|d| *d <= 6)
                    .collect()
            })
            .unwrap_or_default();
        if days.is_empty() {
            match cfg.get("day").and_then(|v| v.as_str()) {
                Some("*") => days = (0..=6).collect(),
                Some(d) => {
                    if let Ok(n) = d.parse::<u64>() {
                        if n <= 6 {
                            days.push(n);
                        }
                    }
                }
                None => {}
            }
        }
        days.sort_unstable();
        days.dedup();
        let hour = cfg.get("hour").and_then(|v| v.as_u64()).unwrap_or(0).min(23);
        let minute = cfg.get("minute").and_then(|v| v.as_u64()).unwrap_or(0).min(59);
        let enabled = cfg
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(!days.is_empty())
            && !days.is_empty();
        json!({"days": days, "enabled": enabled, "hour": hour, "minute": minute})
    }

    pub async fn reboot_set(&self, payload: &Value) -> AppResult<Value> {
        let hour = payload.get("hour").and_then(|v| v.as_u64()).unwrap_or(0);
        let minute = payload.get("minute").and_then(|v| v.as_u64()).unwrap_or(0);
        if hour > 23 || minute > 59 {
            return Err(bad_request("定时时间非法"));
        }
        // 关键修复：原实现把 payload 整份写入，cron 只传 day/hour/minute，
        // 于是 days 与 enabled 被冲掉，页面永远显示 days=[]、enabled=false。
        let cfg = Self::reboot_normalize(payload);
        if cfg.get("enabled").and_then(|v| v.as_bool()) != Some(true) {
            return Err(bad_request("定时重启缺少有效日期"));
        }
        self.db.config_set("reboot.schedule", &cfg).await?;
        self.apply_reboot_cron(&cfg)?;
        Ok(json!({"status": "ok", "config": cfg}))
    }

    pub async fn reboot_clear(&self) -> AppResult<Value> {
        self.db.config_delete("reboot.schedule").await?;
        let cleared = json!({"days": [], "enabled": false, "hour": 0, "minute": 0});
        self.apply_reboot_cron(&cleared)?;
        Ok(json!({"status": "ok", "cleared": true}))
    }

    /// 把定时重启真正落到 crontab（原实现只存库，设备从不重启）。
    /// 只增删带标记的两行，绝不触碰其它行（例如看门狗兜底任务）。
    fn apply_reboot_cron(&self, cfg: &Value) -> AppResult<()> {
        const CRON_FILE: &str = "/var/spool/cron/crontabs/root";
        const MARKER: &str = "# udx710-reboot-schedule";

        let existing = std::fs::read_to_string(CRON_FILE).unwrap_or_default();
        let mut kept: Vec<String> = Vec::new();
        let mut skip_next = false;
        for line in existing.lines() {
            let t = line.trim();
            if t == MARKER {
                skip_next = true; // 标记行后紧跟的 cron 行一并移除
                continue;
            }
            if skip_next {
                skip_next = false;
                continue;
            }
            if t.is_empty() {
                continue;
            }
            kept.push(line.to_string());
        }

        let enabled = cfg.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        if enabled {
            let hour = cfg.get("hour").and_then(|v| v.as_u64()).unwrap_or(0);
            let minute = cfg.get("minute").and_then(|v| v.as_u64()).unwrap_or(0);
            let days: Vec<u64> = cfg
                .get("days")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_u64()).collect())
                .unwrap_or_default();
            if days.is_empty() {
                return Err(bad_request("定时重启缺少有效日期"));
            }
            let dow = if days.len() >= 7 {
                "*".to_string()
            } else {
                days.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(",")
            };
            kept.push(MARKER.to_string());
            kept.push(format!("{} {} * * {} /sbin/reboot", minute, hour, dow));
        }

        let mut out = kept.join("\n");
        out.push('\n');
        std::fs::write(CRON_FILE, out)
            .map_err(|e| internal(format!("写入 crontab 失败: {e}")))?;
        Ok(())
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
