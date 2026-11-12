//! 插件系统：受限 JSON 协议（报告 07 章）。
//!
//! - 插件拆成 manifest / ui / permissions / data 四部分；
//! - 宿主渲染器只解释有限 view 类型，绝不执行任意 JS / HTML；
//! - 权限在安装与调用时双重校验，默认拒绝，未知权限一律拒绝；
//! - 数据按插件 ID 隔离并有配额；脚本以非执行方式存储；
//! - 旧版 Vue 模板插件不再执行脚本，进入 `legacy_disabled` 兼容状态。

use crate::crypto::new_id;
use serde_json::{json, Value};
use udx_domain::error::{bad_request, internal, AppError, AppResult};
use udx_domain::models::{
    PluginManifest, PluginPermissionDoc, PluginUiSpec, PluginViewNode, SUPPORTED_PLUGIN_ACTIONS,
    SUPPORTED_VIEW_TYPES,
};
use udx_domain::permissions::perms;
use udx_platform_linux::safe_component;
use udx_storage::{Db, PLUGIN_STORAGE_QUOTA};

const MAX_VIEW_DEPTH: usize = 16;
const MAX_VIEW_NODES: usize = 256;

#[derive(Clone)]
pub struct PluginService {
    db: Db,
}

impl PluginService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> AppResult<Value> {
        let rows = self.db.plugin_list().await?;
        Ok(json!({"plugins": rows, "protocol_version": 1}))
    }

    /// 安装插件：解析并校验 manifest / ui / permissions。
    pub async fn install(&self, payload: &Value) -> AppResult<Value> {
        // 旧版 Vue 模板插件：明确拒绝执行任意 JS，给出迁移指引。
        if payload.get("template").is_some() || payload.get("js").is_some() {
            return Err(bad_request(
                "旧版 Vue 模板/JS 插件不再执行任意脚本，请按 docs/PLUGIN_PROTOCOL.md 转换为 JSON UI",
            ));
        }

        let manifest: PluginManifest = serde_json::from_value(
            payload.get("manifest").cloned().unwrap_or(json!({})),
        )
        .map_err(|e| bad_request(format!("manifest 非法: {e}")))?;
        let ui: PluginUiSpec = serde_json::from_value(
            payload.get("ui").cloned().unwrap_or(json!({})),
        )
        .map_err(|e| bad_request(format!("ui.json 非法: {e}")))?;
        let permission_doc: PluginPermissionDoc = serde_json::from_value(
            payload.get("permissions").cloned().unwrap_or(json!({})),
        )
        .map_err(|e| bad_request(format!("permissions.json 非法: {e}")))?;

        validate_manifest(&manifest)?;
        validate_ui(&ui)?;
        validate_permission_doc(&permission_doc)?;
        validate_permission_consistency(&ui, &permission_doc)?;

        let ui_value = serde_json::to_value(&ui)
            .map_err(|e| internal(format!("UI 序列化失败: {e}")))?;
        let declared = json!(permission_doc.permissions);
        self.db
            .plugin_insert(
                &manifest.id,
                &manifest.name,
                &manifest.version,
                true,
                Some(&ui_value),
                &declared,
            )
            .await?;

        Ok(json!({
            "status": "ok",
            "id": manifest.id,
            "compatibility": "json_ui_v1",
            "permissions": permission_doc.permissions,
        }))
    }

    pub async fn delete(&self, id: &str) -> AppResult<Value> {
        validate_plugin_id(id)?;
        let n = self.db.plugin_delete(id).await?;
        Ok(json!({"status": "ok", "deleted": n}))
    }

    pub async fn delete_all(&self) -> AppResult<Value> {
        self.db.plugin_delete_all().await?;
        Ok(json!({"status": "ok", "cleared": true}))
    }

    /// 插件数据按 ID 隔离；配额由存储层强制。
    pub async fn storage_get(&self, id: &str) -> AppResult<Value> {
        validate_plugin_id(id)?;
        let data = self.db.plugin_data_get(id, "state").await?;
        Ok(data.unwrap_or(json!({})))
    }

    pub async fn storage_set(&self, id: &str, payload: &Value) -> AppResult<Value> {
        validate_plugin_id(id)?;
        // 调用时再次校验权限（安装时已校验一次）。
        let row = self
            .db
            .plugin_get(id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("插件 {id} 不存在")))?;
        let declared: Vec<String> =
            serde_json::from_str(&udx_storage::row_str(&row, "permissions"))
                .unwrap_or_default();
        if !declared.iter().any(|p| p == perms::PLUGIN_STORAGE_WRITE) {
            return Err(AppError::Forbidden(perms::PLUGIN_STORAGE_WRITE));
        }
        self.db
            .plugin_data_set(id, "state", payload, PLUGIN_STORAGE_QUOTA)
            .await?;
        Ok(json!({"status": "ok"}))
    }

    pub async fn storage_delete(&self, id: &str) -> AppResult<Value> {
        validate_plugin_id(id)?;
        let n = self.db.plugin_data_delete(id, "state").await?;
        Ok(json!({"status": "ok", "deleted": n}))
    }

    /// 宿主动作白名单：插件 action 只能命中已注册名称。
    pub fn is_action_allowed(&self, action: &str) -> bool {
        SUPPORTED_PLUGIN_ACTIONS.contains(&action)
    }

    // ---------------- 脚本（非执行存储） ----------------

    pub async fn script_list(&self) -> AppResult<Value> {
        Ok(json!({"scripts": self.db.script_list().await?}))
    }

    pub async fn script_get(&self, name: &str) -> AppResult<Value> {
        let safe = safe_component(name)?;
        let content = self.db.script_get(&safe).await?;
        Ok(json!({"name": safe, "content": content, "executable": false}))
    }

    pub async fn script_upload(&self, payload: &Value) -> AppResult<Value> {
        let name = payload.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let safe = safe_component(name)?;
        let content = payload
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad_request("缺少脚本内容"))?;
        if content.len() > 256 * 1024 {
            return Err(bad_request("脚本超过 256 KiB"));
        }
        self.db.script_upsert(&safe, content).await?;
        Ok(json!({
            "status": "ok",
            "name": safe,
            "executable": false,
            "note": "脚本以非执行方式存储；执行需独立管理员审批"
        }))
    }

    pub async fn script_update(&self, name: &str, payload: &Value) -> AppResult<Value> {
        let safe = safe_component(name)?;
        let content = payload
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad_request("缺少脚本内容"))?;
        self.db.script_upsert(&safe, content).await?;
        Ok(json!({"status": "ok", "name": safe, "executable": false}))
    }

    pub async fn script_delete(&self, name: &str) -> AppResult<Value> {
        let safe = safe_component(name)?;
        let n = self.db.script_delete(&safe).await?;
        Ok(json!({"status": "ok", "deleted": n}))
    }

    pub fn new_id() -> String {
        new_id()
    }
}

