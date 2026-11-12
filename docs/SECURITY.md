# 安全审计对照（技术报告第 08 章）

本文件把《UDX710-TOOLS Rust + 原生前端 Html5 详细技术报告》第 08 章「安全审计：重构前应先修复的问题」的 **8 个观察点**，与旧 C 实现逐一对照，给出「等级 / 原问题 / 本次修复 / 验证方式 / 残留风险」五栏结论，作为 Rust + Axum 重构实现与验收的对照基线。端点级约束另见 `docs/API_CONTRACT.md`，机器清单见 `tests/contract/api_endpoints.json`。

> **源码位置说明**：下文引用的 `src/handlers/http_server.c`、`src/handlers/handlers.c`、`src/include/lib/http_utils.h` 等路径与行号，均为 **v2.x 已删除源码**的历史位置；原始实现可在 git 历史（如 `3a6c5f1`）查阅，路由 / 端点 / 组件基线已固化在 `tests/contract/v2_baseline.json`。

> **审查边界**：以下结论来自静态源码审查，等价于风险清单与修复设计，并不等于已在真机上完成利用验证；每一项都必须在第 11 章测试验收中复核（见 `tests/README.md`）。

> **用户明确覆盖指令**：报告第 3 项原建议为「取消 Web 任意 Shell，或严格限制为管理员诊断功能」。本项目用户要求 **保留 Web 任意 Shell**。因此该项处置不是删除功能，而是把「内部设备动作」与「Web 任意 Shell 诊断通道」彻底分离，并对后者施加一整组强制约束。详见 §3。

## 对照总览

| # | 等级 | 观察点（报告原文浓缩） | 修复要点 | 残留风险摘要 |
| --- | --- | --- | --- | --- |
| 1 | 严重 | 认证白名单放行 POST / OPTIONS 的 `/api/device_control`（含 reboot / poweroff） | 移除免认证；按动作授权；OPTIONS 仅处理预检 | 已认证管理员仍可重启设备，靠授权/审计约束 |
| 2 | 严重 | Token 验证经 SQLite CLI / `sh -c`，SQL 字符串拼接存在注入风险链 | 改用 `rusqlite` 参数化查询，彻底不走 Shell | 旧库数据迁移期的兼容读取 |
| 3 | 严重 | Shell 命令执行仅用危险命令字符串黑名单 | 内部动作改命令 ID 白名单 + 固定 argv；Web Shell 独立保留并强约束 | **任意 Shell 仍是设备上最高权限入口，Token 泄露即等同 RCE** |
| 4 | 高 | `run_command_timeout` 委托普通执行、无真正超时 | `tokio::time::timeout` + `kill_on_drop` + 进程组清理 | 子进程忽略 SIGKILL 的极端情况 |
| 5 | 高 | OTA 使用 `curl -k` / `wget --no-check-certificate` | 恢复 TLS 校验 + 签名/摘要 + 原子替换 + 失败回滚 | 签名密钥管理与密钥轮换 |
| 6 | 高 | 脚本上传直接拼接文件名并设置执行权限 | 文件名 allowlist、目录约束、原子写入、非执行存储 | 已上传的历史可执行脚本需清理 |
| 7 | 高 | Webhook URL / Header 进入 Shell 命令字符串 | HTTP 客户端 + SSRF 防护 + 响应体上限 + 连接超时 | DNS rebinding、内网地址判定的完备性 |
| 8 | 中 | 同步 D-Bus / `sleep` / 阻塞进程调用 | 资源执行器 + 异步 zbus + 有界阻塞池 | 阻塞池耗尽时的降级策略 |

---

## 1. 认证白名单放行设备控制（严重）

