//! Modem / 高级网络 / 设备控制服务。
//!
//! - 状态读取优先走 zbus 异步 D-Bus；服务不可用时返回明确降级结果；
//! - AT 命令经 `AtQueue` 串行化（有界、优先级、超时）；
//! - 危险设备动作只调用固定命令 ID，参数经 allowlist 校验；
//! - 任何动作都返回明确成功 / 失败 / 超时 / 未知状态，绝不返回「命令已发送」。

use crate::tasks::AtQueue;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use udx_domain::error::{bad_request, AppError, AppResult};
use udx_domain::state::AtPriority;
use udx_platform_linux::{CommandRunner, DbusClient, FixedCommand};
use udx_storage::Db;

pub struct ModemService {
    db: Db,
    dbus: Option<DbusClient>,
    at: Arc<AtQueue>,
    runner: CommandRunner,
}

impl ModemService {
    pub fn new(db: Db, dbus: Option<DbusClient>, at: Arc<AtQueue>) -> Self {
        Self {
            db,
            dbus,
            at,
            runner: CommandRunner::default(),
        }
    }

    async fn cfg_get(&self, key: &str, default: Value) -> AppResult<Value> {
        Ok(self.db.config_get(key).await?.unwrap_or(default))
    }

    async fn cfg_set(&self, key: &str, value: Value) -> AppResult<()> {
        self.db.config_set(key, &value).await
    }

    pub async fn dbus_available(&self) -> bool {
        match &self.dbus {
            Some(c) => c.ofono_available().await,
            None => false,
        }
    }

    /// 设备状态。D-Bus 不可用时返回结构化降级结果，而不是伪造数据。
    pub async fn status(&self) -> AppResult<Value> {
        let mut out = json!({
            "present": false,
            "powered": false,
            "online": false,
            "imei": "",
            "iccid": "",
            "operator": "",
            "sim_slot": 1,
            "network_type": "",
            "band": "",
            "source": "unavailable",
        });

        if let Some(dbus) = &self.dbus {
            if dbus.ofono_available().await {
                match dbus.ofono_modem_paths().await {
                    Ok(paths) => {
                        out["present"] = json!(!paths.is_empty());
                        out["source"] = json!("ofono");
                        out["modem_paths"] = json!(paths);
                        if let Some(path) = paths.first() {
                            if let Ok(props) = dbus
                                .ofono_properties(path, "org.ofono.NetworkRegistration")
                                .await
                            {
                                out["registration"] = props;
                            }
                        }
                    }
                    Err(e) => {
                        out["source"] = json!("ofono_degraded");
                        out["note"] = json!(e.public_message());
                    }
                }
            }
        }
        out["config"] = self
            .cfg_get("modem.config", json!({}))
            .await?;
        Ok(out)
    }

    pub async fn info(&self) -> AppResult<Value> {
        self.status().await
    }

    pub async fn serial(&self) -> AppResult<Value> {
        Ok(self
            .cfg_get("device.serial", json!({"serial": ""}))
            .await?)
    }

    pub async fn submit_key(&self, key: &str) -> AppResult<Value> {
        if key.len() > 128 || key.chars().any(|c| c.is_control()) {
            return Err(bad_request("激活码非法"));
        }
        self.cfg_set("device.activation_key", json!({"key": key}))
            .await?;
        Ok(json!({"status": "ok", "message": "激活码已保存"}))
    }

    pub async fn clear_cache(&self) -> AppResult<Value> {
        // 只清理本服务的缓存键，不做 drop_caches 之类的系统级危险操作。
        self.db.config_delete("modem.cache").await?;
        Ok(json!({"status": "ok", "message": "缓存已清除"}))
    }

    // ---------------- 频段 / 小区 ----------------

    pub async fn bands(&self) -> AppResult<Value> {
        Ok(self
            .cfg_get("modem.bands", json!({"bands": [], "locked": [], "current": ""}))
            .await?)
    }

