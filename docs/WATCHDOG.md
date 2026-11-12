# 看门狗与自愈机制（3.1.0）

需求：**防止前端和后端死掉或者假死**。方案分成三层，任何一层卡死都不会连带其他层一起失效。

| 层 | 实现 | 检测对象 | 自愈动作 |
| --- | --- | --- | --- |
| 后端进程级 | `crates/services/src/watchdog.rs`（独立 OS 线程） | Tokio runtime 假死（调度器不再推进任何任务） | 以退出码 75 结束进程，由 systemd / procd 重新拉起 |
| 前端 UI 线程级 | `web/js/core/watchdog.js` 心跳漂移 | 浏览器主线程长时间无响应（死循环 / 同步阻塞） | 提示条 + 宽限期后自动 `location.reload()`，可取消 / 立即刷新 |
| 前后端连接级 | `web/js/core/watchdog.js` 健康探测 | 后端失联、`/api/health` 连续超时 | 持续重试；恢复后自动重建 WS / SSE 实时通道 |

---

## 1. 后端看门狗

### 结构与原理

```
Tokio runtime (worker threads)
   └── feeder 任务 ── 每 2s ──> WatchdogState.last_beat_ms（AtomicU64）
                                      ▲
独立 OS 线程 udx-watchdog ────────────┘ 每 1s 读取
   └── 超过 stall_after_secs 未更新 → trip() 一次 → exit(75)
                                              ↓
                        systemd Restart=on-failure / procd respawn → 新进程
```

**关键点**：monitor 是不进入 Tokio 调度器的 `std::thread`。当 runtime 真卡死时，feeder 也会停摆（这正是判定依据）；若 monitor 也在 runtime 内，它会一起卡住，看门狗就失去意义。

**为什么不走优雅关闭**：runtime 已假死时，`shutdown` 广播、`axum::graceful_shutdown`、`tokio::signal` 都无法被调度执行。唯一可靠路径是让进程退出，交给进程外的 supervisor 重启。

### 防误杀设计

| 风险 | 处理 |
| --- | --- |
| 启动阶段尚未喂狗 | `WatchdogState::new()` 初始化时先喂一次，保证完整 `stall_after_secs` 宽限期 |
| 正常停机被误判 | `AppServices::shutdown()` 先调用 `watchdog.stop()`，monitor 检测到 `stopped` 立即退出 |
| 测试 / runtime 销毁后残留线程误杀进程 | monitor 只持有 `Weak<WatchdogState>`；`AppServices` 释放后 `upgrade()` 失败即自行退出 |
| 重复触发 | `trip()` 用 `compare_exchange` 保证只执行一次 |
| 无 supervisor 的开发环境 | `action = "log"`：只写告警 + `/api/health` 标记 `stalled`，不结束进程 |
| 心跳写日志 | 只写原子时间戳，不记录 Token / 命令 / 请求 URI |

### 配置（`config.toml`）

```toml
[watchdog]
enabled = true
heartbeat_interval_secs = 2     # feeder 喂狗间隔
stall_after_secs = 30           # 超过该时长无心跳判定假死
check_interval_secs = 1         # monitor 检查间隔
action = "exit"                 # "exit" 配合 supervisor 重启；"log" 仅告警
```

完整样例见 `config.example.toml`。`action` 大小写不敏感。

### 状态观测

`GET /api/health`（公开只读）在保留旧字段的基础上新增：

```json
{
  "status": "ok", "version": "3.0.0", "uptime_secs": 42,
  "db_ok": true, "dbus_ok": false, "active_tasks": 0,
  "watchdog": {
    "enabled": true,
    "stalled": false,
    "tripped": false,
    "stopped": false,
    "action": "exit",
    "age_ms": 1203,
    "stall_after_ms": 30000,
    "heartbeats": 22,
    "monitor": "os-thread",
    "exit_code": 75
  }
}
```

字段不含任何凭证或设备敏感信息，可被前端、外部监控或 supervisor 安全读取。