- **等级**：严重
- **原问题**：`src/handlers/http_server.c:70-76` 的 `is_auth_whitelist` 把 `POST` 与 `OPTIONS` 的 `/api/device_control` 直接放行；而 `src/handlers/handlers.c:224-249` 的 handler 支持 `reboot` / `poweroff` 动作。未认证攻击者可远程关机/重启设备。旧版 `HTTP_HANDLE_OPTIONS`（`src/include/lib/http_utils.h:29-33`）还会在方法检查宏里提前返回，使 OPTIONS 请求进入业务 handler 前的路径更容易被误用。
- **本次修复**：
  - 从白名单彻底移除 `/api/device_control`；契约中该端点 `auth=required`（`tests/contract/api_endpoints.json`），并由 `verify.py` 的 (c) 强制校验。
  - 权限细分为 `device.control.reboot` 与 `device.control.poweroff`，按 `action` 分别授权，最小权限。
  - **OPTIONS 仅处理 CORS 预检，不执行任何业务动作**：由 Axum/tower-http 的 CORS 层在路由分发前统一应答预检，预检请求不进入任何设备 handler；返回只允许的方法/头白名单，不带副作用。
  - 重启/关机前写独立审计日志（actor、时间、动作、结果），前端二次确认。
- **验证方式**：`cargo test` 中间件测试——未登录 `POST /api/device_control` 返回 401；`OPTIONS /api/device_control` 返回预检响应且不触发 reboot/poweroff（用 mock 设备执行器断言零调用）；对照 `verify.py` (c)。真机按报告 11.2「未登录访问所有写接口均应拒绝；OPTIONS 预检不得触发设备动作」执行。
- **残留风险**：合法管理员 Token 被盗仍可重启设备；靠 Token 生命周期（§9）、权限细分与审计缓解，不改变该动作本身的高影响。

## 2. Token 验证经 SQLite CLI / Shell，SQL 拼接注入（严重）

- **等级**：严重
- **原问题**：`src/system/database.c:138-139`、`164`、`191`、`224`、`320` 通过 `snprintf` 把 SQL 拼进 `sh -c "sqlite3 '<db>' \"<sql>\""` 执行；`src/system/auth.c:171-173`、`199-201` 把 Token 直接拼进 `INSERT`/`SELECT`；`src/system/security.c:144-150` 依赖手写 `db_escape_string`。这形成「Shell 注入 + SQL 注入」双风险链，且 Token 验证位于每个请求的热路径。
- **本次修复**：
  - 数据层统一改为 `rusqlite` **参数化查询 / 预处理语句**（`crates/storage`），彻底移除 `sqlite3` CLI 与 `sh -c`；用户数据只作为绑定参数，永不进入 SQL 文本或命令行。
  - 密码与密保答案改为**加盐哈希**（旧版 `auth.c:68`、`security.c:29-31` 为无盐 SHA-256），并做恒定时间比较。
  - 表名/列名等无法参数化的部分只允许来自代码内常量白名单。
- **验证方式**：单元测试覆盖引号、分号、注释符、超长与 Unicode 输入；断言生成的 SQL 文本不含用户输入；集成测试用「`token=' OR 1=1 --`」类载荷确认认证不被绕过；对旧库执行迁移并验证可恢复。
- **残留风险**：历史数据库中已写入的脏数据、以及旧版无盐哈希口令需强制轮换；迁移脚本本身必须可回滚。

## 3. Shell 命令执行仅用黑名单（严重）—— 保留 Web 任意 Shell 的处置

- **等级**：严重
- **原问题**：`src/system/plugin.c:17-39` 仅用 9 条危险命令字符串黑名单（`rm -rf /`、`mkfs`、`dd if=` 等）做检查，随后 `plugin.c:67` 直接 `popen(cmd, "r")` 执行用户输入的整条命令。黑名单可轻易绕过（`\rm`、变量展开、`base64 -d | sh`、`python -c`、重定向、通配符、编码变体），且 `popen` 无超时、无并发限制。
- **本次修复（遵循用户「保留 Web 任意 Shell」的明确指令，不取消功能）**：把命令执行拆成两条互不混用的通道。

  **通道 A — 内部设备动作（命令 ID 白名单 + 固定 argv）**：所有 `/api/*` 业务端点（网络、USB、充电、AT、OTA、netif、rathole、ipv6-proxy、密保、清理缓存等）不得接受命令字符串。实现为「命令 ID → 固定可执行文件 + 固定 argv 模板」的注册表，用户输入只作为受校验的参数值注入 argv 槽位（枚举/范围/allowlist），**绝不拼接 Shell、绝不调用 `sh -c`、绝不使用 `popen`**；参考报告 6.3 的 `tokio::process::Command` 固定 argv 范例。

  **通道 B — Web 任意 Shell（独立、显式的高风险管理员诊断通道）**：保留 `POST /api/shell` 与 `WebSocket /api/terminal/ws`，二者共用 `shell.execute` 权限与同一套强制约束。

