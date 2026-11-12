-- UDX710-TOOLS 重构版数据库初始化
-- 说明：
-- 1. 所有查询使用参数化绑定，禁止字符串拼接 SQL（报告 08 章）。
-- 2. 密码与密保答案只存 salted hash，绝不存明文；Token 只存 hash。
-- 3. 审计表记录设备动作与 Web Shell 调用，不记录密码 / Token / 短信正文。
-- 4. 迁移只做 CREATE IF NOT EXISTS，可重复执行，保证升级失败时可恢复。

CREATE TABLE IF NOT EXISTS users (
    subject       TEXT PRIMARY KEY,
    password_salt TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'admin',
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tokens (
    token_hash TEXT PRIMARY KEY,
    token_id   TEXT NOT NULL UNIQUE,
    subject    TEXT NOT NULL,
    role       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_tokens_expires ON tokens(expires_at);

CREATE TABLE IF NOT EXISTS login_attempts (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    at         INTEGER NOT NULL,
    client_key TEXT NOT NULL,
    success    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_login_attempts ON login_attempts(client_key, at);

CREATE TABLE IF NOT EXISTS audit_log (
    id          TEXT PRIMARY KEY,
    at          INTEGER NOT NULL,
    actor       TEXT NOT NULL,
    action      TEXT NOT NULL,
    permission  TEXT NOT NULL,
    outcome     TEXT NOT NULL,
    detail      TEXT NOT NULL,
    duration_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_at ON audit_log(at);

CREATE TABLE IF NOT EXISTS config (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS plugins (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    installed_at INTEGER NOT NULL,
    ui           TEXT,
    permissions  TEXT
);

-- 插件数据按 plugin_id 隔离；配额由服务层限制（默认 64 KiB / 插件）。
CREATE TABLE IF NOT EXISTS plugin_data (
    plugin_id  TEXT NOT NULL,
    key        TEXT NOT NULL,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

-- 脚本以「非执行」方式存储：内容存数据库/数据目录，落盘文件不带执行位。
CREATE TABLE IF NOT EXISTS scripts (
    name       TEXT PRIMARY KEY,
    content    TEXT NOT NULL,
    size       INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS apn_templates (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    config     TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS rathole_services (
    id         TEXT PRIMARY KEY,
    config     TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ipv6_rules (
    id         TEXT PRIMARY KEY,
    config     TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS sms_sent (
    id        TEXT PRIMARY KEY,
    recipient TEXT NOT NULL,
    content   TEXT NOT NULL,
    at        INTEGER NOT NULL,
    status    TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS security (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    question1     TEXT,
    answer1_hash  TEXT,
    question2     TEXT,
    answer2_hash  TEXT,
    salt          TEXT,
    updated_at    INTEGER
);

CREATE TABLE IF NOT EXISTS update_state (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    version      TEXT,
    status       TEXT,
    package_path TEXT,
    sha256       TEXT,
    last_error   TEXT,
    updated_at   INTEGER
);
