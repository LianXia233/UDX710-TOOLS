# 测试、契约校验与验收

本目录存放 Rust + Axum 重构期间的 **API 契约静态校验** 与测试说明。契约是「先定义 contract，再写实现」（技术报告第 10 章迁移原则）的落地物：机器可读清单在 `tests/contract/api_endpoints.json`，校验脚本在 `tests/contract/verify.py`，端点总表与语义见 `docs/API_CONTRACT.md`，安全对照见 `docs/SECURITY.md`。

## 1. API 契约静态校验（verify.py）

### 运行

在仓库根目录执行（脚本以自身位置定位仓库根，从任意工作目录调用均可）：

```
python3 tests/contract/verify.py
echo $?
```

纯 Python 3 标准库，无第三方依赖，不依赖 Rust 工具链，可作为 CI 的第一道快速闸门。

### 退出码

- `0`：全部校验通过；
- `1`：存在失败项（失败明细会逐条打印）。

### 校验项

| 项 | 内容 |
| --- | --- |
| (a) | `api_endpoints.json` 中 `path` 唯一；每条含 7 个规定字段；`auth` 取值合法（`required` / `public` / `public-get`） |
| (b) | 从 `src/handlers/http_server.c` 正则提取所有 `mg_match(hm->uri, mg_str("..."), NULL)` 的 URI，把 `/*` 归一化为 `/:id`，逐个确认能在契约中找到对应条目；契约里的 Axum 占位符 `/:id`、`/:name`、`/:service_id`、`/:rule_id` 统一归一化为 `/:id` 以便跨命名比对；**允许 GET / POST 共用同一 path** |
| (c) | 所有写接口（POST / PUT / DELETE / WS）`auth` 不为 `public`，且 `/api/device_control` 必须 `auth=required` |

### 输出内容

契约端点数、C 有效/注释路由数、写接口数、三项校验结论、**按功能域分组的统计表**（端点数 / 写接口数 / 三种 auth 分布 / C 路由命中 / 未命中）、未匹配项、注释路由、契约新增项、协议豁免的公开写入口。

### 两个必须理解的容错 / 例外

1. **注释路由容错**：`http_server.c` 中 `// else if (mg_match(... "/api/apn" ...))` 是旧版注释掉的 APN 管理路由。脚本按行判断并跳过，单独列为「C 注释路由」，不作为覆盖失败。契约仍保留 `/api/apn`，作为前端 `useApi.js` 中 `getApnList()` / `setApnConfig()` 的兼容别名，见输出的「注释路由的兼容别名条目」。
2. **协议豁免的公开写入口**：登录与忘记密码恢复流程在协议上必须免认证，否则设备无法登录或恢复。脚本用 `PUBLIC_WRITE_EXEMPT` 显式列出 3 个入口并单独打印：`/api/auth/login`、`/api/security/verify`、`/api/security/reset-password`。除此之外任何写接口标 `public` 即判失败。`public-get` 的语义是「仅 GET 免认证，其余方法仍需认证」，因此含写方法的 `public-get` 端点（如 `/api/charge/config`）视为合规。

### 契约文件结构（api_endpoints.json）

顶层为数组，每个对象含 7 字段：

| 字段 | 说明 |
| --- | --- |
| `path` | Axum 风格路径，占位符 `/:id` / `/:name` / `/:service_id` / `/:rule_id`；保留原始拼写如 `/api/claen/cron`、`/api/get/Total` |
| `methods` | HTTP 方法数组；SSE 用 `GET`、WebSocket 终端用 `WS` |
| `auth` | `required` / `public` / `public-get` |
| `permission` | 建议的细粒度权限名，多方法端点用逗号分隔多个 |
| `request` | 请求字段描述（query / body / path 参数） |
| `response` | 典型 200 响应字段描述 |
| `notes` | 兼容、加固、风险与差异说明 |

### 数据来源

`src/handlers/http_server.c`（完整路由表与 `is_auth_whitelist`）、`src/handlers/handlers.c` 与 `src/system/*.c`（请求/响应字段与安全实现）、`web/src/composables/useApi.js`（前端调用约定）、技术报告第 08 / 11 章。

## 1.5 功能移植对照（parity.py）

