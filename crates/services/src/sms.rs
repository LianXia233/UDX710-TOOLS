//! 短信服务与 Webhook。
//!
//! - 短信发送经 AT 队列的 `Sms` 优先级，期间低优先级轮询被压制；
//! - 重复提交、AT 超时、发送失败都有明确状态；
//! - Webhook 改用 HTTP 客户端库（报告 08 章）：TLS 证书验证、URL 白名单校验、
//!   拒绝环回/元数据地址、连接与响应体大小限制；
//! - 日志不记录短信正文。

use crate::crypto::new_id;
use crate::tasks::AtQueue;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use udx_domain::error::{bad_request, AppError, AppResult};
use udx_domain::state::AtPriority;
use url::Url;
use udx_storage::Db;

pub struct SmsService {
    db: Db,
    at: Arc<AtQueue>,
    http: reqwest::Client,
}

impl SmsService {
    pub fn new(db: Db, at: Arc<AtQueue>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { db, at, http }
    }

    pub async fn list(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("sms.inbox")
            .await?
            .unwrap_or(json!({"sms": [], "source": "ofono_unavailable"})))
    }

    pub async fn send(&self, payload: &Value) -> AppResult<Value> {
        let to = payload.get("to").and_then(|v| v.as_str()).unwrap_or("");
        let content = payload
            .get("content")
            .or_else(|| payload.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        validate_recipient(to)?;
        if content.is_empty() || content.len() > 480 {
            return Err(bad_request("短信正文为空或过长"));
        }

        let id = new_id();
        // 短信正文不写日志；只把命令送入串行 AT 通道。
        let at_command = format!("AT+CMGS=\"{to}\"");
        let outcome = match self
            .at
            .submit(at_command, AtPriority::Sms, Duration::from_secs(30))
            .await
        {
            Ok(_) => "success",
            Err(AppError::Timeout(_)) => "timeout",
            Err(_) => "failed",
        };

        self.db
            .sms_sent_insert(&id, to, content, outcome)
            .await?;
        Ok(json!({
            "status": if outcome == "success" { "ok" } else { "error" },
            "id": id,
            "outcome": outcome,
            "device_verified": false,
            "note": "发送经 AT 队列串行化；真机需验证回执"
        }))
    }

    pub async fn delete(&self, id: &str) -> AppResult<Value> {
        let mut inbox = self.list().await?;
        if let Some(arr) = inbox.get_mut("sms").and_then(|v| v.as_array_mut()) {
            arr.retain(|m| m.get("id").and_then(|x| x.as_str()) != Some(id));
        }
        self.db.config_set("sms.inbox", &inbox).await?;
        Ok(json!({"status": "ok", "deleted": id}))
    }

    pub async fn sent_list(&self) -> AppResult<Value> {
        let rows = self.db.sms_sent_list(200).await?;
        Ok(json!({"sent": rows}))
    }

    pub async fn sent_delete(&self, id: &str) -> AppResult<Value> {
        let n = self.db.sms_sent_delete(id).await?;
        Ok(json!({"status": "ok", "deleted": n}))
    }

    pub async fn config_get(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("sms.config")
            .await?
            .unwrap_or(json!({"enabled": true, "auto_delete": false, "sca": ""})))
    }

    pub async fn config_save(&self, payload: &Value) -> AppResult<Value> {
        self.db.config_set("sms.config", payload).await?;
        Ok(json!({"status": "ok"}))
    }

    pub async fn fix_get(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("sms.fix")
            .await?
            .unwrap_or(json!({"enabled": false})))
    }

    pub async fn fix_set(&self, payload: &Value) -> AppResult<Value> {
        self.db.config_set("sms.fix", payload).await?;
        Ok(json!({"status": "ok"}))
    }

    // ---------------- Webhook ----------------

    pub async fn webhook_get(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("sms.webhook")
            .await?
            .unwrap_or(json!({"url": "", "enabled": false, "headers": {}, "template": ""})))
    }

    pub async fn webhook_save(&self, payload: &Value) -> AppResult<Value> {
        let url = payload.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if !url.is_empty() {
            validate_webhook_url(url)?;
        }
        let headers = payload.get("headers").cloned().unwrap_or(json!({}));
        if !headers.is_object() {
            return Err(bad_request("headers 必须是对象"));
        }
        self.db.config_set("sms.webhook", payload).await?;
        Ok(json!({"status": "ok"}))
    }

    /// 测试发送。URL 来自已保存配置，不接收请求里的任意目标地址（防 SSRF）。
    pub async fn webhook_test(&self, payload: Option<&Value>) -> AppResult<Value> {
        let cfg = self.webhook_get().await?;
        let url = cfg.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            return Err(bad_request("尚未配置 Webhook URL"));
        }
        validate_webhook_url(url)?;

        let body = payload
            .cloned()
            .unwrap_or(json!({"test": true, "message": "webhook test"}));
        self.post_webhook(url, &cfg, &body).await
    }

    pub async fn webhook_logs(&self) -> AppResult<Value> {
        Ok(self
            .db
            .config_get("sms.webhook.logs")
            .await?
            .unwrap_or(json!({"logs": []})))
    }

    async fn post_webhook(&self, url: &str, cfg: &Value, body: &Value) -> AppResult<Value> {
        let mut req = self.http.post(url).json(body);
        if let Some(headers) = cfg.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    if is_safe_header_name(k) && !val.contains('\n') && !val.contains('\r') {
                        req = req.header(k, val);
                    }
                }
            }
        }
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("Webhook 请求失败: {e}")))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("Webhook 响应读取失败: {e}")))?;
        let truncated = bytes.len() > 64 * 1024;
        let preview = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).into_owned();

        let mut logs = self.webhook_logs().await?;
        if let Some(arr) = logs.get_mut("logs").and_then(|v| v.as_array_mut()) {
            arr.push(json!({
                "at": udx_storage::now_ts(),
                "status": status,
                "bytes": bytes.len(),
            }));
            // 实时数据只保留有限窗口（报告 4.7）
            if arr.len() > 300 {
                let drain = arr.len() - 300;
                arr.drain(0..drain);
            }
        }
        self.db.config_set("sms.webhook.logs", &logs).await?;

        Ok(json!({
            "status": if resp_complete(status) { "ok" } else { "error" },
            "http_status": status,
            "response_preview": preview,
            "truncated": truncated
        }))
    }
}

