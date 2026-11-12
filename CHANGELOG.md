# 更新日志 / Changelog

本项目所有值得记录的变更都会写在这里。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循语义化版本。

> 分支说明：本仓库当前只维护 `main` 主线。旧文档提到的 `SZ50` 分支在远端已不存在，所有改动与发布均以 `main` 为准。

---

## [Unreleased]

仓库清理：只保留重构后的实现，v2.x 旧源码与旧构建链全部移出工作树。

### Removed

- 删除旧 C 后端 `src/`（61 文件）、旧 Vue3 前端 `web/src/`（39 文件）、`web/package.json` / `package-lock.json` / `vite.config.js` / `tailwind.config.js` / `postcss.config.js`。
- 删除 vendored 工具链 `bin/` / `include/` / `lib/` / `share/`（852 文件，旧 C 构建的 GLib / Mongoose 依赖）。
- 删除旧插件文档与示例：`docs/PLUGIN_GUIDE.md`、`docs/插件开发指南2.0.md`、`docs/default_plugin.js`、`docs/example_system_toolkit.js`（描述已废弃的 Vue 动态模板插件）。
- 合计删除 **961 个文件 / 465,698 行**；`LICENSE`、历史界面截图与二维码等非源码资源保留。

### Changed

- **基线快照化**：新增 `tests/contract/v2_baseline.json`，固化 91 条 C 路由、90 条 `mg_match` 有效路由 + 1 条注释路由、37 个前端端点、27 个组件、16 个菜单项，附来源 commit `3a6c5f1` 与 100 个旧源码文件的合并 sha256。
- `verify.py` / `parity.py` 改为读取该快照，不再依赖已删除的旧源码；两者均新增快照完整性校验（schema、非空、sha256），空基线直接 FAIL，避免门禁假通过。
- 同步 `build.yml` 注释、`docs/FEATURE_PARITY.md`、`docs/MIGRATION.md`、`docs/API_CONTRACT.md`、`docs/SECURITY.md`、`tests/README.md`、中英 README 中关于旧源码位置的表述。

### Notes

- 原始 v2.x 实现仍可从 git 历史查阅 / 回退；`parity.py` 对照结论不变（PASS，零遗漏），`verify.py` 仍 PASS（95 端点 / 90 条 C 路由全命中）。
- 报告要求的真机验收（阶段 8）尚未完成；本次为代码层清理，不代表真机指标达标。

---

## [3.1.0] - 2026-11-13

本版在 3.0.0 全量重构基础上，补齐**界面表现层**与**运行时自愈能力**，并同步文档。功能完整性已用可复跑脚本逐项核对，详见 [docs/FEATURE_PARITY.md](docs/FEATURE_PARITY.md)。

### Added

- **功能完整性对照**：新增 `tests/contract/parity.py`，自动比对旧版 C 后端路由、旧 Vue 组件调用的端点、组件映射与菜单项。结果：旧路由 91 条 / 前端端点 37 条 / 组件映射 27 项 / 菜单 16 项，**未覆盖均为 0**；新增 `docs/FEATURE_PARITY.md` 记录证据表。
- **白色毛玻璃主题**：`web/css/tokens.css` 重写为白色半透明玻璃变量（含深色玻璃变体），`web/css/glass.css` 提供 `backdrop-filter` 模糊、白边、柔和阴影与渐变光晕背景；亮/暗模式、系统偏好、`prefers-reduced-motion` 全部保留。
- **动态 SVG**：新增 `web/icons/aurora.svg`（背景光晕，SMIL 漂浮）、`web/icons/brand-live.svg`（品牌动态标识，兼作 favicon）、`web/icons/pulse-wave.svg`（脉冲波形）。全部为同源静态资源，不引入 JS 动画、不违反 CSP（`img-src 'self' data:`）。
- **三种弹窗形态**：新增 `web/js/components/dialog-variant.js` 与 `web/css/sheets.css`
  - `Centered Modal Dialog` —— 桌面默认居中对话框
  - `Modal Bottom Sheet` —— 手机默认底部抽屉，带拖拽把手，点击/下滑关闭，适配底部安全区
  - `Right Side Sheet` —— 右侧抽屉，用于日志、JSON 详情、长表单（已接入插件预览/存储/脚本、Rathole 日志/服务端配置、IPv6 推送日志、接口统计）
  - `auto` 按视口自动选择；窄屏下右侧抽屉自动退化为底部抽屉；「设置」页可强制切换并即时预览三种样式（偏好存 `sessionStorage`）。
