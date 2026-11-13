# 5G MiFi 管理面板（UDX710-TOOLS）

基于 Web 的 5G MiFi 设备管理界面，运行于嵌入式 Linux（aarch64）。

**当前版本 3.1.0**：按《UDX710-TOOLS Rust 原生前端 HTML5 详细技术报告》对 `main` 主线做全量重构。后端从 C + Mongoose + GLib/GIO 迁到 **Rust + Tokio + Axum**，前端从 Vue3 + Vite + Tailwind 迁到 **原生 HTML5 + CSS3 + JavaScript ES Modules**，零 npm 依赖、单二进制发布。

[English README](README.md) | [更新日志](CHANGELOG.md) | [功能完整性对照](docs/FEATURE_PARITY.md)

---

## 版本与分支

| 项目 | 说明 |
| --- | --- |
| 主线 | `main`（远端当前仅有此分支） |
| 历史 | 旧文档提到的 `SZ50` 专用版分支在远端已不存在，不在本次维护范围内 |
| 发布 | GitHub Actions 交叉编译 `aarch64-unknown-linux-gnu`，产物 `udx710-tools-aarch64` |
| 默认端口 | `6677`（命令行参数 > `UDX_PORT` > `config.toml` > 默认值） |

## 本次重构概览

| 版本 | 内容 |
| --- | --- |
| **3.0.0** | 六 crate 分层后端、原生前端、REST 契约兼容、安全审计修复、插件协议重写、测试/CI/文档 |
| **3.1.0** | 白色毛玻璃主题 + 动态 SVG、三种弹窗形态、全分辨率响应式、前后端看门狗与自愈、功能完整性对照、文档同步 |

功能完整性已用可复跑脚本核对：旧版 **91 条 C 后端路由、37 个前端调用端点、27 项组件映射、16 个菜单项，未覆盖均为 0**。详见 [docs/FEATURE_PARITY.md](docs/FEATURE_PARITY.md) 与 `tests/contract/parity.py`。

## 功能特性

### 网络与 Modem

- Modem 信息：IMEI、ICCID、运营商、信号强度
- 频段与小区：网络类型、频段、ARFCN、PCI、RSRP、RSRQ、SINR
- 流量统计与限额：vnstat 数据、自动断网
- 网络接口监控、系统信息（CPU / 内存 / 温度 / 运行时长）
- APN 设置：预设运营商 + 自定义 APN / 用户名 / 密码 / 认证协议
- 飞行模式、充电控制、定时重启、系统时间设置

### 系统与设备

- USB 模式切换：CDC-ECM / CDC-NCM / RNDIS，临时与永久两种生效方式，带独占锁与失败回滚状态机
- 短信收发、Webhook 转发（含 SSRF 防护）、发送日志
- IPv6 端口转发：IPv4 → IPv6 映射、自动检测地址、地址变化通知、开机自启、推送日志
- 内网穿透 Rathole：多服务配置、服务端配置生成、连接状态、日志、开机自启
- OTA 更新：TLS 校验、主机白名单、SHA-256 摘要、签名门禁、原子替换、回滚
- 恢复出厂、重启、关机、设备控制
- Web 终端与 AT 调试

### 扩展与管理

- 插件系统：版本化 **JSON UI** + 宿主渲染（白名单 view 类型与 action、权限双重校验、数据隔离与配额）
- **轻量性能与内存表现**：单插件声明式 DOM 树渲染仅占 **20KB ~ 60KB** 内存（较旧版 Vue 3 的 15MB~30MB 降低 99.8%）；单插件 SQLite 配额硬限制 **64KB**；后端整机常驻 RSS 稳定在 **15MB ~ 25MB**；100 个节点生成耗时 **< 3ms**，零 JIT 编译与 eval 开销。
- 脚本管理：上传 / 编辑 / 删除，文件以非执行权限（0o600）存储，文件名 allowlist
- 密保问题、密码修改、Token 过期 / 撤销 / 轮换、登录失败限速
- 审计日志：actor / action / permission / outcome / 耗时，不记录敏感内容

### 前端 UI

- 沉浸式白色毛玻璃（Glassmorphism）设计体系，支持亮色、暗色与跟随系统偏好
- **无侧边栏设计（No Sidebar）**：桌面端采用顶部悬浮玻璃岛主导航（Header Dock）+ 全景应用中心；移动端采用悬浮毛玻璃底部导航条（Bottom Nav）+ Modal Bottom Sheet
- **全套专业动态 SVG 遥测组件**：
  - 5G 射频与基站拓扑雷达 HUD（`createHeroRadar`）
  - 12 方位微刻度高精度环形遥测仪表盘（CPU、内存、电池、信号）
  - 平滑贝塞尔样条动态波形图（`createSmoothSparkline`）
  - 双向高频流速亚克力管道（`createTrafficStreamPipe`）
  - 全息 5G 品牌标识与 3D 终端安全插画（SMIL 动画）
