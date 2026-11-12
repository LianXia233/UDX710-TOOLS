# API 契约（Rust + Axum 重构）

本文件是 UDX710-TOOLS 从 C/Mongoose 后端重构为 Rust + Axum 的 **端点契约总表与语义说明**，遵循技术报告第 10 章「先定义 API contract，再写 Rust 实现」的迁移原则。

- 机器可读清单：`tests/contract/api_endpoints.json`（95 条，每条 7 字段）
- 静态校验：`python3 tests/contract/verify.py`（用法见 `tests/README.md`）
- 安全对照：`docs/SECURITY.md`（报告第 08 章 8 个观察点）
- 数据源：`src/handlers/http_server.c`（路由表与 `is_auth_whitelist`）、`src/handlers/handlers.c` / `src/system/*.c`（请求/响应字段）、`web/src/composables/useApi.js`（前端调用约定）、技术报告第 08 / 11 章。

> 契约是**兼容目标 + 加固设计**的结合：路径与字段尽量保持旧版以便前端平滑迁移，认证/权限/响应语义则按第 08 章结论收紧。

## 1. 概览

| 指标 | 值 |
| --- | --- |
| 契约端点总数 | 95 |
| C 源码有效 `mg_match` 路由（归一化去重后） | 90（全部被契约覆盖，未匹配 0） |
| C 源码注释掉、容错跳过的路由 | 1（`/api/apn`） |
| `auth=required` | 87 |
| `auth=public` | 5 |
| `auth=public-get` | 3 |
| 写接口（POST / PUT / DELETE / WS） | 69 |
| 新增（C 源码无对应路由） | `/api/events`（SSE）、`/api/terminal/ws`（WS）、`/api/serial`、`/api/key` |

**路径约定**：统一使用 Axum 风格占位符 `/:id`、`/:name`、`/:service_id`、`/:rule_id`（对应旧 C 路由的 `/*` 通配）；**保留原始拼写**，不做大小写/拼写修正，例如 `/api/claen/cron`（旧版拼写错误）、`/api/get/Total`（大写 T）。带多方法的端点在同一 `path` 上共存（允许 GET / POST 共用），由 `methods` 数组区分。

## 2. 端点总表（按功能域分组）

#### 认证 auth
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/auth/login` | POST | `public` | `auth.login` |
| `/api/auth/status` | GET | `public` | `auth.status.read` |
| `/api/auth/logout` | POST | `required` | `auth.logout` |
| `/api/auth/password` | POST | `required` | `auth.password.change` |

#### 系统信息
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/info` | GET | `public-get` | `system.info.read` |
| `/api/serial` | GET | `required` | `system.serial.read` |
| `/api/key` | POST | `required` | `system.activation.write` |

#### 设备控制
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/device_control` | POST | `required` | `device.control.execute` |
| `/api/clear_cache` | POST | `required` | `system.cache.clear` |

#### AT 调试
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/at` | POST | `required` | `modem.at.execute` |

#### 网络配置
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/set_network` | POST | `required` | `network.mode.write` |
| `/api/switch` | POST | `required` | `sim.slot.write` |
| `/api/airplane_mode` | POST | `required` | `network.airplane.write` |

#### 高级网络 radio
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/bands` | GET | `required` | `radio.band.read` |
| `/api/lock_bands` | POST | `required` | `radio.band.lock` |
| `/api/unlock_bands` | POST | `required` | `radio.band.unlock` |
| `/api/cells` | GET | `required` | `radio.cell.read` |
| `/api/lock_cell` | POST | `required` | `radio.cell.lock` |
| `/api/unlock_cell` | POST | `required` | `radio.cell.unlock` |
| `/api/current_band` | GET | `public-get` | `radio.band.read` |

