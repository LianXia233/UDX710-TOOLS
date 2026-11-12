# example.status — 设备状态示例插件

符合 `docs/PLUGIN_PROTOCOL.md` v1 的最小可用插件：展示设备 / 调制解调器状态，
并提供一个「刷新」按钮。只读、无脚本、无 Shell 访问，适合作为旧插件迁移时
第一个转换对象（报告第 07 章「兼容迁移」）。

## 目录结构

```
example.status/
├── manifest.json       # 声明：id / name / version / min_api_version
├── ui.json             # 受限 JSON UI（view 树）
├── permissions.json    # 权限与动作声明
└── README.md
```

`assets/` 在本示例中不需要（纯展示插件），按协议可省略或保留空目录。
目录名必须与 `manifest.json` 的 `id` 一致。

## 功能

- 设备名称（`text` / `device.name`）
- 连接状态（`status` / `modem.connected`）
- 网络制式（`badge` / `modem.network_type`）
- 信号强度、运营商（`row` 内两个 `text`）
- 接口概览（`table` / `network.interfaces`，列：`name` / `state` / `ip`）
- 分隔线（`divider`）
- 「刷新」按钮（`button` 触发 `device.status.refresh`）

## 权限

`permissions.json` 声明两项，均为低风险：

| ID | kind | 用途 |
| --- | --- | --- |
| `device.status.read` | data | 读取状态数据快照 |
| `device.status.refresh` | action | 点击刷新时重新采集一次状态 |

`ui.json.permissions` 只列 `device.status.read`，是上述声明的子集；
`device.status.refresh` 必须命中宿主注册动作白名单。安装时声明 + 调用时校验，
任何未声明权限默认拒绝。详见 `docs/PLUGIN_PROTOCOL.md` 第 6 节。

## 宿主需要提供的契约

- 注册动作白名单：`device.status.refresh`（无参数或仅受控参数，返回明确
  成功 / 失败 / 超时 / 未知状态）。
- 按 `example.status` 隔离的只读数据快照：`device.name`、`modem.connected`、
  `modem.network_type`、`modem.signal_dbm`、`modem.operator`、
  `network.interfaces[]`（每项含 `name` / `state` / `ip`）。
- 宿主插件协议版本需 >= `min_api_version`（此处为 1）。

## 安装与验证

1. 把整个 `example.status/` 目录放到宿主插件目录，保持目录名与 `manifest.json.id` 一致。
2. 宿主安装时校验：严格 JSON + Schema、`id` 一致、权限已声明、动作在白名单内、
   `ui.json.permissions` 是 `permissions.json` 的子集、无路径穿越、无禁用文件类型。
3. 打开插件页面应看到状态卡片与「刷新」按钮；点击刷新触发一次状态重新采集。
4. 渲染安全：所有文本经 `textContent` 赋值，未执行任何插件 JS / HTML。
   把某个 `label` 改成 `<img onerror=...>` 应被原样转义显示，不会执行。
5. 默认拒绝：删掉 `permissions.json` 中的 `device.status.read` 后，
   插件应被拒绝加载或状态字段为空，不允许「先放行再补声明」。

## 从旧版迁移的对照

旧版 `docs/PLUGIN_GUIDE.md`、`docs/default_plugin.js`、`docs/example_system_toolkit.js`
用 `window.PLUGIN` + `{{ }}` 模板 + `this.$api.shell()` 实现同类页面。本插件对应关系：

| 旧能力 | 本插件 |
| --- | --- |
| `window.PLUGIN` 元信息 | `manifest.json` |
| `template` + `{{ variable }}` | `ui.json` 的 `view` 树 + 只读 `bind` |
| `@click="refresh"` + `methods` | `button.action` = `device.status.refresh` |
| `this.$data` / `this.$refresh()` | 动作返回后由宿主刷新对应 `bind` 数据快照 |
| `this.$api.shell('...')` | 不再使用，由宿主 `device.status.refresh` 动作封装，无任意命令 |

完整映射与「第一阶段不破坏插件生态」的迁移步骤见 `docs/PLUGIN_PROTOCOL.md` 第 9 节。