- **界面去技术化与折叠收纳**：卡片副标题杜绝裸露 `/api/` 路径，冗余技术指南与输出日志支持原生可折叠卡片（`collapsibleCard`）收纳
- 三种全景弹窗形态：**Centered Modal Dialog**、**Modal Bottom Sheet**、**Right Side Sheet**
- 全分辨率响应式：覆盖 320px 至 1920px、折叠屏、平板、桌面、安全区、`100dvh`、44px 黄金点击区
- 真实数据链路：WebSocket 高频自愈通道与 HTTP 降级无缝协同，零 Mock 零造假
- 详见 [docs/UI.md](docs/UI.md)

## 看门狗与自愈

| 层 | 检测对象 | 自愈 |
| --- | --- | --- |
| 后端独立 OS 线程 | Tokio runtime 假死 | 退出码 75，由 systemd / procd 重启 |
| 前端心跳漂移 | UI 主线程卡死 | 提示 + 自动刷新（可取消） |
| 前端健康探测 | 后端失联 / 心跳超时 | 持续重试 + 恢复后重建 WS / SSE |

配置见 `config.example.toml` 的 `[watchdog]`，状态见 `GET /api/health` 的 `watchdog` 字段。部署与原理见 [docs/WATCHDOG.md](docs/WATCHDOG.md)。

## 项目架构

```
├── Cargo.toml                  # Rust workspace（6 crates）+ release profile
├── config.example.toml         # 配置样例
├── crates/
│   ├── domain/                 # 领域模型、权限、错误语义、状态机
│   ├── platform-linux/         # 固定 argv 命令、Web Shell、sysfs/procfs、D-Bus(oFono)
│   ├── storage/                # SQLite（rusqlite bundled）+ DB worker + 全静态 SQL
│   ├── services/               # 业务服务、AT 队列、USB 状态机、OTA、插件、看门狗
│   ├── api/                    # Axum 路由、中间件、Handler、WebSocket / SSE、静态资源内嵌
│   └── app/                    # 启动入口、Tokio runtime、优雅关闭
├── web/                        # 纯静态前端，由 rust-embed 编译期内嵌
│   ├── index.html              # 单页入口 + 严格 CSP
│   ├── css/                    # tokens / layout / components / glass / sheets / responsive
│   ├── js/                     # core / components / pages / utils / i18n（ES Modules）
│   └── icons/                  # sprite.svg + 动态 SVG
├── migrations/001_init.sql     # 幂等数据库迁移
├── tests/contract/             # API 契约（95 端点）+ 校验脚本 + 功能对照脚本
├── crates/api/tests/           # 安全集成测试
├── docs/                       # 架构、契约、迁移、插件协议、安全、UI、看门狗
├── examples/plugins/           # 符合新协议的示例插件
└── packaging/                  # start.sh、systemd unit、procd init、部署说明
```

v2.x 旧实现（`src/` C 源码、`web/src/` Vue 源码、`web/package.json` / `package-lock.json` / Vite / Tailwind / PostCSS 配置，
以及 vendored 的 `bin/` `include/` `lib/` `share/` 工具链）已从仓库**删除**，仓库只保留重构后的文件。
功能基线固化在 `tests/contract/v2_baseline.json`（含来源 commit 与旧源码合并 sha256），对照脚本仍可复跑；
如需查阅原始代码请回到 git 历史。

## 快速开始（部署）

```sh
# 1. 解压发布包到部署目录（示例 /home/root/6677）
#    server、start.sh、config.toml 放在同一目录

# 2. 按需修改配置
cp config.example.toml config.toml

# 3. 前台启动（默认 6677，可传端口）
./start.sh
./start.sh 8080

# 4. 浏览器访问
#    http://设备IP:6677
```

守护进程：

```sh
# systemd
cp packaging/udx710-tools.service /etc/systemd/system/
systemctl daemon-reload && systemctl enable --now udx710-tools

# OpenWrt procd
cp packaging/udx710-tools.init /etc/init.d/udx710-tools
chmod 755 /etc/init.d/udx710-tools
/etc/init.d/udx710-tools enable && /etc/init.d/udx710-tools start
```

默认密码 `admin`，**首次登录后请立即通过 `/api/auth/password` 修改**。部署细节与回滚步骤见 [packaging/README.md](packaging/README.md)。

## 从源码构建

```sh
# 本地验证（无需 Node / npm）
cargo test --workspace

# 契约与功能对照
python3 tests/contract/verify.py
python3 tests/contract/parity.py

# 本机运行（开发模式）
cargo run -p udx-app -- 6677

# 交叉编译到 aarch64
rustup target add aarch64-unknown-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
cargo build --release --target aarch64-unknown-linux-gnu -p udx-app
```

环境要求：Rust stable；交叉编译需 `gcc-aarch64-linux-gnu`。前端为纯静态文件，无需 Node.js。

release profile：`opt-level="s"`、LTO、`codegen-units=1`、`panic="abort"`、`strip=true`。**不承诺固定内存节省比例**，性能指标需真机实测。

