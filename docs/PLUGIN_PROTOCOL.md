# UDX710-TOOLS 插件协议 v1（受限 JSON UI）

> 依据《UDX710-TOOLS Rust + 原生前端 Html5 详细技术报告》第 07 章
> 「插件系统：从动态 Vue 模板转为受限插件协议」。
>
> 旧版（`docs/PLUGIN_GUIDE.md`、`docs/插件开发指南2.0.md`、`docs/default_plugin.js`、
> `docs/example_system_toolkit.js`）允许插件上传任意 Vue 模板 + JavaScript：通过
> `window.PLUGIN`、`{{ }}` 插值、`@click`、`v-model`、`methods` 以及 `this.$api.shell()`
> 在页面内运行时编译模板并执行系统命令。新版前端为原生 HTML5 / ES Modules，
> 宿主**不再编译 Vue 模板、不再执行插件提供的任意 JS**。
>
> 本协议把插件拆成「声明 / 界面 / 权限 / 数据」四部分，用有限组件类型加动作白名单
> 覆盖原有能力，安全边界由宿主掌握。

## 1. 设计目标与安全边界

- **默认拒绝**：任何未声明的权限、未注册的动作、未支持的数据类型一律拒绝。
- **最小权限**：插件只能读取声明过的数据，只能触发声明过且在白名单内的动作。
- **不执行任意代码**：宿主渲染器只解释 `ui.json` 的有限 view 类型，不执行插件 JS / HTML。
- **只读数据路径**：`bind` 仅用于从宿主数据快照读取展示值，不能写回宿主状态。
- **动作白名单**：所有副作用必须经由宿主注册动作，参数按 Schema 校验。
- **数据隔离**：插件数据按插件 ID 隔离，禁止跨插件、跨目录、路径穿越访问。
- **可迁移**：协议版本化（`schema_version`、`min_api_version`），第一阶段不破坏插件生态。

## 2. 插件包结构

```
example-plugin/
├── manifest.json       # 插件声明（id / name / version / min_api_version）
├── ui.json             # 受限 UI 描述（view 树）
├── permissions.json    # 所需权限与动作声明（安装时校验的权威来源）
└── assets/             # 本地静态资源（只读，仅允许静态文件类型）
```

约定：四个文件均为 UTF-8、LF 换行的严格 JSON；目录名与 `manifest.json` 的 `id` 一致；
`assets/` 可为空但建议保留。包内不得出现 `.js`、`.html`、可执行文件或符号链接。

## 3. manifest.json

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `id` | string | 是 | 全局唯一插件 ID，须匹配 `^[a-z0-9][a-z0-9._-]{1,63}$`；也是数据隔离目录名 |
| `name` | string | 是 | 插件显示名称 |
| `version` | string | 是 | 语义化版本号（SemVer） |
| `min_api_version` | integer | 是 | 所需宿主插件协议最低版本；宿主低于此值则拒绝安装 |
| `description` | string | 否 | 简短描述 |
| `author` | string | 否 | 作者 |

示例：

```json
{
  "id": "example.status",
  "name": "设备状态",
  "version": "1.0.0",
  "min_api_version": 1,
  "description": "展示设备与调制解调器状态，并提供刷新按钮（受限 JSON UI 协议示例）"
}
```

## 4. ui.json

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `schema_version` | integer | UI 描述格式版本，当前为 `1` |
| `id` | string | 必须与 `manifest.json.id` 一致 |
| `title` | string | 插件页面标题 |
| `permissions` | string[] | 本视图所需的**只读数据权限**，必须是 `permissions.json` 声明的子集 |
| `view` | object | 单一根 view 节点，构成组件树 |

`view` 树由有限节点组成；每个节点必须带 `type`，未知类型、未知字段或字段类型不符一律拒绝加载。
宿主会限制节点总数与嵌套深度，防止超大 JSON 造成渲染阻塞。

## 5. 受支持的 view 类型

