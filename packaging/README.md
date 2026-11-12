# UDX710-TOOLS 部署与打包说明

本目录提供 Rust 重构版（`udx-app`）在 UDX710 设备（ARM64 / aarch64）上的部署物料。
CI 产物为 GitHub Actions artifact `udx710-tools-aarch64`，其内容与本目录打包结果一致。

## 1. 产物说明：单二进制 + 内嵌静态资源

- `server` 是唯一的可执行文件，由 workspace 中 `-p udx-app` 交叉编译后重命名而来
  （原始路径 `target/aarch64-unknown-linux-gnu/release/udx-app`）。
- 前端 `web/`（HTML5 / CSS3 / 原生 ES Modules）在**编译期**由 `rust-embed` 内嵌进 `server` 二进制，
  运行时直接由 Axum 提供静态资源，**不需要** `web/dist/`、Node.js、Vite 或任何 npm 步骤。
- 因此交付物很小：一个 `server` 加一个 `start.sh`；`udx710-tools.service` 与本文档为可选部署辅助。
- 静态资源一旦变更必须重新编译 `server`；设备端没有独立的静态文件目录可热替换。
- release 体积优化（`opt-level = "s"` / `lto = true` / `codegen-units = 1` / `panic = "abort"` /
  `strip = true`）见根 `Cargo.toml` 的 `[profile.release]` 与 CI workflow 顶部注释（报告第 09 章 9.5）。

## 2. 部署目录结构

部署到设备的 `/home/root/6677/`：

```
/home/root/6677/
├── server                  # 主程序（由 udx-app 重命名，chmod 755）
├── server.bak              # 上一版备份，回滚用（部署前生成）
├── start.sh                # 启动脚本（chmod 755）
├── udx710-tools.service    # systemd unit 副本（实际安装到 /etc/systemd/system/）
├── README.md               # 本说明
├── data/                   # 运行时生成：SQLite、配置、按插件 ID 隔离的插件数据
└── Plugins/                # 兼容期保留的旧版插件 / 脚本目录
    ├── plugins/
    └── scripts/
```

`start.sh` 会 `cd` 到自身所在目录并 `exec ./server`，因此可从任意工作目录被 systemd 调用。

## 3. 端口

- 默认监听 **6677**（沿用旧版）。
- 覆盖方式：`./start.sh 8080` 或 `UDX710_PORT=8080 ./start.sh`。
- `start.sh` 把端口写入环境变量 `UDX710_PORT` 并透传首个位置参数；server 以 `UDX710_PORT` 为权威来源。
- systemd 默认不传参、使用 6677；如需改端口，在 unit 的 `ExecStart` 后追加参数，
  或添加 `Environment=UDX710_PORT=...`。
- 变更端口后需同步调整设备防火墙 / 访问入口，并确认前端 API 基址与监听地址一致。

## 4. systemd 启用

```sh
sudo cp /home/root/6677/udx710-tools.service /etc/systemd/system/udx710-tools.service
sudo systemctl daemon-reload
sudo systemctl enable --now udx710-tools
sudo systemctl status udx710-tools
sudo journalctl -u udx710-tools -f
```

- `Restart=on-failure` 加 `RestartSec=3`：异常退出 3 秒后自动拉起。
- 加固项，以及「为何不启用 PrivateDevices / PrivateNetwork / ProtectHome」，见 unit 内注释；
  设备访问（D-Bus、/dev、sysfs、netlink）优先级高于沙箱收紧。
- 健康检查：`systemctl is-active udx710-tools` 与 `ss -ltnp | grep 6677`。

## 5. 回滚步骤（保留 server.bak）

核心原则：**先备份、再覆盖、后重启；任何一步失败都能退回上一版。**

```sh
cd /home/root/6677

# 1) 部署前：保留当前可用版本
cp server server.bak

# 2) 放入新版本，保持可执行权限
cp /path/to/artifact/server server.new
chmod 755 server.new
mv server.new server

# 3) 重启并观察
sudo systemctl restart udx710-tools
sudo journalctl -u udx710-tools -n 50 --no-pager
ss -ltnp | grep 6677

# 4) 若启动失败或健康检查不通过：回退
cp server.bak server
sudo systemctl restart udx710-tools
```

- `server.bak` 与 `server` 同为单文件，回滚只替换该文件；静态资源随二进制一起回退，不会新旧混用。
- 若升级过程中执行了数据库迁移，需按 `migrations/` 的可逆脚本回退后再启动旧版。
- 代码层回退点：每一次阶段提交 / tag 都在 git 历史中，`git revert` 或 `git checkout <tag>` 可回到旧版实现。

## 6. 本地复现 ARM64 交叉编译

与 `.github/workflows/build.yml` 完全一致：

