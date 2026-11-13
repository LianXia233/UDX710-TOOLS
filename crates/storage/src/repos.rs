//! 全部仓储操作。
//!
//! 安全不变量：**本文件中每一条 SQL 都是编译期字符串常量**，
//! 所有可变数据通过 `P`（参数绑定）传入。不存在任何 `format!` 拼接的 SQL，
//! 也不会通过 `sh -c` 调用 sqlite3 CLI（报告 08 章严重项）。

use crate::db::{row_i64, row_str, Db, DbRow, P};
use serde_json::Value as JsonValue;
use std::time::{SystemTime, UNIX_EPOCH};
use udx_domain::error::{bad_request, internal, AppResult};
use udx_domain::models::{TokenRecord, UserRecord};

pub const AUDIT_INSERT: &str = "INSERT OR REPLACE INTO audit_log(id,at,actor,action,permission,outcome,detail,duration_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)";
const AUDIT_LIST: &str = "SELECT id,at,actor,action,permission,outcome,detail,duration_ms FROM audit_log ORDER BY at DESC LIMIT ?1";

const USER_GET: &str = "SELECT subject,password_salt,password_hash,role,created_at,updated_at FROM users WHERE subject = ?1";
const USER_UPSERT: &str = "INSERT INTO users(subject,password_salt,password_hash,role,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(subject) DO UPDATE SET password_salt=excluded.password_salt, password_hash=excluded.password_hash, role=excluded.role, updated_at=excluded.updated_at";

const TOKEN_INSERT: &str = "INSERT INTO tokens(token_hash,token_id,subject,role,created_at,expires_at,revoked) VALUES(?1,?2,?3,?4,?5,?6,?7)";
const TOKEN_GET: &str = "SELECT token_hash,token_id,subject,role,created_at,expires_at,revoked FROM tokens WHERE token_hash = ?1 AND revoked = 0 AND expires_at > ?2";
const TOKEN_REVOKE: &str = "UPDATE tokens SET revoked = 1 WHERE token_id = ?1";
const TOKEN_PURGE: &str = "DELETE FROM tokens WHERE expires_at < ?1";

const ATTEMPT_INSERT: &str = "INSERT INTO login_attempts(at,client_key,success) VALUES(?1,?2,?3)";
const ATTEMPT_COUNT: &str = "SELECT COUNT(*) AS n FROM login_attempts WHERE client_key = ?1 AND success = 0 AND at > ?2";
const ATTEMPT_CLEAR: &str = "DELETE FROM login_attempts WHERE client_key = ?1";

const CONFIG_GET: &str = "SELECT key,value,updated_at FROM config WHERE key = ?1";
const CONFIG_SET: &str = "INSERT INTO config(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at";
const CONFIG_DELETE: &str = "DELETE FROM config WHERE key = ?1";

const PLUGIN_LIST: &str = "SELECT id,name,version,enabled,installed_at,ui,permissions FROM plugins ORDER BY installed_at DESC";
const PLUGIN_GET: &str = "SELECT id,name,version,enabled,installed_at,ui,permissions FROM plugins WHERE id = ?1";
const PLUGIN_INSERT: &str = "INSERT INTO plugins(id,name,version,enabled,installed_at,ui,permissions) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET name=excluded.name, version=excluded.version, enabled=excluded.enabled, ui=excluded.ui, permissions=excluded.permissions";
const PLUGIN_DELETE: &str = "DELETE FROM plugins WHERE id = ?1";
const PLUGIN_DELETE_ALL: &str = "DELETE FROM plugins";
const PLUGIN_DATA_CLEAN: &str = "DELETE FROM plugin_data";

const PLUGIN_DATA_GET: &str = "SELECT value FROM plugin_data WHERE plugin_id = ?1 AND key = ?2";
const PLUGIN_DATA_SET: &str = "INSERT INTO plugin_data(plugin_id,key,value,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(plugin_id,key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at";
const PLUGIN_DATA_DELETE: &str = "DELETE FROM plugin_data WHERE plugin_id = ?1 AND key = ?2";
const PLUGIN_DATA_SIZE: &str = "SELECT COALESCE(SUM(LENGTH(value)),0) AS n FROM plugin_data WHERE plugin_id = ?1";

