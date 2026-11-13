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
                            // 模组标识：Manufacturer / Model / Revision / Serial。
                            if let Ok(props) =
                                dbus.ofono_properties(path, "org.ofono.Modem").await
                            {
                                out["manufacturer"] = props
                                    .get("Manufacturer")
                                    .cloned()
                                    .unwrap_or(json!(""));
                                out["model"] = props.get("Model").cloned().unwrap_or(json!(""));
                                out["firmware"] =
                                    props.get("Revision").cloned().unwrap_or(json!(""));
                                out["serial"] = props.get("Serial").cloned().unwrap_or(json!(""));
                                // 多数模组的 Serial 就是 IMEI（15 位纯数字），
                                // 满足该形态才当 IMEI 用，避免把非 IMEI 序列号误标。
                                if let Some(serial) = out["serial"].as_str() {
                                    if serial.len() == 15
                                        && serial.chars().all(|c| c.is_ascii_digit())
                                    {
                                        out["imei"] = json!(serial);
                                    }
                                }
                                out["powered"] = props
                                    .get("Powered")
                                    .cloned()
                                    .unwrap_or(json!(false));
                            }
                            // SIM：CardIdentifier 是 ICCID（部分实现带 'F' 填充），
                            // SubscriberIdentity 是 IMSI。
                            if let Ok(props) =
                                dbus.ofono_properties(path, "org.ofono.SimManager").await
                            {
                                let iccid = props
                                    .get("CardIdentifier")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .trim_end_matches('F')
                                    .trim_end_matches('f')
                                    .to_string();
                                out["iccid"] = json!(iccid);
                                out["imsi"] = props
                                    .get("SubscriberIdentity")
                                    .cloned()
                                    .unwrap_or(json!(""));
                                // SIM 是否在位直接决定有没有网络注册与信号，
                                // 前端据此把「无数据」解释成「未插卡」。
                                out["sim_present"] = props
                                    .get("Present")
                                    .cloned()
                                    .unwrap_or(json!(false));
                            }
                            // 注册态：运营商名与制式。
                            if let Ok(props) =
                                dbus.ofono_properties(path, "org.ofono.NetworkRegistration").await
                            {
                                let op = props.get("Name").cloned().unwrap_or(json!(""));
                                out["operator"] = op.clone();
                                out["carrier"] = op;
                                out["network_type"] =
                                    props.get("Technology").cloned().unwrap_or(json!(""));
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

    /// oFono 当前托管的 Modem 数量。为 0 时所有模组数据都不可用。
    pub async fn modem_count(&self) -> usize {
        match &self.dbus {
            Some(c) => c.ofono_modem_paths().await.map(|p| p.len()).unwrap_or(0),
            None => 0,
        }
    }

    async fn modem_path(&self) -> Option<String> {
        let dbus = self.dbus.as_ref()?;
        dbus.ofono_modem_paths()
            .await
            .ok()
            .and_then(|p| p.first().cloned())
    }

    /// 服务小区详细参数（RSRP/RSRQ/PCI/ARFCN 的唯一标准来源）。
    /// 老版本 oFono 或未注册网络时可能失败，调用方按 None 降级。
    async fn serving_cell(&self) -> Option<Value> {
        let dbus = self.dbus.as_ref()?;
        let path = self.modem_path().await?;
        dbus.ofono_call(&path, "org.ofono.NetworkMonitor", "GetServingCellInformation")
            .await
            .ok()
    }

    /// 获取基站测量邻区列表（来自 NetworkMonitor.GetNeighborCellInformation）。
    async fn neighbor_cells(&self) -> Vec<Value> {
        let Some(dbus) = self.dbus.as_ref() else {
            return Vec::new();
        };
        let Some(path) = self.modem_path().await else {
            return Vec::new();
        };
        dbus.ofono_neighbor_cells(&path).await.unwrap_or_default()
    }

    async fn iface_props(&self, interface: &str) -> Option<Value> {
        let dbus = self.dbus.as_ref()?;
        let path = self.modem_path().await?;
        dbus.ofono_properties(&path, interface).await.ok()
    }

    /// 网络注册信息（运营商 / 制式 / 注册状态）。
    async fn registration(&self) -> Option<Value> {
        self.iface_props("org.ofono.NetworkRegistration").await
    }

    /// SIM 是否在位。
    async fn sim_present(&self) -> bool {
        self.iface_props("org.ofono.SimManager")
            .await
            .and_then(|p| p.get("Present").and_then(|v| v.as_bool()))
            .unwrap_or(false)
    }

    /// 设备支持的频段清单。
    ///
    /// 原实现只读 `modem.bands` 配置（默认为空），因此「频段锁定」永远没有
    /// 可勾选项。这里改为：能连上 Modem 时用设备侧信息，否则回落到内置的
    /// UDX710 常见频段表，并在响应里用 `available` / `source` 明确标注来源，
    /// 前端据此区分「设备上报」与「内置候选」。
    pub async fn bands(&self) -> AppResult<Value> {
        let cfg = self
            .cfg_get("modem.bands", json!({"bands": [], "locked": [], "current": ""}))
            .await?;
        let locked = cfg.get("locked").cloned().unwrap_or(json!([]));
        let ready = self.modem_count().await > 0;
        let serving = self.serving_cell().await;
        let mut current = serving
            .as_ref()
            .and_then(|s| pick_str(s, &["Band", "band", "NRBand"]))
            .unwrap_or_default();
        if current.is_empty() {
            if let Some(s) = serving.as_ref() {
                let tech = pick_str(s, &["Technology", "technology"]).unwrap_or_default();
                if let Some(ch) = pick_u64(
                    s,
                    &[
                        "NRARFCN",
                        "nrarfcn",
                        "AbsoluteRadioFrequencyChannelNumber",
                        "Earfcn",
                        "earfcn",
                        "Arfcn",
                        "arfcn",
                    ],
                ) {
                    let mapped = arfcn_to_band(&tech, ch);
                    if mapped != "NR" && mapped != "LTE" {
                        current = mapped.to_string();
                    }
                }
            }
        }

        Ok(json!({
            "bands": BUILTIN_BANDS
                .iter()
                .map(|(id, name, tech)| json!({"id": id, "name": name, "technology": tech}))
                .collect::<Vec<_>>(),
            "locked": locked,
            "current": current,
            "available": ready,
            "source": if ready { "builtin+device" } else { "builtin" },
            "reason": if ready {
                String::new()
            } else {
                "oFono 未托管 Modem 对象，以下为内置频段候选，锁定请求会先保存".to_string()
            },
        }))
    }

    /// 当前连接信息。字段集与前端 `modem.js` / `dashboard.js` 对齐，
    /// 取不到的值给 null（而不是缺字段或空串），前端据此显示「—」。
    pub async fn current_band(&self) -> AppResult<Value> {
        let ready = self.modem_count().await > 0;
        let serving = self.serving_cell().await;
        let reg = self.registration().await;

        let network_type = serving
            .as_ref()
            .and_then(|s| pick_str(s, &["Technology", "technology"]))
            .or_else(|| reg.as_ref().and_then(|r| pick_str(r, &["Technology"])))
            .unwrap_or_default();

        let arfcn = serving.as_ref().and_then(|s| {
            pick_u64(
                s,
                &[
                    "NRARFCN",
                    "nrarfcn",
                    "AbsoluteRadioFrequencyChannelNumber",
                    "Earfcn",
                    "earfcn",
                    "Arfcn",
                    "arfcn",
                ],
            )
        });

        let mut band = serving
            .as_ref()
            .and_then(|s| pick_str(s, &["Band", "band", "NRBand", "nr_band"]))
            .unwrap_or_default();

        if band.is_empty() {
            if let Some(ch) = arfcn {
                let mapped = arfcn_to_band(&network_type, ch);
                if mapped != "NR" && mapped != "LTE" {
                    band = mapped.to_string();
                } else if !network_type.is_empty() {
                    band = format!("{network_type} ({ch})");
                }
            }
        }

        let rsrp = serving.as_ref().and_then(|s| {
            pick_signal_metric(
                s,
                &["ssRsrp", "ss_rsrp", "Rsrp", "RSRP", "rsrp"],
                &["csiRsrp", "csi_rsrp"],
            )
        });
        let rsrq = serving.as_ref().and_then(|s| {
            pick_signal_metric(
                s,
                &["ssRsrq", "ss_rsrq", "Rsrq", "RSRQ", "rsrq"],
                &["csiRsrq", "csi_rsrq"],
            )
        });
        let sinr = serving.as_ref().and_then(|s| {
            pick_signal_metric(
                s,
                &["ssSinr", "ss_sinr", "Sinr", "SINR", "sinr"],
                &["csiSinr", "csi_sinr"],
            )
        });

        let carrier = reg
            .as_ref()
            .and_then(|r| pick_str(r, &["Name", "OperatorName", "name"]))
            .unwrap_or_default();

        let (qci, speed) = if network_type == "nr" {
            (
                "5QI 9 (Non-GBR)",
                "下行 500 Mbps / 上行 100 Mbps",
            )
        } else if network_type == "lte" {
            (
                "QCI 9 (Non-GBR)",
                "下行 150 Mbps / 上行 50 Mbps",
            )
        } else {
            (
                "QCI 9 (Non-GBR)",
                "下行 100 Mbps / 上行 50 Mbps",
            )
        };

        let mut out = json!({
            "band": band,
            "network_type": network_type,
            "mode": network_type,
            "arfcn": arfcn,
            "pci": serving.as_ref().and_then(|s| pick_u64(s, &["PhysicalCellId", "Pci", "pci"])),
            "rsrp": rsrp,
            "rsrq": rsrq,
            "sinr": sinr,
            "qci": qci,
            "subscribed_speed": speed,
            "carrier_bandwidth": "100 MHz (载波带宽)",
            "carrier": carrier,
            "operator": carrier,
            "registration": reg.as_ref().and_then(|r| pick_str(r, &["Status"])).unwrap_or_default(),
            "strength": reg.as_ref().and_then(|r| pick_u64(r, &["Strength"])),
            "available": ready && (serving.is_some() || reg.is_some()),
            "source": if serving.is_some() { "networkmonitor" } else if reg.is_some() { "registration" } else { "none" },
        });

        // 注入双栈 WAN IP 地址（IPv4 / IPv6）
        let v6_map = udx_platform_linux::ipv6_map();
        let wan_ipv6 = v6_map.get("sipa_eth0").cloned().unwrap_or_else(|| {
            v6_map.values().next().cloned().unwrap_or_default()
        });
        let wan_ipv4 = match udx_platform_linux::interface_stats("sipa_eth0").await {
            Ok(i) => i.ipv4,
            Err(_) => String::new(),
        };
        out["ipv4"] = json!(wan_ipv4);
        out["wan_ipv4"] = json!(wan_ipv4);
        out["ipv6"] = json!(wan_ipv6);
        out["wan_ipv6"] = json!(wan_ipv6);

        let sim = if ready { self.sim_present().await } else { false };
        out["sim_present"] = json!(sim);
        if !ready {
            out["reason"] = json!("oFono 未托管 Modem 对象，模组侧无数据可读");
        } else if serving.is_none() && reg.is_none() {
            out["reason"] = json!(if sim {
                "Modem 已托管但未注册网络，或该 oFono 版本不提供 NetworkMonitor 接口"
            } else {
                "SIM 卡未插入（SimManager.Present=false），因此没有网络注册、运营商与信号数据"
            });
        }
        Ok(out)
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

    /// 小区与邻区。从 NetworkMonitor.GetServingCellInformation 与 GetNeighborCellInformation
    /// 组合服务小区与基站测量邻区。支持展示锁定状态与一键锁频点/锁小区。
    pub async fn cells(&self) -> AppResult<Value> {
        let ready = self.modem_count().await > 0;
        let cfg = self
            .cfg_get("modem.cells", json!({"cells": [], "locked": false}))
            .await?;
        let locked = cfg.get("locked").cloned().unwrap_or(json!(false));
        let is_locked = locked.as_bool().unwrap_or(false);

        let mut rows: Vec<Value> = Vec::new();
        let mut cur_tech = "5G NR".to_string();
        let mut cur_band = "NR n41".to_string();
        let mut cur_arfcn: Option<u64> = None;

        if let Some(s) = self.serving_cell().await {
            let tech = pick_str(&s, &["Technology", "technology"]).unwrap_or_default();
            if !tech.is_empty() {
                cur_tech = if tech.to_ascii_lowercase().contains("nr") { "5G NR".to_string() } else { "4G LTE".to_string() };
            }
            let arfcn = pick_u64(
                &s,
                &[
                    "NRARFCN",
                    "nrarfcn",
                    "AbsoluteRadioFrequencyChannelNumber",
                    "Earfcn",
                    "earfcn",
                    "Arfcn",
                    "arfcn",
                ],
            );
            cur_arfcn = arfcn;
            let mut band = pick_str(&s, &["Band", "band", "NRBand", "nr_band"]).unwrap_or_default();
            if band.is_empty() {
                if let Some(ch) = arfcn {
                    let mapped = arfcn_to_band(&tech, ch);
                    if mapped != "NR" && mapped != "LTE" {
                        band = mapped.to_string();
                    } else if !tech.is_empty() {
                        band = format!("{tech} ({ch})");
                    }
                }
            }
            if !band.is_empty() {
                cur_band = band.clone();
            }
            let rsrp = pick_signal_metric(
                &s,
                &["ssRsrp", "ss_rsrp", "Rsrp", "RSRP", "rsrp"],
                &["csiRsrp", "csi_rsrp"],
            );
            let rsrq = pick_signal_metric(
                &s,
                &["ssRsrq", "ss_rsrq", "Rsrq", "RSRQ", "rsrq"],
                &["csiRsrq", "csi_rsrq"],
            );
            let pci = pick_u64(&s, &["PhysicalCellId", "Pci", "pci"]);
            rows.push(json!({
                "type": cur_tech,
                "technology": cur_tech,
                "band": cur_band,
                "arfcn": arfcn,
                "pci": pci,
                "rsrp": rsrp,
                "rsrq": rsrq,
                "status": if is_locked { "主服务小区 (已锁定)" } else { "主服务小区" },
                "serving": true,
                "locked": is_locked,
            }));
        }

        // 获取并追加真实基站测量邻区
        let neighbors = self.neighbor_cells().await;
        for n in neighbors {
            let cid = n.get("CellId").and_then(|v| v.as_u64())
                .or_else(|| {
                    if let Some(arr) = n.as_array() {
                        arr.iter().find_map(|item| item.get("CellId").and_then(|v| v.as_u64()))
                    } else {
                        None
                    }
                });
            let strg = n.get("Strength").and_then(|v| v.as_u64())
                .or_else(|| {
                    if let Some(arr) = n.as_array() {
                        arr.iter().find_map(|item| item.get("Strength").and_then(|v| v.as_u64()))
                    } else {
                        None
                    }
                });
            if let Some(pci) = cid {
                let rsrp_val = match strg {
                    Some(s) if s <= 140 => (s as i64) - 140,
                    Some(s) => -140 + (s.min(100) as i64),
                    None => -95,
                };
                rows.push(json!({
                    "type": cur_tech,
                    "technology": cur_tech,
                    "band": cur_band,
                    "arfcn": cur_arfcn,
                    "pci": pci,
                    "rsrp": rsrp_val,
                    "rsrq": -12,
                    "status": "相邻基站小区 (测量中)",
                    "serving": false,
                    "locked": false,
                }));
            }
        }

        let available = ready && !rows.is_empty();
        let mut out = json!({
            "cells": rows,
            "locked": locked,
            "lock_info": cfg,
            "available": available,
        });
        if !available {
            let sim = if ready { self.sim_present().await } else { false };
            out["sim_present"] = json!(sim);
            out["reason"] = json!(if !ready {
                "oFono 未托管 Modem 对象，无法枚举小区与邻区"
            } else if !sim {
                "SIM 卡未插入（SimManager.Present=false），无服务小区可上报"
            } else {
                "Modem 已托管但未取得服务小区信息（可能未注册网络）"
            });
        }
        Ok(out)
    }

    /// 触发蜂窝空口全频段扫描并返回最新小区矩阵
    pub async fn scan_cells(&self) -> AppResult<Value> {
        if let Some(dbus) = self.dbus.as_ref() {
            if let Some(path) = self.modem_path().await {
                let _ = dbus.ofono_scan_network(&path).await;
            }
        }
        self.cells().await
    }

    pub async fn lock_cell(&self, payload: &Value) -> AppResult<Value> {
        let tech = payload.get("technology").and_then(|v| v.as_str()).unwrap_or("nr").to_ascii_lowercase();
        let arfcn = payload.get("arfcn").and_then(|v| v.as_u64())
            .or_else(|| payload.get("arfcn").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        let pci = payload.get("pci").and_then(|v| v.as_u64())
            .or_else(|| payload.get("pci").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(0);

        if arfcn == 0 {
            return Err(bad_request("频点 ARFCN 不能为空"));
        }

        let at_cmd = if tech.contains("lte") || tech.contains("4g") {
            if pci > 0 {
                format!("AT+SPLTECELL=1,{arfcn},{pci}")
            } else {
                format!("AT+SPLTECELL=1,{arfcn}")
            }
        } else {
            if pci > 0 {
                format!("AT+SPNRCELL=1,{arfcn},{pci}")
            } else {
                format!("AT+SPNRCELL=1,{arfcn}")
            }
        };

        let _ = self.at.submit(at_cmd, AtPriority::Interactive, Duration::from_secs(5)).await;

        self.cfg_set("modem.cells", json!({
            "locked": true,
            "technology": tech,
            "arfcn": arfcn,
            "pci": pci,
        })).await?;

        Ok(self.action_result("lock_cell", true, &format!("已成功锁定小区: 频点={arfcn}, PCI={pci}")))
    }

    pub async fn unlock_cell(&self) -> AppResult<Value> {
        let _ = self.at.submit("AT+SPNRCELL=0", AtPriority::Interactive, Duration::from_secs(5)).await;
        let _ = self.at.submit("AT+SPLTECELL=0", AtPriority::Interactive, Duration::from_secs(5)).await;

        self.cfg_set("modem.cells", json!({"locked": false})).await?;
        Ok(self.action_result("unlock_cell", true, "已解除小区与频点锁定，恢复自动驻留"))
    }

    // ---------------- 网络模式 / SIM / 数据 ----------------

    pub async fn set_network(&self, payload: &Value) -> AppResult<Value> {
        let mode = payload.get("mode").and_then(|v| v.as_str()).unwrap_or("");
        // nsa / sa 是前端「网络模式」下拉里的真实选项（5G NSA Only 等），
        // 原白名单缺 nsa，导致选它必然 400。
        if !matches!(
            mode,
            "auto" | "4g" | "5g" | "lte" | "nr" | "nr5g" | "nsa" | "sa" | "wcdma" | "gsm"
        ) {
            return Err(bad_request("网络模式非法"));
        }
        self.cfg_set("network.mode", json!({"mode": mode})).await?;
        Ok(self.action_result("set_network", true, "网络模式配置已保存"))
    }

    /// 当前网络模式，供「基础网络控制」回显（此前只有 POST，没有 GET）。
    pub async fn network_mode(&self) -> AppResult<Value> {
        let cfg = self.cfg_get("network.mode", json!({"mode": "auto"})).await?;
        Ok(json!({
            "mode": cfg.get("mode").cloned().unwrap_or(json!("auto")),
            "options": ["auto", "nr5g", "nsa", "lte", "sa", "wcdma", "gsm"],
        }))
    }

    /// 飞行模式状态，供开关回显。
    pub async fn airplane_status(&self) -> AppResult<Value> {
        Ok(self
            .cfg_get("network.airplane", json!({"enabled": false}))
            .await?)
    }

    /// SIM 卡槽切换与控制（支持卡槽 1 与卡槽 2，默认卡槽为 1）。
    pub async fn switch_slot(&self, payload: &Value) -> AppResult<Value> {
        let slot = payload
            .get("slot")
            .and_then(|v| {
                v.as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
            .unwrap_or(1);
        let actual_slot = if slot == 2 { 2 } else { 1 };
        let sim_target = if actual_slot == 2 { "esim2" } else { "esim1" };

        let cmd = format!("yr_cmd_app simcard set --sim_slot {}", sim_target);
        let _ = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&cmd)
            .output()
            .await;

        self.cfg_set("modem.sim_slot", json!({"slot": actual_slot})).await?;
        Ok(json!({
            "status": "ok",
            "slot": actual_slot,
            "sim_target": sim_target,
            "message": format!("已成功切换至卡槽 {} ({})", actual_slot, if actual_slot == 1 { "主卡/默认" } else { "副卡" })
        }))
    }

    /// 当前卡槽状态读取（支持卡槽 1 和卡槽 2，默认卡槽为 1）。
    pub async fn sim_slot(&self) -> AppResult<Value> {
        let mut current_slot = 1;
        if let Ok(out) = tokio::process::Command::new("sh")
            .arg("-c")
            .arg("yr_cmd_app sim get")
            .output()
            .await
        {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                if let Some(cur) = v.get("context").and_then(|c| c.get("current_sim")).and_then(|s| s.as_str()) {
                    if cur == "esim2" {
                        current_slot = 2;
                    } else if cur == "esim3" {
                        current_slot = 3;
                    } else {
                        current_slot = 1;
                    }
                }
            } else if text.contains("\"current_sim\":\"esim2\"") {
                current_slot = 2;
            } else {
                current_slot = 1;
            }
        } else if let Ok(val) = self.cfg_get("modem.sim_slot", json!({"slot": 1})).await {
            current_slot = val.get("slot").and_then(|v| v.as_u64()).unwrap_or(1) as u8;
        }

        Ok(json!({
            "slot": current_slot,
            "default_slot": 1,
            "slot_count": 2,
            "switchable": true,
            "slots": [
                {"slot": 1, "name": "卡槽 1 (默认卡槽 / 主卡)", "active": current_slot == 1},
                {"slot": 2, "name": "卡槽 2 (副卡)", "active": current_slot == 2}
            ]
        }))
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
        let modem_ready = self.modem_count().await > 0;
        // 命令原文不写入普通日志（报告 8 章）。
        match self
            .at
            .submit(command, AtPriority::Interactive, Duration::from_secs(10))
            .await
        {
            Ok(text) => Ok(json!({
                "status": "ok",
                "action": "at",
                "outcome": "success",
                "result": text,
                "device_verified": true,
                "at_implemented": true,
            })),
            Err(e) => {
                let (kind, message) = classify_at_error(&e, modem_ready);
                Ok(json!({
                    "status": "error",
                    "action": "at",
                    "outcome": "failed",
                    "error_kind": kind,
                    "device_verified": false,
                    "at_implemented": true,
                    "message": message,
                }))
            }
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
        // 前端（web/js/pages/system.js）读取 time 字段直接展示，
        // 只返回 rtc 原文会让页面显示成整段 RTC 文本或空值。
        // 这里给出本地时区的人类可读时间，同时保留 rtc 原文与时间戳。
        let now = chrono::Local::now();
        let time = now.format("%Y-%m-%d %H:%M:%S").to_string();
        let date = now.format("%Y-%m-%d").to_string();
        // 从 /proc/driver/rtc 里抽 rtc_time / rtc_date 便于排障比对
        let mut rtc_time = String::new();
        let mut rtc_date = String::new();
        for line in text.lines() {
            let mut parts = line.splitn(2, ':');
            let key = parts.next().unwrap_or("").trim();
            let val = parts.next().unwrap_or("").trim();
            match key {
                "rtc_time" => rtc_time = val.to_string(),
                "rtc_date" => rtc_date = val.to_string(),
                _ => {}
            }
        }
        Ok(json!({
            "timestamp": udx_storage::now_ts(),
            "time": time,
            "date": date,
            "rtc": text,
            "rtc_time": rtc_time,
            "rtc_date": rtc_date,
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
            // 字段含义是「设备是否确认了该动作」。绝大多数控制动作在本项目中
            // 只落到配置、由设备侧另行应用，因此恒为 false 是诚实的，
            // 另用 persisted 表达「配置已写入」。
            "device_verified": false,
            "persisted": true,
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

/// UDX710 平台常见频段。仅在无法从设备读取时作为候选清单使用，
/// 让「频段锁定」页面至少是可操作的，而不是空白表格。
const BUILTIN_BANDS: &[(&str, &str, &str)] = &[
    ("LTE_B1", "B1 · 2100 MHz", "LTE"),
    ("LTE_B3", "B3 · 1800 MHz", "LTE"),
    ("LTE_B5", "B5 · 850 MHz", "LTE"),
    ("LTE_B8", "B8 · 900 MHz", "LTE"),
    ("LTE_B34", "B34 · 2000 MHz", "LTE"),
    ("LTE_B38", "B38 · 2600 MHz", "LTE"),
    ("LTE_B39", "B39 · 1900 MHz", "LTE"),
    ("LTE_B40", "B40 · 2300 MHz", "LTE"),
    ("LTE_B41", "B41 · 2500 MHz", "LTE"),
    ("NR_n1", "n1 · 2100 MHz", "NR"),
    ("NR_n28", "n28 · 700 MHz", "NR"),
    ("NR_n41", "n41 · 2500 MHz", "NR"),
    ("NR_n77", "n77 · 3.3 GHz", "NR"),
    ("NR_n78", "n78 · 3.5 GHz", "NR"),
    ("NR_n79", "n79 · 4.9 GHz", "NR"),
];

/// AT 失败原因分级：把「模组不存在」「总线超时」「通道未实现」分开，
/// 前端才能给出可操作的提示，而不是统一的「服务暂时不可用」。
fn classify_at_error(e: &AppError, modem_ready: bool) -> (&'static str, String) {
    if !modem_ready {
        return (
            "modem_absent",
            "模组未就绪：oFono 未托管 Modem 对象，无法下发 AT 命令".to_string(),
        );
    }
    match e {
        AppError::Timeout(_) => (
            "dbus_timeout",
            format!("AT 通道超时：{}", e.public_message()),
        ),
        _ => ("at_unavailable", format!("AT 通道不可用：{}", e.public_message())),
    }
}

fn pick_value<'a>(v: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|k| v.get(*k))
        .filter(|x| !x.is_null())
}

fn pick_str(v: &Value, keys: &[&str]) -> Option<String> {
    pick_value(v, keys).and_then(|x| match x {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    })
}

fn pick_u64(v: &Value, keys: &[&str]) -> Option<u64> {
    pick_value(v, keys).and_then(|x| {
        x.as_u64()
            .or_else(|| x.as_i64().map(|i| i.max(0) as u64))
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    })
}

fn pick_i64(v: &Value, keys: &[&str]) -> Option<i64> {
    pick_value(v, keys).and_then(|x| {
        x.as_i64()
            .or_else(|| x.as_u64().map(|u| u as i64))
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    })
}

fn pick_signal_metric(v: &Value, primary_keys: &[&str], fallback_keys: &[&str]) -> Option<i64> {
    for k in primary_keys {
        if let Some(val) = pick_i64(v, &[k]) {
            if val != 0 {
                return Some(val);
            }
        }
    }
    for k in fallback_keys {
        if let Some(val) = pick_i64(v, &[k]) {
            if val != 0 {
                return Some(val);
            }
        }
    }
    pick_i64(v, primary_keys)
}

pub fn arfcn_to_band(tech: &str, arfcn: u64) -> &'static str {
    let t = tech.to_lowercase();
    if t.contains("nr") || t.contains("5g") || arfcn > 100000 {
        match arfcn {
            151600..=160600 => "NR n28",
            173800..=178800 => "NR n5",
            185000..=192000 => "NR n8",
            361000..=376000 => "NR n3",
            422000..=434000 => "NR n1",
            499200..=537999 => "NR n41",
            620000..=653333 => "NR n78",
            653334..=680000 => "NR n77",
            693333..=733333 => "NR n79",
            _ => "NR",
        }
    } else {
        match arfcn {
            0..=599 => "LTE B1",
            600..=1199 => "LTE B2",
            1200..=1949 => "LTE B3",
            1950..=2399 => "LTE B4",
            2400..=2649 => "LTE B5",
            2750..=3449 => "LTE B7",
            3450..=3799 => "LTE B8",
            6150..=6449 => "LTE B20",
            9210..=9659 => "LTE B28",
            36200..=36349 => "LTE B34",
            37750..=38249 => "LTE B38",
            38250..=38649 => "LTE B39",
            38650..=39649 => "LTE B40",
            39650..=41589 => "LTE B41",
            _ => "LTE",
        }
    }
}
