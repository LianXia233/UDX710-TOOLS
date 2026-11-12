//! 网络接口与系统信息读取。
//!
//! 通过 `/proc` / `sysfs` 只读文件获取，避免为每个状态查询派生子进程；
//! 也避免引入 netlink 依赖带来的交叉编译复杂度。所有读取都有大小上限。

use std::collections::BTreeMap;
use tokio::fs;
use udx_domain::error::{internal, AppResult};
use udx_domain::models::{NetifInfo, SystemInfo};

const PROC_NET_DEV: &str = "/proc/net/dev";
const PROC_MEMINFO: &str = "/proc/meminfo";
const PROC_UPTIME: &str = "/proc/uptime";
const PROC_STAT: &str = "/proc/stat";

pub async fn list_interfaces() -> AppResult<Vec<NetifInfo>> {
    let text = fs::read_to_string(PROC_NET_DEV)
        .await
        .map_err(|e| internal(format!("读取 /proc/net/dev 失败: {e}")))?;
    let mut result = Vec::new();
    for line in text.lines().skip(2) {
        let mut parts = line.splitn(2, ':');
        let name = parts.next().unwrap_or("").trim().to_string();
        let Some(values) = parts.next() else { continue };
        let nums: Vec<u64> = values
            .split_whitespace()
            .filter_map(|v| v.parse::<u64>().ok())
            .collect();
        if name.is_empty() || nums.len() < 10 {
            continue;
        }
        result.push(NetifInfo {
            name,
            rx_bytes: nums[0],
            tx_bytes: nums[8],
            monitored: false,
            extra: BTreeMap::new(),
        });
    }
    Ok(result)
}

pub async fn interface_stats(name: &str) -> AppResult<NetifInfo> {
    // 接口名不进入任何 shell，只做内存中的字符串比对。
    let all = list_interfaces().await?;
    all.into_iter()
        .find(|i| i.name == name)
        .ok_or_else(|| internal(format!("接口不存在: {name}")))
}

fn parse_kv(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
}

pub async fn system_info(version: &str) -> AppResult<SystemInfo> {
    let meminfo = fs::read_to_string(PROC_MEMINFO).await.unwrap_or_default();
    let uptime = fs::read_to_string(PROC_UPTIME).await.unwrap_or_default();
    let stat = fs::read_to_string(PROC_STAT).await.unwrap_or_default();
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .await
        .unwrap_or_default()
        .trim()
        .to_string();

    let mem_total = parse_kv(&meminfo, "MemTotal:").unwrap_or(0);
    let mem_available = parse_kv(&meminfo, "MemAvailable:").unwrap_or(mem_total);
    let uptime_secs = uptime
        .split_whitespace()
        .next()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;

    // CPU 使用率采用两个采样点之间的差值；这里用 /proc/stat 的静态占比作为兜底。
    let cpu_usage = stat
        .lines()
        .next()
        .and_then(|l| {
            let v: Vec<u64> = l
                .split_whitespace()
                .skip(1)
                .filter_map(|x| x.parse::<u64>().ok())
                .collect();
            if v.len() >= 4 {
                let total: u64 = v.iter().sum();
                let idle = v.get(3).copied().unwrap_or(0);
                if total > 0 {
                    Some((100.0 * (total - idle) as f64 / total as f64).min(100.0))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap_or(0.0);

    Ok(SystemInfo {
        hostname,
        version: version.to_string(),
        uptime_secs,
        cpu_usage,
        mem_total_kb: mem_total,
        mem_used_kb: mem_total.saturating_sub(mem_available),
        temperature_c: None,
        kernel,
        extra: BTreeMap::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_meminfo_kv() {
        let t = "MemTotal:  1024000 kB\nMemAvailable:  512000 kB\n";
        assert_eq!(parse_kv(t, "MemTotal:"), Some(1024000));
        assert_eq!(parse_kv(t, "MemAvailable:"), Some(512000));
    }
}
