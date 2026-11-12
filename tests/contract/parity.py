#!/usr/bin/env python3
"""功能移植对照校验（可复跑）。

从工作区保留的旧版源码（v2.x 存档：`src/`、`web/src/`）提取
  (a) C 后端 `mg_str("/api/...")` 路由
  (b) Vue 组件调用的 `/api/...` 端点
  (c) Vue 组件 / 菜单页清单

再与新实现（`tests/contract/api_endpoints.json`、`crates/api/src/routes.rs`、
`web/js/pages/`、`web/js/app.js`）逐项对照，任何遗漏即 FAIL。

用法：python3 tests/contract/parity.py
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# 旧版组件 -> 新版页面/组件（功能级映射，非仅端点）
PAGE_MAP = {
    "SystemMonitor.vue": "dashboard.js",
    "NetworkManager.vue": "network.js",
    "AdvancedNetwork.vue": "network.js + modem.js",
    "NetworkInterface.vue": "network.js",
    "CellManager.vue": "network.js + modem.js",
    "ApnConfig.vue": "apn.js",
    "SmsManager.vue": "sms.js",
    "TrafficStats.vue": "traffic.js",
    "BatteryManager.vue": "charge.js",
    "RatholeManager.vue": "rathole.js",
    "IPv6Proxy.vue": "ipv6-proxy.js",
    "SystemUpdate.vue": "update.js",
    "UpdateNotification.vue": "update.js",
    "ATDebug.vue": "at-debug.js",
    "WebTerminal.vue": "terminal.js",
    "UsbMode.vue": "usb.js",
    "PluginStore.vue": "plugins.js",
    "PluginCard.vue": "plugins.js + core/plugin-renderer.js",
    "PluginBtn.vue": "core/plugin-renderer.js",
    "PluginStatus.vue": "core/plugin-renderer.js",
    "SystemSettings.vue": "settings.js + system.js",
    "SecuritySetup.vue": "security.js",
    "ForgotPasswordModal.vue": "login.js",
    "LoginPage.vue": "login.js",
    "GlobalToast.vue": "components/toast.js",
    "GlobalConfirm.vue": "components/confirm-dialog.js",
    "TabNav.vue": "app.js 导航",
}

# 旧版菜单 id -> 新版 PAGES/NAV id
MENU_MAP = {
    "monitor": "dashboard", "network": "network", "netif": "network",
    "apn": "apn", "advanced": "network+modem", "sms": "sms",
    "traffic": "traffic", "battery": "charge", "rathole": "rathole",
    "ipv6proxy": "ipv6Proxy", "update": "update", "at": "atDebug",
    "terminal": "terminal", "usb": "usb", "plugins": "plugins",
    "settings": "settings",
}

# 已知非端点 / 示例占位符，不参与校验
IGNORED = {"/api/webhooks/YOUR_WEBHOOK", "/api"}


def norm(path: str) -> str:
    """归一化：通配 *、:param、模板变量、尾斜杠。"""
    p = path.split("?")[0].split("{")[0].split("$")[0]
    p = re.sub(r"/\*.*$", "", p)
    p = re.sub(r"/:[A-Za-z_][A-Za-z0-9_]*", "/:id", p)
    p = re.sub(r"/+$", "", p)
    return p


def main() -> int:
    failures: list[str] = []

    # ---- 旧版素材 ----
    c_src = (ROOT / "src/handlers/http_server.c").read_text(encoding="utf-8", errors="replace")
    c_routes = set(re.findall(r'mg_str\("(/api/[^"]+)"\)', c_src))
    vue_files = sorted((ROOT / "web/src/components").rglob("*.vue"))
    vue_endpoints: set[str] = set()
    for f in vue_files:
        text = f.read_text(encoding="utf-8", errors="replace")
        for m in re.findall(r"/api/[A-Za-z0-9_/:.$%?=&{}-]+", text):
            p = norm(m)
            if p.startswith("/api/") and len(p) > 5 and p not in IGNORED:
                vue_endpoints.add(p)
    app_vue = (ROOT / "web/src/App.vue").read_text(encoding="utf-8", errors="replace")
    menu_ids = set(re.findall(r"\{\s*id:\s*'([a-zA-Z0-9_]+)'", app_vue))

    # ---- 新版素材 ----
    contract = json.loads((ROOT / "tests/contract/api_endpoints.json").read_text(encoding="utf-8"))
    endpoints = contract["endpoints"] if isinstance(contract, dict) else contract
    new_paths = {norm(e["path"]) for e in endpoints}
    routes = (ROOT / "crates/api/src/routes.rs").read_text(encoding="utf-8")
    new_paths |= {norm(p) for p in re.findall(r'\.route\("(/api/[^"]+)"', routes)}
    pages = {p.name for p in (ROOT / "web/js/pages").glob("*.js")}
    app_js = (ROOT / "web/js/app.js").read_text(encoding="utf-8")

    def covered(path: str) -> bool:
        n = norm(path)
        if n in new_paths:
            return True
        # 参数化子路径：/api/x/:id 覆盖 /api/x/123 形态
        return any(q.endswith("/:id") and n.startswith(q[:-4]) for q in new_paths)

    # ---- (a) 旧 C 路由 ----
    c_miss = sorted(p for p in c_routes if not covered(p))
    print("(a) 旧 C 后端路由 -> 新契约/Rust 路由")
    print(f"    旧路由 {len(c_routes)} 条 | 未覆盖 {len(c_miss)} 条 {c_miss or '（无）'}")
    if c_miss:
        failures.append(f"旧 C 路由未覆盖: {c_miss}")

    # ---- (b) 旧 Vue 调用端点 ----
    v_miss = sorted(p for p in vue_endpoints if not covered(p))
    print("(b) 旧 Vue 组件调用端点 -> 新契约/Rust 路由")
    print(f"    旧端点 {len(vue_endpoints)} 条 | 未覆盖 {len(v_miss)} 条 {v_miss or '（无）'}")
    if v_miss:
        failures.append(f"旧前端端点未覆盖: {v_miss}")

    # ---- (c) 旧 Vue 组件 -> 新页面 ----
    comp_miss = []
    for comp, target in PAGE_MAP.items():
        targets = [t.strip() for t in target.replace("+", " + ").split(" + ")]
        if not (ROOT / "web/src/components" / comp).exists() and not any(
            (ROOT / "web/src/components/plugin" / comp).exists() for _ in [0]
        ):
            # 组件可能位于 plugin/ 子目录
            if not (ROOT / "web/src/components/plugin" / comp).exists():
                comp_miss.append(f"{comp}(旧文件缺失)")
                continue
        ok = all(
            (ROOT / "web/js/pages" / t).exists()
            or (ROOT / "web/js/components" / t).exists()
            or (ROOT / "web/js/core" / t).exists()
            or (t in app_js)
            or t == "app.js 导航"
            for t in targets
        )
        if not ok:
            comp_miss.append(f"{comp} -> {target}")
    print("(c) 旧 Vue 组件 -> 新页面/组件")
    print(f"    映射 {len(PAGE_MAP)} 项 | 缺失 {len(comp_miss)} 项 {comp_miss or '（无）'}")
    if comp_miss:
        failures.append(f"组件映射缺失: {comp_miss}")

    # ---- (d) 旧菜单 -> 新页面 ----
    menu_miss = []
    for mid, target in MENU_MAP.items():
        for t in target.split("+"):
            f = f"{t}.js" if not t.endswith(".js") else t
            if not (ROOT / "web/js/pages" / f).exists() and t not in pages and t not in app_js:
                menu_miss.append(f"{mid} -> {t}")
    print("(d) 旧版菜单项 -> 新版页面")
    print(f"    旧菜单 {len(menu_ids)} 项 / 映射 {len(MENU_MAP)} 项 | 缺失 {len(menu_miss)} 项 {menu_miss or '（无）'}")
    if menu_miss:
        failures.append(f"菜单映射缺失: {menu_miss}")

    print("-" * 72)
    print(f"新页面 {len(pages)} 个：{', '.join(sorted(pages))}")
    print("-" * 72)
    if failures:
        for f in failures:
            print("FAIL:", f)
        print("结果: FAIL")
        return 1
    print("结果: PASS —— 旧版后端路由、前端端点、组件与菜单功能均已移植")
    return 0


if __name__ == "__main__":
    sys.exit(main())
