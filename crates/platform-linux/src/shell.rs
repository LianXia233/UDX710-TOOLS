//! **Web 任意 Shell —— 用户明确要求保留的能力**。
//!
//! 技术报告 08 章建议「取消 Web 任意 Shell，或严格限制为管理员诊断功能」。
//! 本项目按用户覆盖指令选择后者：保留完整任意 Shell（一次性执行 + 交互式终端），
//! 但把它作为一条**独立、显式、可关闭的高风险通道**，与 `command.rs` 的
//! 「命令 ID 白名单 + 固定 argv」设备动作通道严格隔离。
//!
//! 强制约束：
//! 1. 有效 Token + `shell.execute` / `terminal.session` 细粒度权限（默认仅 Admin）；
//! 2. 全局开关 `shell.enabled`，部署者可一键关闭；
//! 3. 命令长度上限、NUL/控制字符拒绝；
//! 4. `Semaphore` 并发上限，满时明确返回 Busy（不排队堆积）；
//! 5. `tokio::time::timeout` + `kill_on_drop` + 独立进程组 + 超时整组 SIGKILL；
//! 6. stdout/stderr 输出上限，防止输出耗尽内存；
//! 7. 独立审计日志：actor / 时间 / 命令（凭据脱敏）/ 退出码 / 时长；
//! 8. 交互会话有总时长上限与累计输出上限；
//! 9. 任何 tracing 日志都不输出命令原文、密码、Token、短信正文。

use crate::command::kill_process_group;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::process::{Child, Command};
use tokio::sync::Semaphore;
use tokio::time::timeout;
use udx_domain::error::{bad_request, internal, AppError, AppResult};
use udx_domain::models::{AuditEntry, AuditSink, ShellResult};
use udx_domain::permissions::{perms, Principal};
use udx_domain::state::ActionOutcome;

pub const DEFAULT_MAX_COMMAND_LEN: usize = 8 * 1024;
pub const DEFAULT_MAX_OUTPUT: usize = 64 * 1024;
pub const DEFAULT_SESSION_OUTPUT_CAP: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ShellPolicy {
    /// 用户要求保留，故默认 true；部署者可在配置中关闭。
    pub enabled: bool,
    pub max_command_len: usize,
    pub timeout: Duration,
    pub max_output_bytes: usize,
    pub max_concurrent: usize,
    pub session_timeout: Duration,
    pub session_output_cap: usize,
    /// 是否把命令原文写入审计表（会先做凭据脱敏）。
    pub audit_command: bool,
}

impl Default for ShellPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            max_command_len: DEFAULT_MAX_COMMAND_LEN,
            timeout: Duration::from_secs(30),
            max_output_bytes: DEFAULT_MAX_OUTPUT,
            max_concurrent: 2,
            session_timeout: Duration::from_secs(15 * 60),
            session_output_cap: DEFAULT_SESSION_OUTPUT_CAP,
            audit_command: true,
        }
    }
}

pub struct ShellExecutor {
    policy: ShellPolicy,
    permits: Arc<Semaphore>,
    audit: Arc<dyn AuditSink>,
}

impl ShellExecutor {
    pub fn new(policy: ShellPolicy, audit: Arc<dyn AuditSink>) -> Self {
        let permits = Arc::new(Semaphore::new(policy.max_concurrent.max(1)));
        Self {
            policy,
            permits,
            audit,
        }
    }

    pub fn policy(&self) -> &ShellPolicy {
        &self.policy
    }

    pub fn audit(&self) -> Arc<dyn AuditSink> {
        self.audit.clone()
    }

    /// 一次性执行任意 shell 命令（`/api/shell`）。
    pub async fn execute(&self, principal: &Principal, command: &str) -> AppResult<ShellResult> {
        self.authorize(principal, perms::SHELL_EXECUTE)?;
        self.validate_command(command)?;

        let _permit = self
            .permits
            .try_acquire()
            .map_err(|_| AppError::Busy("Web Shell 并发已满，请稍后重试".to_string()))?;

        let started = Instant::now();
        let (result, outcome) = match self.run_once(command).await {
            Ok(r) => {
                let oc = if r.exit_code == 0 {
                    ActionOutcome::Success
                } else {
                    ActionOutcome::Failed
                };
                (Ok(r), oc)
            }
            Err(e) => {
                let oc = if matches!(&e, AppError::Timeout(_)) {
                    ActionOutcome::Timeout
                } else {
                    ActionOutcome::Unknown
                };
                (Err(e), oc)
            }
        };

        self.record_with_detail(
            principal,
            "shell.execute",
            perms::SHELL_EXECUTE,
            outcome,
            started.elapsed().as_millis() as u64,
            redact_sensitive(command),
        );
        result
    }