```sh
sudo apt-get update
sudo apt-get install -y gcc-aarch64-linux-gnu
rustup target add aarch64-unknown-linux-gnu

export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc   # rusqlite(bundled) 编译 SQLite C 源码需要

cargo test --workspace                                      # 宿主测试
cargo build --release --target aarch64-unknown-linux-gnu -p udx-app

mkdir -p dist/6677
cp target/aarch64-unknown-linux-gnu/release/udx-app dist/6677/server
cp packaging/start.sh dist/6677/start.sh
cp packaging/udx710-tools.service dist/6677/udx710-tools.service
cp packaging/udx710-tools.init dist/6677/udx710-tools.init
cp config.example.toml dist/6677/config.example.toml
chmod 755 dist/6677/server dist/6677/start.sh dist/6677/udx710-tools.init
# CI 的 Package artifact 步骤已包含以上全部文件
```

在设备上核对产物架构：`file server` 应显示 `ARM aarch64`；
`readelf -h server | grep Machine` 应为 `AArch64`。

## 7. rustls 证书与 glibc / ABI 兼容性

### TLS / 证书

- HTTP 客户端使用 `reqwest` + `rustls-tls`，**不依赖 OpenSSL**；OTA / HTTPS 不再使用 `-k` /
  `--no-check-certificate`（报告第 08 章要求）。
- 目标固件必须提供可信 CA 根证书（通常是 `/etc/ssl/certs/ca-certificates.crt`），
  或由构建时内嵌的 webpki roots 提供。
- 上线前在真机验证：证书链校验正常；过期、域名不匹配、自签证书必须**失败关闭**；
  设备时钟错误会导致校验失败，需确认 RTC / NTP。
- rustls 与根证书随 `server` 一起升级；固件若裁剪过 CA 包需同步处理。

### glibc / ABI

- target `aarch64-unknown-linux-gnu` 为**动态链接**，运行时依赖目标固件的 glibc 与动态库。
- **构建机交叉工具链的 glibc 版本不得高于目标设备固件**，否则常见报错 `GLIBC_2.xx not found`。
  用 `aarch64-linux-gnu-gcc --version` 与设备上 `ldd --version` / `getconf GNU_LIBC_VERSION` 对比。
- 上机检查动态依赖：`readelf -d server | grep NEEDED`、设备上 `ldd server`，
  确认 `libc.so.6`、`libgcc_s.so.1` 等均可解析。
- `zbus` / D-Bus、`netlink`、sysfs 依赖目标系统运行时；报告第 09 章强调：
  **不要为「单文件」盲目全静态链接所有系统库**。
  若固件 glibc 过旧且无法升级，应使用与固件匹配的 sysroot 重建，
  或评估 `aarch64-unknown-linux-musl` 静态方案，但 musl 下 D-Bus / NSS / 动态插件行为需重新真机验证。

## 8. 验收提醒

报告第 09 / 11 章要求的 RSS / PSS、峰值内存、72 小时长稳、内存压力下 OOM 行为、OTA 回滚等，
均需在**真实 UDX710 设备**上实测；本地交叉编译成功不代表资源指标达标，
也不承诺任何具体内存节省数值。

## 9. 看门狗与自愈（3.1.0）

发布包内包含 `udx710-tools.service`（systemd）与 `udx710-tools.init`（OpenWrt procd），
配合 `config.toml` 的 `[watchdog]` 段实现「假死自动重启」。

```toml
[watchdog]
enabled = true
heartbeat_interval_secs = 2
stall_after_secs = 30
check_interval_secs = 1
action = "exit"        # 配合 supervisor；无 supervisor 时改为 "log"
```

| 场景 | 配置 | 行为 |
| --- | --- | --- |
| systemd 托管 | `action = "exit"` + `Restart=on-failure` | 假死 → exit(75) → 3 秒后重启 |
| OpenWrt procd | `action = "exit"` + `respawn 3600 3 0` | 假死 → exit(75) → procd 重新拉起 |
| 直接 `./start.sh` 调试 | `action = "log"` | 只告警 + `/api/health` 标记 `stalled`，不结束进程 |

检查状态：

```sh
curl -s http://127.0.0.1:6677/api/health | grep -o '"watchdog":{[^}]*}'
# 关注 stalled / tripped / age_ms / heartbeats
```

systemd 安装：

```sh
cp udx710-tools.service /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now udx710-tools
journalctl -u udx710-tools -f | grep watchdog
```

OpenWrt 安装：

```sh
cp udx710-tools.init /etc/init.d/udx710-tools
chmod 755 /etc/init.d/udx710-tools
/etc/init.d/udx710-tools enable && /etc/init.d/udx710-tools start
```

注意：

- 正常 `SIGTERM` 优雅关闭会先停止监控线程，退出码为 0，**不会**触发重启。
- 前端另有 `web/js/core/watchdog.js` 检测 UI 假死与后端失联，恢复后自动重建 WS / SSE。
- 真机需验证一次人为假死场景下的 respawn 时延，避免重启风暴；原理见 `docs/WATCHDOG.md`。