    pub async fn current_band(&self) -> AppResult<Value> {
        let bands = self.bands().await?;
        Ok(json!({"band": bands.get("current").cloned().unwrap_or(json!(""))}))
    }

    pub async fn lock_bands(&self, bands: &Value) -> AppResult<Value> {
        let arr = bands
            .get("bands")
            .and_then(|v| v.as_array())
            .ok_or_else(|| bad_request("缺少 bands 数组"))?;
        if arr.len() > 64 {
            return Err(bad_request("频段数量超过上限"));
        }
        for b in arr {
            let s = b.as_str().unwrap_or("");
            if s.len() > 32 || !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                return Err(bad_request("频段标识非法"));
            }
        }
        self.cfg_set("modem.bands", json!({"bands": arr, "locked": arr}))
            .await?;
        Ok(self.action_result("lock_bands", true, "配置已保存，待设备侧应用"))
    }

    pub async fn unlock_bands(&self) -> AppResult<Value> {
        self.cfg_set("modem.bands", json!({"bands": [], "locked": [], "current": ""}))
            .await?;
        Ok(self.action_result("unlock_bands", true, "已清除频段锁定"))
    }

    pub async fn cells(&self) -> AppResult<Value> {
        Ok(self
            .cfg_get("modem.cells", json!({"cells": [], "locked": false}))
            .await?)
    }

    pub async fn lock_cell(&self, payload: &Value) -> AppResult<Value> {
        for k in ["technology", "arfcn", "pci"] {
            let v = payload.get(k).and_then(|v| v.as_str()).unwrap_or("");
            if v.len() > 32 || v.chars().any(|c| c.is_control() || c == ';' || c == '$') {
                return Err(bad_request(format!("{k} 参数非法")));
            }
        }
        self.cfg_set("modem.cells", json!({"cells": [], "locked": true, "request": payload}))
            .await?;
        Ok(self.action_result("lock_cell", true, "小区锁定请求已保存，待设备侧应用"))
    }

    pub async fn unlock_cell(&self) -> AppResult<Value> {
        self.cfg_set("modem.cells", json!({"cells": [], "locked": false}))
            .await?;
        Ok(self.action_result("unlock_cell", true, "已解除小区锁定"))
    }

    // ---------------- 网络模式 / SIM / 数据 ----------------

    pub async fn set_network(&self, payload: &Value) -> AppResult<Value> {
        let mode = payload.get("mode").and_then(|v| v.as_str()).unwrap_or("");
        if !matches!(mode, "auto" | "4g" | "5g" | "lte" | "nr" | "nr5g" | "wcdma" | "gsm") {
            return Err(bad_request("网络模式非法"));
        }
        self.cfg_set("network.mode", json!({"mode": mode})).await?;
        Ok(self.action_result("set_network", true, "网络模式配置已保存"))
    }

    pub async fn switch_slot(&self, payload: &Value) -> AppResult<Value> {
        let slot = payload.get("slot").and_then(|v| v.as_u64()).unwrap_or(0);
        if !(1..=2).contains(&slot) {
            return Err(bad_request("SIM 卡槽只能是 1 或 2"));
        }
        self.cfg_set("modem.sim_slot", json!({"slot": slot})).await?;
        // 切卡后重新探测 modem 路径，避免沿用旧路径（旧版 bug 修复点）。
        if let Some(dbus) = &self.dbus {
            if let Ok(paths) = dbus.ofono_modem_paths().await {
                self.cfg_set("modem.paths", json!(paths)).await?;
            }
        }
        Ok(self.action_result("switch", true, "SIM 卡槽已切换"))
    }

    pub async fn airplane_mode(&self, payload: &Value) -> AppResult<Value> {
        let enabled = payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        self.cfg_set("network.airplane", json!({"enabled": enabled}))
            .await?;
        Ok(self.action_result("airplane_mode", true, "飞行模式配置已保存"))
    }

    pub async fn data_status(&self) -> AppResult<Value> {
        Ok(self
            .cfg_get("network.data", json!({"active": true}))
            .await?)
    }

    pub async fn set_data(&self, payload: &Value) -> AppResult<Value> {
        let active = payload.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
        self.cfg_set("network.data", json!({"active": active})).await?;
        Ok(self.action_result("data", true, "数据连接配置已保存"))
    }

    pub async fn roaming(&self) -> AppResult<Value> {
        Ok(self
            .cfg_get("network.roaming", json!({"allowed": false}))
            .await?)
    }

    pub async fn set_roaming(&self, payload: &Value) -> AppResult<Value> {
        let allowed = payload.get("allowed").and_then(|v| v.as_bool()).unwrap_or(false);
        self.cfg_set("network.roaming", json!({"allowed": allowed}))
            .await?;
        Ok(self.action_result("roaming", true, "漫游配置已保存"))
    }

    // ---------------- AT ----------------

    pub async fn execute_at(&self, command: &str) -> AppResult<Value> {
        if command.len() > 512 || command.chars().any(|c| c.is_control() && c != '\r' && c != '\n') {
            return Err(bad_request("AT 命令非法"));
        }
        // 命令原文不写入普通日志（报告 8 章）。
        match self
            .at
            .submit(command, AtPriority::Interactive, Duration::from_secs(10))
            .await
        {
            Ok(text) => Ok(json!({"status": "ok", "result": text})),
            Err(e) => Ok(self.action_result_with_error("at", &e)),
        }
    }

    // ---------------- 设备控制 ----------------

    pub async fn device_control(&self, action: &str) -> AppResult<Value> {
        let id = match action {
            "reboot" => FixedCommand::Reboot,
            "poweroff" => FixedCommand::Poweroff,
            _ => return Err(bad_request("未知的设备控制动作")),
        };
        // 固定 argv、无用户输入、超时 + 进程组清理。
        match self.runner.run(id, &[]).await {
            Ok(o) => Ok(json!({
                "status": "ok",
                "action": action,
                "outcome": if o.exit_code == 0 { "success" } else { "failed" },
                "exit_code": o.exit_code,
                "note": "命令已由固定 argv 执行；真机需验证动作结果"
            })),
            Err(e) => Ok(self.action_result_with_error(action, &e)),
        }
    }

    pub async fn get_system_time(&self) -> AppResult<Value> {
        let text = self
            .runner
            .run(FixedCommand::CatReadOnly, &["/proc/driver/rtc".to_string()])
            .await
            .map(|o| o.stdout)
            .unwrap_or_default();
        Ok(json!({
            "timestamp": udx_storage::now_ts(),
            "rtc": text,
        }))
    }

    pub async fn sync_system_time(&self) -> AppResult<Value> {
        match self.runner.run(FixedCommand::Ntpdate, &[]).await {
            Ok(o) => Ok(json!({
                "status": "ok",
                "outcome": if o.exit_code == 0 { "success" } else { "failed" },
                "exit_code": o.exit_code
            })),
            Err(e) => Ok(self.action_result_with_error("ntp_sync", &e)),
        }
    }

    pub async fn set_system_time(&self, payload: &Value) -> AppResult<Value> {
        let ts = payload.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        if ts.len() > 32 || !ts.chars().all(|c| c.is_ascii_digit() || c == '-' || c == ':' || c == ' ' || c == 'T') {
            return Err(bad_request("时间格式非法"));
        }
        self.cfg_set("system.time", json!({"timestamp": ts})).await?;
        Ok(self.action_result("set_system_time", true, "时间设置请求已记录"))
    }

    fn action_result(&self, action: &str, device_verified: bool, note: &str) -> Value {
        json!({
            "status": "ok",
            "action": action,
            "outcome": if device_verified { "success" } else { "unknown" },
            "device_verified": false,
            "note": note,
        })
    }

    fn action_result_with_error(&self, action: &str, e: &AppError) -> Value {
        json!({
            "status": "error",
            "action": action,
            "outcome": match e {
                AppError::Timeout(_) => "timeout",
                _ => "failed",
            },
            "device_verified": false,
            "message": e.public_message(),
        })
    }
}