| `type` | 类别 | 说明 | 关键字段 |
| --- | --- | --- | --- |
| `column` | 容器 | 纵向排列子节点 | `children[]` |
| `row` | 容器 | 横向排列子节点 | `children[]` |
| `text` | 展示 | 一行文本 | `label`、`bind` |
| `status` | 展示 | 把布尔 / 枚举值渲染为状态灯 | `label`、`bind` |
| `badge` | 展示 | 徽标 / 标签 | `label`、`bind` 或 `value` |
| `button` | 动作 | 触发一个白名单动作 | `label`、`action`、`params?` |
| `table` | 展示 | 把数组数据渲染为表格 | `bind`（数组路径）、`columns[]` |
| `input` | 输入 | 单行文本输入 | `name`、`label`、`placeholder?` |
| `select` | 输入 | 下拉选择 | `name`、`label`、`options[]` |
| `divider` | 布局 | 分隔线 | 无 |

### 5.1 `bind` 是只读数据路径

- 形如 `device.name`、`modem.connected`、`network.interfaces[0].ip`（点号分隔，可选下标）。
- 宿主按插件 ID 提供**只读数据快照**，解析前做前缀白名单校验；解析结果仅用于展示。
- 禁止：`..`、绝对路径 `/`、协议前缀、`plugin://` 跨插件引用、`window` / `document` /
  `globalThis` 等全局对象、任意表达式或函数调用。
- 路径不存在时渲染为空占位，不抛错、不回退到任意取值。
- `columns[].bind` 是相对于表格每行元素的子路径，例如 `name` 表示 `row.name`。

### 5.2 `input` / `select` 不写回数据

`bind` 一律只读，输入控件不得写回宿主状态。输入值通过 `name` 命名，
在按钮 `params` 中以受控占位符 `$input.<name>` 引用，由宿主组装成动作参数：

```json
{
  "type": "row",
  "children": [
    { "type": "input", "name": "host", "label": "目标地址", "placeholder": "IP 或域名" },
    { "type": "select", "name": "count", "label": "次数",
      "options": [
        { "value": 1, "label": "1 次" },
        { "value": 4, "label": "4 次" }
      ] },
    { "type": "button", "label": "Ping", "action": "network.ping",
      "params": { "host": "$input.host", "count": "$input.count" } }
  ]
}
```

`$input.*` 只是宿主渲染器的**受控占位符**，不是表达式求值，也不执行 JS。
所有写入副作用只能通过 `action` 完成。

### 5.3 `action` 必须命中宿主注册动作白名单

- `action` 必须是宿主已注册的动作 ID（例如 `device.status.refresh`、`network.ping`）；
  未注册、拼写不符或类型不符，在安装 / 加载阶段即拒绝，不会渲染成可点击按钮。
- 每个注册动作附带参数 Schema（类型、长度、枚举、取值范围），`params` 按该 Schema 校验，
  校验失败拒绝执行。
- 调用时再做一次权限校验（见第 6 节）：动作所需的权限必须在 `permissions.json` 中声明且已授予。
- 动作返回明确状态：成功 / 失败 / 超时 / 未知，不能只返回「命令已发送」（报告第 10 章）。
- 动作执行与设备交互由宿主完成，插件无法提供可执行文件路径、命令字符串或脚本内容。

`table` 的列定义示例：

```json
{
  "type": "table",
  "label": "接口概览",
  "bind": "network.interfaces",
  "columns": [
    { "title": "接口", "bind": "name" },
    { "title": "状态", "bind": "state" },
    { "title": "地址", "bind": "ip" }
  ]
}
```

## 6. 权限模型：安装时声明 + 调用时校验，默认拒绝

1. **安装时**：宿主读取 `permissions.json`，逐条校验权限 ID 是否在已知权限表内、
   `ui.json.permissions` 与所有 `action` 所需权限是否都已声明；未知权限、缺失声明直接拒绝安装。
2. **调用时**：每次 `bind` 读取或 `action` 执行前，再次校验该插件是否被授予对应权限；
   权限被撤销后立即失效。
3. **默认拒绝**：未声明、未授予、未知、越权的请求一律拒绝并记录审计日志，绝不「先放行再补声明」。
4. **高风险权限**需管理员显式审批，且不得由 `input` / `select` 拼接出任意参数。

### 6.1 权限清单示例与风险等级

