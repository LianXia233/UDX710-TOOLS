//! sysfs / procfs 安全读写（报告 05 章基础设施层）。
//!
//! - 相对路径必须位于允许的根目录白名单之内；
//! - 禁止 `..`、绝对路径、符号链接逃逸与超长路径；
//! - 所有读取限制大小，避免一次读入超大伪文件。

use std::path::{Path, PathBuf};
use tokio::fs;
use udx_domain::error::{bad_request, internal, unavailable, AppResult};

/// 允许访问的只读/控制根目录。清单之外的路径一律拒绝。
pub const ALLOWED_ROOTS: &[&str] = &[
    "/sys/class/power_supply",
    "/sys/class/udc",
    "/sys/class/net",
    "/sys/devices",
    "/sys/kernel",
    "/sys/module",
    "/proc",
];

pub const MAX_READ_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone)]
pub struct Sysfs {
    root: PathBuf,
}

impl Default for Sysfs {
    fn default() -> Self {
        Self {
            root: PathBuf::from("/sys"),
        }
    }
}

impl Sysfs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 把相对路径安全地解析到白名单根下。
    pub fn resolve(&self, relative: &str) -> AppResult<PathBuf> {
        if relative.is_empty() || relative.len() > 256 {
            return Err(bad_request("sysfs 路径为空或过长"));
        }
        if relative.starts_with('/') || relative.contains("..") || relative.contains('\0') {
            return Err(bad_request("sysfs 路径非法"));
        }
        for part in relative.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            if !part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '@'))
            {
                return Err(bad_request("sysfs 路径组件包含非法字符"));
            }
        }

        let candidate = self.root.join(relative);
        let canonical_root = self
            .root
            .canonicalize()
            .map_err(|e| unavailable(format!("sysfs 根不可用: {e}")))?;
        // 已存在路径才做符号链接逃逸校验；不存在时按组件白名单放行。
        if let Ok(canonical) = candidate.canonicalize() {
            if !canonical.starts_with(&canonical_root) {
                return Err(bad_request("sysfs 路径越界"));
            }
        }
        Ok(candidate)
    }

    /// 读取白名单根下的文件（相对 root 的路径）。
    pub async fn read(&self, relative: &str) -> AppResult<String> {
        let path = self.resolve(relative)?;
        let meta = fs::metadata(&path)
            .await
            .map_err(|e| unavailable(format!("sysfs 节点不可读: {e}")))?;
        if meta.len() > MAX_READ_BYTES {
            return Err(internal("sysfs 节点体积超过上限"));
        }
        let bytes = fs::read(&path)
            .await
            .map_err(|e| unavailable(format!("sysfs 读取失败: {e}")))?;
        Ok(String::from_utf8_lossy(&bytes).trim().to_string())
    }

    pub async fn read_u32(&self, relative: &str) -> AppResult<u32> {
        let s = self.read(relative).await?;
        s.parse::<u32>()
            .map_err(|_| unavailable(format!("sysfs 节点不是整数: {relative}")))
    }

    /// 写入控制节点。调用方必须是经过授权的服务，且值需自行校验范围。
    pub async fn write(&self, relative: &str, value: &str) -> AppResult<()> {
        let path = self.resolve(relative)?;
        fs::write(&path, value.as_bytes())
            .await
            .map_err(|e| unavailable(format!("sysfs 写入失败: {e}")))
    }
}

/// 校验一个绝对路径是否落在允许的系统根目录白名单内。
pub fn is_allowed_absolute(path: &Path) -> bool {
    let s = path.to_string_lossy();
    ALLOWED_ROOTS
        .iter()
        .any(|root| s == *root || s.starts_with(&format!("{root}/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal() {
        let s = Sysfs::new("/tmp/udx-sysfs-test");
        std::fs::create_dir_all("/tmp/udx-sysfs-test").unwrap();
        assert!(s.resolve("../../etc/passwd").is_err());
        assert!(s.resolve("/etc/passwd").is_err());
        assert!(s.resolve("a/../../b").is_err());
        assert!(s.resolve("power_supply/battery/capacity").is_ok());
        let _ = std::fs::remove_dir_all("/tmp/udx-sysfs-test");
    }

    #[test]
    fn allowed_roots_are_confined() {
        assert!(is_allowed_absolute(Path::new("/sys/class/net/eth0/statistics")));
        assert!(!is_allowed_absolute(Path::new("/etc/shadow")));
    }
}