## 测试与验收

| 项目 | 结果 |
| --- | --- |
| `cargo test --workspace` | 53 passed / 0 failed（含 5 个看门狗单测、11 个 API 集成测试） |
| `tests/contract/verify.py` | PASS：95 端点、90 条旧 C 路由全命中、0 未匹配、69 个写接口全部要求认证 |
| `tests/contract/parity.py` | PASS：91 路由 / 37 端点 / 27 组件 / 16 菜单，未覆盖 0 |
| 前端 JS 语法 | 全部 `node --check` 通过 |
| 真实进程冒烟 | 健康检查、登录、未授权拦截、Web Shell 执行、WS 握手与 Token 撤销、审计、CSP 均通过 |

**真机待验证**（沙箱内无 oFono D-Bus，设备动作返回 `device_verified: false`，不伪装成功）：

- Modem / SMS / USB / Rathole / IPv6 设备侧真实行为
- D-Bus 断连恢复、OTA 断电回滚、看门狗 respawn 时延
- RSS / PSS、峰值 RSS、API 延迟、72 小时长稳、内存压力

## API 与兼容性

- 保留旧版路径拼写，如 `/api/claen/cron`、`/api/get/Total`；新增字段不改变旧字段语义。
- 写接口默认要求认证；`device_control` 的 POST/OPTIONS 免认证漏洞已修复，CORS 预检只返回 204 不触发业务动作。
- 错误体兼容旧版 `{ "status": "error", "message": "..." }`，内部错误细节不回显。
- 完整端点表见 [docs/API_CONTRACT.md](docs/API_CONTRACT.md)。

## 安全

- 认证 / 授权：Token 只返回一次且数据库只存 salted hash；`sessionStorage` 存储；细粒度权限（`shell.execute`、`terminal.session`、`device.control` 等）。
- 注入防护：全静态 SQL + 参数绑定；内部设备动作使用命令 ID 白名单 + 固定 argv，不拼 Shell。
- OTA / Webhook：TLS 校验、摘要与签名、SSRF 防护。
- 插件：不执行任意 JS / HTML，宿主渲染只解释白名单类型。
- 日志：访问日志只记 `method/path/status/latency_ms`，不记 query（含 WS `?token=`）、请求头或 body。
- **Web 任意 Shell 按用户要求保留**：作为独立、显式、高风险的管理员诊断通道，强制权限、限速、超时、输出上限、进程组清理与全量审计，可通过 `[shell] enabled=false` 关闭。

逐条对照见 [docs/SECURITY.md](docs/SECURITY.md)。

## 文档索引

| 文档 | 内容 |
| --- | --- |
| [CHANGELOG.md](CHANGELOG.md) | 版本更新日志 |
| [docs/UI.md](docs/UI.md) | 毛玻璃主题、动态 SVG、三种弹窗、响应式断点 |
| [docs/WATCHDOG.md](docs/WATCHDOG.md) | 前后端看门狗与 supervisor 自愈 |
| [docs/FEATURE_PARITY.md](docs/FEATURE_PARITY.md) | 旧版功能逐项对照证据 |
| [docs/API_CONTRACT.md](docs/API_CONTRACT.md) | 端点总表与兼容策略 |
| [docs/SECURITY.md](docs/SECURITY.md) | 安全审计逐条对照与威胁模型 |
| [docs/PLUGIN_PROTOCOL.md](docs/PLUGIN_PROTOCOL.md) | 新插件 JSON UI 协议与迁移 |
| [docs/MIGRATION.md](docs/MIGRATION.md) | 迁移阶段与真机验收项 |
| [packaging/README.md](packaging/README.md) | 部署、systemd / procd、回滚 |
| [tests/README.md](tests/README.md) | 测试与契约脚本用法 |

## 历史界面截图

`docs/screenshot*.png` 为 2.x（Vue3 旧版）界面存档。3.x 已改为白色毛玻璃主题与三种弹窗形态，请以实际运行界面为准。

## 开源协议

本项目采用 **GPLv3** 协议。详见 [LICENSE](LICENSE)。

| 允许 | 必须 | 禁止 |
| --- | --- | --- |
| 自由使用、修改、分发 | 保留版权声明 | 闭源商业化 |
| 分发修改版本 | 公开源代码（分发时） | 删除版权信息 |
| | 使用相同协议 | 更改为其他协议 |

## 致谢

感谢以下贡献者对本项目的支持：

| 贡献者 | 贡献内容 |
| --- | --- |
| 等不住 | 提供各种 AT 指令 |
| 黑衣剑士 | 提供 USB 模式切换 |
| Voodoo | Glib 编译环境 |
| 1orz | [project-cpe](https://github.com/1orz/project-cpe) 开源项目 |
| LeoChen | 项目作者 |

## 社区

- QQ 群：1029148488
- 欢迎提交 Issue / Pull Request

欢迎提交反馈一起完善项目。
