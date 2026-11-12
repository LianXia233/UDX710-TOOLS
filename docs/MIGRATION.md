# UDX710-TOOLS 重构迁移对照（技术报告第 10 章）

> 依据《UDX710-TOOLS Rust + 原生前端 Html5 详细技术报告》第 10 章
> 「迁移路线：渐进式，不推倒重来」。

## 0. 本次交付的性质（必读声明）

**本次为代码层全量重构；真机 RSS/72 小时长稳/内存压力/回滚测试需设备实测，不承诺任何具体内存节省数值。**

- 本次在代码层把 C + Mongoose + Vue3 全量重写为 Rust + Tokio + Axum 后端
  与原生 HTML5 / ES Modules 前端，并交付 CI、打包、插件协议与迁移文档。
- 报告第 09 章 9.2 明确：没有目标设备、固件、并发量、功能负载和测量方法，
  不能承诺「节省 50% 内存」或「后端只占 5 MB」；Rust 不自动等于低 RSS，
  异步任务数量、缓冲区、TLS、数据库缓存、日志队列和静态资源缓存都会影响常驻内存。
- 下表所有「完成标准」均指**代码层已完成的条件**；「未完成 / 需真机验收项」列
  标记的内容本次不宣称完成，必须在真实 UDX710 设备上补齐。

## 1. 阶段 0-8 对照表

| 阶段（报告第 10 章） | 报告工作内容 | 本次重构对应交付物 | 完成标准（代码层） | 未完成 / 需真机验收项 |
| --- | --- | --- | --- | --- |
| 0 · 基线 | 冻结 API / 功能清单、设备测试步骤；测量当前 RSS / 启动时间 / API 延迟 | 根 `Cargo.toml`（workspace 与契约边界）、`web/js/core/api.js` API 契约、本 `docs/MIGRATION.md` | 模块分层与接口契约在代码中固化，CI 可重复构建 | 需实测：当前版本 RSS / 启动时间 / API 延迟基线、可重复基准数据与真机测量脚本。无设备，未采集 |
| 1 · 安全止血 | 修复免认证设备控制、SQL / Shell 注入路径、OTA TLS、脚本路径与任意命令执行 | `crates/storage`（rusqlite 参数化，去除 sqlite3 CLI / `sh -c`）、`crates/platform-linux`（命令 ID 白名单 + 固定 argv、子进程超时与 kill_on_drop）、`crates/api`（认证 / 授权中间件、OPTIONS 仅预检）、`reqwest` rustls-tls（去 `-k`）、脚本路径约束 | 代码层移除已知注入 / 免认证 / 任意命令路径，`cargo test --workspace` 通过 | 需实测：报告第 11 章安全测试全套（越权、路径遍历、Shell 元字符、畸形 JSON、Token 重放 / 暴力登录、OTA 证书 / 签名 / 降级失败关闭、插件跨权限读取与模板注入 / XSS） |
| 2 · Rust 服务骨架 | Axum 路由、认证、日志、配置、静态资源托管、健康检查 | `crates/app`（启动 / 配置 / 优雅关闭）、`crates/api`（Axum 路由 / 认证 / 日志 / 健康检查 / 静态托管）、rust-embed 内嵌 `web/` | workspace 可编译、宿主测试通过、CI `cargo test --workspace` 绿灯 | 需实测：在目标设备启动、长时间 API 框架稳定、真实监听 / 端口 / 防火墙联动 |
| 3 · 低风险模块 | 系统信息、只读状态、配置读取、流量统计 | `crates/services` 中的只读状态 / 系统信息 / 配置 / 流量统计 | 只读接口结构化，返回字段与旧版 `Code` / `Error` / `Data` 契约对齐 | 需实测：新旧接口结果逐项一致性对照、真实设备数据比对 |
| 4 · 数据层与 D-Bus | SQLite 参数化访问、zbus、oFono 状态和短信读取 | `crates/storage`（参数化 + 可逆 `migrations/`）、`crates/platform-linux`（zbus / oFono 状态与短信读取） | 异步 D-Bus、断线重连与超时逻辑就位，编译 / 单测通过 | 需实测：断线恢复、超时、并发、oFono 真实状态机、Modem 拔插与短信读取 |
| 5 · 设备控制 | 网络、USB、飞行模式、Modem 控制；加入资源锁和状态机 | `crates/services` 网络 / USB / 飞行模式 / Modem 控制 + 资源锁与状态机 | 动作返回明确 success / failure / timeout / unknown 状态；命令走白名单固定 argv | 需实测：重复操作、超时、设备拔插、USB 模式切换、断电与回滚测试 |
| 6 · 前端迁移 | 先 API 层，再页面，再组件；逐页移除 Vue 运行时 | `web/`（index.html + ES Modules：`js/core/api.js`、`store.js`、`components/`、`pages/` + 原生 CSS3），无 Vue / Vite 运行时依赖，rust-embed 内嵌，CI 不再执行 npm / make | 页面只通过 REST / WS API 访问后端，构建链路纯 Rust | 需实测：功能对照完成、移动端与桌面端浏览器通过、WS 实时状态、不含 `unsafe-eval` 的 CSP 生效 |
| 7 · 插件 / OTA | 新插件协议、兼容层、签名更新和回滚 | `docs/PLUGIN_PROTOCOL.md`（受限 JSON UI 协议）、`examples/plugins/example.status/`、兼容迁移策略（本文件第 3 节）、OTA rustls 证书校验与签名 / 回滚约定 | 新协议、权限默认拒绝、数据隔离与动作白名单定义完成；示例插件可加载 | 需实测：旧插件迁移清单逐个转 JSON UI、`docs/PLUGIN_GUIDE.md` / `docs/default_plugin.js` / `docs/example_system_toolkit.js` 等可信内置脚本插件评估结论、更新故障恢复与降级包失败关闭、真机 OTA 演练 |
| 8 · 清理与发布 | 移除旧 C / GLib 模块和旧前端依赖，打包和长稳测试 | `.github/workflows/build.yml`（Rust CI + aarch64 交叉编译 + artifact `udx710-tools-aarch64`）、`packaging/`（start.sh / systemd unit / README）、`.gitignore`（`/target/` `/tools/` `*.profraw`，Cargo.lock 提交） | 单一 Rust 构建链路，无 npm / make 残留路径，打包 / 重命名 / 回滚步骤齐备 | 需实测：72 小时长稳、内存压力下 OOM 行为与恢复、RSS / PSS / 峰值 RSS、API p50 / p95 / p99、任务队列、FD / 线程数、文件体积；旧 C / GLib 与旧前端依赖彻底移除后资源指标达标 |