const SCRIPT_LIST: &str = "SELECT name,size,updated_at FROM scripts ORDER BY name";
const SCRIPT_GET: &str = "SELECT name,content,size,updated_at FROM scripts WHERE name = ?1";
const SCRIPT_UPSERT: &str = "INSERT INTO scripts(name,content,size,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(name) DO UPDATE SET content=excluded.content, size=excluded.size, updated_at=excluded.updated_at";
const SCRIPT_DELETE: &str = "DELETE FROM scripts WHERE name = ?1";

const APN_LIST: &str = "SELECT id,name,config,updated_at FROM apn_templates ORDER BY updated_at DESC";
const APN_GET: &str = "SELECT id,name,config,updated_at FROM apn_templates WHERE id = ?1";
const APN_UPSERT: &str = "INSERT INTO apn_templates(id,name,config,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name, config=excluded.config, updated_at=excluded.updated_at";
const APN_DELETE: &str = "DELETE FROM apn_templates WHERE id = ?1";

const RATHOLE_LIST: &str = "SELECT id,name,config,updated_at FROM rathole_services ORDER BY updated_at DESC";
const RATHOLE_GET: &str = "SELECT id,name,config,updated_at FROM rathole_services WHERE id = ?1";
const RATHOLE_UPSERT: &str = "INSERT INTO rathole_services(id,name,config,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name, config=excluded.config, updated_at=excluded.updated_at";
const RATHOLE_DELETE: &str = "DELETE FROM rathole_services WHERE id = ?1";

const IPV6_LIST: &str = "SELECT id,config,updated_at FROM ipv6_rules ORDER BY updated_at DESC";
const IPV6_GET: &str = "SELECT id,config,updated_at FROM ipv6_rules WHERE id = ?1";
const IPV6_UPSERT: &str = "INSERT INTO ipv6_rules(id,config,updated_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET config=excluded.config, updated_at=excluded.updated_at";
const IPV6_DELETE: &str = "DELETE FROM ipv6_rules WHERE id = ?1";

const SMS_SENT_LIST: &str = "SELECT id,recipient,content,at,status FROM sms_sent ORDER BY at DESC LIMIT ?1";
const SMS_SENT_INSERT: &str = "INSERT INTO sms_sent(id,recipient,content,at,status) VALUES(?1,?2,?3,?4,?5)";
const SMS_SENT_DELETE: &str = "DELETE FROM sms_sent WHERE id = ?1";


const UPDATE_GET: &str = "SELECT version,status,package_path,sha256,last_error,updated_at FROM update_state WHERE id = 1";
const UPDATE_SET: &str = "INSERT INTO update_state(id,version,status,package_path,sha256,last_error,updated_at) VALUES(1,?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET version=excluded.version, status=excluded.status, package_path=excluded.package_path, sha256=excluded.sha256, last_error=excluded.last_error, updated_at=excluded.updated_at";

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 通用实体：仅这三类，各自的 SQL 均为静态常量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entity {
    ApnTemplate,
    RatholeService,
    Ipv6Rule,
}

impl Entity {
    fn list_sql(self) -> &'static str {
        match self {
            Self::ApnTemplate => APN_LIST,
            Self::RatholeService => RATHOLE_LIST,
            Self::Ipv6Rule => IPV6_LIST,
        }
    }
    fn get_sql(self) -> &'static str {
        match self {
            Self::ApnTemplate => APN_GET,
            Self::RatholeService => RATHOLE_GET,
            Self::Ipv6Rule => IPV6_GET,
        }
    }
    fn delete_sql(self) -> &'static str {
        match self {
            Self::ApnTemplate => APN_DELETE,
            Self::RatholeService => RATHOLE_DELETE,
            Self::Ipv6Rule => IPV6_DELETE,
        }
    }
}

/// 插件数据默认配额（报告 07 章：数据隔离 + 最大文件大小）。
pub const PLUGIN_STORAGE_QUOTA: usize = 64 * 1024;

impl Db {
    // ---------------- 用户与 Token ----------------

    pub async fn user_get(&self, subject: &str) -> AppResult<Option<UserRecord>> {
        let rows = self
            .query(USER_GET, P::new().t(subject).done())
            .await?;
        Ok(rows.first().map(|r| UserRecord {
            subject: row_str(r, "subject"),
            password_salt: row_str(r, "password_salt"),
            password_hash: row_str(r, "password_hash"),
            role: row_str(r, "role"),
            created_at: row_i64(r, "created_at"),
            updated_at: row_i64(r, "updated_at"),
        }))
    }

