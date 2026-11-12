//! 持久化层：SQLite 参数化访问、专用 DB worker、配置与插件数据隔离。
//!
//! - 所有 SQL 为编译期常量，参数通过绑定传入（报告 08 章 SQL 注入修复）；
//! - 同步 `rusqlite` 放在独立 OS 线程，不阻塞 Tokio 事件循环（报告 06 章）；
//! - 审计条目异步落库，插件数据按插件 ID 隔离并有配额（报告 07 章）。

pub mod db;
pub mod repos;

pub use db::{row_i64, row_str, Db, DbRow, P, SqlParams, MIGRATIONS};
pub use repos::{now_ts, Entity, AUDIT_INSERT, PLUGIN_STORAGE_QUOTA};