```
python3 tests/contract/verify.py     # API 契约：端点 / 认证 / 旧 C 路由命中
python3 tests/contract/parity.py     # 功能完整性：旧版功能是否全部移植
```

`parity.py` 从仓库中保留的 v2.x 旧源码（`src/handlers/http_server.c`、`web/src/components/*.vue`、`web/src/App.vue`）提取四类基线，逐项与新实现对照：

| 项 | 基线 | 新实现 | 当前结果 |
| --- | --- | --- | --- |
| (a) | 旧 C 后端 `mg_str("/api/...")` 路由 | `api_endpoints.json` + `routes.rs` | 91 条 / 未覆盖 0 |
| (b) | 旧 Vue 组件调用的端点 | 同上 | 37 条 / 未覆盖 0 |
| (c) | 旧 Vue 组件 / 组件映射 | `web/js/pages/`、`web/js/components/` | 27 项 / 缺失 0 |
| (d) | 旧版菜单项 | 新版 `NAV` / `PAGES` | 16 项 / 缺失 0 |

纯 Python 3 标准库，无第三方依赖。已知非端点占位符（如 `/api/webhooks/YOUR_WEBHOOK`）在 `IGNORED` 中显式排除，通配 `/*` 按 `/:id` 归一化比对。

> 这两个脚本与 `cargo test --workspace` 已同时纳入 `.github/workflows/build.yml` 的发布门禁。**删除旧版 `src/` / `web/src/` 存档前**，需先为 `parity.py` 指定基线来源，否则对照脚本会失去基线。

## 2. Rust 侧测试（cargo test --workspace）

仓库根 `Cargo.toml` 为 workspace，成员：`crates/domain`、`crates/platform-linux`、`crates/storage`、`crates/services`、`crates/api`、`crates/app`。

```
cargo test --workspace
cargo test --workspace -- --nocapture      # 显示测试输出
cargo test -p udx-api                      # 只跑单个 crate
```

建议的测试分层（与契约对应）：

- `crates/api`：路由注册测试 —— 遍历 Axum Router 已注册的路径/方法，与 `tests/contract/api_endpoints.json` 做一致性比对（可复用 verify.py 的归一化思路）；认证中间件测试：未登录访问所有写接口返回 401；OPTIONS 预检不触发设备动作。
- `crates/services` / `crates/domain`：命令 ID 白名单 + 固定 argv、超时与输出上限、SQL 参数化、Webhook SSRF 校验、OTA 签名与回滚的单元测试。
- `crates/storage`：参数化查询与数据库迁移可恢复性测试。
- `crates/app`：集成测试，覆盖报告第 11 章功能/安全用例。

> `verify.py` 是静态契约校验（无需编译），`cargo test --workspace` 是运行时行为测试；两者都要通过，不能以「编译通过」代替验收（报告 11.4）。

## 3. 前端手工验收清单

基于 `web/src/composables/useApi.js` 的调用约定，逐页在浏览器（桌面 + 移动）手工勾选：