    pub async fn user_upsert(&self, u: &UserRecord) -> AppResult<()> {
        self.execute(
            USER_UPSERT,
            P::new()
                .t(u.subject.clone())
                .t(u.password_salt.clone())
                .t(u.password_hash.clone())
                .t(u.role.clone())
                .i(u.created_at)
                .i(u.updated_at)
                .done(),
        )
        .await?;
        Ok(())
    }

    pub async fn token_insert(&self, t: &TokenRecord) -> AppResult<()> {
        self.execute(
            TOKEN_INSERT,
            P::new()
                .t(t.token_hash.clone())
                .t(t.token_id.clone())
                .t(t.subject.clone())
                .t(t.role.clone())
                .i(t.created_at)
                .i(t.expires_at)
                .b(t.revoked)
                .done(),
        )
        .await?;
        Ok(())
    }

    pub async fn token_find(&self, hash: &str, now: i64) -> AppResult<Option<TokenRecord>> {
        let rows = self
            .query(TOKEN_GET, P::new().t(hash).i(now).done())
            .await?;
        Ok(rows.first().map(|r| TokenRecord {
            token_hash: row_str(r, "token_hash"),
            token_id: row_str(r, "token_id"),
            subject: row_str(r, "subject"),
            role: row_str(r, "role"),
            created_at: row_i64(r, "created_at"),
            expires_at: row_i64(r, "expires_at"),
            revoked: row_i64(r, "revoked") != 0,
        }))
    }

    pub async fn token_revoke(&self, token_id: &str) -> AppResult<()> {
        self.execute(TOKEN_REVOKE, P::new().t(token_id).done())
            .await?;
        Ok(())
    }

    pub async fn token_purge(&self, now: i64) -> AppResult<usize> {
        self.execute(TOKEN_PURGE, P::new().i(now).done()).await
    }

    // ---------------- 登录限速 ----------------

    pub async fn login_attempt_record(&self, client_key: &str, success: bool) -> AppResult<()> {
        self.execute(
            ATTEMPT_INSERT,
            P::new().i(now_ts()).t(client_key).b(success).done(),
        )
        .await?;
        Ok(())
    }

    pub async fn login_failed_count(&self, client_key: &str, window_secs: i64) -> AppResult<u32> {
        let rows = self
            .query(
                ATTEMPT_COUNT,
                P::new()
                    .t(client_key)
                    .i(now_ts() - window_secs)
                    .done(),
            )
            .await?;
        Ok(rows.first().map(|r| row_i64(r, "n") as u32).unwrap_or(0))
    }

    pub async fn login_attempts_clear(&self, client_key: &str) -> AppResult<()> {
        self.execute(ATTEMPT_CLEAR, P::new().t(client_key).done())
            .await?;
        Ok(())
    }

    // ---------------- 配置 ----------------

    pub async fn config_get(&self, key: &str) -> AppResult<Option<JsonValue>> {
        let rows = self.query(CONFIG_GET, P::new().t(key).done()).await?;
        Ok(rows.first().and_then(|r| {
            let raw = row_str(r, "value");
            serde_json::from_str(&raw).ok()
        }))
    }

    pub async fn config_set(&self, key: &str, value: &JsonValue) -> AppResult<()> {
        let text = serde_json::to_string(value)
            .map_err(|e| internal(format!("配置序列化失败: {e}")))?;
        self.execute(
            CONFIG_SET,
            P::new().t(key).t(text).i(now_ts()).done(),
        )
        .await?;
        Ok(())
    }

    pub async fn config_delete(&self, key: &str) -> AppResult<()> {
        self.execute(CONFIG_DELETE, P::new().t(key).done()).await?;
        Ok(())
    }

    // ---------------- 插件与插件数据 ----------------

    pub async fn plugin_list(&self) -> AppResult<Vec<DbRow>> {
        self.query(PLUGIN_LIST, P::new().done()).await
    }

    pub async fn plugin_get(&self, id: &str) -> AppResult<Option<DbRow>> {
        Ok(self
            .query(PLUGIN_GET, P::new().t(id).done())
            .await?
            .into_iter()
            .next())
    }

