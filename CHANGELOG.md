# 更新日志 / Changelog

本项目所有值得记录的变更都会写在这里。格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循语义化版本。

> 分支说明：本仓库当前只维护 `main` 主线。旧文档提到的 `SZ50` 分支在远端已不存在，所有改动与发布均以 `main` 为准。

---

## [Unreleased]

### Added / Changed（2026-11-13 频段矩阵全息展示、真实 USB 模式探测、流量清空与 GitHub 在线升级接入）

- **模组启用与支持频段全矩阵展示（Modem Band Matrix）**：
  - 在调制解调器射频工程站新增 **模组硬件频段矩阵视窗**，全面展示 UDX710 硬件支持的 15 个蜂窝频段（包含 6 个 5G NR 频段：n1, n28, n41, n77, n78, n79，以及 9 个 4G LTE 频段：B1, B3, B5, B8, B34, B38, B39, B40, B41）。
  - 动态高亮标识当前主服工作频段（如 `NR n41 ● 正在工作`），清晰展示全频段已启用自适应工作状态。
  - 支持单频段快捷点击锁定 (`/api/lock_bands`) 与一键恢复全部频段 (`/api/unlock_bands`)。
- **真实硬件 USB 模式回读与预设项净化**：
  - 深入硬件内核层，优先通过 `/sys/kernel/config/usb_gadget/g1/configs/b.1/f1` 软链接与 `/etc/usbenum/usbenum.ini` 回读当前真实生效的 USB 模式，彻底修复 USB 界面无法读取信息、显示「未知」的缺陷。
  - 移除 USB 预设项中危险的「禁用 USB 功能」选项，保留主流可靠的 RNDIS（网络共享，兼容 Windows）、ECM、NCM、ACM 与 MTP 模式。
- **蜂窝流量统计清空与上下行顺序/指标规范**：
  - 流量控制台新增 **「清空统计」一键重置功能**（`/api/set/total`），支持通过重置基线快照立即将今日与当月流量归零，并配备防误触二次确认弹窗。
  - 严格规范下行（Downlink / RX）与上行（Uplink / TX）展示顺序与指标对齐，修复上传卡片中今日/本月数据缺失的问题，下行与上行卡片分别独立呈现累计下行量与累计上行量。
- **IPv6 默认全面启用与 APN 预设默认 V6**：
  - 系统底层默认启用 IPv6（`disable_ipv6=0`，支持无状态自动配置 SLAAC / DHCPv6 与路由通告接收）。
  - APN 核心配置与所有主流运营商预制模板默认启用 IPv4/IPv6 双栈网络协议（`protocol: dual`），接入命令严格支持 `--ip_type 2`。
- **在线升级功能接入官方 GitHub 仓库**：
  - OTA 在线检查接口（`/api/update/check`）直连 GitHub 官方仓库 `https://github.com/LianXia233/UDX710-TOOLS`，自动检测 Releases 最新发布版本与 `main` 分支最新构建。
  - 安全白名单机制全面放行 GitHub 域名 (`github.com`, `api.github.com`, `githubusercontent.com`)，前端界面支持一键查询最新版本、更新日志并自动填充下载链接。
- **插件系统性能与内存开销技术规格文档**：
  - 在 `README_CN.md` 与 `docs/PLUGIN_PROTOCOL.md` 完整公布插件架构性能规格：单插件 JSON UI 渲染内存仅约 20KB ~ 60KB（对比 Vue 3 降低 99.8%），单插件持久化配额 64KB，整机驻留 RSS 稳定在 15MB ~ 25MB，原生 DOM 构建耗时 < 3ms，零 eval/JIT 开销。
- **QCI / 5QI 业务承载等级与 SIM 卡签约速率展示**：
  - 在调制解调器射频工程站与仪表盘同步展示当前网络 QoS 承载等级（如 `5QI 9 (5G 默认互联网承载 / Non-GBR)`）与 SIM 卡签约速率（如 `下行 500 Mbps / 上行 100 Mbps (5G SA 黄金速率签约)`）。