- [ ] 登录：正确密码拿到 Token；错误密码返回 401 且清空本地 Token；连续失败触发限速
- [ ] 登录状态：`/api/auth/status` 返回 `authenticated`；前端同时兼容旧字段 `logged_in`（两者为 false 时清除本地 Token）
- [ ] 登出：本地 Token 清除，再用旧 Token 访问写接口返回 401
- [ ] 修改密码：旧密码错误返回 401；成功后所有设备强制重新登录
- [ ] 系统信息 `/api/info`：字段齐全，空值/异常值不导致页面崩溃
- [ ] 设备控制：reboot / poweroff 需登录 + 二次确认 + 审计提示
- [ ] 网络：飞行模式、网络模式、SIM 卡切换；失败时恢复原配置
- [ ] 频段/小区：列表、锁定/解锁、当前频段刷新
- [ ] 流量：总量、配置、限额设置（GB→字节换算）、清空统计
- [ ] 定时重启：读取/设置（day/hour/minute）、清除；`claen` 拼写保持不变
- [ ] 系统时间：读取、NTP 同步
- [ ] 充电：配置（GET 免认证 / POST 需认证）、手动开/关
- [ ] AT 调试：空命令、超时、成功/失败三种 `Code/Error/Data` 展示
- [ ] USB：模式读取/设置、热切换
- [ ] APN：列表/配置/模板增删改/套用/清除（含 `/api/apn` 兼容别名）
- [ ] 数据连接与漫游：读取/设置
- [ ] netif：接口列表、流量统计、监听开关
- [ ] 短信：收件箱、发送（重复提交防护）、已发送、删除、配置、修复开关
- [ ] Webhook：配置、测试投递、日志（证书/SSRF 错误有明确提示）
- [ ] 插件：列表、上传、删除单个、删除全部（保留内置 `_default` 插件）
- [ ] 插件存储与脚本：GET/POST/DELETE/PUT、跨插件数据隔离
- [ ] Shell / Web 终端：仅管理员可见；超时、输出上限、审计提示生效
- [ ] 密保：状态、设置、找回密码全流程、恢复出厂设置（二次确认）
- [ ] Rathole：配置、服务增删、启停、状态、日志、自启
- [ ] IPv6 代理：配置、规则增删、启停/重启、状态、上报、测试、日志
- [ ] 手机壳模式：读取/设置
- [ ] 实时推送 `/api/events`（SSE）：断线重连、心跳、`Last-Event-ID`
- [ ] CSP：页面无 `unsafe-eval`，插件不再运行时编译模板；控制台无 CSP 违规
- [ ] 401 统一处理：触发 `auth-required` 事件跳转登录页
- [ ] 白色毛玻璃主题：亮 / 暗 / 跟随系统三套变量，卡片 `backdrop-filter` 生效，无内联样式 CSP 违规
- [ ] 动态 SVG：背景光晕、品牌标识、脉冲波形正常播放；`prefers-reduced-motion` 下动画停用
- [ ] 弹窗三形态：桌面 Centered Dialog、手机 Bottom Sheet（把手点击 / 下滑关闭）、日志详情 Right Side Sheet；设置页可强制切换并预览
- [ ] 响应式：320 / 360 / 390 / 414 / 600 / 768 / 1024 / 1440 / 1920 宽度、手机横屏均无横向溢出；表格可横向滚动；触屏点击区不小于 44px；刘海安全区不遮挡
- [ ] 看门狗：UI 主线程卡死提示可取消 / 立即刷新；后端失联后持续重试并在恢复时重建 WS / SSE；`/api/health` 返回 `watchdog` 状态

## 4. 真机验收项（技术报告第 11 章）

### 11.1 功能测试

- 逐 API 比较 HTTP 状态码、JSON 字段、空值、错误码和边界输入。
- Modem 不在线、SIM 未就绪、D-Bus 服务重启、USB 设备拔插时的降级表现。
- 短信发送重复提交、AT 超时、命令排队和取消。
- 网络切换失败时恢复原配置；重启前确认授权和操作审计。
- OTA 下载中断、校验失败、磁盘空间不足、更新后启动失败回滚。

### 11.2 安全测试

- 未登录访问所有写接口均应拒绝；OPTIONS 预检不得触发设备动作。
- 测试路径遍历、Shell 元字符、SQL 特殊字符、超长输入和畸形 JSON。
- 测试插件跨权限调用、越权读取其他插件数据、模板注入和 XSS。
- 测试 Token 过期、撤销、重放、暴力登录和 CSRF 风险。
- OTA 证书错误、签名错误和降级包均必须失败关闭。

### 11.3 稳定性与资源测试

| 场景 | 建议验收项 |
| --- | --- |
| 连续运行 72 小时 | 无崩溃；无持续增长的 FD、线程数和内存趋势 |
| 并发只读请求 | 队列有界；延迟和内存峰值在预设预算内 |
| 设备动作并发 | 同一硬件资源无竞态；冲突请求明确返回 Busy |
| D-Bus 断连恢复 | 服务恢复后自动重连；无无限重试风暴 |
| 低内存压力 | 可控拒绝新任务；关键设备恢复操作优先级明确 |
| 升级 / 回滚 | 断电或写入失败后可恢复到可启动版本 |

另按报告 9.3 记录 RSS/PSS、峰值 RSS、API 延迟 p50/p95/p99、任务队列长度、文件体积；按 9.4 至少执行 24 小时基础稳定性测试与 72 小时长稳测试。

### 11.4 发布门槛

每个模块必须同时满足 **功能兼容、安全测试、设备实测、资源基线** 四项要求；不能因为编译通过就认定重构完成。
