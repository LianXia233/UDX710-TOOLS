//! 认证、Token 生命周期与密保（报告 08 章安全默认值）。
//!
//! - 登录失败限速：基于 client_key 的时间窗计数；
//! - Token 有过期、撤销与轮换；数据库只存 hash；
//! - 所有写操作默认要求认证，敏感动作由 API 层再查细粒度权限；
//! - 忘记密码 / 密保流程不绕过认证模型：公开端点只能完成验证与重置，
//!   不能读取设备资源。

use crate::config::AuthConfig;
use crate::crypto::{constant_time_eq, hash_password, new_id, new_salt, new_token, sha256_hex};
use serde_json::{json, Value};
use udx_domain::error::{bad_request, AppError, AppResult};
use udx_domain::models::{TokenRecord, UserRecord};
use udx_domain::permissions::Principal;
use udx_storage::{now_ts, Db};

#[derive(Clone)]
pub struct AuthService {
    db: Db,
    cfg: AuthConfig,
}

impl AuthService {
    pub fn new(db: Db, cfg: AuthConfig) -> Self {
        Self { db, cfg }
    }

    pub fn config(&self) -> &AuthConfig {
        &self.cfg
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// 首次启动建立默认管理员（兼容旧版默认密码 `admin`）。
    /// 若用户已修改过密码，不会覆盖。
    pub async fn ensure_default_admin(&self) -> AppResult<()> {
        if self.db.user_get("admin").await?.is_some() {
            return Ok(());
        }
        let salt = new_salt();
        let now = now_ts();
        self.db
            .user_upsert(&UserRecord {
                subject: "admin".to_string(),
                password_salt: salt.clone(),
                password_hash: hash_password(&self.cfg.default_password, &salt),
                role: "admin".to_string(),
                created_at: now,
                updated_at: now,
            })
            .await?;
        tracing::warn!(
            target: "security",
            "已创建默认管理员账号（默认密码），请登录后立即修改"
        );
        Ok(())
    }

    /// 登录：限速 → 恒定时间校验 → 清除失败计数 → 签发 Token。
    pub async fn login(&self, password: &str, client_key: &str) -> AppResult<String> {
        let failed = self
            .db
            .login_failed_count(client_key, self.cfg.login_window_secs)
            .await?;
        if failed >= self.cfg.login_max_failures {
            return Err(AppError::RateLimited);
        }

        let user = self
            .db
            .user_get("admin")
            .await?
            .ok_or_else(|| AppError::ServiceUnavailable("账号未初始化".to_string()))?;

        let candidate = hash_password(password, &user.password_salt);
        if !constant_time_eq(&candidate, &user.password_hash) {
            self.db
                .login_attempt_record(client_key, false)
                .await?;
            return Err(AppError::Unauthorized);
        }

        self.db.login_attempt_record(client_key, true).await?;
        self.db.login_attempts_clear(client_key).await?;

        let now = now_ts();
        let _ = self.db.token_purge(now).await;
        let token = new_token();
        self.db
            .token_insert(&TokenRecord {
                token_hash: sha256_hex(&token),
                token_id: new_id(),
                subject: user.subject.clone(),
                role: user.role.clone(),
                created_at: now,
                expires_at: now + self.cfg.token_ttl_secs,
                revoked: false,
            })
            .await?;
        Ok(token)
    }

    /// 校验 Bearer Token → 构造 Principal。过期与撤销均视为未授权。
    pub async fn verify(&self, token: &str) -> AppResult<Principal> {
        if token.is_empty() || token.len() > 256 {
            return Err(AppError::Unauthorized);
        }
        let rec = self
            .db
            .token_find(&sha256_hex(token), now_ts())
            .await?
            .ok_or(AppError::Unauthorized)?;
        Ok(Principal::new(
            rec.subject,
            udx_domain::Role::parse(&rec.role),
            rec.token_id,
            rec.expires_at,
        ))
    }

    pub async fn logout(&self, principal: &Principal) -> AppResult<()> {
        self.db.token_revoke(&principal.token_id).await
    }

    pub async fn change_password(
        &self,
        principal: &Principal,
        old: &str,
        new: &str,
    ) -> AppResult<()> {
        if new.len() < self.cfg.min_password_len {
            return Err(bad_request(format!(
                "新密码至少需要 {} 个字符",
                self.cfg.min_password_len
            )));
        }
        let user = self
            .db
            .user_get(&principal.subject)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if !constant_time_eq(&hash_password(old, &user.password_salt), &user.password_hash) {
            return Err(AppError::Unauthorized);
        }
        let salt = new_salt();
        self.db
            .user_upsert(&UserRecord {
                subject: user.subject,
                password_salt: salt.clone(),
                password_hash: hash_password(new, &salt),
                role: user.role,
                created_at: user.created_at,
                updated_at: now_ts(),
            })
            .await?;
        // 口令轮换：撤销本账号其它 Token，防止旧凭证继续有效。
        self.db
            .execute(
                "UPDATE tokens SET revoked = 1 WHERE subject = ?1 AND token_id <> ?2",
                udx_storage::P::new()
                    .t(principal.subject.clone())
                    .t(principal.token_id.clone())
                    .done(),
            )
            .await?;
        Ok(())
    }

    pub async fn status(&self, principal: Option<&Principal>) -> AppResult<Value> {
        Ok(json!({
            "authenticated": principal.is_some(),
            "subject": principal.map(|p| p.subject.as_str()).unwrap_or(""),
            "role": principal.map(|p| p.role.as_str()).unwrap_or(""),
            "default_password": self.is_default_password_active().await?,
        }))
    }

    async fn is_default_password_active(&self) -> AppResult<bool> {
        let Some(user) = self.db.user_get("admin").await? else {
            return Ok(false);
        };
        Ok(constant_time_eq(
            &hash_password(&self.cfg.default_password, &user.password_salt),
            &user.password_hash,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AuthConfig;

    #[tokio::test]
    async fn login_is_rate_limited_and_token_can_be_revoked() {
        let db = Db::open(":memory:").unwrap();
        let auth = AuthService::new(db, AuthConfig::default());
        auth.ensure_default_admin().await.unwrap();

        for _ in 0..5 {
            let _ = auth.login("wrong", "client-a").await;
        }
        assert!(matches!(
            auth.login("admin", "client-a").await,
            Err(AppError::RateLimited)
        ));

        let token = auth.login("admin", "client-b").await.unwrap();
        let principal = auth.verify(&token).await.unwrap();
        assert_eq!(principal.subject, "admin");
        auth.logout(&principal).await.unwrap();
        assert!(auth.verify(&token).await.is_err());
    }

}