    pub async fn plugin_insert(
        &self,
        id: &str,
        name: &str,
        version: &str,
        enabled: bool,
        ui: Option<&JsonValue>,
        permissions: &JsonValue,
    ) -> AppResult<()> {
        let ui_text = ui.map(|v| v.to_string());
        let params = match ui_text {
            Some(text) => P::new()
                .t(id)
                .t(name)
                .t(version)
                .b(enabled)
                .i(now_ts())
                .t(text)
                .t(permissions.to_string())
                .done(),
            None => P::new()
                .t(id)
                .t(name)
                .t(version)
                .b(enabled)
                .i(now_ts())
                .n()
                .t(permissions.to_string())
                .done(),
        };
        self.execute(PLUGIN_INSERT, params).await?;
        Ok(())
    }

    pub async fn plugin_delete(&self, id: &str) -> AppResult<usize> {
        self.execute(PLUGIN_DELETE, P::new().t(id).done()).await
    }

    pub async fn plugin_delete_all(&self) -> AppResult<()> {
        self.execute(PLUGIN_DELETE_ALL, P::new().done()).await?;
        self.execute(PLUGIN_DATA_CLEAN, P::new().done()).await?;
        Ok(())
    }

    pub async fn plugin_data_get(&self, plugin_id: &str, key: &str) -> AppResult<Option<JsonValue>> {
        let rows = self
            .query(
                PLUGIN_DATA_GET,
                P::new().t(plugin_id).t(key).done(),
            )
            .await?;
        Ok(rows
            .first()
            .and_then(|r| serde_json::from_str(&row_str(r, "value")).ok()))
    }

    /// 写入前检查按插件 ID 计算的配额，超限明确拒绝。
    pub async fn plugin_data_set(
        &self,
        plugin_id: &str,
        key: &str,
        value: &JsonValue,
        quota: usize,
    ) -> AppResult<()> {
        let text = serde_json::to_string(value)
            .map_err(|e| internal(format!("插件数据序列化失败: {e}")))?;
        let rows = self
            .query(PLUGIN_DATA_SIZE, P::new().t(plugin_id).done())
            .await?;
        let used = rows.first().map(|r| row_i64(r, "n") as usize).unwrap_or(0);
        if used.saturating_add(text.len()) > quota {
            return Err(bad_request(format!(
                "插件数据超过配额 {quota} 字节"
            )));
        }
        self.execute(
            PLUGIN_DATA_SET,
            P::new()
                .t(plugin_id)
                .t(key)
                .t(text)
                .i(now_ts())
                .done(),
        )
        .await?;
        Ok(())
    }

    pub async fn plugin_data_delete(&self, plugin_id: &str, key: &str) -> AppResult<usize> {
        self.execute(
            PLUGIN_DATA_DELETE,
            P::new().t(plugin_id).t(key).done(),
        )
        .await
    }

    // ---------------- 脚本（非执行存储） ----------------

    pub async fn script_list(&self) -> AppResult<Vec<DbRow>> {
        self.query(SCRIPT_LIST, P::new().done()).await
    }

    pub async fn script_get(&self, name: &str) -> AppResult<Option<String>> {
        let rows = self.query(SCRIPT_GET, P::new().t(name).done()).await?;
        Ok(rows.first().map(|r| row_str(r, "content")))
    }

    pub async fn script_upsert(&self, name: &str, content: &str) -> AppResult<()> {
        self.execute(
            SCRIPT_UPSERT,
            P::new()
                .t(name)
                .t(content)
                .i(content.len() as i64)
                .i(now_ts())
                .done(),
        )
        .await?;
        Ok(())
    }

    pub async fn script_delete(&self, name: &str) -> AppResult<usize> {
        self.execute(SCRIPT_DELETE, P::new().t(name).done()).await
    }

    // ---------------- 通用实体 ----------------

    pub async fn entity_list(&self, e: Entity) -> AppResult<Vec<DbRow>> {
        self.query(e.list_sql(), P::new().done()).await
    }

    pub async fn entity_get(&self, e: Entity, id: &str) -> AppResult<Option<DbRow>> {
        Ok(self
            .query(e.get_sql(), P::new().t(id).done())
            .await?
            .into_iter()
            .next())
    }