#### 流量统计
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/get/Total` | GET | `required` | `traffic.total.read` |
| `/api/get/set` | GET | `required` | `traffic.config.read` |
| `/api/set/total` | POST | `required` | `traffic.limit.write` |

#### 系统时间
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/get/time` | GET | `required` | `system.time.read` |
| `/api/set/time` | POST | `required` | `system.time.sync` |

#### 定时重启
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/get/first-reboot` | GET | `required` | `system.reboot.read` |
| `/api/set/reboot` | GET | `required` | `system.reboot.schedule` |
| `/api/claen/cron` | GET | `required` | `system.reboot.clear` |

#### 充电控制
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/charge/config` | GET, POST | `public-get` | `charge.config.read, charge.config.write` |
| `/api/charge/on` | POST | `required` | `charge.control.on` |
| `/api/charge/off` | POST | `required` | `charge.control.off` |

#### 短信 sms
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/sms` | GET | `required` | `sms.read` |
| `/api/sms/send` | POST | `required` | `sms.send` |
| `/api/sms/sent` | GET | `required` | `sms.sent.read` |
| `/api/sms/sent/:id` | DELETE | `required` | `sms.sent.delete` |
| `/api/sms/config` | GET, POST | `required` | `sms.config.read, sms.config.write` |
| `/api/sms/webhook` | GET, POST | `required` | `sms.webhook.read, sms.webhook.write` |
| `/api/sms/webhook/test` | POST | `required` | `sms.webhook.test` |
| `/api/sms/webhook/logs` | GET | `required` | `sms.webhook.logs.read` |
| `/api/sms/fix` | GET, POST | `required` | `sms.fix.read, sms.fix.write` |
| `/api/sms/:id` | DELETE | `required` | `sms.delete` |

#### OTA 更新
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/update/version` | GET | `required` | `ota.version.read` |
| `/api/update/upload` | POST | `required` | `ota.upload` |
| `/api/update/download` | POST | `required` | `ota.download` |
| `/api/update/extract` | POST | `required` | `ota.extract` |
| `/api/update/install` | POST | `required` | `ota.install` |
| `/api/update/check` | GET | `required` | `ota.check` |

#### USB 模式
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/usb/mode` | GET, POST | `required` | `usb.mode.read, usb.mode.write` |
| `/api/usb-advance` | POST | `required` | `usb.mode.hot_switch` |

#### 数据/漫游
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/data` | GET, POST | `required` | `data.connection.read, data.connection.write` |
| `/api/roaming` | GET, POST | `required` | `data.roaming.read, data.roaming.write` |

#### 网络接口 netif
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/netif/list` | GET | `required` | `netif.read` |
| `/api/netif/stats` | POST | `required` | `netif.read` |
| `/api/netif/monitor` | POST | `required` | `netif.monitor.write` |

#### APN 配置
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/apn` | GET, POST | `required` | `apn.config.read, apn.config.write` |
| `/api/apn/config` | GET, POST | `required` | `apn.config.read, apn.config.write` |
| `/api/apn/templates` | GET, POST | `required` | `apn.template.read, apn.template.create` |
| `/api/apn/templates/:id` | PUT, DELETE | `required` | `apn.template.update, apn.template.delete` |
| `/api/apn/apply` | POST | `required` | `apn.apply` |
| `/api/apn/clear` | POST | `required` | `apn.clear` |

#### Shell 诊断
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/shell` | POST | `required` | `shell.execute` |

#### 插件与存储
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/plugins/all` | DELETE | `required` | `plugin.delete_all` |
| `/api/plugins` | GET, POST | `required` | `plugin.read, plugin.write` |
| `/api/plugins/:name` | DELETE | `required` | `plugin.delete` |
| `/api/plugins/storage/:name` | GET, POST, DELETE | `required` | `plugin.storage.read, plugin.storage.write, plugin.storage.delete` |