## 10. Yocto / 无 supervisor 设备（loader.sh + 外部看门狗）

UDX710 实机为 Yocto 系统，**既没有 systemd 也没有 procd**：`/etc/init.d` 位于只读 squashfs，
`crond` 仅以随包 `busybox-aarch64` 形式存在。此时第 4 节与第 9 节的两种 supervisor 都不可用，
必须走设备自带的 `/home/root/loader.sh` 钩子。

### 10.1 存储与自启事实（实测）

```
/dev/ubi0_userdata /mnt/userdata ubifs rw,ubi=0,vol=21
overlay /home overlay rw,upperdir=/mnt/userdata/home/upper,workdir=/mnt/userdata/home/work
overlay /var  overlay rw,upperdir=/mnt/userdata/var/upper,workdir=/mnt/userdata/var/work
```

- `/home` 与 `/var` 都是 overlay，可写层落在 `ubi0_21`（userdata 卷），**断电重启后保留**。
- `/etc` 为 squashfs 只读，无法新增 init 脚本，`/etc/init.d` 不可写。
- 现有开机链路：`/home/root/loader.sh` → `busybox-aarch64 crond` + `6677/start.sh` + `ttyd/start.sh`。
  多次重启实测该脚本均被执行（crond / server / ttyd 全部由它拉起），是设备上唯一可用的自启钩子。

### 10.2 推荐组合：内建 watchdog + 外部 shell 看门狗

| 层 | 职责 | 配置 |
| --- | --- | --- |
| `server` 内建 `[watchdog]` | runtime 假死 → `exit(75)` | `action = "exit"` |
| `/home/root/6677/panel_watchdog.sh` | 进程缺失 / 端口未监听 / HTTP 无响应 → 重启 | 10 秒周期、连续 2 次失败、30 秒冷却、20 秒启动宽限 |
| `/home/root/loader.sh` 末尾 | 开机拉起看门狗并重建 crontab | 复用原脚本的 `$DIR` 变量，不新增硬编码路径 |
| `/var/spool/cron/crontabs/root` | 每分钟兜底 | 脚本单例拦截，已有常驻实例则立即退出 |

两层互补：内建 watchdog 处理「runtime 卡死」，外部脚本处理「进程消失 / 端口未监听 / 整机重启后未自启」。
外部脚本的探活目标是静态首页 `/`，不请求 `/api/info`，避免探活本身触发 oFono/AT 查询。

### 10.3 部署步骤（A/B 灰度）

```sh
# 0) 控制机侧校验产物
sha256sum -c SHA256SUMS
file server                      # 期望 ARM aarch64 且 statically linked

# 1) 上传到临时目录，先不动运行文件
mkdir -p /tmp/udx-new
cp server /tmp/udx-new/ && chmod 755 /tmp/udx-new/server

# 2) 灰度：换端口独立试跑，6677 继续对外服务
cd /tmp/udx-new
UDX710_PORT=6678 ./server > /tmp/udx-6678.log 2>&1 &

# 3) 健康与接口冒烟
curl -s http://127.0.0.1:6678/api/health
curl -s http://127.0.0.1:6678/api/info | head -c 200
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:6678/
# 观察 10 秒：ps 状态应为 S，/proc/<pid>/stat 的 utime/stime 不再快速递增

# 4) 切换（保留回滚点）
cd /home/root/6677
cp server server.bak
kill <6678 实例 pid>
cp /tmp/udx-new/server ./server && chmod 755 ./server
./start.sh &                     # 无 supervisor 时也可交给外部看门狗在 ≤20 秒内拉起

# 5) 回滚
cp server.bak server && kill $(pidof server); ./start.sh &
```

注意：`server` 与前端是同一个二进制（`rust-embed` 内嵌），**前端变更同样需要重新部署 server**，
不存在单独替换 `web/` 的热更路径。

### 10.4 验收清单

- `curl -s http://127.0.0.1:6677/api/health` 返回 `status:"ok"` 且 `watchdog.stalled` 为 `false`。
- 静态首页、`/api/auth/status`、`/api/info` 均 200；旧版 85 条以上路由抽检通过。
- `ps` 中 server 状态为 S；记录 `/proc/<pid>/status` 的 `VmRSS` 作为后续对比基线。
- 人为 `kill` server，确认外部看门狗在 20 秒内拉起（日志出现 `restart begin/done`）。
- 人为阻塞 D-Bus 制造假死，确认内建 watchdog 以退出码 75 结束进程并被外部脚本拉起。
- 重启设备后确认 `/home/root/6677/panel_watchdog.sh` 由 `loader.sh` 拉起，crontab 内容已重建。
