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
use std::os::fd::FromRawFd;
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
    ///
    /// 使用伪终端（PTY）而不是纯管道：管道模式下 shell 不输出提示符、不回显输入、
    /// 没有作业控制，`vim`/`top`/`Ctrl-C` 全部失效，用户会误判为「终端不可用」。
    /// PTY 是交互式 shell 的正常载体，且不引入任何新的系统依赖（libc 已在用）。
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

        let (master, slave) = open_pty()?;

        let mut cmd = Command::new("/bin/sh");
        // 子进程三路都指向 PTY 从端；PTY 天然把 stderr 合并进同一流。
        let stdio = |fd: std::os::fd::RawFd| -> Stdio {
            // SAFETY: fd 由 dup 得到，所有权转移给 File，由 File 负责关闭。
            Stdio::from(unsafe { std::fs::File::from_raw_fd(fd) })
        };
        let stdin_fd = dup_fd(slave).map_err(|e| {
            close_fd(master);
            close_fd(slave);
            e
        })?;
        let stdout_fd = dup_fd(slave).map_err(|e| {
            close_fd(master);
            close_fd(slave);
            e
        })?;
        let stderr_fd = dup_fd(slave).map_err(|e| {
            close_fd(master);
            close_fd(slave);
            e
        })?;

        cmd.stdin(stdio(stdin_fd))
            .stdout(stdio(stdout_fd))
            .stderr(stdio(stderr_fd))
            .env("TERM", "xterm-256color")
            .kill_on_drop(true);

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // 注意：PTY 模式不能再调用 process_group(0)。那会把子进程设为进程组
            // 领导者，而 setsid() 对已是组领导的进程会返回 EPERM，导致 spawn 失败
            //（表现为 /api/terminal/ws 直接 500）。setsid 本身就会新建会话与进程组，
            // 因此 kill_process_group(pid) 依然有效。
            // SAFETY: pre_exec 内只调用 async-signal-safe 的 setsid/ioctl。
            unsafe {
                cmd.as_std_mut().pre_exec(|| {
                    if libc::setsid() < 0 {
                        let err = std::io::Error::last_os_error();
                        // 已是会话首进程时 EPERM 可忽略；其余错误才算失败。
                        if err.raw_os_error() != Some(libc::EPERM) {
                            return Err(err);
                        }
                    }
                    if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }

        let spawned = cmd
            .spawn()
            .map_err(|e| internal(format!("启动交互式 shell 失败: {e}")));
        // 无论成败，父进程都要关掉从端，否则 master 永远读不到 EOF。
        close_fd(slave);
        let child = match spawned {
            Ok(c) => c,
            Err(e) => {
                close_fd(master);
                return Err(e);
            }
        };
        let pid = child.id().unwrap_or(0);

        // 初始尺寸给一个常规值，前端连上后会立刻发 resize 覆盖。
        let _ = apply_winsize(master, 120, 32);

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
            master,
            streams_taken: false,
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
    /// PTY 主端 fd，用于 `TIOCSWINSZ`；-1 表示已关闭。
    master: std::os::fd::RawFd,
    streams_taken: bool,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

/// 终端写端（PTY 主端 dup）。
pub type ShellWriter = Box<dyn tokio::io::AsyncWrite + Unpin + Send>;
/// 终端读端（PTY 主端 dup）。PTY 会把 stderr 合并进同一流，因此不再单列 stderr。
pub type ShellReader = Box<dyn tokio::io::AsyncRead + Unpin + Send>;

impl InteractiveShell {
    /// 取走 PTY 的读写端。读/写各持一份 dup 出来的 fd，与 master 原 fd 独立，
    /// 因此可以在读取的同时用原 fd 做 `TIOCSWINSZ`。
    pub fn take_streams(&mut self) -> AppResult<(ShellWriter, ShellReader)> {
        if self.streams_taken {
            return Err(internal("终端数据流已被取走"));
        }
        let read_fd = dup_fd(self.master)?;
        let write_fd = match dup_fd(self.master) {
            Ok(fd) => fd,
            Err(e) => {
                close_fd(read_fd);
                return Err(e);
            }
        };
        self.streams_taken = true;
        // SAFETY: 两个 fd 均由 dup 得到且成功，所有权转移给 File。
        let reader = tokio::fs::File::from_std(unsafe { std::fs::File::from_raw_fd(read_fd) });
        let writer = tokio::fs::File::from_std(unsafe { std::fs::File::from_raw_fd(write_fd) });
        Ok((Box::new(writer) as ShellWriter, Box::new(reader) as ShellReader))
    }

    /// 下发终端窗口尺寸（`TIOCSWINSZ`）。PTY 下这会向 shell 触发 SIGWINCH，
    /// 使 `top`/`vim` 等全屏程序能正确重排。
    pub fn resize(&self, cols: u16, rows: u16) -> AppResult<()> {
        apply_winsize(self.master, cols, rows)
    }
}

impl Drop for InteractiveShell {
    fn drop(&mut self) {
        kill_process_group(self.pid);
        let _ = self.child.start_kill();
        // master 是最后一个持有者，关闭后内核才会回收 PTY。
        close_fd(self.master);
        self.master = -1;
    }
}

/// 打开一对伪终端，返回 (master, slave) 原始 fd。
fn open_pty() -> AppResult<(std::os::fd::RawFd, std::os::fd::RawFd)> {
    let mut master: libc::c_int = -1;
    let mut slave: libc::c_int = -1;
    // SAFETY: 标准 openpty 用法；成功后两个 fd 由调用方负责关闭。
    let rc = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if rc != 0 || master < 0 || slave < 0 {
        return Err(internal(format!(
            "创建 PTY 失败: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok((master, slave))
}

fn dup_fd(fd: std::os::fd::RawFd) -> AppResult<std::os::fd::RawFd> {
    if fd < 0 {
        return Err(internal("无效的文件描述符"));
    }
    // SAFETY: dup 返回新的 fd，失败返回 -1。
    let new_fd = unsafe { libc::dup(fd) };
    if new_fd < 0 {
        return Err(internal(format!(
            "复制文件描述符失败: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(new_fd)
}

fn close_fd(fd: std::os::fd::RawFd) {
    if fd >= 0 {
        // SAFETY: fd 由本模块创建，且只在关闭后置 -1，不会重复关闭同一值。
        unsafe {
            libc::close(fd);
        }
    }
}

fn apply_winsize(fd: std::os::fd::RawFd, cols: u16, rows: u16) -> AppResult<()> {
    if fd < 0 {
        return Err(internal("PTY 已关闭"));
    }
    let ws = libc::winsize {
        ws_row: rows.max(1),
        ws_col: cols.max(1),
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: ws 是栈上有效结构，fd 是已打开的 pty master。
    let rc = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ as _, &ws) };
    if rc != 0 {
        return Err(internal(format!(
            "设置终端尺寸失败: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
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