### 单测覆盖

`cargo test -p udx-services watchdog::` 覆盖：启动宽限期、阈值边界（恰好等于阈值不算超时）、喂狗重置、禁用态永不超时、trip 只触发一次、状态字段与退出码。测试用 `beat_at(now)` / `is_stalled_at(now)` 注入时间，不依赖真实等待。

---

## 2. 前端看门狗

`web/js/core/watchdog.js` 在 `app.js::boot()` 末尾启动，参数可用 `watchdog.start(options)` 覆盖。

| 检测 | 默认参数 | 行为 |
| --- | --- | --- |
| UI 主线程假死 | 1s 心跳 tick，漂移 > 8000ms 判定 | 显示错误提示条「界面曾无响应 X 秒，即将自动刷新」，3s 后自动 reload；提供「立即刷新」与「暂不刷新」 |
| 后端失联 | 每 15s 探测 `/api/health`，5s 超时，连续 3 次失败 | 显示警告条「后端连续 N 次无响应，正在持续重试」，提供「立即重试」 |
| 后端恢复 | 探测成功后 | 关闭并重连 `realtime`（WS / SSE 自行降级），显示「后端已恢复连接」2.6s 后自动收起 |
| 后端看门狗超时 | 读取 `health.watchdog.stalled` | 显示「后端心跳超时，守护进程正在处理」 |
| 后台标签页 | `document.hidden` 时跳过漂移判定，回到前台重置心跳并立即探测一次 | 避免浏览器节流定时器导致误报 |
| 网络恢复 | `window.online` | 清零失败计数并立即探测 |

- 提示条 DOM 已在 `web/index.html` 预置，样式在 `web/css/glass.css`（`.watchdog-banner`），位置适配底部安全区。
- 不写 `localStorage`、不注入内联样式，兼容严格 CSP。
- `getStatus()` 返回 `{ started, backendUp, frozen, failures, lastTickAgeMs }` 便于调试。

**前端无法重启后端进程**，因此连接级只做「持续探测 + 恢复后重建实时通道」；真正的进程级重启由后端看门狗 + supervisor 完成。

---

## 3. 部署：让退出码 75 变成重启

### systemd（通用 Linux）

`packaging/udx710-tools.service`：

```ini
Restart=on-failure
RestartSec=3
```

退出码 75 非 0，触发重启；优雅关闭（SIGTERM → exit 0）不会重启。若希望更激进可改 `Restart=always`，但会掩盖主动停机意图，默认不启用。

### OpenWrt / procd

`packaging/udx710-tools.init`：

```sh
USE_PROCD=1
procd_set_param respawn 3600 3 0
```

`respawn <threshold> <delay> <retries>`：连续运行超过 3600s 重置重试计数，重启间隔 3s，重试次数不限（0）。

安装：

```sh
cp packaging/udx710-tools.init /etc/init.d/udx710-tools
chmod 755 /etc/init.d/udx710-tools
/etc/init.d/udx710-tools enable && /etc/init.d/udx710-tools start
```

### 无 supervisor 的场景

直接 `./start.sh` 运行时请把 `action` 改为 `"log"`，否则假死会直接结束进程而无人拉起。此时仍可通过 `/api/health` 的 `watchdog.stalled` 观测到异常，由外部脚本决定处理方式。

---

## 4. 边界与真机待验证

- **未在真机验证**：runtime 真实死锁场景下的退出与 respawn 时延、supervisor 重启风暴、OTA 期间看门狗与升级进程的交互。建议真机上人为制造一次阻塞（例如临时把 `stall_after_secs` 调小）验证闭环。
- 看门狗只覆盖「进程 / 主线程级」假死；单个 API 请求超时由 `limits.request_timeout_secs` 与 Shell / AT 命令级超时分别兜底。
- 前端自动 `reload()` 会丢失当前页面未提交的表单内容，因此提供「暂不刷新」入口；危险操作确认框在刷新后需要重新确认。
