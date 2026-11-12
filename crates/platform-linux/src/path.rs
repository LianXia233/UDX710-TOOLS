//! 路径与写入安全（报告 08 章「脚本上传路径直接拼接用户提供文件名」）。
//!
//! - 文件名 allowlist：仅 `[A-Za-z0-9_.-]`，禁止 `..`、路径分隔符与超长；
//! - 目录约束：组件只能落在指定根目录之下；
//! - 原子写入：临时文件 → rename，避免半写状态；
//! - 落盘文件一律 0o600（非执行），脚本执行权限由独立审批流程决定。

use std::path::{Path, PathBuf};
use udx_domain::error::{bad_request, internal, AppResult};

pub const MAX_COMPONENT_LEN: usize = 96;

/// 校验单个路径组件，返回规范化后的组件。
pub fn safe_component(input: &str) -> AppResult<String> {
    let name = input.trim();
    if name.is_empty() {
        return Err(bad_request("名称不能为空"));
    }
    if name.len() > MAX_COMPONENT_LEN {
        return Err(bad_request("名称过长"));
    }
    if name == "." || name == ".." || name.contains("..") {
        return Err(bad_request("名称包含非法路径片段"));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(bad_request("名称包含非法字符"));
    }
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
    if !name.chars().all(allowed) {
        return Err(bad_request("名称只允许字母、数字、下划线、点、连字符"));
    }
    // 隐藏文件与以点开头的名字容易绕过目录枚举，直接拒绝。
    if name.starts_with('.') {
        return Err(bad_request("名称不能以点开头"));
    }
    Ok(name.to_string())
}

/// 把用户提供的组件安全地拼接到 root 之下，并校验最终父目录等于 root。
pub fn resolve_under(root: &Path, component: &str) -> AppResult<PathBuf> {
    let safe = safe_component(component)?;
    let root = root
        .canonicalize()
        .map_err(|e| internal(format!("根目录不可用: {e}")))?;
    let candidate = root.join(safe);
    if candidate.parent() != Some(root.as_path()) {
        return Err(bad_request("路径越界"));
    }
    Ok(candidate)
}

/// 原子写入：同目录临时文件 → rename；权限 0o600，杜绝执行位。
pub async fn atomic_write(path: &Path, content: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| internal("目标文件缺少父目录"))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|e| internal(format!("创建目录失败: {e}")))?;

    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file"),
        uuid_like_suffix()
    ));

    tokio::fs::write(&tmp, content)
        .await
        .map_err(|e| internal(format!("写入临时文件失败: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|e| internal(format!("设置文件权限失败: {e}")))?;
    }

    tokio::fs::rename(&tmp, path)
        .await
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            internal(format!("原子替换失败: {e}"))
        })?;
    Ok(())
}

fn uuid_like_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}", n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_shell_metacharacters() {
        assert!(safe_component("../etc/passwd").is_err());
        assert!(safe_component("a/b").is_err());
        assert!(safe_component("a;rm -rf").is_err());
        assert!(safe_component("$(id)").is_err());
        assert!(safe_component(".hidden").is_err());
        assert_eq!(safe_component("plugin_v1.2.js").unwrap(), "plugin_v1.2.js");
    }

    #[test]
    fn resolve_stays_inside_root() {
        let dir = std::env::temp_dir().join("udx-path-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = resolve_under(&dir, "ok.txt").unwrap();
        assert_eq!(p.parent(), Some(dir.as_path()));
        assert!(resolve_under(&dir, "../escape").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
