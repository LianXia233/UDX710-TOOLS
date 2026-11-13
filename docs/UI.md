# WebUI 设计说明（3.1.0）

前端为 **零依赖原生实现**：HTML5 + CSS3 + JavaScript ES Modules，无 npm、无构建步骤、无运行时模板编译。静态资源由 `rust-embed` 编译期内嵌进单二进制。

## 1. 视觉主题：白色毛玻璃（Glassmorphism）

| 层 | 文件 | 职责 |
| --- | --- | --- |
| 设计变量 / 重置 | `web/css/tokens.css` | 白色半透明玻璃调色板、`--glass-blur`、`--glass-shadow`、圆角、间距；亮/暗/系统偏好三套变量 |
| 布局 | `web/css/layout.css` | 顶部悬浮玻璃岛主导航（Header Dock）、无侧边栏设计、移动端底部导航（Bottom Nav）、12 列网格、功能全景中心 |
| 组件 | `web/css/components.css` | 卡片、折叠卡片（`<details>` 结合）、按钮、输入、徽标、表格、Toast、原生 `<dialog>` 基础样式 |
| 玻璃质感 | `web/css/glass.css` | `backdrop-filter: blur() saturate()`、白边、内高光、渐变光晕背景、看门狗提示条 |
| 弹窗形态 | `web/css/sheets.css` | 三种弹窗的定位、尺寸、把手、进退场动画 |
| 响应式 | `web/css/responsive.css` | 全分辨率断点、安全区、`100dvh`、触屏点击区 |

加载顺序（见 `web/index.html`）：`tokens → layout → components → glass → sheets → responsive`。`responsive.css` 必须最后加载，用于覆盖各断点下的尺寸与安全区。

- 深色模式保留同样的玻璃质感（变量切换，不换布局）；`prefers-reduced-motion` 下禁用所有动画与 `scroll-behavior`。
- **严格 CSP 不变**：`style-src 'self'`，样式全部来自同源 stylesheet；JS 只通过 CSSOM（`element.style`）设置 CSS 变量，不注入 `<style>` 标签。
- 交互态（hover 抬升、激活渐变）在触屏设备上自动关闭，避免误触。

## 2. 动态 SVG 与遥测可视化组件

| 文件 | 用途 | 动画方式 |
| --- | --- | --- |
| `web/icons/aurora.svg` | 全屏背景光晕，`body::before` 以 `background-image` 引入 | SMIL（`animate` / `animateTransform`）漂浮 |
| `web/icons/brand-live.svg` | 顶部玻璃岛品牌标识 + favicon | SMIL 双向差速旋转刻度环 / 呼吸光晕 / 3级射频波束 |
| `web/icons/login-hero.svg` | 登录卡片 3D 终端矢量视觉 | SMIL 旋转雷达环 / OLED 状态屏 / 动态波纹 |
| `web/icons/pulse-wave.svg` | 仪表盘脉冲波形装饰 | SMIL 路径位移 |
| `web/js/components/svg-gauges.js` | 核心动态遥测矢量引擎（`createHeroRadar`、`createCircularGauge`、`createSmoothSparkline`、`createTrafficStreamPipe`） | 原生 DOM SVG 命名空间 API，高帧率动态补间 |
| `web/icons/sprite.svg` | 70 个 `<symbol>` 图标，通过 `<use href>` 引用 | 静态，颜色由 `currentColor` 继承 |

均为同源静态资源或纯原生 DOM 生成，严格符合 `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:`；不依赖 JS 定时器动画循环、不引入外部 CDN 或构建工具。卡片标题自动清洗后端接口端点（`sanitizeSubtitle`），冗长参数与帮助说明支持使用 `collapsibleCard` 优雅折叠。

## 3. 三种弹窗形态

统一由 `web/js/components/dialog-variant.js` 解析，`modal.js`（通用）与 `confirm-dialog.js`（确认）共用。

| 形态 | variant | 典型场景 | 表现 |
| --- | --- | --- | --- |
| **Centered Modal Dialog** | `center` | 桌面表单、通用设置 | 视口居中，宽度 `min(--dlg-width, 560px)`，缩放淡入 |
| **Modal Bottom Sheet** | `bottom-sheet` | 手机默认形态 | 贴底全宽、顶部圆角、拖拽把手，点击或下滑 >110px 关闭，从底部滑入，底部留 `safe-area-inset-bottom` |
| **Right Side Sheet** | `side-sheet` | 日志、JSON 详情、长表单/长文本 | 贴右全高 `100dvh`，宽度 `min(--dlg-width, 520px)`，从右侧滑入，内容区独立滚动 |