    pub async fn entity_upsert(&self, e: Entity, id: &str, config: &JsonValue) -> AppResult<()> {
        let text = config.to_string();
        let name = config
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let now = now_ts();
        match e {
            Entity::ApnTemplate => {
                self.execute(
                    APN_UPSERT,
                    P::new().t(id).t(name).t(text).i(now).done(),
                )
                .await?;
            }
            Entity::RatholeService => {
                self.execute(
                    RATHOLE_UPSERT,
                    P::new().t(id).t(name).t(text).i(now).done(),
                )
                .await?;
            }
            Entity::Ipv6Rule => {
                self.execute(IPV6_UPSERT, P::new().t(id).t(text).i(now).done())
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn entity_delete(&self, e: Entity, id: &str) -> AppResult<usize> {
        self.execute(e.delete_sql(), P::new().t(id).done()).await
    }

    // ---------------- 短信已发送 ----------------

    pub async fn sms_sent_insert(
        &self,
        id: &str,
        recipient: &str,
        content: &str,
        status: &str,
    ) -> AppResult<()> {
        self.execute(
            SMS_SENT_INSERT,
            P::new()
                .t(id)
                .t(recipient)
                .t(content)
                .i(now_ts())
                .t(status)
                .done(),
        )
        .await?;
        Ok(())
    }

    pub async fn sms_sent_list(&self, limit: i64) -> AppResult<Vec<DbRow>> {
        self.query(SMS_SENT_LIST, P::new().i(limit.clamp(1, 1000)).done())
            .await
    }

    pub async fn sms_sent_delete(&self, id: &str) -> AppResult<usize> {
        self.execute(SMS_SENT_DELETE, P::new().t(id).done()).await
    }

    // ---------------- 密保 ----------------
    pub async fn update_state_get(&self) -> AppResult<Option<DbRow>> {
        Ok(self
            .query(UPDATE_GET, P::new().done())
            .await?
            .into_iter()
            .next())
    }

    pub async fn update_state_set(
        &self,
        version: &str,
        status: &str,
        package_path: &str,
        sha256: &str,
        last_error: &str,
    ) -> AppResult<()> {
        self.execute(
            UPDATE_SET,
            P::new()
                .t(version)
                .t(status)
                .t(package_path)
                .t(sha256)
                .t(last_error)
                .i(now_ts())
                .done(),
        )
        .await?;
        Ok(())
    }

    // ---------------- 审计 ----------------

    pub async fn audit_list(&self, limit: i64) -> AppResult<Vec<DbRow>> {
        self.query(AUDIT_LIST, P::new().i(limit.clamp(1, 1000)).done())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn tokens_expire_and_can_be_revoked() {
        let db = Db::open(":memory:").unwrap();
        let now = now_ts();
        db.token_insert(&TokenRecord {
            token_hash: "h1".into(),
            token_id: "t1".into(),
            subject: "admin".into(),
            role: "admin".into(),
            created_at: now,
            expires_at: now + 3600,
            revoked: false,
        })
        .await
        .unwrap();
        assert!(db.token_find("h1", now).await.unwrap().is_some());
        db.token_revoke("t1").await.unwrap();
        assert!(db.token_find("h1", now).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn plugin_data_is_isolated_and_quota_limited() {
        let db = Db::open(":memory:").unwrap();
        db.plugin_data_set("p1", "k", &json!({"v": 1}), 16)
            .await
            .unwrap();
        assert!(db.plugin_data_get("p1", "k").await.unwrap().is_some());
        // 按插件 ID 隔离：p2 读不到 p1 的数据
        assert!(db.plugin_data_get("p2", "k").await.unwrap().is_none());

        let big = json!({"data": "x".repeat(100)});
        assert!(db.plugin_data_set("p1", "k2", &big, 16).await.is_err());

        let within_quota = json!({"data": "x".repeat(200)});
        assert!(db
            .plugin_data_set("p1", "k3", &within_quota, PLUGIN_STORAGE_QUOTA)
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn sql_injection_payload_is_bound_not_executed() {
        let db = Db::open(":memory:").unwrap();
        db.config_set("safe", &json!({"ok": true})).await.unwrap();
        let evil = "safe'; DROP TABLE config; --";
        assert!(db.config_get(evil).await.unwrap().is_none());
        assert!(db.config_get("safe").await.unwrap().is_some());
    }
}
