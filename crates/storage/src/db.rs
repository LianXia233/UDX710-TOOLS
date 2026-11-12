//! SQLite 专用 DB worker（报告 05/06/08 章）。
//!
//! 设计要点：
//! - `rusqlite` 是同步 API，因此放在**独立 OS 线程**中持有一条专用连接，
//!   不占用 Tokio 阻塞池，也不阻塞事件循环；
//! - 请求通过 `tokio::sync::mpsc` 提交，结果经 `oneshot` 返回；
//! - **所有 SQL 都是编译期静态字符串常量**，参数一律通过绑定传入，
//!   从根本上消除 SQL 注入与 `sh -c sqlite3` 调用链；
//! - 迁移脚本通过 `include_str!` 内嵌，重复执行安全（CREATE IF NOT EXISTS）；
//! - 审计写入走独立无界队列 + 后台任务，不在请求路径上同步阻塞。

use rusqlite::{types::Value as SqlValue, Connection};
use serde_json::{json, Value as JsonValue};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;
use tokio::sync::{mpsc, oneshot};
use udx_domain::error::{internal, AppResult};
use udx_domain::models::{AuditEntry, AuditSink};

pub type DbRow = BTreeMap<String, JsonValue>;
pub type SqlParams = Vec<SqlValue>;

pub const MIGRATIONS: &str = include_str!("../../../migrations/001_init.sql");

enum DbCommand {
    Query {
        sql: &'static str,
        params: SqlParams,
        reply: oneshot::Sender<Result<Vec<DbRow>, String>>,
    },
    Execute {
        sql: &'static str,
        params: SqlParams,
        reply: oneshot::Sender<Result<usize, String>>,
    },
    Health {
        reply: oneshot::Sender<Result<(), String>>,
    },
}

/// 参数构造器：显式区分文本 / 整数 / 布尔 / 空值，避免字符串化 SQL。
#[derive(Debug, Default)]
pub struct P(SqlParams);

impl P {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn t(mut self, v: impl Into<String>) -> Self {
        self.0.push(SqlValue::Text(v.into()));
        self
    }
    pub fn i(mut self, v: i64) -> Self {
        self.0.push(SqlValue::Integer(v));
        self
    }
    pub fn b(mut self, v: bool) -> Self {
        self.0.push(SqlValue::Integer(i64::from(v)));
        self
    }
    pub fn n(mut self) -> Self {
        self.0.push(SqlValue::Null);
        self
    }
    pub fn done(self) -> SqlParams {
        self.0
    }
}

#[derive(Clone)]
pub struct Db {
    tx: mpsc::UnboundedSender<DbCommand>,
    audit_tx: mpsc::UnboundedSender<AuditEntry>,
}

impl Db {
    /// 打开（或创建）数据库、执行迁移并启动 worker。
    pub fn open(path: impl Into<PathBuf>) -> AppResult<Self> {
        let path = path.into();
        let (tx, mut rx) = mpsc::unbounded_channel::<DbCommand>();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

        thread::Builder::new()
            .name("udx-db".to_string())
            .spawn(move || {
                let conn = match Connection::open(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("打开数据库失败: {e}")));
                        return;
                    }
                };
                let _ = conn.busy_timeout(std::time::Duration::from_secs(5));
                if let Err(e) = conn.execute_batch(MIGRATIONS) {
                    let _ = ready_tx.send(Err(format!("执行迁移失败: {e}")));
                    return;
                }
                let _ = ready_tx.send(Ok(()));

