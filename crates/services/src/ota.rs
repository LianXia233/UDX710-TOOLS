//! OTA 更新（报告 08 章：恢复 TLS 验证、校验摘要、原子替换、失败回滚）。
//!
//! - 使用 reqwest + rustls，默认校验证书；不再使用 `curl -k` / `wget --no-check-certificate`；
//! - 下载目标须通过 URL 白名单与主机校验；响应体有上限；
//! - 下载/上传后校验 SHA-256 摘要；
//! - 安装前备份当前二进制，原子替换，失败自动回滚；
//! - 签名校验为强制开关：未配置签名时拒绝安装（真机需接入设备公钥验签）。

use crate::config::OtaConfig;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;
use udx_domain::error::{bad_request, AppError, AppResult};
use udx_platform_linux::{atomic_write, safe_component};
use udx_storage::{now_ts, Db};
use url::Url;

pub struct OtaService {
    db: Db,
    cfg: OtaConfig,
    http: reqwest::Client,
    package_dir: PathBuf,
}

impl OtaService {
    pub fn new(db: Db, cfg: OtaConfig, package_dir: impl Into<PathBuf>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(cfg.download_timeout_secs))
            .connect_timeout(std::time::Duration::from_secs(10))
            // rustls 默认验证证书；这里显式声明不使用危险开关。
            .danger_accept_invalid_certs(false)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            db,
            cfg,
            http,
            package_dir: package_dir.into(),
        }
    }

    pub async fn version(&self) -> AppResult<Value> {
        let state = self.db.update_state_get().await?;
        Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "state": state
                .as_ref()
                .map(|r| udx_storage::row_str(r, "status"))
                .unwrap_or_default(),
        }))
    }

    /// 检查更新：不主动访问不可信源，只回报本地状态与已登记的包信息。
    pub async fn check(&self) -> AppResult<Value> {
        let state = self.db.update_state_get().await?;
        Ok(json!({
            "current_version": env!("CARGO_PKG_VERSION"),
            "update_available": state
                .as_ref()
                .map(|r| !udx_storage::row_str(r, "package_path").is_empty())
                .unwrap_or(false),
            "status": state
                .as_ref()
                .map(|r| udx_storage::row_str(r, "status"))
                .unwrap_or_else(|| "idle".to_string()),
            "tls_verification": self.cfg.verify_tls,
        }))
    }

    /// 上传更新包。文件名经 allowlist，内容以非执行方式原子写入。
    pub async fn upload(&self, payload: &Value) -> AppResult<Value> {
        let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("update.bin");
        let safe = safe_component(name)?;
        let content = payload
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad_request("缺少 content"))?;
        if content.len() > self.cfg.max_package_bytes {
            return Err(AppError::TooLarge("更新包超过大小上限".to_string()));
        }
        let bytes = content.as_bytes();
        let digest = hex::encode(Sha256::digest(bytes));
        let path = self.package_dir.join(&safe);
        atomic_write(&path, bytes).await?;

        self.db
            .update_state_set(
                env!("CARGO_PKG_VERSION"),
                "uploaded",
                &path.to_string_lossy(),
                &digest,
                "",
            )
            .await?;
        Ok(json!({
            "status": "ok",
            "name": safe,
            "size": bytes.len(),
            "sha256": digest,
            "executable": false,
        }))
    }

    /// 从受控 URL 下载更新包：TLS 验证 + 主机白名单 + 体积上限 + 摘要校验。
    pub async fn download(&self, payload: &Value) -> AppResult<Value> {
        let raw_url = payload.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let expected = payload.get("sha256").and_then(|v| v.as_str()).unwrap_or("");
        self.validate_url(raw_url)?;
        if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(bad_request("必须提供 64 位 SHA-256 摘要"));
        }

        let mut resp = self
            .http
            .get(raw_url)
            .send()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("更新包下载失败: {e}")))?;
        if !resp.status().is_success() {
            return Err(AppError::ServiceUnavailable(format!(
                "更新包服务器返回 {}",
                resp.status()
            )));
        }
        if let Some(len) = resp.content_length() {
            if len > self.cfg.max_package_bytes as u64 {
                return Err(AppError::TooLarge("更新包超过大小上限".to_string()));
            }
        }

        let mut buf = Vec::new();
        let mut hasher = Sha256::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| AppError::ServiceUnavailable(format!("下载流中断: {e}")))?
        {
            if buf.len() + chunk.len() > self.cfg.max_package_bytes {
                return Err(AppError::TooLarge("更新包超过大小上限".to_string()));
            }
            hasher.update(&chunk);
            buf.extend_from_slice(&chunk);
        }
        let digest = hex::encode(hasher.finalize());
        if digest != expected {
            self.db
                .update_state_set(env!("CARGO_PKG_VERSION"), "checksum_failed", "", &digest, "摘要不匹配")
                .await?;
            return Err(bad_request("更新包摘要校验失败，已拒绝写入"));
        }

        let name = safe_component(
            payload.get("name").and_then(|v| v.as_str()).unwrap_or("update.bin"),
        )?;
        let path = self.package_dir.join(&name);
        atomic_write(&path, &buf).await?;
        self.db
            .update_state_set(env!("CARGO_PKG_VERSION"), "downloaded", &path.to_string_lossy(), &digest, "")
            .await?;
        Ok(json!({"status": "ok", "size": buf.len(), "sha256": digest}))
    }

    /// 解包：当前实现只做存在性与摘要复核，不执行包内脚本。
    pub async fn extract(&self) -> AppResult<Value> {
        let state = self
            .db
            .update_state_get()
            .await?
            .ok_or_else(|| bad_request("尚未上传或下载更新包"))?;
        let path = udx_storage::row_str(&state, "package_path");
        let bytes = fs::read(&path)
            .await
            .map_err(|e| bad_request(format!("更新包不可读: {e}")))?;
        let digest = hex::encode(Sha256::digest(&bytes));
        let expected = udx_storage::row_str(&state, "sha256");
        if !expected.is_empty() && digest != expected {
            return Err(bad_request("更新包摘要复核失败"));
        }
        self.db
            .update_state_set(env!("CARGO_PKG_VERSION"), "extracted", &path, &digest, "")
            .await?;
        Ok(json!({"status": "ok", "sha256": digest, "note": "解包不执行包内脚本"}))
    }

    /// 安装：签名门禁 → 备份 → 原子替换 → 失败回滚。
    pub async fn install(&self, payload: &Value) -> AppResult<Value> {
        if self.cfg.require_signature {
            let sig = payload.get("signature").and_then(|v| v.as_str()).unwrap_or("");
            if sig.len() < 16 {
                return Err(bad_request("缺少更新包签名，拒绝安装"));
            }
            // 真机需接入设备公钥做真实验签；此处为强制门禁，不放行无签名的包。
        }
        let state = self
            .db
            .update_state_get()
            .await?
            .ok_or_else(|| bad_request("尚未准备更新包"))?;
        let package = PathBuf::from(udx_storage::row_str(&state, "package_path"));
        let package = self.confine_to_package_dir(&package)?;
        let bytes = fs::read(&package)
            .await
            .map_err(|e| bad_request(format!("更新包不可读: {e}")))?;

        let target = PathBuf::from("server");
        let backup = PathBuf::from("server.bak");
        if target.exists() {
            fs::copy(&target, &backup)
                .await
                .map_err(|e| AppError::Internal(format!("备份当前版本失败: {e}")))?;
        }

        if let Err(e) = atomic_write(&target, &bytes).await {
            if backup.exists() {
                let _ = fs::rename(&backup, &target).await;
            }
            self.db
                .update_state_set(env!("CARGO_PKG_VERSION"), "install_failed", "", "", "写入失败已回滚")
                .await?;
            return Err(e);
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).await;
        }

        self.db
            .update_state_set(env!("CARGO_PKG_VERSION"), "installed", "", "", "")
            .await?;
        Ok(json!({
            "status": "ok",
            "outcome": "success",
            "backup": backup.to_string_lossy(),
            "installed_at": now_ts(),
            "note": "已原子替换并保留 server.bak，可手动回滚"
        }))
    }

    fn validate_url(&self, raw: &str) -> AppResult<()> {
        let parsed = Url::parse(raw).map_err(|_| bad_request("更新 URL 非法"))?;
        if self.cfg.verify_tls && parsed.scheme() != "https" {
            return Err(bad_request("启用证书验证时只允许 HTTPS 更新源"));
        }
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(bad_request("更新 URL 协议非法"));
        }
        if parsed.username() != "" || parsed.password().is_some() {
            return Err(bad_request("更新 URL 不得包含凭据"));
        }
        let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
        if host.is_empty()
            || host == "localhost"
            || host.starts_with("127.")
            || host.starts_with("169.254.")
        {
            return Err(bad_request("更新源主机被禁止"));
        }
        if !self.cfg.allowed_hosts.is_empty()
            && !self.cfg.allowed_hosts.iter().any(|h| h.eq_ignore_ascii_case(&host))
        {
            return Err(bad_request("更新源不在主机白名单内"));
        }
        Ok(())
    }

    fn confine_to_package_dir(&self, p: &Path) -> AppResult<PathBuf> {
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| bad_request("更新包路径非法"))?;
        let safe = safe_component(name)?;
        Ok(self.package_dir.join(safe))
    }
}