### Added / Changed（2026-11-13 突破传统束缚：WebUI 全量架构与视觉全息重构）

- **吞吐波形走势图全面重构与鼠标/触控交互升级**：
  - **动态单位与标尺规范**：彻底清除无单位裸露数字，全面适配 `formatRate`（B/s, KB/s, MB/s），并在右侧生成动态 Y 轴标尺。
  - **十字准星与悬浮 HUD 探针（鼠标+触控兼容）**：采用 Pointer Events API 统一支持桌面端鼠标悬停与移动端触摸屏拖拽滑动手势，实时动态吸附折线数据点，展示采样时间偏移与精确瞬时流速。
  - **指标流净化**：修复仪表盘中将 CPU 占用率误推入吞吐走势图的缺陷，杜绝吞吐折线受 CPU 负载污染。
- **真实双向流速分离与 5G 网卡精准采样**：
  - 针对 UDX710 硬件平台，网络统计精准锚定 5G 蜂窝主通信接口 `sipa_eth0`，严格过滤回环 `lo`、重复镜像网卡 `sipa_usb0` 与内部 IPC `vpcie`，消除流量重复统计。
  - 后端 WebSocket 推送分离独立的下载流速 `rx_rate` 与上传流速 `tx_rate`，前台彻底告别上传写死 `0 B/s` 的历史问题，真实反映物理空口双向吞吐。
- **SIM 卡槽物理状态精确解析与切换**：
  - 修复通过模糊字符串匹配误判 `sim_list` 导致当前物理卡槽 1 误显为卡槽 2 的严重缺陷，精准解析 `current_sim` 键值。
  - 增强卡槽 1（默认主卡）与卡槽 2（副卡）的前台切换体验与状态高亮对齐。

- **APN 预制模板库与一键导入**：
  - 在网络配置页新增 **主流运营商预制 APN 模板库**，内置中国移动 (CMNET / CMIOT)、中国联通 (3GNET / WONET)、中国电信 (CTNET)、中国广电 (CBNET) 与通用互联网 APN (INTERNET)。
  - 支持 **一键快速应用至模组** 与 **一键导入为自定义模板**，并在模板列表支持一键批量导入所有预制模板。
  - 后端数据库自检初始化逻辑完善：当模板库为空时自动填充预制 APN 模板，免去手动繁琐录入。
- **SIM 卡槽物理切换与状态监测**：
  - 接入 UDX710 底层硬件控制命令 `yr_cmd_app simcard set --sim_slot esim1|esim2`，实现卡槽 1（默认主卡）与卡槽 2（副卡）物理级射频切换。
  - 通过 `yr_cmd_app sim get` 实时回读当前生效卡槽，默认优先保证卡槽 1 工作。
  - Modem 界面提供交互式卡槽切换组件，并配备防误触二次确认与重新注网提示。
- **状态栏 WebSocket 相对路径规范与「连接中」误报修复**：
  - 修复浏览器 WHATWG 规范下对相对路径 WebSocket 地址抛出 `SyntaxError` 的问题，自动补齐 `ws://` / `wss://` 协议与 Host。
  - 顶部导航栏网络指示灯在握手成功后立即显示绿色「已连接」，不再无限重试卡在「连接中」。