- **威胁模型**：
  - **资产**：设备文件系统与配置、Modem/串口、认证数据库（口令哈希与 Token）、短信与 AT 数据、同网段其他设备、OTA 回滚分区。
  - **攻击者**：① 已持有效管理员 Token 的合法用户（误操作或恶意）；② 通过 XSS / CSRF / Token 泄露获得会话的外部攻击者；③ 同一 LAN 内的邻近攻击者；④ 通过 OTA / 插件供应链植入的载荷。
  - **入口**：`/api/shell`（一次一命令）、`/api/terminal/ws`（交互式长连接，更易驻留）。
  - **能力边界**：任意 Shell 一旦调用成功即等同设备上的进程执行权限，因此**不得把黑名单当作安全边界**；安全边界是「谁能调用」与「调用被如何限制、记录」。

- **滥用风险**：任意文件读写/删除、读取口令哈希与 Token、植入后门与自启动持久化、把设备变为跳板或挖矿/DoS（fork 炸弹、内存耗尽）、篡改 OTA 回滚分区、嗅探同网段流量、绕过其他所有端点的权限模型。

- **缓解措施（强制约束，逐条落地）**：

  | 约束 | 实现要求 |
  | --- | --- |
  | 有效 Token + 细粒度权限 | 两个端点均 `auth=required`；`/api/shell` 需 `shell.execute`、`/api/terminal/ws` 需 `terminal.session`；WS 握手时校验并在**每条 input 帧**重新 `auth.verify`，Token 撤销 / 过期 / 权限回收后立即断开会话 |
  | 登录失败限速 | `/api/auth/login` 与 `/api/security/verify` 按来源 IP + 账户做指数退避 / 锁定，防止暴力获取 Token 后调用 Shell |
  | 速率限制 | 对 `/api/shell` 与 WS 帧做每 Token / 每 IP 的令牌桶限流，超限返回 429 并记审计 |
  | 命令长度上限 | 单条命令（及 WS 单帧）设字节上限，拒绝超长与畸形 JSON |
  | 真超时 + 进程清理 | `tokio::time::timeout` 包裹；子进程 `.kill_on_drop(true)`；以独立进程组启动，超时/断开时 kill 整个进程组，杜绝孤儿 shell 与后台驻留 |
  | stdout / stderr 输出上限 | 截断到固定上限（如 64 KiB），超限丢弃并标记，防止内存耗尽与信息批量外泄 |
  | 并发上限 | 全局 + 每 Token 的并发 Shell 会话上限，超限拒绝；WS 空闲超时与心跳 |
  | 独立审计日志 | 专用审计通道记录 **actor / 时间 / 命令 / 退出码 / 执行时长 / 来源**；**绝不记录密码与 Token**；日志不可被该通道自身修改 |
  | 可配置开关 `shell.enabled` | **默认启用**（用户要求保留）；关闭时两端点返回 404/403，且审计仍记录被拒尝试 |
  | 网络与前端配套 | 生产优先 HTTPS、限制管理来源 IP；CSP 无 `unsafe-eval`（§9）降低 XSS 窃取 Token 的概率 |

- **部署者应如何决策是否启用**：默认启用，但需按部署环境明确决策——
  - **适合启用**：设备仅在隔离可信 LAN、管理员唯一且可信、已启用 HTTPS、Token 不长期驻留浏览器、有定期审计复核、且确有远程诊断刚需。
  - **应设 `shell.enabled=false` 并改用外部串口/SSH 诊断**：设备暴露在不可信网络或公网、无 HTTPS、多管理员/多租户共享、合规要求禁止任意命令执行、无法把 Token 从 `localStorage` 迁出、或无法保证审计日志不可篡改。
  - 启用期间应配套强口令、Token 轮换与短期有效期、定期复核审计日志、最小化持有 `shell.execute` 的账户数量。