#### 脚本管理
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/scripts` | GET, POST | `required` | `script.read, script.write` |
| `/api/scripts/:name` | PUT, DELETE | `required` | `script.update, script.delete` |

#### 密保 security
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/security/status` | GET | `required` | `security.read` |
| `/api/security/setup` | POST | `required` | `security.setup` |
| `/api/security/questions` | GET | `public` | `security.questions.read` |
| `/api/security/verify` | POST | `public` | `security.verify` |
| `/api/security/reset-password` | POST | `public` | `security.password.reset` |
| `/api/security/factory-reset` | POST | `required` | `security.factory_reset` |

#### 内网穿透 rathole
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/rathole/config` | GET, POST | `required` | `rathole.config.read, rathole.config.write` |
| `/api/rathole/services` | GET, POST | `required` | `rathole.service.read, rathole.service.create` |
| `/api/rathole/services/:service_id` | PUT, DELETE | `required` | `rathole.service.update, rathole.service.delete` |
| `/api/rathole/start` | POST | `required` | `rathole.control.start` |
| `/api/rathole/stop` | POST | `required` | `rathole.control.stop` |
| `/api/rathole/status` | GET | `required` | `rathole.status.read` |
| `/api/rathole/logs` | GET | `required` | `rathole.logs.read` |
| `/api/rathole/server-config` | GET | `required` | `rathole.server_config.read` |
| `/api/rathole/autostart` | POST | `required` | `rathole.autostart.write` |

#### IPv6 代理
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/ipv6-proxy/config` | GET, POST | `required` | `ipv6proxy.config.read, ipv6proxy.config.write` |
| `/api/ipv6-proxy/rules` | GET, POST | `required` | `ipv6proxy.rule.read, ipv6proxy.rule.create` |
| `/api/ipv6-proxy/rules/:rule_id` | PUT, DELETE | `required` | `ipv6proxy.rule.update, ipv6proxy.rule.delete` |
| `/api/ipv6-proxy/start` | POST | `required` | `ipv6proxy.control.start` |
| `/api/ipv6-proxy/stop` | POST | `required` | `ipv6proxy.control.stop` |
| `/api/ipv6-proxy/restart` | POST | `required` | `ipv6proxy.control.restart` |
| `/api/ipv6-proxy/status` | GET | `required` | `ipv6proxy.status.read` |
| `/api/ipv6-proxy/send` | POST | `required` | `ipv6proxy.send` |
| `/api/ipv6-proxy/test` | POST | `required` | `ipv6proxy.test` |
| `/api/ipv6-proxy/send-logs` | GET | `required` | `ipv6proxy.send_logs.read` |

#### 手机壳模式
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/phone-case` | GET, POST | `required` | `phone_case.read, phone_case.write` |

#### 实时推送 SSE
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/events` | GET | `required` | `events.subscribe` |

#### Web 终端 WS
| path | methods | auth | permission |
| --- | --- | --- | --- |
| `/api/terminal/ws` | WS | `required` | `shell.execute` |

## 3. 通用响应与错误语义

### 3.1 保留旧版语义（兼容基线）

旧版存在多种异构响应格式，Rust 侧以 **`{status, message}`** 作为统一规范（对齐旧版认证中间件与登录的 401 格式），并为特殊端点保留兼容层：

| 情形 | 旧版格式（源码位置） | 新版规范 |
| --- | --- | --- |
| 通用成功 | `{"status":"success","message":"..."}`（`http_utils.h:92-93` `HTTP_SUCCESS`） | 保持 |
| **通用错误** | `{"status":"error","message":"..."}`（中间件 401 `http_server.c:155-156`、登录失败 `handlers.c:1846`） | **保持该语义为规范错误格式** |
| 401 未授权 | `401 {"status":"error","message":"未授权，请先登录"}` | 保持状态码 401 + 同结构 |
| 401 密码错误 / 旧密码错误 | `401 {"status":"error","message":"密码错误"}` / `{"status":"error","message":"旧密码错误"}` | 保持 401 + 同结构 |
| 通用错误宏 `HTTP_ERROR` | `{"error":"..."}`（`http_utils.h:88-89`），**与上面不一致** | 统一收敛为 `{status:"error",message:...}`；旧前端只判断 `response.ok` 与 401，不依赖 `error` 字段，兼容无碍 |
| AT / Shell 结果 | `{"Code":0|1,"Error":"...","Data":...}`（`handlers.c:105-113`、`1289-1295`） | **保留 `Code/Error/Data` 兼容层**，前端 AT 调试页与 Web Shell 依赖该结构 |
| 数据型成功 | 直接返回业务对象（`/api/info`、`/api/auth/status` 等） | 保持 |