**自动选择规则（`auto`，默认）**

1. 视口宽度 ≤ 760px → `bottom-sheet`；否则 `center`。
2. `side-sheet` 在窄屏自动退化为 `bottom-sheet`，避免内容被压成一条。
3. 「设置 → 外观与语言 → 弹窗样式」可强制切换并即时预览；偏好存 `sessionStorage`（key `udx.dialog.variant`），关闭标签页恢复自动。
4. 优先级：页面显式 `variant` > 用户偏好 > `auto` 规则。

**实际接入位置**

- `side-sheet`：插件 UI 预览 / 插件存储 / 脚本编辑、Rathole 运行日志 / 服务端配置片段、IPv6 推送日志、网络接口统计
- `auto`（手机 Bottom Sheet / 桌面 Centered）：APN 模板、IPv6 规则、Rathole 服务、插件上传、退出登录确认、所有 `dangerConfirm`
- `center`：桌面下所有 `auto` 弹窗自动解析

**交互与无障碍**

- ESC、点击遮罩、关闭按钮、Bottom Sheet 把手统一走 `dismissDialog()` 的退场动画路径，避免原生 `<dialog>` 直接关闭导致 DOM 残留。
- `dismissible: false` 时拦截 ESC 与遮罩点击，只能由按钮决策（用于极高风险确认）。
- `requireText` 高风险确认在输入指定短语前禁用确认按钮。
- 弹窗打开后焦点落在关闭按钮 / 主操作按钮；文本一律 `textContent`，不注入 HTML。

## 4. 全分辨率响应式

`web/css/responsive.css` 断点一览：

| 断点 | 目标设备 | 关键处理 |
| --- | --- | --- |
| ≤ 359px | 320px 超窄屏 / 折叠屏外屏 | 间距与圆角收紧、双列指标卡、按钮组拉伸、Toast 通栏 |
| ≤ 600px | 手机（360 / 390 / 414） | 侧边栏变抽屉、顶栏精简、表单单列、输入框 16px 防 iOS 缩放、表格横向滚动、Bottom Sheet 全宽 |
| 601–760px | 大屏手机 / 小平板 | 单列网格、适中间距、居中弹窗留边 |
| 761–1024px | 平板 | 侧边栏收窄至 208px、12 列网格降为 6 列语义、侧边抽屉宽度收敛 |
| 1025–1439px | 桌面 | 标准 12 列、侧边抽屉 `46vw` 上限 |
| ≥ 1440px | 大屏 | 内容居中限宽 1560px、字号与图表放大 |
| ≥ 1920px | 超宽屏 | 限宽 1720px、指标列加宽 |
| 矮视口横屏 | 手机横屏 / 车载 | ≤480px 高时压缩侧栏与顶栏、页面留白收紧、登录层顶部对齐 |

通用规则：

- `100dvh` 替代 `100vh`，避免移动端地址栏收起导致高度跳变（保留 `100vh` 回退）。
- `env(safe-area-inset-*)` 覆盖顶栏、侧栏底部、页面底部、Toast、看门狗提示条、Bottom Sheet 底部与横屏左右刘海；`index.html` 已设 `viewport-fit=cover`。
- 触屏 `44px` 最小点击区（导航项、按钮、图标按钮、关闭按钮、抽屉把手）；`pointer: coarse` 下禁用 hover 位移。
- 表格 `overflow-x: auto` + `overscroll-behavior-x: contain`，长 JSON / 终端输出用 `overflow-wrap: anywhere` 防横向溢出。
- 字号用 `clamp()` 平滑缩放；`prefers-reduced-motion` 全局降级。

## 5. 开发约定

- 新增页面弹窗优先 `variant: 'auto'`；长内容 / 日志详情用 `side-sheet`。
- 需要指定宽度时传 `width`（写入 `--dlg-width`，不要写内联 `max-width` 覆盖定位规则）。
- 所有 DOM 通过 `utils/dom.js` 的 `el()` / `button()` 构建，禁止 `innerHTML`；插件 UI 由 `core/plugin-renderer.js` 宿主渲染，仅解释白名单类型。
- 新增动画优先 CSS keyframes 或 SMIL，不引入 JS 动画循环。