| 权限 ID | 说明 | 风险等级 |
| --- | --- | --- |
| `device.status.read` | 读取设备状态 / 健康信息 | 低 |
| `device.info.read` | 读取系统信息（版本、运行时间） | 低 |
| `device.status.refresh` | 触发一次状态重新采集（动作） | 低 |
| `modem.info.read` | 读取 Modem 型号 / 制式 / 运营商 | 低 |
| `network.read` | 读取网络接口 / 路由 / 流量统计 | 低 |
| `sms.read` | 读取短信列表 | 中（隐私数据） |
| `plugin.data.read` / `plugin.data.write` | 读写本插件隔离数据 | 中 |
| `script.run` | 运行已上传且列入 allowlist 的脚本 | 高 |
| `modem.at.write` | 下发 AT 命令 | 高 |
| `sms.send` | 发送短信 | 高（费用与滥用风险） |
| `network.write` | 修改网络 / USB 模式 / 飞行模式 | 高 |
| `device.control` | 重启 / 关机等控制 | 严重 |
| `shell.execute` | 执行 Shell 命令 | 严重（第一阶段不向第三方插件开放） |
| `ota.write` | 触发 OTA 升级 | 严重 |

报告第 08 章要求所有设备写操作默认要求认证与细粒度授权；`shell.execute`、
`device.control`、`ota.write` 在兼容期内仅对可信内置能力开放，第三方插件一律默认拒绝。

### 6.2 permissions.json 示例

```json
{
  "schema_version": 1,
  "plugin_id": "example.status",
  "permissions": [
    {
      "id": "device.status.read",
      "kind": "data",
      "risk": "low",
      "reason": "读取设备名称、调制解调器连接状态、信号、运营商与网络接口概览"
    },
    {
      "id": "device.status.refresh",
      "kind": "action",
      "risk": "low",
      "reason": "点击「刷新」时请求宿主重新采集一次状态快照"
    }
  ]
}
```

`kind` 为 `data`（只读数据）或 `action`（动作）；`risk` 取 `low` / `medium` / `high` / `critical`。
`reason` 供管理员审批与审计展示。

## 7. 数据隔离

- **按插件 ID 隔离**：每个插件的数据根目录为 `data/<plugin_id>/`，
  `plugin_id` 必须匹配第 3 节正则，且不为空、不含路径分隔符。
- **禁止路径穿越**：所有 `assets/` 与数据访问路径在规范化（canonicalize）后，
  必须仍位于该插件目录内；拒绝 `..`、绝对路径、URL 编码绕过、符号链接逃逸。
  拒绝访问其他插件目录、宿主配置、`/etc`、设备节点与系统目录。
- **配额与上限**：单文件大小上限、插件总配额、文件数量上限，以及 `ui.json` 节点数 /
  嵌套深度上限均在宿主侧强制；超限写入直接拒绝，防止磁盘与渲染资源被耗尽。
- **assets 只读**且仅允许静态资源扩展名 / MIME allowlist（如 `png` `svg` `css` `woff2`），
  明确排除 `js` / `html` / 可执行文件，避免脚本落地被执行。
- 插件之间不共享内存状态；需要共享的数据必须由宿主通过受控 API 显式提供。

## 8. 宿主渲染器安全约束（不执行任意 JS / HTML）

- 渲染器只按 `view.type` 创建 DOM 节点（`createElement` + `textContent`），
  **不解析**插件提供的任何 HTML 字符串。
- 禁用 `eval`、`new Function`、`setTimeout(string)`、`innerHTML`、`outerHTML`、
  `insertAdjacentHTML`、`document.write`，不设置 `onclick` 等内联事件属性。
- 所有文本（含 `bind` 取值、`label`、`title`、表格单元格）一律用 `textContent` 赋值，
  天然完成转义，杜绝 XSS 与模板注入。
- 页面设置 CSP：`script-src 'self'; object-src 'none'; base-uri 'none'`，
  不含 `unsafe-eval`、`unsafe-inline`（报告第 08 章安全默认值）。
- `ui.json` 由严格 JSON 解析器读取（不是 JS 对象字面量），按 `schema_version` 做 Schema 校验，
  未知字段 / 类型不符 / 超长 / 超深一律拒绝。