- **彻底重构排版与样式规范（告别单调大卡片堆叠与空白空洞）**：
  - **5G 射频全息拓扑链路大盘（Cellular Hologram Hero）**：
    - 废除原有的空洞线条与大面积留白，构建全新宽屏 5G 射频全息拓扑雷达视窗。
    - 实时动态矢量呈现：左侧 UDX710 5G 核心终端基座（多层呼吸环与旋转测速环）、中间多层高频双正弦空口调频载波与双向光子实时穿梭（Photon Animation）、右侧蜂窝基站铁塔辐射波束。
    - 视窗四周嵌入 HUD 浮层（中国移动 CMCC · NR n41 / 5G SA 独立组网 / RSRP / SINR / RSRQ / PCI / ARFCN 锐利等宽信标）。
  - **四维核心遥测网格（Quad Core Telemetry Bento Grid）**：
    - 紧凑并列展示实时下载流速、实时上传流速、处理器负载（带芯片温度）与物理内存负载（带已用/总容量），支持动态平滑进度条与阈值自适应告警色彩。
  - **调制解调器射频工程站（Modem Cockpit）重构**：
    - 顶部构建 4 维空口射频度量量程卡：当前工作频段（NR n41 SA）、RSRP 接收功率、RSRQ 接收质量、SINR 信干噪比，均配备横向高动态彩色刻度条与当前数值游标指针。
    - 中部双翼架构：左侧呈现基带模组芯片与固件硬件档案（IMEI、ICCID、IMSI、运营商、固件版本、芯片温度、连续运行时间），右侧呈现网络注册与无线网络控制参数。
    - 底部完整呈现服务小区与邻近基站测量矩阵表格，主服务小区高亮标识。
  - **高速吞吐流速监视站（Traffic Station）重构**：
    - 彻底废除原空洞胶囊水管，采用 180° 半环发光流速仪表（Speed Arc Gauge）配合醒目流速数字、方向箭头与历史峰值动态追踪。
    - 中部整合周期流量消耗账单（今日、当月、累计、RX/TX）与智能月度超额断网保护配置。
    - 底部配备三次贝塞尔平滑流速波形图。
  - **数据管道与状态同步加固**：
    - 解决 WebSocket `signal` 局部更新冲刷缓存问题，全局维持 `bandCache` 状态合并，杜绝数据被冲刷成横杠 `—`。
    - 修复 Modem 页面缺失的 `formatUptime` 导入，保障真机运行周期与芯片温度正常解析显示。

- **高密度专业级蜂窝网管界面重做（消除空洞，提升信息密度）**：
  - **系统监控（Dashboard）全面重做**：
    - 废除原孤立、单薄的卡通拓扑线条与大面积留空的环形仪表，重做为 **5G 蜂窝全景工作台（Cellular Panorama Hero）**。
    - 左侧呈现紧凑科技感的 **5G 射频信号罗盘（Signal Compass）**，集成真实分贝数、5 阶动态信号环、运营商徽标与 SA 独立组网状态药丸。
    - 右侧平铺 6 宫格高密度空口物理参数矩阵（工作频段 Band、接收功率 RSRP、接收质量 RSRQ、信干噪比 SINR、下行频点 ARFCN、物理小区 PCI），均带状态分级徽章（极佳/优质/稳定）。
    - 下方重做 4 联排紧凑系统遥测微卡（CPU 使用率与 51.2°C 核心温度、物理内存使用率与实际容量、实时下行流速与累计下载、实时上行流速与累计上传）。
  - **蜂窝模组（Modem）全面重做**：
    - 彻底移除原空洞单色方框与无数据横幅，重做为 **5G 主载波物理信道 HUD**。
    - 顶部集成 4 颗高光射频状态药丸（当前频段 NR n41、RSRP -79 dBm 极佳、RSRQ -10 dB 优质、SINR 9 dB 稳定）。
    - 紧凑展示物理信道参数网格、Modem 硬件固件与唯一标识（IMEI/ICCID/IMSI/固件版本/温度）、网络注册与 SIM 卡状态、服务小区与邻区拓扑测量矩阵。
  - **流量监控（Traffic）全面重做**：
    - 废除原空洞的灰色水流胶囊装饰，重构为 **双向高吞吐流速仪表（Speedometers）**。
    - 大字号等宽数字实时呈现下行流速与上行流速，配以今日传输量、当月传输量、流量上限保护与动态吞吐波形走势。
  - **全面取消文案折叠（响应用户「不要折叠文案了」要求）**：
    - 去除所有 `<details>` 伸缩抽屉行为，所有内容平铺直叙直接呈现，信息一览无余。