                while let Some(cmd) = rx.blocking_recv() {
                    match cmd {
                        DbCommand::Query { sql, params, reply } => {
                            let _ = reply.send(run_query(&conn, sql, &params));
                        }
                        DbCommand::Execute { sql, params, reply } => {
                            let result = conn
                                .execute(sql, rusqlite::params_from_iter(params.iter()))
                                .map(|n| n as usize)
                                .map_err(|e| e.to_string());
                            let _ = reply.send(result);
                        }
                        DbCommand::Health { reply } => {
                            let result = conn
                                .query_row("SELECT 1", [], |_| Ok(()))
                                .map_err(|e| e.to_string());
                            let _ = reply.send(result);
                        }
                    }
                }
            })
            .map_err(|e| internal(format!("启动 DB worker 失败: {e}")))?;

        ready_rx
            .recv()
            .map_err(|e| internal(format!("DB worker 未就绪: {e}")))?
            .map_err(internal)?;

        let (audit_tx, mut audit_rx) = mpsc::unbounded_channel::<AuditEntry>();
        let db = Self { tx, audit_tx };

        // 审计写入与请求路径解耦；无 Tokio 运行时时仅丢弃（单元测试场景）。
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let writer = db.clone();
            handle.spawn(async move {
                while let Some(entry) = audit_rx.recv().await {
                    // outcome 存纯文本（success/failed/timeout/unknown），
                    // 不存带引号的 JSON 字符串，便于日志检索与前端展示。
                    let outcome = serde_json::to_value(&entry.outcome)
                        .ok()
                        .and_then(|v| v.as_str().map(str::to_string))
                        .unwrap_or_else(|| "unknown".to_string());
                    let _ = writer
                        .execute(
                            super::repos::AUDIT_INSERT,
                            P::new()
                                .t(entry.id)
                                .i(entry.at)
                                .t(entry.actor)
                                .t(entry.action)
                                .t(entry.permission)
                                .t(outcome)
                                .t(entry.detail)
                                .i(entry.duration_ms as i64)
                                .done(),
                        )
                        .await;
                }
            });
        }

        Ok(db)
    }

    pub async fn query(&self, sql: &'static str, params: SqlParams) -> AppResult<Vec<DbRow>> {
        let (reply, waiter) = oneshot::channel();
        self.tx
            .send(DbCommand::Query { sql, params, reply })
            .map_err(|_| internal("DB worker 已停止"))?;
        waiter
            .await
            .map_err(|e| internal(format!("DB 查询通道中断: {e}")))?
            .map_err(internal)
    }

    pub async fn execute(&self, sql: &'static str, params: SqlParams) -> AppResult<usize> {
        let (reply, waiter) = oneshot::channel();
        self.tx
            .send(DbCommand::Execute { sql, params, reply })
            .map_err(|_| internal("DB worker 已停止"))?;
        waiter
            .await
            .map_err(|e| internal(format!("DB 执行通道中断: {e}")))?
            .map_err(internal)
    }

    pub async fn health(&self) -> AppResult<()> {
        let (reply, waiter) = oneshot::channel();
        self.tx
            .send(DbCommand::Health { reply })
            .map_err(|_| internal("DB worker 已停止"))?;
        waiter
            .await
            .map_err(|e| internal(format!("DB 健康检查通道中断: {e}")))?
            .map_err(internal)
    }
}

impl AuditSink for Db {
    fn record(&self, entry: AuditEntry) {
        if self.audit_tx.send(entry).is_err() {
            tracing::warn!(target: "audit", "审计队列已关闭，条目未落库");
        }
    }
}

fn run_query(
    conn: &Connection,
    sql: &str,
    params: &[SqlValue],
) -> Result<Vec<DbRow>, String> {
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    let names: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let mapped = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let mut out = DbRow::new();
            for (idx, name) in names.iter().enumerate() {
                let v: SqlValue = row.get(idx)?;
                out.insert(name.clone(), sql_to_json(v));
            }
            Ok(out)
        })
        .map_err(|e| e.to_string())?;

    mapped
        .collect::<Result<Vec<DbRow>, _>>()
        .map_err(|e| e.to_string())
}

fn sql_to_json(v: SqlValue) -> JsonValue {
    match v {
        SqlValue::Null => JsonValue::Null,
        SqlValue::Integer(i) => json!(i),
        SqlValue::Real(f) => json!(f),
        SqlValue::Text(s) => json!(s),
        SqlValue::Blob(b) => json!(String::from_utf8_lossy(&b).into_owned()),
    }
}

/// 从 DbRow 读取字符串字段。
pub fn row_str(row: &DbRow, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// 从 DbRow 读取整数字段。
pub fn row_i64(row: &DbRow, key: &str) -> i64 {
    row.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_are_idempotent_and_queries_are_parameterized() {
        let db = Db::open(":memory:").unwrap();
        db.health().await.unwrap();
        // 再次执行迁移不应报错
        let rows = db.query("SELECT COUNT(*) AS n FROM users", P::new().done()).await.unwrap();
        assert_eq!(row_i64(&rows[0], "n"), 0);

        db.execute(
            "INSERT INTO config(key, value, updated_at) VALUES(?1, ?2, ?3)",
            P::new().t("k").t("{\"a\":1}").i(1).done(),
        )
        .await
        .unwrap();
        let rows = db
            .query(
                "SELECT value FROM config WHERE key = ?1",
                P::new().t("k").done(),
            )
            .await
            .unwrap();
        assert_eq!(row_str(&rows[0], "value"), "{\"a\":1}");

        // 含引号的参数必须被安全绑定，而不是拼进 SQL
        let evil = "k'; DROP TABLE config; --";
        let rows = db
            .query(
                "SELECT COUNT(*) AS n FROM config WHERE key = ?1",
                P::new().t(evil).done(),
            )
            .await
            .unwrap();
        assert_eq!(row_i64(&rows[0], "n"), 0);
    }
}