- **WS 握手认证与日志脱敏（本次落地）**：浏览器 `WebSocket` / `EventSource` 无法设置 `Authorization` 头，前端把会话 Token 作为 `?token=` 附加（见 `web/js/core/websocket.js`）。后端按下列约束收敛，避免凭据散落到任意 URL 或日志：

  | 环节 | 实现 |
  | --- | --- |
  | 读取范围 | `crates/api/src/middleware.rs::query_token` **仅**在 `/api/ws`、`/api/terminal/ws` 且带 `Upgrade: websocket` 的握手，以及 `/api/events` 的 SSE 请求上读取 query token；普通 REST 请求（含 `POST /api/shell`）一律只认 `Authorization: Bearer`。query 与 token 长度均设上限。 |
  | 会话内复核 | `gate` 把 `SessionToken` 放入请求扩展；`crates/api/src/ws.rs::terminal_session` 对每条 `input` 帧重新校验 Token 与 `terminal.session` 权限，失效即发错误帧并断开（对应上表「逐帧复核」）。 |
  | 访问日志 | 改用 `request_log` 中间件，**只记录 method / path / status / 耗时**，不记录 query、请求头或 body；已移除 `TraceLayer::new_for_http()`，其 `DefaultMakeSpan` 会把含 `?token=` 的完整 URI 写进 tracing span。 |
  | 前端脱敏 | `web/js/pages/rathole.js` 的穿透密钥输入框留空表示不修改，列表 / 日志 / 服务端配置弹窗均掩码，不回显密钥。 |

- **验证方式**：`cargo test` 覆盖——无 Token / 无 `shell.execute` 权限调用被拒；超限命令、超长输出、超时命令（`sleep 999`）被 `kill_on_drop` 终止且无残留子进程；并发上限；审计日志字段齐全且不含密码/Token；`shell.enabled=false` 时两端点不可用。真机按报告 11.2 注入 Shell 元字符、超长输入与畸形 JSON，并确认超时后进程组被清理。
- **残留风险**：**这是用户明确接受的取舍**。即使全部缓解到位，任意 Shell 仍是设备上权限最高的入口，Token/XSS/会话泄露即等同远程命令执行；缓解措施只降低发生概率与影响范围，不改变该能力本身。若未来威胁模型变化，应重新评估是否关闭。

## 4. run_command_timeout 无真正超时（高）

- **等级**：高
- **原问题**：`src/system/exec_utils.c:75-83` 的 `run_command_timeout` 注释即写明「简化实现：直接调用 run_command / TODO: 实现真正的超时机制」，且参数传递有误（把 `va_list` 当 `char*` 传入）。任何依赖它的设备动作都可能无限阻塞。
- **本次修复**：以 `tokio::process::Command` + `tokio::time::timeout(Duration, child.wait_with_output())` 实现真超时；`.kill_on_drop(true)`；独立进程组 + 超时/断连时 kill 整组；输出大小上限（报告 6.3）；所有设备动作统一经资源执行器，超时返回明确的 Timeout 状态而非「命令已发送」。
- **验证方式**：单元测试用 `sleep` / 不退出子进程断言超时触发且进程组无残留；断言输出超限时返回 `OutputTooLarge`；报告 11.1「AT 超时、命令排队和取消」。
- **残留风险**：子进程进入不可中断睡眠或忽略 SIGKILL 的极端内核状态；需配合看门狗与整体请求超时兜底。

## 5. OTA 跳过 TLS 证书验证（高）

- **等级**：高
- **原问题**：`src/system/update.c:33,35` 用 `curl -k` / `wget --no-check-certificate`，`116,118` 检查版本同样跳过证书；`77-96` 直接 `chmod +x` 并执行包内安装脚本，无签名/摘要校验、无原子替换、无失败回滚。
- **本次修复**：
  - 恢复 **TLS 证书验证**（`reqwest` + rustls，禁用 `-k` 等价行为）；下载/版本检查 URL 来自服务端配置或 allowlist，禁止内网与云元数据地址（SSRF）。
  - 更新包强制 **签名或可信摘要校验**（先验后用）；连接超时、整体超时、下载体与解压后体积/条目数上限。
  - **原子替换 + 失败回滚**：解压到临时目录（防 Zip Slip），校验通过后通过 A/B 或「备份 + 原子 rename」切换，失败自动回滚到可启动版本；断电/写入失败可恢复（报告 11.3）。
  - `ota.install` 需 `auth=required` + 授权 + 审计；禁止从不可信源执行更新包。