- **全分辨率响应式**：新增 `web/css/responsive.css`，覆盖 320 / 360 / 390 / 414 / 600 / 760 / 1024 / 1440 / 1920 宽度、手机横屏矮视口、折叠屏与平板；使用 `100dvh`、`env(safe-area-inset-*)`、触屏 44px 最小点击区、表格横向滚动、输入框 16px 防 iOS 聚焦缩放、防横向溢出。
- **前后端看门狗**
  - 后端：`crates/services/src/watchdog.rs` —— Tokio runtime 内周期喂狗 + **独立 OS 线程**监测心跳；超时判定假死后以退出码 75 结束进程，由 systemd `Restart=on-failure` / OpenWrt procd `respawn` 自动拉起。配置见 `config.toml [watchdog]`，状态暴露于 `GET /api/health` 的 `watchdog` 字段。
  - 前端：`web/js/core/watchdog.js` —— UI 主线程假死检测（心跳漂移）、`/api/health` 后端失联探测、自动重载与实时通道重建；已在 `app.js` 启动。
  - 部署：新增 `packaging/udx710-tools.init`（OpenWrt procd），`packaging/udx710-tools.service` 补自愈说明；详见 [docs/WATCHDOG.md](docs/WATCHDOG.md)。
- **文档**：新增 [docs/UI.md](docs/UI.md)、[docs/WATCHDOG.md](docs/WATCHDOG.md)、[CHANGELOG.md](CHANGELOG.md)；`README.md` / `README_CN.md` 重写为 Rust + 原生前端架构说明。

### Fixed

- `web/js/app.js` 的 `checkAuth()` 同时兼容 `authenticated` 与 `logged_in` 字段，修复与旧版 / 反向代理返回体不一致导致登录态判断失效的问题。
- 统一弹窗的 ESC / 遮罩点击 / 关闭按钮走同一条带退场动画的关闭路径，避免原生 `<dialog>` 直接关闭导致 DOM 残留。

### Security

- 看门狗状态与访问日志均不含 Token、密码、命令原文；`/api/health` 仍为公开只读端点，不暴露任何敏感信息。
- 弹窗偏好存 `sessionStorage`（与 Token 同域、关闭标签页失效），不写 `localStorage` 持久化凭证。
- 严格 CSP 保持不变：`script-src 'self'`、`style-src 'self'`，无 `unsafe-eval` / `unsafe-inline`；所有 SVG 动画为 CSS / SMIL 声明式。

---

## [3.0.0] - 2026-11-13

按《UDX710-TOOLS Rust 原生前端 HTML5 详细技术报告》对主线 `main` 做全量重构。

### Changed

- **后端**：C + Mongoose + GLib/GIO → **Rust + Tokio + Axum**，拆分为 `crates/{domain,platform-linux,storage,services,api,app}` 六个 crate；`web/` 静态资源由 `rust-embed` 编译期内嵌，产出单二进制。
- **前端**：Vue 3 + Vite + Tailwind + FontAwesome + vue-i18n → **HTML5 + CSS3 + 原生 JavaScript ES Modules**，零 npm 依赖、无构建步骤。
- **数据库**：SQLite（rusqlite bundled）+ 专用 OS 线程 DB worker + mpsc/oneshot，全部 SQL 为编译期静态常量 + 参数绑定；新增 `migrations/001_init.sql`。
- **设备动作**：内部命令改为命令 ID 白名单 + 固定 argv + 参数校验 + 超时 + 进程组 SIGKILL；AT 队列有界、按优先级调度；USB 模式切换改为带独占锁与失败回滚的状态机。
- **插件系统**：旧版动态 Vue 模板 / 任意 JS → 版本化 **JSON Schema + 宿主渲染**（白名单 view 类型与 action、权限双重校验、插件数据隔离与配额）；脚本上传改为非执行存储（0o600）与文件名 allowlist。
- **实时通道**：WS 状态推送、SSE 事件流、交互式 Web 终端；WS/SSE 支持 `?token=` 握手认证（仅升级请求），握手后对每条 input 帧复核 Token 与 `terminal.session` 权限。
- **构建与部署**：GitHub Actions 改为 Rust stable + `cargo test --workspace` + aarch64 交叉编译；新增 `packaging/start.sh`、systemd unit、部署 README；`Cargo.lock` 纳入版本控制。
- **性能口径**：release profile 使用 `opt-level="s"` / LTO / `panic="abort"` / strip；**不承诺任何固定内存节省比例**，RSS/PSS、72 小时长稳、内存压力等指标需真机实测。