    /// 交互式会话授权与进程启动（`/api/terminal/ws`）。
    pub fn spawn_interactive(
        &self,
        principal: &Principal,
    ) -> AppResult<InteractiveShell> {
        self.authorize(principal, perms::TERMINAL_SESSION)?;
        let _permit = self
            .permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::Busy("Web 终端并发已满，请稍后重试".to_string()))?;

        let mut cmd = Command::new("/bin/sh");
        // 非 tty 的交互式 shell：stdin/stdout/stderr 全部管道化，
        // 不分配 PTY，避免额外系统依赖与终端逃逸面。
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.as_std_mut().process_group(0);
        }

        let child = cmd
            .spawn()
            .map_err(|e| internal(format!("启动交互式 shell 失败: {e}")))?;
        let pid = child.id().unwrap_or(0);
        // take_streams 需要在返回前拿走管道；这里先不取，由 API 层调用。

        self.record(
            principal,
            "terminal.session.start",
            perms::TERMINAL_SESSION,
            ActionOutcome::Success,
            0,
        );

        Ok(InteractiveShell {
            child,
            pid,
            policy: self.policy.clone(),
            _permit,
        })
    }

    /// 供 API 层记录会话结束/超时事件。
    pub fn record(
        &self,
        principal: &Principal,
        action: &str,
        permission: &str,
        outcome: ActionOutcome,
        duration_ms: u64,
    ) {
        self.record_with_detail(
            principal,
            action,
            permission,
            outcome,
            duration_ms,
            String::new(),
        );
    }

    /// 写入审计表。命令原文只在 `audit_command=true` 时以脱敏形式落库。
    pub fn record_with_detail(
        &self,
        principal: &Principal,
        action: &str,
        permission: &str,
        outcome: ActionOutcome,
        duration_ms: u64,
        detail: String,
    ) {
        let detail = if self.policy.audit_command {
            detail
        } else {
            String::from("[审计详情已按配置关闭]")
        };
        self.audit.record(AuditEntry {
            id: gen_id(),
            at: now_ts(),
            actor: principal.subject.clone(),
            action: action.to_string(),
            permission: permission.to_string(),
            outcome,
            detail,
            duration_ms,
        });
    }

    fn authorize(&self, principal: &Principal, permission: &str) -> AppResult<()> {
        if !self.policy.enabled {
            return Err(AppError::Forbidden("shell.execute (已在配置中禁用)"));
        }
        if principal.is_expired(now_ts()) {
            return Err(AppError::Unauthorized);
        }
        if !principal.can(permission) {
            return Err(AppError::Forbidden(
                if permission == perms::SHELL_EXECUTE {
                    perms::SHELL_EXECUTE
                } else {
                    perms::TERMINAL_SESSION
                },
            ));
        }
        Ok(())
    }

    fn validate_command(&self, command: &str) -> AppResult<()> {
        if command.is_empty() {
            return Err(bad_request("命令不能为空"));
        }
        if command.len() > self.policy.max_command_len {
            return Err(bad_request("命令过长"));
        }
        if command.chars().any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t')) {
            return Err(bad_request("命令包含非法控制字符"));
        }
        Ok(())
    }

    async fn run_once(&self, command: &str) -> AppResult<ShellResult> {
        let mut cmd = Command::new("/bin/sh");
        cmd.arg("-c").arg(command);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.as_std_mut().process_group(0);
        }

        let started = Instant::now();
        let child = cmd
            .spawn()
            .map_err(|e| internal(format!("启动 shell 失败: {e}")))?;
        let pid = child.id().unwrap_or(0);

        let output = match timeout(self.policy.timeout, child.wait_with_output()).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(internal(format!("等待 shell 失败: {e}"))),
            Err(_) => {
                kill_process_group(pid);
                return Err(AppError::Timeout(format!(
                    "Web Shell 执行超过 {:?}，进程组已终止",
                    self.policy.timeout
                )));
            }
        };

        let mut truncated = false;
        let mut stdout = output.stdout;
        let mut stderr = output.stderr;
        if stdout.len() > self.policy.max_output_bytes {
            stdout.truncate(self.policy.max_output_bytes);
            truncated = true;
        }
        if stderr.len() > self.policy.max_output_bytes {
            stderr.truncate(self.policy.max_output_bytes);
            truncated = true;
        }

        // 命令原文（脱敏后）只进审计表；普通日志只记录长度与退出码，
        // 避免把密码、Token、短信正文或完整 AT 参数写进 tracing。
        tracing::debug!(
            target: "audit",
            command_len = command.len(),
            exit_code = output.status.code().unwrap_or(-1),
            "web_shell_executed"
        );

        Ok(ShellResult {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
            duration_ms: started.elapsed().as_millis() as u64,
            truncated,
        })
    }
}