- 若未来确需第三方 JS 插件：必须放入独立 Worker / iframe 沙箱，配合严格 CSP、
  消息协议与权限代理；**不能把 iframe 当成完整安全边界**（报告第 7.3 节）。

## 9. 兼容迁移：第一阶段不破坏插件生态

### 9.1 盘点对象

| 旧文件 | 内容 |
| --- | --- |
| `docs/PLUGIN_GUIDE.md` | `window.PLUGIN` 规范、`$api.shell/toast/alert/confirm/runScript`、HTTP 接口 |
| `docs/插件开发指南2.0.md` | 插件开发文档 2.0 |
| `docs/default_plugin.js` | 「使用文档」内置插件（模板 + `$api` 演示） |
| `docs/example_system_toolkit.js` | 「系统工具箱」示例（监控 / 网络 / 文件 / 脚本 / 自定义 Shell） |

旧运行时能力：`{{ }}` 插值、`@click`、`v-model`、`data()`、`methods`、`mounted()`、
`this.$data`、`this.$refresh()`、`$api.shell()`（任意命令）等。

### 9.2 能力映射（旧 -> 新）

| 旧能力 | 新协议对应 |
| --- | --- |
| `window.PLUGIN` 元信息 | `manifest.json` |
| `template` + `{{ variable }}` | `ui.json` 的 `view` 树 + 只读 `bind` |
| `@click="method"` + `methods` | `button.action` + 宿主注册动作白名单 |
| `v-model` / `input` | `input` / `select` 的 `name` + 动作 `params`（`$input.*`） |
| `this.$data` / `this.$refresh()` | `bind` 数据快照；动作返回后由宿主刷新对应 `bind` |
| `mounted()` 自动加载 | 宿主按 `view` 初始化拉取数据快照，或受控初始动作 |
| `$api.toast` / `alert` / `confirm` | 宿主 UI 动作（低风险），或以按钮结果状态呈现 |
| `$api.shell(cmd)` 任意命令 | **不再提供**；拆为具体最小权限动作（如 `network.ping`），`shell.execute` 单独审批 |
| `$api.runScript(name)` | `script.run` 动作 + 脚本名 allowlist |
| 文件管理 / 自定义 Shell | 拆为受控动作或归入可信内置能力单独评估 |

### 9.3 迁移步骤

1. **盘点**：列出现有插件清单、模板语法、调用过的 `$api`、涉及的 Shell 命令与脚本。
2. **归类**：纯展示类直接转 JSON UI；动作类提取为宿主注册动作并补齐权限声明；
   命令 / 脚本类单独评估。
3. **逐个转换**：先转只读展示插件（例如 `examples/plugins/example.status/`），
   再转带动作插件；每转换一个即补一条宿主动作与权限声明。
4. **可信内置脚本插件单独评估**：`default_plugin.js`、`example_system_toolkit.js` 中
   调用 `$api.shell`、文件管理、自定义命令的部分，可能暂时保留为「可信内置」并经管理员审批 +
   `script.run` / `shell.execute` 高权限，或拆分为若干最小权限动作。
   **不在第一阶段一次性移除**，避免破坏插件生态。
5. **兼容层**：第一阶段宿主可同时读取旧插件清单，仅展示「待迁移」占位并标注不可用旧能力；
   依靠 `min_api_version` 与 `schema_version` 版本化保证后续平滑共存。
6. **强制切换时间点**：待存量插件盘点与迁移清单完成、真机回归通过后，
   再在后续阶段把默认渲染器切换为纯 JSON UI，并保留旧版回退（git 历史 / `server.bak`）。

> 报告第 7.3 节明确：先盘点现有插件 API、模板语法和组件，再逐个转换为 JSON UI；
> 对必须执行脚本的可信内置插件单独评估，**不要在第一阶段一次性破坏插件生态**。

## 10. 完整示例 ui.json

以下为可直接使用的设备状态插件 UI（与 `examples/plugins/example.status/ui.json` 一致），
覆盖 `column` / `row` / `text` / `status` / `badge` / `divider` / `table` / `button`：