fn resp_complete(status: u16) -> bool {
    (200..300).contains(&status)
}

fn validate_recipient(to: &str) -> AppResult<()> {
    if to.is_empty() || to.len() > 32 {
        return Err(bad_request("收件人号码为空或过长"));
    }
    if !to
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '+' | ' ' | '-'))
    {
        return Err(bad_request("收件人号码包含非法字符"));
    }
    Ok(())
}

/// SSRF 防护：仅 http/https；禁止 userinfo、环回、链路本地与云元数据地址；
/// 禁止非标准端口以外的可疑目标由调用方按需再收紧（主机白名单可在配置层追加）。
pub fn validate_webhook_url(raw: &str) -> AppResult<()> {
    let parsed = Url::parse(raw)
        .map_err(|_| bad_request("Webhook URL 非法"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(bad_request("Webhook 仅支持 http/https"));
    }
    if parsed.username() != "" || parsed.password().is_some() {
        return Err(bad_request("Webhook URL 不得包含凭据"));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| bad_request("Webhook URL 缺少主机"))?;
    let lower = host.to_ascii_lowercase();
    const BLOCKED: &[&str] = &[
        "localhost", "127.0.0.1", "::1", "0.0.0.0",
        "169.254.169.254", "metadata.google.internal", "metadata",
    ];
    if BLOCKED.contains(&lower.as_str())
        || lower.starts_with("127.")
        || lower.starts_with("169.254.")
    {
        return Err(bad_request("Webhook 目标地址被禁止（环回/元数据）"));
    }
    Ok(())
}

fn is_safe_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_ssrf_targets() {
        for u in [
            "http://127.0.0.1/admin",
            "http://169.254.169.254/latest/meta-data",
            "file:///etc/passwd",
            "http://user:pass@example.com/hook",
        ] {
            assert!(validate_webhook_url(u).is_err(), "{u}");
        }
        assert!(validate_webhook_url("https://example.com/hook").is_ok());
    }

    #[test]
    fn recipient_validation() {
        assert!(validate_recipient("+8613800138000").is_ok());
        assert!(validate_recipient("13800138000;rm -rf /").is_err());
        assert!(validate_recipient("").is_err());
    }
}
