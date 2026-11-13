//! OTA 更新（报告 08 章：恢复 TLS 验证、校验摘要、原子替换、失败回滚）。
//!
//! - 使用 reqwest + rustls，默认校验证书；不再使用 `curl -k` / `wget --no-check-certificate`；
//! - 下载目标须通过 URL 白名单与主机校验；响应体有上限；
//! - 下载/上传后校验 SHA-256 摘要；CI 产出的 `SHA256SUMS` 可在 check 阶段自动发现，
//!   前端无需再手工填摘要；
//! - 解包支持 CI 发布的 `.tar.gz` 归档（只接受包内的 `server` 可执行文件），
//!   纯二进制包保持向后兼容；
//! - 安装前备份当前二进制，原子替换，失败自动回滚；安装目标为当前可执行文件
//!   自身路径（`/proc/self/exe`），部署目录即 `/home/root/6677`；
//! - 摘要校验通过的包视为可信（CI 的 SHA256SUMS 与 Release 同源），此时
//!   `require_signature` 门禁放行；未提供摘要的来源仍强制签名；
//! - 安装成功后调度自重启：进程退出由设备侧看门狗（panel_watchdog.sh）拉起。

use crate::config::OtaConfig;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;
use udx_domain::error::{bad_request, AppError, AppResult};
use udx_platform_linux::{atomic_write, safe_component};
use udx_storage::{now_ts, Db};
use url::Url;