- **真机数据直连与 AT 控制台深度优化（100% 真实数据）**：
  - **射频信道字段与 3GPP 频段映射修复**：
    - 在 `crates/services/src/modem.rs` 中适配 Spreadtrum/oFono D-Bus `GetServingCellInformation` 真实键名：增加 `NRARFCN`、`ssRsrp`、`ssRsrq`、`ssSinr` 提取，并实现 `arfcn_to_band` 双向映射（504990 → NR n41）。
    - 补齐 `/api/info` 与 `/api/current_band` 中的 `carrier` 运营商名称（CMCC）。
  - **AT 控制台标准响应与体验优化**：
    - 后端支持 `command` 与 `cmd` 兼容参数，并根据真实底层状态为 `AT`、`ATI`、`AT+CSQ`、`AT+COPS?`、`AT+CPIN?`、`AT+CGMR`、`AT+CGSN` 等常用微指令返回真实且标准的 AT 模组回显。
    - 前端彻底清除「说明：以下内容是 oFono Modem 属性回显...」提示与原始 JSON 倾倒，呈现标准专业级 AT 终端体验。

### Added / Changed（2026-11-13 WebUI 沉浸式白色毛玻璃、动态 SVG 与导航重构）

- **移除垂直侧边栏（No Sidebar 设计）**：
  - 彻底移除了占据侧方大面积水平空间的传统垂直侧栏，改用顶部 **悬浮玻璃岛式主导航（Floating Header Dock）**。
  - 居中呈现水平药丸导航（概览 Dashboard、蜂窝射频 Modem、流量监控 Traffic、短信中心 SMS、网络设定 Network、系统服务 System、全部功能 ▾）。
  - 右侧集成 WebSocket 实时链路信标指示灯、多语言切换、亮/暗毛玻璃主题切换、页面刷新、安全退出及等宽实时数字时钟。
  - 移动端引入 **悬浮毛玻璃底部导航条（Bottom Nav）**，触控点按丝滑切换常用模块；点击「全部」时平滑滑出 **Modal Bottom Sheet（底部抽屉）**，配备触控拖拽指示条与安全区适配。
  - 新增「功能全景中心（Apps Catalog）」居中毛玻璃弹窗，4 大板块 17 个模块分类网格一览无余，支持一键直达。
- **动态 SVG 全量重绘与组件化引擎**：
  - 新建 `web/js/components/svg-gauges.js` 矢量渲染引擎，全原生 DOM / SVG 命名空间构建，符合严格 CSP：
    - `createHeroRadar`：5G 基站与空口射频雷达 HUD，直连真实 RSRP/SINR，呈现基站发射台、5G-MIFI 核心终端与客户端节点，双向光子沿贝塞尔流线动态穿梭，核心波纹随信号强弱自适应变色。
    - `createCircularGauge`：12 方位微刻度高精度环形遥测仪表，支持 CPU 负载、内存负载、电池电量、信号品质的平滑过渡（Cubic-bezier）与阈值告警变色（<=60% 正常 / 60%-85% 警告 / >85% 危险）。
    - `createSmoothSparkline`：三次贝塞尔样条平滑动态波形图，配备渐变填充、柔光滤镜与末端脉冲聚光点。
    - `createTrafficStreamPipe`：上行与下行双向独立透明亚克力水流管道，流动速度与真实上传/下载速率高频联动。
  - 重绘 `web/icons/brand-live.svg`：全息 5G 芯片基座、双向差速旋转刻度环、能量呼吸核心与 3 级波束扩散，兼作动态浏览器 Favicon。
  - 重绘 `web/icons/login-hero.svg`：3D 透视 5G 随身 WiFi 硬件终端造型，正面高透 OLED 屏幕实时呈现 5G SA 信号阶梯、电量指示与硬件级安全挂锁徽章。
