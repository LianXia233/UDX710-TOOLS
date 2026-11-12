# 功能移植对照矩阵（v2.x → v3.0）

本文件回答「功能是否全部移植、WebUI 功能有没有少」。结论由可复跑脚本 `tests/contract/parity.py` 自动生成，不依赖人工记忆。

```bash
python3 tests/contract/parity.py     # 结果: PASS
python3 tests/contract/verify.py     # API 契约: PASS（95 端点 / 90 条 C 路由全命中）
```

> 旧版源码（`src/`、`web/src/`、`lib/`、`bin/`）作为 **v2.x 存档**保留在仓库中，仅用于对照，不参与 v3 构建。v3 的实现位于 `crates/`、`web/js/`、`web/css/`，CI 只构建 Rust workspace。

---

## 1. 后端路由对照

| 维度 | 旧版（C / Mongoose） | 新版（Rust / Axum） | 结论 |
| --- | --- | --- | --- |
| 路由定义 | `src/handlers/http_server.c` 中 91 条 `mg_str("/api/...")` | `crates/api/src/routes.rs` 84 条 `.route(...)` + `tests/contract/api_endpoints.json` 95 个端点 | **全覆盖** |
| 通配子路径 | `/api/apn/templates/*`、`/api/sms/*`、`/api/plugins/storage/*` 等 8 类 | 等价改为 `:id` 参数化路由 + 参数化 SQL（`service_id`、模板 id 等） | 语义等价，且消除了拼接注入 |
| 新增端点 | — | `/api/health`、`/api/events`（SSE 降级）、`/api/ws`（状态推送）、`/api/terminal/ws`（交互式 Shell）、`/api/audit/logs`、`/api/serial`、`/api/key` | 兼容增强，不改变旧客户端行为 |

`verify.py` 的 (a) 项逐条把 90 条有效 C 路由与新契约匹配，**未匹配 = 0**；`/api/apn` 为注释路由，保留兼容别名。

## 2. 前端端点对照

旧版 24 个 Vue 组件共调用 **39 个** `/api/...` 端点，`parity.py` (b) 项逐条匹配新契约与新路由表，**未覆盖 = 0**。

唯一被排除的是 `/api/webhooks/YOUR_WEBHOOK`（`SmsManager.vue` 中的示例占位文本，非真实端点）。

## 3. 组件 / 页面映射（WebUI 功能对照）

| 旧版 Vue 组件 | 新版页面 / 模块 | 主要功能 |
| --- | --- | --- |
| `SystemMonitor.vue` | `pages/dashboard.js` | CPU / 内存 / 温度 / 运行时长 / 实时状态 |
| `NetworkManager.vue` | `pages/network.js` | 网络模式、切卡、飞行模式、数据/漫游 |
| `AdvancedNetwork.vue` | `network.js` + `modem.js` | 锁频段 / 解锁、锁小区 / 解锁、频段与小区查询 |
| `NetworkInterface.vue` | `pages/network.js` | 网卡列表、流量统计、监控 |
| `CellManager.vue` | `network.js` + `modem.js` | 小区信息、锁小区 |
| `ApnConfig.vue` | `pages/apn.js` | APN 配置、模板 CRUD、应用/清空 |
| `SmsManager.vue` | `pages/sms.js` | 收件箱、发短信、已发送、配置、Webhook、修复 |
| `TrafficStats.vue` | `pages/traffic.js` | 流量总量、阈值设置、清零 |
| `BatteryManager.vue` | `pages/charge.js` | 充电配置、开/关充电 |
| `RatholeManager.vue` | `pages/rathole.js` | 配置、服务 CRUD、启停、状态、日志、服务端配置、自启 |
| `IPv6Proxy.vue` | `pages/ipv6-proxy.js` | 配置、规则 CRUD、启停/重启、状态、推送、日志 |
| `SystemUpdate.vue` | `pages/update.js` | 版本检查、上传、下载、解压、安装 |
| `UpdateNotification.vue` | `pages/update.js` | 更新可用提示（由版本检查驱动） |
| `ATDebug.vue` | `pages/at-debug.js` | AT 指令调试 |
| `WebTerminal.vue` | `pages/terminal.js` | Web 任意 Shell（HTTP + WebSocket 交互终端） |
| `UsbMode.vue` | `pages/usb.js` | USB 模式、进阶设置 |
| `PluginStore.vue` | `pages/plugins.js` | 插件上传、列表、删除、存储 |
| `PluginCard/Btn/Status.vue` | `core/plugin-renderer.js` | 宿主渲染（JSON Schema，替代动态 Vue 模板） |
| `SystemSettings.vue` | `pages/settings.js` + `pages/system.js` | 修改密码、设备控制、时间、定时重启、清缓存、手机壳 |
| `SecuritySetup.vue` | `pages/security.js` | 密保设置、状态、恢复出厂 |
| `ForgotPasswordModal.vue` | `pages/login.js` | 密保问题、校验、重置密码 |
| `LoginPage.vue` | `pages/login.js` | 登录、记住密码 |
| `GlobalToast.vue` | `components/toast.js` | 全局提示 |
| `GlobalConfirm.vue` | `components/confirm-dialog.js` | 全局确认框 |
| `TabNav.vue` | `app.js` 导航（NAV 分组） | 分组导航、响应式侧栏 |