### 3.2 HTTP 状态码

| 码 | 语义 |
| --- | --- |
| 200 | 成功（含业务层 `status=error` 的旧兼容情况，逐步收敛到 4xx/5xx） |
| 400 | 参数缺失/非法、边界校验失败 |
| 401 | 未认证或 Token 失效（前端 `useApi.js` 收到 401 会清 Token 并派发 `auth-required` 事件） |
| 403 | 已认证但缺少细粒度权限（如无 `shell.execute`） |
| 404 | 端点不存在 / `shell.enabled=false` 时的 Shell 端点 |
| 405 | 方法不允许 |
| 429 | 登录失败限速 / 速率限制 |
| 500 | 设备动作失败、内部错误 |
| 503 / Busy | 同一硬件资源被占用（报告 11.3 冲突请求明确返回 Busy） |

### 3.3 CORS / OPTIONS

`OPTIONS` **仅处理 CORS 预检，不执行任何业务动作**：由 Axum/tower-http 的 CORS 层在路由分发前统一应答，预检请求不进入设备 handler。旧版 `Access-Control-Allow-Origin: *`（`http_utils.h:20,23`）在生产环境应收紧为明确的允许来源，且不再把 `OPTIONS` 列入任何认证白名单豁免。

## 4. 认证与权限模型

### 4.1 认证方式

- `Authorization: Bearer <token>`（旧版 `http_server.c:97-119`），Token 服务端存储并带过期时间。
- `auth` 三值语义：

| 值 | 含义 |
| --- | --- |
| `required` | 所有方法都需有效 Token（默认，覆盖全部 69 个写接口及绝大多数读接口） |
| `public` | 所有方法免认证，仅限登录与忘记密码恢复的引导入口 |
| `public-get` | **仅 GET 免认证，其余方法仍需认证**（如 `/api/charge/config` 的 POST 需认证） |

### 4.2 免认证端点（白名单对照）

| 端点 | 旧版 `is_auth_whitelist` | 新契约 | 说明 |
| --- | --- | --- | --- |
| `/api/auth/login` | public（任意方法） | `public` | 登录入口，协议必需 |
| `/api/auth/status` | public（任意方法） | `public` | 登录状态 |
| `/api/auth/logout` | **在认证中间件之前处理，实际免 Token** | `required` | 加固：要求有效 Token 并只撤当前 Token |
| `/api/auth/password` | **在认证中间件之前处理，实际免 Token** | `required` | 加固：有效 Token + 旧密码 |
| `/api/info` | GET public | `public-get` | 保持 |
| `/api/charge/config` | GET public | `public-get` | POST 需认证 |
| `/api/current_band` | GET public | `public-get` | 保持 |
| `/api/device_control` | **POST + OPTIONS public（严重漏洞）** | `required` | 报告 08 章第 1 项，已移除 |
| `/api/security/questions` | GET public | `public` | 忘记密码流程 |
| `/api/security/verify` | POST public | `public` | 忘记密码流程 |
| `/api/security/reset-password` | POST public | `public` | 忘记密码流程 |