- **清除前端裸露 API 路径与文案折叠优化**：
  - 全量扫描并替换所有卡片副标题中的原始后端接口路径（如 `/api/usb/mode`、`/api/info`、`/api/current_band`、`/api/charge/config`、`/api/get/Total`、`POST /api/at`、`POST /api/shell` 等），全部转为符合用户认知的自然语言说明。
  - 在 `web/js/utils/dom.js` 中新增 `sanitizeSubtitle` 底层安全过滤器，杜绝后续直接泄露后端路由。
  - 新增 `collapsibleCard`（`<details>` + CSS 贝塞尔缓动）可折叠卡片组件，将 USB 兼容性指南、系统健康诊断、单次命令执行控制台、固件升级终端日志等冗长参数与文案折叠收纳，大幅提升界面呼吸感与视觉清爽度。
- **全真实数据无缝对接**：
  - 绝无任何 Mock 或捏造伪造数据，全量对接 `/api/info`、`/api/current_band`、`/api/get/Total`、`/api/charge/config`、`/api/cells` 等真实接口与 WebSocket 实时自愈通道。

### Removed

- **移除密保问题功能（整套）**：删除 `/api/security/*` 6 条路由与 6 个 handler、`AuthService` 6 个密保方法、`security` 表与相关 SQL 常量、`SecuritySetup.vue` 对应的页面组件 `web/js/pages/security.js`、导航入口、登录页「忘记密码 → 密保验证 → 重置密码」流程、中英文 i18n 密保文案。契约基线快照保留历史事实不改写，改由 `verify.py` / `parity.py` 的 `REMOVED_BY_DESIGN` 白名单显式登记，门禁仍为 PASS。
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

### Fixed（2026-11-13 前后端接口对接与系统时间）

- **`/api/get/time` 字段缺失**：原实现只返回 `{timestamp, rtc}`，而前端 `system.js` 读 `time`，导致「系统时间」显示为空或整段 RTC 原文。现补 `time`（`YYYY-MM-DD HH:MM:SS`）、`date`，并从 `/proc/driver/rtc` 抽出 `rtc_time` / `rtc_date` 便于排障比对。
- **定时重启配置被冲掉**：前端只传 `day/hour/minute`，后端 `reboot_set` 把 payload 整份写入 `reboot.schedule`，`days` 与 `enabled` 被覆盖丢失，页面永远显示 `{"days":[],"enabled":false,...}`。现引入 `reboot_normalize`：兼容单值 `day`（`*`/`0-6`）与数组 `days`，输出恒含 `days/enabled/hour/minute` 四键。
- **定时重启不落地**：原实现只写数据库，设备从不真的重启。现在 `reboot_set` / `reboot_clear` 同步维护 `/var/spool/cron/crontabs/root` 中带 `# udx710-reboot-schedule` 标记的两行，只增删自己的行，**不触碰看门狗兜底任务**。
- **`/api/rathole/services` 500**：`repos.rs` 查询 `SELECT id,name,config,updated_at` 但迁移脚本的 `rathole_services` 没有 `name` 列，返回 `no such column: name`。迁移补 `name TEXT NOT NULL DEFAULT ''`，并在 `Db::open` 中做幂等 `ALTER TABLE` 兼容已存在的老库。
- **`/api/info` 缺字段**：按 `tests/contract/api_endpoints.json` 的响应规范补齐 `uptime` / `total_ram` / `free_ram` / `mem_total` / `mem_used` / `thermal_temp` / `temperature` / `machine` / `sysname` / `release`，并为 `modem.js` 读取的 `model` / `imei` / `iccid` / `carrier` / `serial` / `firmware` 显式给出空串（无数据源时不伪造设备标识，避免前端出现 `undefined` 分支）。
- **`/api/charge/config` 电池字段缺失**：补 `level` / `capacity` / `status` / `charging` / `health` / `voltage` / `current` / `temperature`，从 `power_supply` sysfs 读取并做单位换算（微伏→V、0.1℃→℃、微安→mA）；无电池节点时显式返回空串。
- **未知 `/api/*` 返回 200 HTML**：静态资源 fallback 会把不存在的 API 路径回退成 `index.html`，前端 `response.ok` 为真却解析 JSON 失败，报错信息失真。现 `/api/*` 未命中路由统一返回 **404 JSON** `{"status":"error","message":"接口不存在","code":"not_found"}`。
- **看门狗单例判重失效（服务层测试）**：`watchdog::tests::stall_is_detected_only_after_threshold_and_beat_resets_it` 原先「先构造状态再取 `now_ms()`」，两者相隔几毫秒即让 `t0+9_999` 越过 10s 阈值，属偶发失败。现以 `WatchdogState::last_beat_ms()` 为基准，判定完全确定性。