## 2. 渐进迁移与 API 兼容策略

- **API contract 优先**：新旧后端保持同一 REST / WS 契约（路径、方法、字段名、
  返回结构 `Code` / `Error` / `Data`）；`web/js/core/api.js` 只依赖契约，不依赖实现语言。
- **每次只替换一个模块**，不做「大爆炸式」重写；先定义 contract，再写 Rust 实现。
- 新旧后端可在测试固件中并行对比，但**不能同时控制同一硬件资源**
  （串口 / USB / Modem 必须互斥，避免竞争导致设备状态不确定）。
- 数据库迁移必须可恢复（`migrations/` 双向脚本）；任何硬件动作必须有明确成功、
  失败、超时、未知状态，不能只返回「命令已发送」。
- **插件协议版本化**：`schema_version`、`min_api_version` 与动作白名单保证新旧 UI 描述可共存；
  旧插件经盘点后逐个转换为 JSON UI，可信内置脚本插件单独评估（见第 3 节与
  `docs/PLUGIN_PROTOCOL.md` 第 9 节）。
- **配置与端口兼容**：默认 6677，可由 `UDX710_PORT` 覆盖；部署目录沿用 `/home/root/6677`，
  启动入口 `start.sh` 保持一致的调用方式。

## 3. 每阶段保留旧版回退（git 历史即回退点）

- 每一次阶段提交 / tag 都是回退点：`git revert <commit>` 或 `git checkout <tag>`
  可回到任一旧版实现，无需额外维护并行分支；git 历史即回退点。
- **物理清理已执行**：旧 C / GLib 模块、旧 Vue 前端与 vendored 工具链已从工作树删除（961 文件）；
  功能基线固化为 `tests/contract/v2_baseline.json`（来源 commit + 合并 sha256），对照脚本不再依赖旧源码；
  原始实现仍可从 git 历史回退 / 查阅。注意：报告要求的**真机验收（阶段 8）尚未完成**，
  本次为代码层清理，不等于真机指标达标。
- **部署层回滚**：`packaging/README.md` 的 `server.bak` 流程，单文件替换即可回退二进制
  及其内嵌静态资源；systemd `Restart=on-failure` 配合失败版本快速切回；
  OTA 需签名校验 + 失败关闭 + 可回滚。
- **兼容层与第一阶段生态**：插件侧第一阶段不破坏插件生态——旧插件先盘点、列入迁移清单，
  只展示「待迁移」占位；可信内置脚本插件保留评估期，强制切换到纯 JSON UI 放在后续阶段，
  且始终保留旧版回退。
- 数据库迁移可恢复、配置可回退；回退后需保证数据结构与旧版服务端读取兼容。

## 4. 未完成事项汇总（上线前必须在真机补齐）

1. 阶段 0 基线采集：RSS / 启动时间 / API 延迟。
2. 安全测试全套（报告第 11 章）。
3. 24 小时基础稳定性 + 72 小时长稳 + 内存压力下 OOM 行为与服务恢复。
4. 资源指标：RSS / PSS、峰值 RSS、API p50 / p95 / p99、任务队列长度与拒绝数、FD / 线程数、文件体积。
5. Modem / SMS / USB / 网络 / OTA 的重复操作、超时、拔插、断电与回滚演练。
6. 旧插件生态盘点与逐个迁移、可信内置脚本插件单独评估结论。

再次强调：**本次为代码层全量重构；真机 RSS/72 小时长稳/内存压力/回滚测试需设备实测，不承诺任何具体内存节省数值。**