**协议豁免的公开写入口**：`/api/auth/login`、`/api/security/verify`、`/api/security/reset-password` 三个 `public` 且含 POST 的端点，是登录与密码恢复链路协议上必须免认证的入口（移除即导致设备无法登录/恢复）。`verify.py` 的 (c) 用 `PUBLIC_WRITE_EXEMPT` 显式列出并单独打印，除此之外任何写接口标 `public` 即判失败。三者必须配套登录/校验失败限速、尝试次数上限、确认文本校验与审计。

### 4.3 权限命名与授权

- 规范：`<domain>.<resource>.<action>`，如 `radio.band.lock`、`sms.send`、`shell.execute`、`device.status.read`（对齐报告 7.2）。多方法端点在 `permission` 字段中以逗号列出多个权限。
- **按动作授权**：报告 08 章第 1 项要求，`/api/device_control` 建议由 `device.control.execute` 进一步细分为 `device.control.reboot` 与 `device.control.poweroff`。
- **Web 任意 Shell**（用户明确要求保留）：`POST /api/shell` 与 `WS /api/terminal/ws` 共用 `shell.execute`，作为独立高风险管理员诊断通道；强制约束（Token + 权限、限速、命令长度、真超时 + `kill_on_drop` + 进程组清理、输出/并发上限、独立审计、`shell.enabled` 开关）见 `docs/SECURITY.md` §3。
- **Token 生命周期**：过期、登出撤销、改密/重置撤销全部、定期轮换；避免把长期凭证存入 `localStorage`（报告 08 章安全默认值）。
- 权限不足返回 403，未认证返回 401，二者不可混用。

## 5. 兼容策略

1. **路径零改写**：`/api/claen/cron`、`/api/get/Total`、`/api/set/total`、`/api/get/set` 等原始拼写与大小写一律保留；`claen` 是旧版拼写错误，不在契约层修正。
2. **GET / POST 共用 path**：`charge/config`、`sms/config`、`sms/webhook`、`sms/fix`、`usb/mode`、`data`、`roaming`、`phone-case`、`plugins`、`scripts`、`apn/config`、`apn/templates`、`rathole/config`、`rathole/services`、`ipv6-proxy/config`、`ipv6-proxy/rules` 等在同一路径上按方法分发；契约以单条 `methods` 数组表示，`verify.py` 按归一化 path 集合比对，天然支持。
3. **通配路由精确化**：旧 C 的 `/*` 收敛为具名占位符；注册顺序必须**精确路由优先于兜底通配**（旧版 `/api/plugins/*` 排在 `/api/plugins/storage/*` 之前、`/api/sms/*` 兜底所有 `/api/sms/` 子路径，易误匹配，见 §6）。
4. **响应格式兼容层**：统一 `{status,message}`，但 AT / Web Shell 保留 `Code/Error/Data`；旧 `{"error":...}` 由兼容层同时输出或映射，避免破坏仅判断 `ok` 的旧前端。
5. **401 前端约定**：`useApi.js` 的 `request()` / `authFetch()` 收到 401 会 `clearAuthToken()` 并派发 `auth-required` 事件，Rust 侧必须保持 401 语义不变。
6. **认证恢复链路**：`/api/apn`（旧 C 路由被注释）保留为兼容别名，内部转发到 `/api/apn/config`；`/api/serial`、`/api/key` 由 Rust 补实现或前端改调用（见 §6）。
7. **渐进迁移**：按报告第 10 章，新旧后端可在测试固件并行对比结果，但不能同时控制同一硬件资源；每个阶段保留可回滚镜像，数据库迁移可恢复。
8. **新增通道**：`/api/events`（SSE，`GET`，长连接）与 `/api/terminal/ws`（`WS`）为 Rust 侧新增，不对应旧 C `mg_match` 路由；SSE 需心跳/重连/连接数上限，WS 需逐帧权限复核。

## 6. 已知差异清单