fn validate_manifest(m: &PluginManifest) -> AppResult<()> {
    if m.id.is_empty() || m.id.len() > 64 || !is_safe_ident(&m.id) {
        return Err(bad_request("插件 id 非法"));
    }
    if m.name.is_empty() || m.name.len() > 128 {
        return Err(bad_request("插件名称非法"));
    }
    if m.version.is_empty() || m.version.len() > 32 {
        return Err(bad_request("插件版本非法"));
    }
    Ok(())
}

fn validate_ui(ui: &PluginUiSpec) -> AppResult<()> {
    if ui.schema_version != 1 {
        return Err(bad_request("仅支持 schema_version = 1"));
    }
    if ui.id.is_empty() || !is_safe_ident(&ui.id) {
        return Err(bad_request("ui.id 非法"));
    }
    for p in &ui.permissions {
        if !perms::is_known(p) {
            return Err(bad_request(format!("未知权限: {p}")));
        }
    }
    let mut count = 0usize;
    validate_node(&ui.view, 0, &mut count)
}

fn validate_node(node: &PluginViewNode, depth: usize, count: &mut usize) -> AppResult<()> {
    if depth > MAX_VIEW_DEPTH {
        return Err(bad_request("UI 嵌套层级过深"));
    }
    *count += 1;
    if *count > MAX_VIEW_NODES {
        return Err(bad_request("UI 节点数量超过上限"));
    }
    if !SUPPORTED_VIEW_TYPES.contains(&node.kind.as_str()) {
        return Err(bad_request(format!("不支持的 UI 类型: {}", node.kind)));
    }
    if let Some(bind) = &node.bind {
        if bind.len() > 128
            || !bind
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return Err(bad_request("bind 路径非法"));
        }
    }
    if let Some(action) = &node.action {
        if !SUPPORTED_PLUGIN_ACTIONS.contains(&action.as_str()) {
            return Err(bad_request(format!("未注册的插件动作: {action}")));
        }
    }
    for child in &node.children {
        validate_node(child, depth + 1, count)?;
    }
    Ok(())
}

fn validate_permission_doc(doc: &PluginPermissionDoc) -> AppResult<()> {
    for p in &doc.permissions {
        if !perms::is_known(p) {
            return Err(bad_request(format!("未知权限: {p}")));
        }
        // 插件永远不能申请 Web Shell / OTA / 恢复出厂等高风险权限。
        if perms::HIGH_RISK.contains(&p.as_str()) {
            return Err(AppError::Forbidden("插件不得申请高风险权限"));
        }
    }
    Ok(())
}

/// 双重校验：ui.json 与 permissions.json 声明必须一致。
fn validate_permission_consistency(
    ui: &PluginUiSpec,
    doc: &PluginPermissionDoc,
) -> AppResult<()> {
    for p in &ui.permissions {
        if !doc.permissions.contains(p) {
            return Err(bad_request(format!(
                "ui.json 声明了未在 permissions.json 中授权的权限: {p}"
            )));
        }
    }
    Ok(())
}

fn validate_plugin_id(id: &str) -> AppResult<()> {
    validate_manifest(&PluginManifest {
        id: id.to_string(),
        name: "x".into(),
        version: "1".into(),
        min_api_version: 1,
        description: String::new(),
        author: String::new(),
    })
}

fn is_safe_ident(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_view_type_and_arbitrary_action() {
        let ui = PluginUiSpec {
            schema_version: 1,
            id: "example.status".into(),
            title: "状态".into(),
            permissions: vec![],
            view: PluginViewNode {
                kind: "script".into(),
                ..Default::default()
            },
        };
        assert!(validate_ui(&ui).is_err());

        let ui2 = PluginUiSpec {
            view: PluginViewNode {
                kind: "button".into(),
                action: Some("window.location='http://evil'".into()),
                ..Default::default()
            },
            ..ui
        };
        assert!(validate_ui(&ui2).is_err());
    }

    #[test]
    fn plugins_cannot_request_shell_permission() {
        let doc = PluginPermissionDoc {
            schema_version: 1,
            permissions: vec![perms::SHELL_EXECUTE.to_string()],
            max_storage_bytes: 1024,
        };
        assert!(validate_permission_doc(&doc).is_err());
    }
}