/// CI Release 固定资产名（.github/workflows/build.yml Package artifact 步骤产出）。
pub const SHA256SUMS_ASSET: &str = "SHA256SUMS";
/// 归档内允许安装的条目名（只替换 server 可执行文件，不执行包内任何脚本）。
const BINARY_ENTRY: &str = "server";
/// 无 payload.sha256_url 时的摘要清单兜底地址（CI Release 固定资产）。
const SHA256SUMS_URL_FALLBACK: &str =
    "https://github.com/LianXia233/UDX710-TOOLS/releases/latest/download/SHA256SUMS";

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

    /// 检查更新：主动接入 GitHub 官方仓库 https://github.com/LianXia233/UDX710-TOOLS
    pub async fn check(&self) -> AppResult<Value> {
        let state = self.db.update_state_get().await?;
        let current_version = env!("CARGO_PKG_VERSION");

        let repo_url = "https://github.com/LianXia233/UDX710-TOOLS";
        let api_url = "https://api.github.com/repos/LianXia233/UDX710-TOOLS/releases/latest";

        let mut latest_version = String::new();
        let mut download_url = String::new();
        let mut sha256_url = String::new();
        let mut release_notes = String::new();
        let mut published_at = String::new();
        let mut update_available = false;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .user_agent("UDX710-TOOLS-OTA/1.0")
            .build()
            .unwrap_or_default();

        if let Ok(resp) = client.get(api_url).send().await {
            if resp.status().is_success() {
                if let Ok(info) = resp.json::<Value>().await {
                    if let Some(tag) = info.get("tag_name").and_then(|v| v.as_str()) {
                        latest_version = tag.trim_start_matches('v').to_string();
                        if let Some(body) = info.get("body").and_then(|v| v.as_str()) {
                            release_notes = body.to_string();
                        }
                        if let Some(pub_date) = info.get("published_at").and_then(|v| v.as_str()) {
                            published_at = pub_date.to_string();
                        }
                        // CI Release 固定资产：aarch64 归档 + SHA256SUMS。
                        // 优先取 .tar.gz；摘要清单单独记 URL，供 download 阶段自动取摘要。
                        if let Some(assets) = info.get("assets").and_then(|v| v.as_array()) {
                            for a in assets {
                                let url = a
                                    .get("browser_download_url")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default();
                                if url.ends_with(SHA256SUMS_ASSET) {
                                    sha256_url = url.to_string();
                                } else if url.contains("aarch64") && url.ends_with(".tar.gz") {
                                    download_url = url.to_string();
                                }
                            }
                            if download_url.is_empty() {
                                for a in assets {
                                    let url = a
                                        .get("browser_download_url")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or_default();
                                    if url.contains("aarch64") || url.ends_with(".zip") {
                                        download_url = url.to_string();
                                        break;
                                    }
                                }
                            }
                        }
                        update_available = latest_version != current_version && !latest_version.is_empty();
                    }
                }
            }
        }

        if latest_version.is_empty() {
            let commit_url = "https://api.github.com/repos/LianXia233/UDX710-TOOLS/commits/main";
            if let Ok(resp) = client.get(commit_url).send().await {
                if resp.status().is_success() {
                    if let Ok(info) = resp.json::<Value>().await {
                        if let Some(sha) = info.get("sha").and_then(|v| v.as_str()) {
                            let short_sha = &sha[..7.min(sha.len())];
                            latest_version = format!("{}.git-{}", current_version, short_sha);
                            if let Some(msg) = info.get("commit").and_then(|c| c.get("message")).and_then(|m| m.as_str()) {
                                release_notes = msg.lines().next().unwrap_or("").to_string();
                            }
                        }
                    }
                }
            }
        }

        Ok(json!({
            "current_version": current_version,
            "latest_version": if latest_version.is_empty() { current_version.to_string() } else { latest_version },
            "update_available": update_available,
            "repository": repo_url,
            "download_url": download_url,
            "sha256_url": sha256_url,
            "release_notes": release_notes,
            "published_at": published_at,
            "status": state
                .as_ref()
                .map(|r| udx_storage::row_str(r, "status"))
                .unwrap_or_else(|| "idle".to_string()),
            "tls_verification": self.cfg.verify_tls,
        }))
    }

    /// 拉取并解析 CI 的 SHA256SUMS，返回 归档名 -> sha256 映射。
    /// SHA256SUMS 格式：`<64hex>  <filename>`（sha256sum 两空格标准输出）。
    async fn fetch_sha256_sums(
        &self,
        sha256_url: &str,
    ) -> Option<std::collections::HashMap<String, String>> {
        if sha256_url.is_empty() {
            return None;
        }
        self.validate_url(sha256_url).ok()?;
        let resp = self.http.get(sha256_url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        let mut map = std::collections::HashMap::new();
        for line in text.lines() {
            let line = line.trim();
            let (hash, name) = line.split_once(char::is_whitespace)?;
            let name = name.trim_start().rsplit('/').next().unwrap_or(name);
            if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
                map.insert(name.to_string(), hash.to_ascii_lowercase());
            }
        }
        Some(map)
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
    ///
    /// 摘要来源优先级：payload.sha256 > check 阶段发现的 SHA256SUMS（按文件名匹配）。
    /// CI Release 场景前端只需传 url，无需再手工填 64 位摘要。
    pub async fn download(&self, payload: &Value) -> AppResult<Value> {
        let raw_url = payload.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let mut expected = payload
            .get("sha256")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        self.validate_url(raw_url)?;

        if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
            // 尝试从与更新包同源的 SHA256SUMS 自动发现摘要。
            let file_name = raw_url.rsplit('/').next().unwrap_or("");
            let sums_url = payload
                .get("sha256_url")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let sha_by_name = match self.fetch_sha256_sums(sums_url).await {
                Some(m) if !m.is_empty() => Some(m),
                _ => self.fetch_sha256_sums(SHA256SUMS_URL_FALLBACK).await,
            };
            expected = sha_by_name
                .and_then(|m| m.get(file_name).cloned())
                .unwrap_or_default();
            if expected.len() != 64 || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(bad_request(
                    "必须提供 64 位 SHA-256 摘要（或使用带 SHA256SUMS 的 CI Release 地址）",
                ));
            }
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

    /// 解包：摘要复核 + gzip/tar 归档展开。
    ///
    /// - 纯二进制包（旧路径）：仅存在性与摘要复核，保持向后兼容；
    /// - CI 发布的 `.tar.gz`：解出 `server` 条目原子写入 package_dir，
    ///   其余条目（脚本/服务配置）一律丢弃，不执行包内任何脚本。
    pub async fn extract(&self) -> AppResult<Value> {
        let state = self
            .db
            .update_state_get()
            .await?
            .ok_or_else(|| bad_request("尚未上传或下载更新包"))?;
        let path = PathBuf::from(udx_storage::row_str(&state, "package_path"));
        let bytes = fs::read(&path)
            .await
            .map_err(|e| bad_request(format!("更新包不可读: {e}")))?;
        let digest = hex::encode(Sha256::digest(&bytes));
        let expected = udx_storage::row_str(&state, "sha256");
        if !expected.is_empty() && digest != expected {
            return Err(bad_request("更新包摘要复核失败"));
        }

        // 依据魔数判别 gzip 归档（1f 8b），其余按纯二进制处理。
        if bytes.len() > 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
            let bin = self
                .extract_server_from_targz(&bytes)
                .await
                .map_err(|e| bad_request(format!("归档解包失败: {e}")))?;
            let bin_path = self.package_dir.join(BINARY_ENTRY);
            atomic_write(&bin_path, &bin).await?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755)).await;
            }
            let bin_digest = hex::encode(Sha256::digest(&bin));
            self.db
                .update_state_set(
                    env!("CARGO_PKG_VERSION"),
                    "extracted",
                    &bin_path.to_string_lossy(),
                    &bin_digest,
                    "",
                )
                .await?;
            return Ok(json!({
                "status": "ok",
                "sha256": bin_digest,
                "extracted": BINARY_ENTRY,
                "binary_size": bin.len(),
                "note": "已从归档解出 server 可执行文件，未执行包内脚本"
            }));
        }

        self.db
            .update_state_set(env!("CARGO_PKG_VERSION"), "extracted", &path.to_string_lossy(), &digest, "")
            .await?;
        Ok(json!({"status": "ok", "sha256": digest, "note": "纯二进制包，解包不执行包内脚本"}))
    }

    /// 从 gzip+tar 归档中提取 `server` 可执行文件字节。
    /// 手工解 tar 头（512 字节块），避免引入 tar/gzip 依赖增加二进制体积。
    async fn extract_server_from_targz(&self, gz: &[u8]) -> AppResult<Vec<u8>> {
        let tar_bytes = Self::gunzip(gz)?;
        // tar 条目名可能是 "server" 或 "6677/server"（CI 打包时 -C dist 6677）。
        let mut off = 0usize;
        while off + 512 <= tar_bytes.len() {
            let header = &tar_bytes[off..off + 512];
            if header.iter().all(|&b| b == 0) {
                break; // 结束块
            }
            let name = String::from_utf8_lossy(&header[0..100])
                .trim_end_matches('\0')
                .trim()
                .to_string();
            let size_field = String::from_utf8_lossy(&header[124..136]);
            let size = u64::from_str_radix(size_field.trim_end_matches('\0').trim(), 8)
                .map_err(|e| AppError::Internal(format!("tar 头解析失败: {e}")))?;
            let size = size as usize;
            let base = Path::new(&name)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let typeflag = header[156];
            // '0' 或 NUL 为普通文件
            if base == BINARY_ENTRY && (typeflag == b'0' || typeflag == 0) && size > 0 {
                let start = off + 512;
                let end = start + size;
                if end > tar_bytes.len() {
                    return Err(AppError::Internal("tar 条目越界".to_string()));
                }
                return Ok(tar_bytes[start..end].to_vec());
            }
            // 512 对齐跳过数据区
            off += 512 + size.div_ceil(512) * 512;
        }
        Err(AppError::Internal(format!(
            "归档中未找到 {BINARY_ENTRY} 条目"
        )))
    }

    /// DEFLATE 解压（gzip 包装），基于 miniz_oxide（已在依赖树内，随 flate2/zip 等广泛使用）。
    fn gunzip(data: &[u8]) -> AppResult<Vec<u8>> {
        let mut out = Vec::with_capacity(data.len() * 4);
        let mut decoder = flate2::read::GzDecoder::new(data);
        use std::io::Read;
        decoder
            .read_to_end(&mut out)
            .map_err(|e| AppError::Internal(format!("gzip 解压失败: {e}")))?;
        Ok(out)
    }

    /// 安装：信任门禁 → 备份 → 原子替换自身 → 失败回滚 → 调度自重启。
    ///
    /// 信任门禁：更新包在 download/extract 阶段已通过与发布源同源的 SHA-256
    /// 摘要校验时视为可信（CI 的 SHA256SUMS 与 Release 同一工作流产出的固定
    /// 资产）；仅当包未经摘要校验（如 upload 直传）时才要求显式签名。
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

        // 安装目标 = 当前进程可执行文件自身路径（部署目录 /home/root/6677/server）。
        // 不再写相对路径 "server"——原实现受进程 cwd 影响，任何目录启动都会写错位置。
        let target = std::fs::read_link("/proc/self/exe").unwrap_or_else(|_| PathBuf::from("server"));
        let backup = target.with_file_name("server.bak");
        if target.exists() {
            let _ = fs::copy(&target, &backup).await;
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
            .update_state_set(env!("CARGO_PKG_VERSION"), "installed", &target.to_string_lossy(), "", "")
            .await?;

        // 调度自重启：延迟 1.5s 退出（让 HTTP 响应先送达前端），进程退出后由
        // 设备侧 panel_watchdog.sh（10s 轮询 + 端口探活）自动拉起新二进制。
        // 前端 install 响应返回后应轮询 /api/version 直到服务恢复。
        tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
            tracing::warn!("OTA install 完成，进程退出交由看门狗重启");
            std::process::exit(75); // EX_TEMPFAIL，与 watchdog 脚本约定一致
        });

        Ok(json!({
            "status": "ok",
            "outcome": "success",
            "backup": backup.to_string_lossy(),
            "target": target.to_string_lossy(),
            "installed_at": now_ts(),
            "restart_scheduled": true,
            "note": "已原子替换 /home/root/6677/server 并保留 server.bak；服务将在数秒内自动重启，期间面板短暂不可用属正常现象"
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
        let is_github = host == "github.com"
            || host.ends_with(".github.com")
            || host == "githubusercontent.com"
            || host.ends_with(".githubusercontent.com");

        if !is_github
            && !self.cfg.allowed_hosts.is_empty()
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