```json
{
  "schema_version": 1,
  "id": "example.status",
  "title": "设备状态",
  "permissions": ["device.status.read"],
  "view": {
    "type": "column",
    "children": [
      { "type": "text", "label": "设备名称", "bind": "device.name" },
      { "type": "status", "label": "连接状态", "bind": "modem.connected" },
      { "type": "badge", "label": "网络制式", "bind": "modem.network_type" },
      {
        "type": "row",
        "children": [
          { "type": "text", "label": "信号强度", "bind": "modem.signal_dbm" },
          { "type": "text", "label": "运营商", "bind": "modem.operator" }
        ]
      },
      { "type": "divider" },
      {
        "type": "table",
        "label": "接口概览",
        "bind": "network.interfaces",
        "columns": [
          { "title": "接口", "bind": "name" },
          { "title": "状态", "bind": "state" },
          { "title": "地址", "bind": "ip" }
        ]
      },
      {
        "type": "button",
        "label": "刷新",
        "action": "device.status.refresh"
      }
    ]
  }
}
```

该示例与 `examples/plugins/example.status/ui.json` 完全一致，可直接复制使用。
往节点里塞额外键等未知字段，按第 4 节规则由宿主拒绝加载。

## 11. 配套示例插件

`examples/plugins/example.status/` 提供一个符合本协议、可加载的最小插件：
设备状态展示 + 刷新按钮，包含 `manifest.json`、`ui.json`、`permissions.json` 与 `README.md`。
`assets/` 目录在该示例中为空（仅展示类插件不需要静态资源）。

## 12. 插件运行性能、内存占用与资源开销规格

受限 JSON UI 协议相比旧版动态 Vue 模板 / 动态 JS 执行机制，在嵌入式硬件（UDX710 aarch64 Linux 4.14 / 256MB RAM）上实现了极致的轻量化与资源确定性：

### 12.1 内存占用 (Memory Footprint)

- **前端渲染内存占用**：单插件声明式 DOM 树渲染仅占用约 **20 KB ~ 60 KB** 内存。相比旧版 Vue 3 动态运行时单页面动辄 15 MB ~ 30 MB，内存开销直降 **99.8%**，完美杜绝低内存设备频繁 OOM 闪退。
- **SQLite 持久化配额**：单插件专属 Key-Value 状态存储上限硬编码限制为 **64 KB**，防止插件随意写入大文件撑爆 Flash 闪存分区。
- **后端守护进程驻留（RSS）**：Rust 宿主服务在 aarch64 musl 静态编译架构下，整机运行时常驻内存稳定在 **15 MB ~ 25 MB**，即使多插件同时加载也不会引起内存波动。

### 12.2 CPU 与渲染性能 (CPU & Render Performance)

- **毫秒级 DOM 树构建**：纯原生 HTML5 DOM 声明式构建，100 个嵌套组件节点在 ARM Cortex-A55 核心上生成耗时 **< 3ms**。
- **零 JIT 编译与零 eval 开销**：客户端不解释任意 JavaScript，不运行动态模板编译器，杜绝长时间主线程阻塞与页面掉帧。
- **差异化 TextNode 更新**：数据快照刷新仅替换绑定 TextNode 文本节点，单次遥测局部更新的 CPU 抖动 **< 0.1%**。
- **异步非阻塞 Worker**：所有插件触发的白名单动作（如网络诊断、Modem 状态查询）均由 Tokio 异步任务队列在后台线程执行，绝不阻塞 WebUI 主路由与底层 5G 射频高频巡检。

### 12.3 安全配额与物理边界 (Quotas & Hard Limits)

| 限制项 | 额定上限 | 策略机制 |
| --- | --- | --- |
| 插件压缩包体积 | ≤ 512 KB | 超出在上传解析阶段直接拒绝 (HTTP 413) |
| UI 组件节点总数 | ≤ 128 个 | 超过节点上限拒绝渲染，防止超大 JSON 卡顿 |
| UI 树嵌套深度 | ≤ 8 层 | 深度超过 8 层拒绝加载，规避递归爆栈 |
| 允许文件类型 | 仅限 `.json` / `.png` / `.svg` | 严格禁止任何 `.js`、`.sh`、ELF 二进制与符号链接 |
| 数据库存储配额 | ≤ 64 KB / 插件 | 写入超过配额返回拒绝错误 |