### Security

- 修复旧版 `/api/device_control` 的 POST/OPTIONS 免认证漏洞；CORS 预检只返回 204，不触发任何业务动作。
- 认证：salted SHA-256 密码、Token 明文只返回一次且数据库只存 hash、过期 / 撤销 / 轮换、登录失败限速、`sessionStorage` 替代 `localStorage`。
- SQL 注入：全静态 SQL + 参数绑定；Shell 注入：内部动作不使用 `/bin/sh -c`。
- OTA：启用 TLS 证书校验、HTTPS + 主机白名单、SHA-256 摘要、签名门禁、原子替换与 `server.bak` 回滚。
- Webhook：改用 HTTP 客户端替代 Shell 调用，URL 做 SSRF 防护（拒绝环回 / 元数据地址 / userinfo / 非 HTTP(S)）。
- 日志：移除 `TraceLayer` 默认 URI 记录，访问日志只记 `method/path/status/latency_ms`，不记 query（含 WS 的 `?token=`）、请求头或 body；审计不记密码、Token、短信正文、完整 AT 敏感参数。
- 插件：不执行任意 JS/HTML，宿主渲染只解释受支持类型，文本一律 `textContent`。
- **Web 任意 Shell 按用户要求保留**：作为独立、显式、高风险的管理员诊断通道（`shell.execute` / `terminal.session` 权限、并发上限、超时、输出上限、进程组清理、全量审计、可配置开关）。详见 [docs/SECURITY.md](docs/SECURITY.md)。

### Added

- 95 个端点的 API 契约 `tests/contract/api_endpoints.json` + 可复跑校验 `tests/contract/verify.py`（90 条旧 C 路由全命中、0 未匹配）。
- 文档：[docs/API_CONTRACT.md](docs/API_CONTRACT.md)、[docs/MIGRATION.md](docs/MIGRATION.md)、[docs/PLUGIN_PROTOCOL.md](docs/PLUGIN_PROTOCOL.md)、[docs/SECURITY.md](docs/SECURITY.md)、[tests/README.md](tests/README.md)、示例插件 `examples/plugins/example.status/`。
- 测试：workspace 单测 + `crates/api/tests/security_integration.rs` 集成测试，覆盖免认证修复、Web Shell 可用性、OPTIONS 不触发动作、SQL 注入/暴力登录、旧 Vue 模板拒绝、严格 CSP、插件数据隔离、WS query token 认证边界。

### Known Limitations（真机待验证）

- 沙箱内无 oFono D-Bus，设备动作返回 `device_verified: false`，**不伪装成功**；Modem / SMS / USB / Rathole / IPv6 设备侧行为、OTA 断电回滚、D-Bus 断连恢复均需真机验证。
- 性能指标（RSS/PSS、峰值 RSS、API 延迟、72 小时长稳、内存压力）未实测，报告要求真机测量。
- 设计要求中尚未逐项落地：WS 帧级令牌桶、空闲心跳（现仅有会话总时长上限）、CSRF `Origin` 校验（现仅 CORS 头）。详见 `docs/SECURITY.md`。

---

## [2.x 及更早]

原始 C + Vue 3 实现的历史变更请查阅 git 历史。3.0.0 起架构与目录已完全重写，旧构建方式（`make` / `npm run build` / `web/dist`）不再适用。