/// 交互式 shell 句柄。Drop 时整组终止，杜绝会话泄漏。
pub struct InteractiveShell {
    pub child: Child,
    pub pid: u32,
    pub policy: ShellPolicy,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl InteractiveShell {
    pub fn take_streams(
        &mut self,
    ) -> AppResult<(
        tokio::process::ChildStdin,
        tokio::process::ChildStdout,
        tokio::process::ChildStderr,
    )> {
        let stdin = self
            .child
            .stdin
            .take()
            .ok_or_else(|| internal("缺少 stdin"))?;
        let stdout = self
            .child
            .stdout
            .take()
            .ok_or_else(|| internal("缺少 stdout"))?;
        let stderr = self
            .child
            .stderr
            .take()
            .ok_or_else(|| internal("缺少 stderr"))?;
        Ok((stdin, stdout, stderr))
    }
}

impl Drop for InteractiveShell {
    fn drop(&mut self) {
        kill_process_group(self.pid);
        let _ = self.child.start_kill();
    }
}

/// 凭据脱敏：命令原文需要进审计表，但不能把密码 / Token / Bearer 值写进日志。
pub fn redact_sensitive(input: &str) -> String {
    const SENSITIVE_KEYS: &[&str] = &[
        "password", "passwd", "pwd", "token", "secret", "api_key", "apikey",
        "authorization", "access_key", "private_key",
    ];
    const FLAG_TAKES_VALUE: &[&str] = &["-p", "--password", "--token", "bearer"];

    let parts: Vec<&str> = input.split_whitespace().collect();
    let mut out = String::with_capacity(input.len());
    let mut skip_next = false;

    for (i, tok) in parts.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if skip_next {
            out.push_str("***");
            skip_next = false;
            continue;
        }
        let lower = tok.to_ascii_lowercase();
        if FLAG_TAKES_VALUE.contains(&lower.as_str()) {
            out.push_str(tok);
            skip_next = true;
            continue;
        }
        if let Some(pos) = tok.find('=') {
            let key = tok[..pos].to_ascii_lowercase();
            if SENSITIVE_KEYS.iter().any(|k| key.contains(k)) {
                out.push_str(&tok[..=pos]);
                out.push_str("***");
                continue;
            }
        }
        if SENSITIVE_KEYS.iter().any(|k| lower.contains(k)) {
            out.push_str("***");
            continue;
        }
        out.push_str(tok);
    }
    out
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn gen_id() -> String {
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
    fn redacts_credentials_but_keeps_command_shape() {
        let a = redact_sensitive("curl -H 'Authorization: Bearer abc123' http://x");
        assert!(!a.contains("abc123"), "Bearer 值必须脱敏: {a}");
        assert!(a.starts_with("curl"), "命令形状应保留: {a}");

        let b = redact_sensitive("useradd -p hunter2 bob");
        assert!(!b.contains("hunter2"), "密码必须脱敏: {b}");
        assert!(b.contains("useradd"));

        let c = redact_sensitive("echo token=abcdef >> /tmp/f");
        assert!(!c.contains("abcdef"), "token 值必须脱敏: {c}");

        assert_eq!(redact_sensitive("ls -la /tmp"), "ls -la /tmp");
    }

    #[test]
    fn default_policy_keeps_shell_enabled() {
        let p = ShellPolicy::default();
        assert!(p.enabled, "用户要求保留 Web 任意 Shell");
        assert!(p.max_concurrent > 0);
        assert!(p.max_output_bytes > 0);
    }
}