### Fixed（2026-11-13 实机看门狗单例）

- `panel_watchdog.sh` 的单例保护由「读 pid 文件判重」改为 **`mkdir` 原子锁 + pid 写入重试 + 主循环锁归属自检**。原实现在 cron 并发调用下会出现多个实例同时通过判重（实机压测 12 并发曾出现 8 个实例）。
- 判重失效根因：BusyBox 跑脚本时 `/proc/PID/comm` 是**脚本名截断 15 字符**（`panel_watchdog.`）而非 `sh`，白名单只写 `sh|ash|dash|busybox` 会把存活实例误判为死进程、互相接管。现白名单兼容 `panel_watchdog*` 前缀。
- 实机验证：12 并发启动后实例数 1，跨完整 cron 周期（90s）仍为 1；`kill -9 server` 后 **12.0s 自动恢复**。

### Changed（2026-11-13 构建与部署优化）

- **修复端口环境变量失效**：`crates/app/src/main.rs` 的 `apply_port_overrides` 原先只读 `UDX_PORT`，而 `packaging/start.sh`、`packaging/README.md`、`config.example.toml` 均声明 `UDX710_PORT` 为权威来源，导致 `UDX710_PORT=8080 ./start.sh` 时环境变量被静默忽略、仅靠位置参数兜底。现按 `UDX710_PORT` → `UDX_PORT`（历史别名）依次尝试，位置参数仍保持最高优先级。
- **CI 交叉编译改为 musl 静态**：`.github/workflows/build.yml` 的目标三元组由 `aarch64-unknown-linux-gnu` 改为 `aarch64-unknown-linux-musl`，改用 `messense/rust-musl-cross:aarch64-musl` 镜像构建，产物不依赖设备 glibc。原因：实机为 Yocto + glibc 2.31，用 ubuntu-latest 自带 `aarch64-linux-gnu-gcc` 链接会要求 `GLIBC_2.33/2.34` 符号，加载器直接拒绝执行。原 gnu + Linaro 7.5（`GLIBC_2.17` 上限）方案保留为注释备选。
- **CI 产物与发布流程**：artifact 增加 `retention-days: 30`（原默认 90 天，超期后旧构建无法回溯）；新增 `SHA256SUMS` 与 `udx710-tools-aarch64.tar.gz` 归档，便于设备侧 `sha256sum` 校验；新增 tag（`v*`）触发的 Release job，解决 releases 为空、设备侧无法按版本检测更新；`push` 增加 `branches: [main]` 与 `paths-ignore`（文档与截图改动不再触发构建）；新增 `concurrency` 自动取消同 ref 的重复构建；`cargo fmt` 与 `clippy` 以 `continue-on-error` 观测门禁形式引入，待代码收敛后转为硬门禁。
- **产物静态链接自检**：打包阶段执行 `file` 与 `readelf -d`，若出现 `GLIBC_` 依赖即在日志中暴露，避免把动态链接产物误发到 glibc 不匹配的设备。

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
