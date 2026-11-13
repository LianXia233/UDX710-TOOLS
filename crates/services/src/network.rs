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
        // 保证设备底层默认启用 IPv6 双栈
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            Self::ensure_default_ipv6();
        });

        Self {
            db,
            runner: CommandRunner::default(),
            sysfs: Sysfs::default(),
        }
    }

    fn ensure_default_ipv6() {
        let output = std::process::Command::new("yr_cmd_app")
            .args(["apn", "get"])
            .output();
        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            if let Ok(parsed) = serde_json::from_str::<Value>(&stdout) {
                let current_type = parsed.get("context").and_then(|c| c.get("ip_type")).and_then(|v| v.as_u64());
                if current_type == Some(0) {
                    let apn_name = parsed.get("context").and_then(|c| c.get("apn_name")).and_then(|v| v.as_str()).unwrap_or("cmnet");
                    let _ = std::process::Command::new("/bin/sh")
                        .arg("-c")
                        .arg(format!("yr_cmd_app apn set --ip_type 2 --apn_name '{apn_name}' --apn_usrname 'null' --apn_password 'null' --apn_authtype 0"))
                        .status();
                }
            }
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
        // 蜂窝口（sipa_eth0）WAN IP 地址获取（IPv4 与 IPv6 双栈）
        let v6_map = udx_platform_linux::ipv6_map();
        let wan_ipv6 = v6_map.get("sipa_eth0").cloned().unwrap_or_else(|| {
            v6_map.values().next().cloned().unwrap_or_default()
        });
        let wan_ipv4 = match interface_stats("sipa_eth0").await {
            Ok(i) => i.ipv4,
            Err(_) => String::new(),
        };
        obj.insert("wan_ipv4".to_string(), json!(wan_ipv4));
        obj.insert("wan_ipv6".to_string(), json!(wan_ipv6));
        obj.insert("ipv6".to_string(), json!(wan_ipv6));
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
        let total = rx + tx;
        let cfg = self.traffic_config().await?;
        let (today, month) = self.traffic_delta(total).await;
        // 旧字段别名：前端读取 total（小写）；Total 为 v2.x 原始键名，两者同值。
        // limit 是对 much 的语义别名，前端进度条按 total/limit 计算。
        Ok(json!({
            "Total": total,
            "total": total,
            "rx": rx,
            "tx": tx,
            "today": today,
            "month": month,
            "switch": cfg.get("switch").cloned().unwrap_or(json!(0)),
            "much": cfg.get("much").cloned().unwrap_or(json!(0)),
            "limit": cfg.get("much").cloned().unwrap_or(json!(0)),
            // 说明取值口径，避免把「本次开机累计」误读为「账期累计」。
            "today_source": "since_boot",
            "note": "今日/本月按 /proc/net/dev 计数差值计算，重启后计数归零，跨日/跨月自动重置基线",
        }))
    }

    /// 计算今日 / 本月流量增量。
    ///
    /// `/proc/net/dev` 只有开机以来的累计值，没有历史表。这里用「基线快照」
    /// 的方式求差值：记录当日起点与当月起点，跨日/跨月或设备重启时重置基线。
    async fn traffic_delta(&self, total: u64) -> (u64, u64) {
        use chrono::Local;
        let today = Local::now().format("%Y-%m-%d").to_string();
        let month = Local::now().format("%Y-%m").to_string();
        let snap = self
            .db
            .config_get("traffic.snapshot")
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| json!({}));
        let snap_day = snap.get("day").and_then(|v| v.as_str()).unwrap_or("");
        let snap_month = snap.get("month").and_then(|v| v.as_str()).unwrap_or("");
        let has_snap = snap.get("last_total").and_then(|v| v.as_u64());
        // 快照版本：v1 会把「首次运行」的基线记成 0，导致今日=开机以来全部流量。
        // 检测到旧版本快照时一次性重建基线，避免把历史累计算进当天。
        let schema_ok = snap.get("v").and_then(|v| v.as_u64()) == Some(2);

        let last_total = if schema_ok { has_snap.unwrap_or(total) } else { total };
        // total 比上次小说明设备重启过，内核计数已归零，基线必须一起清零。
        let rebooted = schema_ok && has_snap.map(|l| total < l).unwrap_or(false);
        let mut day_base = if schema_ok {
            snap.get("day_base").and_then(|v| v.as_u64()).unwrap_or(total)
        } else {
            total
        };
        let mut month_base = if schema_ok {
            snap.get("month_base")
                .and_then(|v| v.as_u64())
                .unwrap_or(total)
        } else {
            total
        };
        if rebooted {
            // 设备重启后 /proc/net/dev 归零，重启前的当日用量已无法得知。
            // 这里把当前值作为新基线（今日从 0 重新累计），而不是把本次开机
            // 以来的全部流量当作「今日」——后者会让用户看到明显偏大的数字。
            day_base = total;
            month_base = total;
        }
        // 跨日 / 跨月：以上一次采样值作为新基线，避免把整段历史算进当天。
        if schema_ok && !rebooted && snap_day != today {
            day_base = last_total;
        }
        if schema_ok && !rebooted && snap_month != month {
            month_base = last_total;
        }

        // 写库限频：traffic_total 会被 WS 采样任务每 3 秒调用一次，
        // 每次写 SQLite 会与其它请求抢锁并拖慢推送。只在基线真正变化
        // （跨日 / 跨月 / 重启 / 首次）或距上次写入超过 60 秒时才落库。
        let now_s = udx_storage::now_ts();
        let last_write = snap
            .get("written_at")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let need_write = !schema_ok
            || rebooted
            || snap_day != today
            || snap_month != month
            || now_s.saturating_sub(last_write) >= 60;
        if need_write {
            let _ = self
                .db
                .config_set(
                    "traffic.snapshot",
                    &json!({
                        "v": 2,
                        "day": today,
                        "month": month,
                        "day_base": day_base,
                        "month_base": month_base,
                        "last_total": total,
                        "written_at": now_s,
                    }),
                )
                .await;
        }

        (total.saturating_sub(day_base), total.saturating_sub(month_base))
    }

    pub async fn traffic_config(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("traffic.config")
            .await?
            .unwrap_or(json!({"switch": 0, "much": 0})))
    }

    pub async fn traffic_set(&self, payload: &Value) -> AppResult<Value> {
        let is_clear = payload
            .get("action")
            .and_then(|v| v.as_str())
            .map(|s| s == "clear" || s == "reset")
            .unwrap_or(false)
            || payload.get("clear").and_then(|v| v.as_bool()).unwrap_or(false)
            || payload.as_object().map(|m| m.is_empty()).unwrap_or(false);

        if is_clear {
            // 获取当前实际 /proc/net/dev 累计作为新基准点，今日与本月即刻归零
            let output = self
                .runner
                .run(FixedCommand::CatReadOnly, &["/proc/net/dev".to_string()])
                .await;
            let (rx, tx) = match output {
                Ok(o) => parse_netdev_totals(&o.stdout),
                Err(_) => (0, 0),
            };
            let current_total = rx + tx;
            use chrono::Local;
            let today = Local::now().format("%Y-%m-%d").to_string();
            let month = Local::now().format("%Y-%m").to_string();
            let new_snap = json!({
                "v": 2,
                "day": today,
                "month": month,
                "day_base": current_total,
                "month_base": current_total,
                "last_total": current_total,
            });
            self.db.config_set("traffic.snapshot", &new_snap).await?;
            return Ok(json!({
                "status": "ok",
                "cleared": true,
                "message": "蜂窝数据流量统计基准已清空归零"
            }));
        }

        let switch = payload.get("switch").and_then(|v| v.as_u64()).unwrap_or(0);
        let much = payload.get("much").and_then(|v| v.as_u64()).unwrap_or(0);
        if much > 1024u64.saturating_mul(1024 * 1024 * 1024) {
            return Err(bad_request("流量限额超出范围"));
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
            // 无电池节点（本机为模组形态，实测 /sys/class/power_supply 为空）。
            // 显式给出空值 + supported=false，让前端显示「本机无电池」而不是
            // 一排破折号，避免被误判为读取失败。
            for key in ["level", "capacity", "voltage", "current", "temperature", "status", "health"] {
                if !cfg.get(key).map(|v| !v.is_null()).unwrap_or(false) {
                    cfg[key] = json!("");
                }
            }
            cfg["supported"] = json!(false);
            cfg["note"] = json!("设备未暴露 /sys/class/power_supply，本机无电池管理能力");
        } else {
            cfg["supported"] = json!(true);
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
        let stored = self.db.config_get("apn.config").await?;
        if let Some(cfg) = stored {
            return Ok(cfg);
        }
        Ok(json!({
            "apn": "cmnet",
            "username": "",
            "password": "",
            "auth_type": "none",
            "ip_type": "ipv4v6",
            "protocol": "dual"
        }))
    }

    pub async fn apn_set(&self, payload: &Value) -> AppResult<Value> {
        let apn = payload.get("apn").and_then(|v| v.as_str()).unwrap_or("");
        if apn.len() > 128 || apn.chars().any(|c| c.is_control()) {
            return Err(bad_request("APN 非法"));
        }
        let user = payload.get("username").and_then(|v| v.as_str()).unwrap_or("");
        let pass = payload.get("password").and_then(|v| v.as_str()).unwrap_or("");
        let auth_str = payload.get("auth_type").and_then(|v| v.as_str()).unwrap_or("none");
        let auth_int = match auth_str {
            "pap" => 1,
            "chap" => 2,
            "auto" | "pap_or_chap" => 3,
            _ => 0,
        };
        let ip_type_val = match payload.get("ip_type").or_else(|| payload.get("protocol")) {
            Some(Value::String(s)) => match s.to_ascii_lowercase().as_str() {
                "ipv6" => 1,
                "ipv4" => 0,
                _ => 2,
            },
            Some(Value::Number(n)) => n.as_u64().unwrap_or(2),
            _ => 2,
        };

        self.db.config_set("apn.config", payload).await?;
        Self::apply_device_apn(apn, user, pass, auth_int, ip_type_val);
        Ok(json!({"status": "ok", "applied": true, "ip_type": ip_type_val}))
    }

    pub async fn apn_apply(&self, payload: &Value) -> AppResult<Value> {
        let template_id = payload
            .get("template_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| payload.get("template_id").and_then(|v| v.as_u64()).map(|n| n.to_string()));
        let mut target_apn = json!({
            "apn": "cmnet",
            "username": "",
            "password": "",
            "auth_type": "none",
            "ip_type": "ipv4v6",
            "protocol": "dual"
        });
        if let Some(id) = template_id {
            if let Ok(Some(tpl)) = self.db.entity_get(Entity::ApnTemplate, &id).await {
                target_apn = json!(tpl);
            }
        } else if let Ok(Some(cfg)) = self.db.config_get("apn.config").await {
            target_apn = cfg;
        }
        self.apn_set(&target_apn).await
    }

    fn apply_device_apn(apn: &str, user: &str, pass: &str, auth_type: u64, ip_type: u64) {
        let safe_apn = if apn.is_empty() { "cmnet" } else { apn };
        let safe_user = if user.is_empty() { "null" } else { user };
        let safe_pass = if pass.is_empty() { "null" } else { pass };
        let cmd = format!(
            "yr_cmd_app apn set --ip_type {ip_type} --apn_name '{safe_apn}' --apn_usrname '{safe_user}' --apn_password '{safe_pass}' --apn_authtype {auth_type}"
        );
        let _ = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&cmd)
            .status();
    }

    pub async fn apn_templates_list(&self) -> AppResult<Value> {
        let list = self.db.entity_list(Entity::ApnTemplate).await?;
        if list.is_empty() {
            let presets = vec![
                json!({"id": "1", "name": "中国移动 (CMNET - 5G/4G通用)", "apn": "cmnet", "username": "", "password": "", "auth_type": "none", "protocol": "dual"}),
                json!({"id": "2", "name": "中国移动 物联网卡 (CMIOT)", "apn": "cmiot", "username": "", "password": "", "auth_type": "none", "protocol": "dual"}),
                json!({"id": "3", "name": "中国联通 (3GNET - 5G/4G通用)", "apn": "3gnet", "username": "", "password": "", "auth_type": "none", "protocol": "dual"}),
                json!({"id": "4", "name": "中国联通 物联网卡 (WONET)", "apn": "wonet", "username": "", "password": "", "auth_type": "none", "protocol": "dual"}),
                json!({"id": "5", "name": "中国电信 (CTNET - 5G/4G通用)", "apn": "ctnet", "username": "ctnet@myhsp.org", "password": "vnet.mobi", "auth_type": "pap", "protocol": "dual"}),
                json!({"id": "6", "name": "中国广电 5G (CBNET)", "apn": "cbnet", "username": "", "password": "", "auth_type": "none", "protocol": "dual"}),
                json!({"id": "7", "name": "通用互联网 APN (INTERNET)", "apn": "internet", "username": "", "password": "", "auth_type": "none", "protocol": "dual"}),
            ];
            for p in &presets {
                if let Some(id) = p.get("id").and_then(|v| v.as_str()) {
                    let _ = self.db.entity_upsert(Entity::ApnTemplate, id, p).await;
                }
            }
            if let Ok(updated) = self.db.entity_list(Entity::ApnTemplate).await {
                if !updated.is_empty() {
                    return Ok(json!({"templates": updated}));
                }
            }
            return Ok(json!({"templates": presets}));
        }
        Ok(json!({"templates": list}))
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


    pub async fn apn_clear(&self) -> AppResult<Value> {
        self.db.config_delete("apn.config").await?;
        Ok(json!({"status": "ok", "cleared": true}))
    }

    /// APN 工作模式（自动 / 手动 + 绑定模板）。
    ///
    /// 原 `/api/apn/config` 与 `/api/apn` 共用同一个 handler，返回的是 APN
    /// 参数字典，而「APN 模式」页面读的是 `mode` / `template_id` / `auto_start`，
    /// 三个字段永远取不到，模式恒显示「自动（系统管理）」。
    /// 这里把两者拆成独立语义，并用独立配置键，避免互相覆盖。
    pub async fn apn_mode_config(&self) -> AppResult<Value> {
        let raw = self
            .db
            .config_get("apn.mode")
            .await?
            .unwrap_or_else(|| json!({}));
        Ok(json!({
            "mode": raw.get("mode").and_then(|v| v.as_u64()).unwrap_or(0),
            "template_id": raw.get("template_id").and_then(|v| v.as_u64()).unwrap_or(0),
            "auto_start": raw.get("auto_start").and_then(|v| v.as_u64()).unwrap_or(0),
        }))
    }

    pub async fn apn_mode_set(&self, payload: &Value) -> AppResult<Value> {
        let mode = payload.get("mode").and_then(|v| v.as_u64()).unwrap_or(0);
        if mode > 1 {
            return Err(bad_request("APN 模式非法"));
        }
        let template_id = payload
            .get("template_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let auto_start = payload
            .get("auto_start")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if mode == 1 && template_id == 0 {
            return Err(bad_request("手动模式必须选择一个模板"));
        }
        // 只写 apn.mode，绝不触碰 apn.config（APN 参数），
        // 否则保存模式会把参数冲掉。
        self.db
            .config_set(
                "apn.mode",
                &json!({
                    "mode": mode,
                    "template_id": template_id,
                    "auto_start": auto_start,
                }),
            )
            .await?;
        Ok(json!({
            "status": "ok",
            "mode": mode,
            "template_id": template_id,
            "auto_start": auto_start,
        }))
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
    let mut sipa_eth0_rx = 0u64;
    let mut sipa_eth0_tx = 0u64;
    let mut sipa_eth0_found = false;

    let mut fallback_rx = 0u64;
    let mut fallback_tx = 0u64;

    for line in text.lines().skip(2) {
        let Some((iface, values)) = line.split_once(':') else { continue };
        let ifname = iface.trim();
        let nums: Vec<u64> = values
            .split_whitespace()
            .filter_map(|v| v.parse::<u64>().ok())
            .collect();
        if nums.len() < 10 {
            continue;
        }

        if ifname == "sipa_eth0" {
            sipa_eth0_rx = nums[0];
            sipa_eth0_tx = nums[8];
            sipa_eth0_found = true;
        }

        // 过滤回环、SIPA 镜像重复口以及内部 IPC 通道
        if ifname != "lo" && ifname != "sipa_usb0" && !ifname.starts_with("vpcie") {
            fallback_rx = fallback_rx.saturating_add(nums[0]);
            fallback_tx = fallback_tx.saturating_add(nums[8]);
        }
    }

    if sipa_eth0_found {
        (sipa_eth0_rx, sipa_eth0_tx)
    } else {
        (fallback_rx, fallback_tx)
    }
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