**旧版 16 个菜单项 → 新版 18 个页面**（`dashboard / modem / sms / network / traffic / plugins / system / terminal / settings / apn / usb / security / ipv6Proxy / rathole / update / atDebug / charge / login`）。新版额外增加独立的**密保中心**与**设备控制**页，未删减任何旧菜单。

`parity.py` (c)(d) 项校验上述映射中的**每个目标文件确实存在**，缺失即 FAIL。

## 4. 通用能力对照

| 旧版能力 | 新版实现 |
| --- | --- |
| `useApi.js` | `core/api.js`（Token、AbortSignal、超时、401 事件、`ApiError`） |
| `useToast.js` / `useConfirm.js` / `useEventBus.js` | `components/toast.js` / `confirm-dialog.js` / `core/events.js` |
| `vue-i18n` 双语 | `i18n/zh-CN.js` + `en-US.js`（轻量共享 UI 词条） |
| `TabNav` 响应式 | `app.js` + `layout.css`（12 列网格、760px 单列、折叠侧栏） |
| FontAwesome 图标 | `icons/sprite.svg` 本地雪碧图（无外部 CDN） |
| Vite 构建 / Tailwind | 无构建步骤，原生 ES Modules + CSS 变量（`tokens.css`） |
| — | **新增**：白色毛玻璃主题、动态 SVG、三种弹窗形态（Centered / Bottom Sheet / Right Side Sheet）、全分辨率响应式（见 `docs/UI.md`） |
| — | **新增**：前后端看门狗与 supervisor 自愈（见 `docs/WATCHDOG.md`） |
| — | **新增**：功能移植对照 `parity.py`、更新日志 `CHANGELOG.md`、UI / 看门狗文档，并纳入 CI 门禁 |

## 5. 结论

- **后端**：91 条旧路由（含 8 类通配）全部有对应实现，新增 7 个增强端点，写接口 69 个全部强制认证。
- **WebUI**：24 个旧组件、16 个旧菜单、39 个前端调用端点全部有对应页面/模块，**功能没有减少**；页面数由 16 增至 18。
- **行为兼容**：路径、方法、JSON 语义保持；`verify.py` 与 `parity.py` 均为 PASS，可作为回归门禁。

> 未覆盖的是**真机硬件行为**（D-Bus/oFono、Modem AT、rathole/ipv6 进程、OTA 回滚、性能指标），需在目标 aarch64 设备上按 `tests/README.md` 复核；代码层面不伪装设备侧成功。
