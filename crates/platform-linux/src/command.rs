//! 内部设备动作的受控命令执行（报告 6.3「子进程执行规范」）。
//!
//! 关键约束：
//! - **命令 ID 白名单 + 固定 argv**，可执行文件固定，绝不经过 `sh -c`；
//! - 追加参数必须经 `validate_arg` 严格校验，禁止 Shell 元字符与路径穿越；
//! - `tokio::time::timeout` + `kill_on_drop(true)` + 独立进程组 + 超时显式 kill 组；
//! - stdout/stderr 输出上限，防止子进程输出耗尽内存；
//! - stdin 关闭，避免子进程挂起等待输入。
//!
//! 注意：这是「设备内部动作」通道。**Web 任意 Shell 是另一条独立通道**
//! （见 `shell.rs`），用户明确要求保留，二者权限边界不同，不得混用。

use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;
use udx_domain::error::{bad_request, internal, AppError, AppResult};

pub const DEFAULT_MAX_OUTPUT: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub truncated: bool,
}

impl CommandOutput {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "stdout": self.stdout,
            "stderr": self.stderr,
            "exit_code": self.exit_code,
            "duration_ms": self.duration_ms,
            "truncated": self.truncated,
        })
    }
}

/// 设备动作命令 ID。新增动作必须在此登记，不能由请求动态指定可执行文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedCommand {
    IpLinkShow,
    IpAddrShow,
    IpRouteShow,
    IpLinkSet,
    Ntpdate,
    Sysctl,
    DateSet,
    Reboot,
    Poweroff,
    Vnstat,
    CatReadOnly,
    UsbGadgetWrite,
}

impl FixedCommand {
    pub fn program(self) -> &'static str {
        match self {
            Self::IpLinkShow | Self::IpAddrShow | Self::IpRouteShow | Self::IpLinkSet => "/sbin/ip",
            Self::Ntpdate => "/usr/sbin/ntpdate",
            Self::Sysctl => "/sbin/sysctl",
            Self::DateSet => "/bin/date",
            Self::Reboot => "/sbin/reboot",
            Self::Poweroff => "/sbin/poweroff",
            Self::Vnstat => "/usr/bin/vnstat",
            Self::CatReadOnly => "/bin/cat",
            Self::UsbGadgetWrite => "/bin/sh",
        }
    }

    pub fn argv(self) -> &'static [&'static str] {
        match self {
            Self::IpLinkShow => &["-j", "link", "show"],
            Self::IpAddrShow => &["-j", "addr", "show"],
            Self::IpRouteShow => &["-j", "route", "show"],
            Self::IpLinkSet => &["link", "set"],
            Self::Ntpdate => &["ntp.aliyun.com"],
            Self::Sysctl => &[],
            Self::DateSet => &[],
            Self::Reboot | Self::Poweroff => &[],
            Self::Vnstat => &["--json"],
            Self::CatReadOnly => &[],
            // USB gadget 切换使用固定的设备侧脚本，参数仅为模式枚举。
            Self::UsbGadgetWrite => &["/usr/sbin/udx-usb-mode.sh"],
        }
    }

    pub fn allows_extra(self) -> bool {
        matches!(
            self,
            Self::IpLinkSet
                | Self::Sysctl
                | Self::DateSet
                | Self::Vnstat
                | Self::CatReadOnly
                | Self::UsbGadgetWrite
        )
    }

    pub fn timeout(self) -> Duration {
        match self {
            Self::Ntpdate => Duration::from_secs(15),
            Self::Reboot | Self::Poweroff => Duration::from_secs(5),
            Self::UsbGadgetWrite => Duration::from_secs(20),
            _ => Duration::from_secs(5),
        }
    }

    /// 追加参数的类别化校验规则。
    fn validate_extra(self, arg: &str) -> AppResult<()> {
        if self == Self::CatReadOnly {
            if !arg.starts_with("/proc/") && !arg.starts_with("/sys/") {
                return Err(bad_request("只读路径必须位于 /proc 或 /sys"));
            }
            if arg.contains("..") {
                return Err(bad_request("只读路径包含非法片段"));
            }
        }
        if self == Self::UsbGadgetWrite {
            // 只允许三种模式枚举，杜绝注入任何 shell 片段。
            if !matches!(arg, "ecm" | "ncm" | "rndis" | "status") {
                return Err(bad_request("USB 模式参数非法"));
            }
        }
        Ok(())
    }
}