- **验证方式**：报告 11.2「OTA 证书错误、签名错误和降级包均必须失败关闭」；11.1「下载中断、校验失败、磁盘空间不足、更新后启动失败回滚」；11.3「断电或写入失败后可恢复到可启动版本」。
- **残留风险**：签名公钥/密钥的安全存储与轮换流程、降级保护（防回滚到有漏洞的旧版本）需单独设计；固件回滚分区本身的完整性依赖设备引导链。

## 6. 脚本上传拼文件名 + 设置执行权限（高）

- **等级**：高
- **原问题**：`src/system/plugin.c:221-251` 的 `save_plugin` 仅检查 `..` 与 `/` 就直接拼接文件名写盘；`src/system/update.c:88` 对安装脚本执行 `chmod +x`。上传内容可被当作可执行脚本运行。
- **本次修复**：
  - **文件名 allowlist**：仅允许 `[A-Za-z0-9_-]` + 固定扩展名，拒绝路径分隔符、`..`、空字节与超长名；落盘路径由「固定根目录 + 校验后的文件名」拼装，禁止任何用户可控的路径片段。
  - **目录约束**：写入限定在专用非执行目录；**原子写入**（临时文件 + rename）。
  - **非执行存储**：文件不设置执行位，静态资源以只读方式提供；插件按报告第 07 章改为受限 UI 描述协议，不再由宿主执行任意 JS。
  - **脚本执行另设权限审批**：新增 `script.execute` 权限，默认不执行；执行走通道 A 的固定 argv 与超时/输出上限。
- **验证方式**：报告 11.2「路径遍历、Shell 元字符、超长输入和畸形 JSON」；断言落盘文件 mode 不含执行位；断言插件间数据隔离（11.2 越权读取其他插件数据）。
- **残留风险**：旧版已落盘的可执行脚本与插件需在迁移时清理/隔离；兼容层若允许运行旧插件脚本，必须单独隔离评估（报告 7.3）。

## 7. Webhook URL / Header 进入 Shell 命令（高）

- **等级**：高
- **原问题**：Webhook 的 `url` / `headers` / `body`（`src/handlers/handlers.c:635-643`、`2689-2691`）在旧实现中进入 Shell 命令字符串，构成命令注入 + SSRF 风险。
- **本次修复**：
  - 改用 **HTTP 客户端库**（`reqwest`）发起请求，URL / Header 作为结构化参数，绝不拼 Shell。
  - **SSRF 防护**：目标地址 allowlist 或黑名单（拒绝环回、私网、链路本地、云元数据 `169.254.169.254`、非 HTTP(S) scheme）；禁止跟随到内网地址的重定向；解析后逐跳校验 IP（含 DNS 解析结果）。
  - **响应体上限 + 连接超时**（及整体超时），流式读取并在超限时截断；不把远端响应直接回显为可执行内容。
  - IPv6 代理的 `webhook_url` / `send` / `test` 同样适用；日志对 URL/Header 做脱敏。
- **验证方式**：单元测试用内网/元数据/重定向到内网/非 HTTP scheme 的 URL 断言被拒；断言超大响应体被截断、连接超时被终止；报告 11.2 的注入类用例。
- **残留风险**：DNS rebinding（解析时合法、连接时指向内网）需通过「解析后固定 IP 连接」或连接时复校缓解；allowlist 维护成本与误放行风险。

## 8. 同步 D-Bus / sleep / 阻塞进程调用（中）

