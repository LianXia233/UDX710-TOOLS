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
    // IPv4 需要内核地址表，/proc/net/dev 只有计数；一次 getifaddrs 取全量，
    // 避免为每个接口派生子进程。
    let ipv4 = ipv4_map();
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
            mac: read_sysfs(&name, "address").await,
            up: read_flags_up(&name).await,
            mtu: read_sysfs(&name, "mtu").await.parse::<u64>().unwrap_or(0),
            ipv4: ipv4.get(&name).cloned().unwrap_or_default(),
            name,
            rx_bytes: nums[0],
            tx_bytes: nums[8],
            monitored: false,
            extra: BTreeMap::new(),
        });
    }
    Ok(result)
}

/// 读 `/sys/class/net/<if>/<attr>`，失败返回空串（不视为错误）。
async fn read_sysfs(ifname: &str, attr: &str) -> String {
    fs::read_to_string(format!("/sys/class/net/{ifname}/{attr}"))
        .await
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// `flags` 是十六进制位掩码，IFF_UP = 0x1。比 operstate 可靠：
/// SIPA 等点对点接口常报 `unknown`，但数据实际可通。
async fn read_flags_up(ifname: &str) -> bool {
    let raw = read_sysfs(ifname, "flags").await;
    let hex = raw.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(hex, 16)
        .map(|v| v & libc::IFF_UP as u64 != 0)
        .unwrap_or(false)
}

/// 通过 `getifaddrs` 取全部 IPv4，返回 接口名 -> 点分十进制。
/// 调用本身在微秒级完成，不引入阻塞线程池开销。
fn ipv4_map() -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    // SAFETY: 标准 libc 用法。getifaddrs 成功后由我们负责 freeifaddrs，
    // 遍历期间链表由内核维护、不会被并发释放。
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return map;
        }
        let mut cur = head;
        while !cur.is_null() {
            let ifa = &*cur;
            if !ifa.ifa_addr.is_null() && (*ifa.ifa_addr).sa_family as i32 == libc::AF_INET {
                let name = std::ffi::CStr::from_ptr(ifa.ifa_name)
                    .to_string_lossy()
                    .into_owned();
                let sin = ifa.ifa_addr as *const libc::sockaddr_in;
                let raw = u32::from_be((*sin).sin_addr.s_addr);
                map.insert(
                    name,
                    format!(
                        "{}.{}.{}.{}",
                        (raw >> 24) & 0xff,
                        (raw >> 16) & 0xff,
                        (raw >> 8) & 0xff,
                        raw & 0xff
                    ),
                );
            }
            cur = ifa.ifa_next;
        }
        libc::freeifaddrs(head);
    }
    map
}

/// 读 SoC 芯片温度（摄氏度）。
///
/// sysfs 的 `temp` 是毫摄氏度；优先选 type 含 `soc` 的 zone（本机 zone0 =
/// `soc-thmzone`），否则退回 zone0。全部不可读时返回 None，由上层决定如何降级。
async fn read_temperature() -> Option<f64> {
    let mut fallback: Option<f64> = None;
    let mut soc: Option<f64> = None;
    for zone in 0..12u32 {
        let base = format!("/sys/class/thermal/thermal_zone{zone}");
        let Ok(raw) = fs::read_to_string(format!("{base}/temp")).await else {
            continue;
        };
        let Ok(milli) = raw.trim().parse::<i64>() else {
            continue;
        };
        let celsius = milli as f64 / 1000.0;
        // 明显越界视为该节点无效，继续找下一个。
        if !(-40.0..=150.0).contains(&celsius) {
            continue;
        }
        let kind = fs::read_to_string(format!("{base}/type"))
            .await
            .unwrap_or_default()
            .to_lowercase();
        let value = (celsius * 10.0).round() / 10.0;
        if kind.contains("soc") || kind.contains("cpu") || kind.contains("apcpu") {
            if soc.is_none() {
                soc = Some(value);
            }
        } else if fallback.is_none() {
            fallback = Some(value);
        }
    }
    soc.or(fallback)
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
        temperature_c: read_temperature().await,
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