/// 通用参数 allowlist。即便不经过 Shell，也做纵深防御，
/// 拒绝空白、控制字符、Shell 元字符与超长参数。
pub fn validate_arg(id: FixedCommand, arg: &str) -> AppResult<()> {
    if arg.is_empty() || arg.len() > 128 {
        return Err(bad_request("参数为空或过长"));
    }
    if arg.chars().any(|c| c.is_control() || c == '\0') {
        return Err(bad_request("参数包含控制字符"));
    }
    const FORBIDDEN: &[char] = &[
        '$', '`', ';', '|', '&', '<', '>', '(', ')', '{', '}', '\'', '"', '\\', '\n', '\r',
        '*', '?', '!', '#', '~',
    ];
    if arg.chars().any(|c| FORBIDDEN.contains(&c)) {
        return Err(bad_request("参数包含被禁止的字符"));
    }
    id.validate_extra(arg)
}

#[derive(Debug, Clone)]
pub struct CommandRunner {
    pub default_timeout: Duration,
    pub max_output: usize,
}

impl Default for CommandRunner {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(5),
            max_output: DEFAULT_MAX_OUTPUT,
        }
    }
}

impl CommandRunner {
    pub fn new(default_timeout: Duration, max_output: usize) -> Self {
        Self {
            default_timeout,
            max_output,
        }
    }

    /// 执行白名单命令。返回结构化输出，永不把原始异常回显给客户端。
    pub async fn run(&self, id: FixedCommand, extra: &[String]) -> AppResult<CommandOutput> {
        if !id.allows_extra() && !extra.is_empty() {
            return Err(bad_request("该命令不接受附加参数"));
        }
        for arg in extra {
            validate_arg(id, arg)?;
        }

        let mut cmd = Command::new(id.program());
        cmd.args(id.argv()).args(extra);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // 独立进程组：超时后可整组 kill，避免遗留孙进程。
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.as_std_mut().process_group(0);
        }

        let started = Instant::now();
        let child = cmd
            .spawn()
            .map_err(|e| internal(format!("启动 {} 失败: {e}", id.program())))?;
        let pid = child.id().unwrap_or(0);

        // wait_with_output 会接管 stdout/stderr 并等待退出；
        // 超时后 future 被丢弃，child 随之 drop，kill_on_drop 生效。
        let wait = child.wait_with_output();
        let limit = id.timeout().min(self.default_timeout);
        let output = match timeout(limit, wait).await {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => return Err(internal(format!("等待子进程失败: {e}"))),
            Err(_) => {
                kill_process_group(pid);
                return Err(AppError::Timeout(format!(
                    "命令 {} 执行超过 {:?}",
                    id.program(),
                    limit
                )));
            }
        };

        let mut truncated = false;
        let mut stdout = output.stdout;
        let mut stderr = output.stderr;
        if stdout.len() > self.max_output {
            stdout.truncate(self.max_output);
            truncated = true;
        }
        if stderr.len() > self.max_output {
            stderr.truncate(self.max_output);
            truncated = true;
        }

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
            duration_ms: started.elapsed().as_millis() as u64,
            truncated,
        })
    }
}

/// 向整个进程组发 SIGKILL。进程组 ID 等于子进程 PID（process_group(0)）。
#[cfg(unix)]
pub fn kill_process_group(pid: u32) {
    if pid == 0 {
        return;
    }
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
pub fn kill_process_group(_pid: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_metacharacters_are_rejected() {
        for s in ["a;rm -rf /", "$(id)", "`id`", "a|b", "a\nb", "a'b"] {
            assert!(validate_arg(FixedCommand::Sysctl, s).is_err(), "{s}");
        }
        assert!(validate_arg(FixedCommand::Sysctl, "net.ipv4.ip_forward=1").is_ok());
    }

    #[test]
    fn usb_mode_extra_is_enum_only() {
        assert!(validate_arg(FixedCommand::UsbGadgetWrite, "ncm").is_ok());
        assert!(validate_arg(FixedCommand::UsbGadgetWrite, "ncm;reboot").is_err());
    }

    #[test]
    fn readonly_cat_is_confined_to_proc_sys() {
        assert!(validate_arg(FixedCommand::CatReadOnly, "/proc/net/dev").is_ok());
        assert!(validate_arg(FixedCommand::CatReadOnly, "/etc/shadow").is_err());
    }
}