| # | 差异 | 旧版行为 | 新版契约 / 处理 | 影响 |
| --- | --- | --- | --- | --- |
| 1 | `/api/serial` | `useApi.js` 调用，C 路由表未注册 | 列为 `required` 兼容项；建议前端改读 `/api/info` 的 `serial` | 前端可能 404 |
| 2 | `/api/key` | `useApi.js` `submitActivationKey()` 调用，C 路由表未注册 | 列为 `required` 写接口；Rust 需实现激活码校验或前端移除调用 | 前端可能 404 |
| 3 | `/api/apn` | `mg_match` 路由被 `//` 注释，实际生效的是 `/api/apn/config` 等 | 保留兼容别名；`verify.py` 容错跳过注释路由 | 前端 `getApnList/setApnConfig` 依赖 |
| 4 | `auth/logout`、`auth/password` | 位于认证中间件之前，无 Token 验证 | 改为 `required` | 加固，需前端带 Token |
| 5 | `device_control` 免认证 | POST + OPTIONS 列入白名单 | 改为 `required` + 按动作权限 | 修复报告 08 章严重项 |
| 6 | OPTIONS 白名单 | `OPTIONS` 可免认证进入设备控制路径 | 仅 CORS 层处理预检，不执行业务动作 | 修复报告 08 章严重项 |
| 7 | GET 语义的写操作 | `/api/set/reboot`、`/api/claen/cron` 用 GET + query 产生设备写入 | 契约保留 `methods=["GET"]` 以兼容前端，但 `auth=required`；建议后续同时支持 POST 并切换 | 与「写操作用 POST」的直觉不符；CSRF 风险更高，须校验 Origin |
| 8 | `HTTP_ERROR` 响应体 | `{"error":"..."}` | 统一为 `{status:"error",message:...}` | 旧前端不依赖 `error` 字段，兼容 |
| 9 | AT / Shell 响应体 | `{Code,Error,Data}` | 保留兼容层 | 前端强依赖，不可改 |
| 10 | `/api/info` 敏感字段 | 返回 `passwd` / `imei` / `iccid` / `imsi` | 字段保留以兼容，但建议改为需认证读取或脱敏 | 公开 GET 泄露敏感信息 |
| 11 | 通配路由兜底顺序 | `/api/plugins/*` 先于 `/api/plugins/storage/*`；`/api/sms/*` 兜底 | 精确路由优先；具名占位符替代 `*` | 旧版存在误匹配风险 |
| 12 | `rathole/server-config` | handler 为 GET | 契约 `methods=["GET"]` | 与「配置写入」命名直觉不符 |
| 13 | `update/check` | `HTTP_CHECK_ANY`（不限制方法） | 契约按 `GET` | 前端按 GET 调用 |
| 14 | `phone-case` | 单 handler 按方法分发 | 显式列 `GET, POST` | 无 |
| 15 | 插件运行模型 | 上传即 `.js` 文件，前端可执行任意 JS | 受限 UI 描述协议 + 非执行存储 | 报告 07 章，兼容层需逐个迁移旧插件 |
| 16 | 口令 / 密保哈希 | 无盐 SHA-256 | 加盐哈希 + 恒定时间比较 | 旧哈希需强制轮换 |
| 17 | 定时重启参数 | `/api/set/reboot?day=&hour=&minute=` | 契约记为 `request.query` | 参数需范围校验 |
| 18 | CORS | `Access-Control-Allow-Origin: *` | 收紧为明确来源 | 跨域部署需配置 |
| 19 | 实时状态 | 无 SSE / WS 路由 | 新增 `/api/events`、`/api/terminal/ws` | 前端需新增订阅逻辑 |
| 20 | 数据库访问 | `sh -c "sqlite3 ..."` 拼接 | `rusqlite` 参数化 | 修复 SQL / 命令注入链 |

> 完整逐条 `notes` 见 `tests/contract/api_endpoints.json`；安全侧修复与残留风险见 `docs/SECURITY.md`；验收见 `tests/README.md` 与报告第 11 章。
