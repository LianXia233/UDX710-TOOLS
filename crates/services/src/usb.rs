//! USB 模式切换状态机（报告 02/06 章）。
//!
//! - 独占锁：切换过程不可并发重入，冲突返回 Busy；
//! - 明确状态机：Idle → Preparing → Applying → Verifying →（失败则 RollingBack）→ Settled；
//! - 只调用固定命令 ID，模式参数只允许三种枚举；
//! - 失败回滚到切换前的模式，并返回明确 outcome。

use crate::tasks::ExclusiveLock;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use udx_domain::error::{bad_request, AppError, AppResult};
use udx_domain::state::{UsbMode, UsbSwitchPhase, UsbSwitchState};
use udx_platform_linux::{CommandRunner, FixedCommand};
use udx_storage::Db;

pub struct UsbService {
    db: Db,
    lock: ExclusiveLock,
    state: Arc<Mutex<UsbSwitchState>>,
    runner: CommandRunner,
}

impl UsbService {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            lock: ExclusiveLock::new(),
            state: Arc::new(Mutex::new(UsbSwitchState::default())),
            runner: CommandRunner::default(),
        }
    }

    pub async fn get_mode(&self) -> AppResult<Value> {
        let cfg = self
            .db
            .config_get("usb.mode")
            .await?
            .unwrap_or(json!({"mode": "unknown", "permanent": false}));
        let snapshot = self.state.lock().await.clone();
        Ok(json!({
            "mode": cfg.get("mode").cloned().unwrap_or(json!("unknown")),
            "permanent": cfg.get("permanent").cloned().unwrap_or(json!(false)),
            "phase": snapshot.phase,
            "last_error": snapshot.last_error,
        }))
    }

    /// 设置 USB 模式（temporary / permanent），走同一状态机。
    pub async fn set_mode(&self, payload: &Value) -> AppResult<Value> {
        let mode_str = payload.get("mode").and_then(|v| v.as_str()).unwrap_or("");
        let mode = UsbMode::parse(mode_str);
        if mode == UsbMode::Unknown {
            return Err(bad_request("USB 模式只能是 ecm / ncm / rndis"));
        }
        let permanent = payload
            .get("permanent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        self.apply(mode, permanent).await
    }

    /// 热切换（立即生效），与 set_mode 共享独占锁。
    pub async fn advance(&self, payload: &Value) -> AppResult<Value> {
        self.set_mode(payload).await
    }

    async fn apply(&self, mode: UsbMode, permanent: bool) -> AppResult<Value> {
        let _guard = self.lock.try_lock()?;

        let current = {
            let mut st = self.state.lock().await;
            if st.phase != UsbSwitchPhase::Idle {
                return Err(AppError::Busy("USB 切换正在进行中".to_string()));
            }
            let from = self
                .db
                .config_get("usb.mode")
                .await?
                .and_then(|v| v.get("mode").and_then(|x| x.as_str()).map(str::to_string))
                .map(|s| UsbMode::parse(&s))
                .unwrap_or(UsbMode::Unknown);
            st.phase = UsbSwitchPhase::Preparing;
            st.from = from;
            st.to = mode;
            st.permanent = permanent;
            st.started_at = udx_storage::now_ts();
            st.last_error = None;
            from
        };

        self.set_phase(UsbSwitchPhase::Applying).await;
        let result = self
            .runner
            .run(FixedCommand::UsbGadgetWrite, &[mode.as_str().to_string()])
            .await;

        let (outcome, detail) = match result {
            Ok(o) if o.exit_code == 0 => {
                self.set_phase(UsbSwitchPhase::Verifying).await;
                (String::from("success"), String::new())
            }
            Ok(o) => (
                String::from("failed"),
                format!("脚本退出码 {}", o.exit_code),
            ),
            Err(AppError::Timeout(_)) => (String::from("timeout"), String::from("切换超时")),
            Err(e) => (String::from("failed"), e.public_message()),
        };

        if outcome == "success" {
            self.db
                .config_set("usb.mode", &json!({"mode": mode.as_str(), "permanent": permanent}))
                .await?;
        } else {
            self.set_phase(UsbSwitchPhase::RollingBack).await;
            if current != UsbMode::Unknown {
                let _ = self
                    .runner
                    .run(
                        FixedCommand::UsbGadgetWrite,
                        &[current.as_str().to_string()],
                    )
                    .await;
            }
        }

        {
            let mut st = self.state.lock().await;
            st.phase = UsbSwitchPhase::Settled;
            st.last_error = if detail.is_empty() { None } else { Some(detail.clone()) };
        }
        self.set_phase(UsbSwitchPhase::Idle).await;

        Ok(json!({
            "status": if outcome == "success" { "ok" } else { "error" },
            "action": "usb_switch",
            "from": current.as_str(),
            "to": mode.as_str(),
            "permanent": permanent,
            "outcome": outcome,
            "device_verified": false,
            "note": if detail.is_empty() {
                "切换命令经固定 argv 执行，真机需验证 UDC 重新枚举结果"
            } else {
                &detail
            },
        }))
    }

    async fn set_phase(&self, phase: UsbSwitchPhase) {
        self.state.lock().await.phase = phase;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_mode_parsing() {
        assert_eq!(UsbMode::parse("rndis"), UsbMode::Rndis);
        assert_eq!(UsbMode::parse("cdc-ncm"), UsbMode::Ncm);
        assert_eq!(UsbMode::parse("evil;reboot"), UsbMode::Unknown);
    }
}