- **等级**：中
- **原问题**：`src/handlers/http_server.c:539-557` 的事件循环用 `mg_mgr_poll(10)` + 计数式 `sleep` 维护，多个设备模块使用同步 D-Bus 与阻塞进程调用，易阻塞事件循环并放大延迟。
- **本次修复**：拆出**资源执行器**——异步 D-Bus（zbus/tokio）、Tokio 定时器替代 `sleep`、阻塞操作放入**有界阻塞池**；同一 Modem 串口加互斥锁，冲突请求明确返回 Busy；后台任务设有界队列、超时、退避上限与关闭信号（报告 6.1 / 6.2 / 6.4）。
- **验证方式**：报告 11.3「并发只读请求队列有界」「设备动作并发无竞态、冲突返回 Busy」「D-Bus 断连恢复无无限重试风暴」「低内存压力可控拒绝新任务」。
- **残留风险**：阻塞池耗尽会拒绝新任务，需明确降级与优先级（关键恢复操作优先）；同步第三方库的意外误用需 lint / code review 把关。

---

## 9. 安全默认值逐项落地（报告 08 章「安全默认值」）

| 要求 | 落地位置 / 做法 | 验证方式 |
| --- | --- | --- |
| 所有设备写操作默认要求认证 + 细粒度授权 | 契约中所有写接口 `auth=required`；`verify.py` (c) 强制；权限名见 `docs/API_CONTRACT.md` | `verify.py` + 中间件 401 测试 |
| 登录失败限速 | `/api/auth/login`、`/api/security/verify` 按 IP + 账户退避/锁定 | 报告 11.2 暴力登录测试 |
| Token 过期 / 撤销 / 轮换；避免长期凭证存 `localStorage` | 服务端 Token 表带 `expire_time`；登出只撤当前 Token，改密撤全部；建议迁移到短时效 Token + 内存/安全 Cookie 存储 | 报告 11.2「Token 过期、撤销、重放」 |
| 生产优先 HTTPS；隔离 LAN 也有明确威胁模型 | 部署配置强制 TLS；无 HTTPS 时建议关闭 `shell.enabled` | 部署检查表 |
| **CSP 无 `unsafe-eval`；插件模板不运行时编译** | `Content-Security-Policy` 不含 `unsafe-eval` / `unsafe-inline`；前端移除 Vue 运行时模板编译，插件改受限 UI 描述（报告 07 章） | 浏览器控制台无 CSP 违规；报告 11.2 XSS/模板注入 |
| OTA 校验签名/可信摘要 + 失败可回滚 | §5 | 报告 11.1 / 11.2 / 11.3 |
| **日志不记密码、Token、短信正文、完整 AT 敏感参数** | 统一日志脱敏层：`password` / `old_password` / `new_password` / `token` / `answer1|2` / `content`（短信）/ 完整 AT 命令一律掩码或不记；访问日志 `request_log` 只记录 method / path / status / 耗时，**不含 query（WS 的 `?token=`）与请求头**；Shell 审计日志只记命令不记凭证 | 日志采样审查 + `websocket_query_token_is_accepted_only_on_upgrade` + 报告 11.2 |
| CSRF 风险 | 写接口校验 `Origin` / 自定义头，避免仅靠 Cookie 认证；CORS 不使用通配符来源 | 报告 11.2 CSRF 测试 |
| OPTIONS 仅处理 CORS 预检不执行业务动作 | §1，由 CORS 层统一应答 | `verify.py` / 中间件测试 |

## 10. 与第 11 章安全测试的映射

| 报告 11.2 条目 | 对应本文件小节 |
| --- | --- |
| 未登录访问所有写接口均应拒绝；OPTIONS 预检不得触发设备动作 | §1、§9 |
| 路径遍历、Shell 元字符、SQL 特殊字符、超长输入、畸形 JSON | §2、§3、§6、§7 |
| 插件跨权限调用、越权读取其他插件数据、模板注入和 XSS | §6、§9（CSP、插件协议） |
| Token 过期、撤销、重放、暴力登录和 CSRF | §9 |
| OTA 证书错误、签名错误和降级包均必须失败关闭 | §5 |

> 发布门槛（报告 11.4）：每个模块必须同时满足功能兼容、安全测试、设备实测、资源基线四项要求；不能因为编译通过或 `verify.py` 通过就认定重构完成。
