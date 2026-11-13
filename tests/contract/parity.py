#!/usr/bin/env python3
"""功能移植对照校验（可复跑）。

v2.x 旧 C/Vue 源码已从本仓库删除；基线固化在
`tests/contract/v2_baseline.json`（含来源 commit 与旧源码合并 sha256，
生成时从 `src/handlers/http_server.c`、`web/src/components/*.vue`、
`web/src/App.vue` 提取）。本脚本对照：

  (a) 旧 C 后端 `mg_str("/api/...")` 路由
  (b) 旧 Vue 组件调用的 `/api/...` 端点
  (c) 旧 Vue 组件 -> 新页面/组件映射
  (d) 旧菜单项 -> 新页面

与新实现（`tests/contract/api_endpoints.json`、`crates/api/src/routes.rs`、
`web/js/pages/`、`web/js/app.js`）逐项比对，任何遗漏即 FAIL。

用法：python3 tests/contract/parity.py
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASELINE = ROOT / "tests/contract/v2_baseline.json"

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


def load_baseline() -> dict:
    """读取并校验基线快照，避免快照损坏/被清空导致门禁假通过。"""
    if not BASELINE.is_file():
        raise SystemExit(f"FAIL: 基线快照缺失: {BASELINE}")
    data = json.loads(BASELINE.read_text(encoding="utf-8"))
    if data.get("schema") != 1:
        raise SystemExit("FAIL: 基线快照 schema 版本不匹配")
    for key in ("c_routes", "vue_endpoints", "vue_components", "menu_ids"):
        value = data.get(key)
        if not isinstance(value, list) or not value:
            raise SystemExit(f"FAIL: 基线快照字段为空或格式错误: {key}")
    src = data.get("source", {})
    if not src.get("old_source_sha256") or not src.get("old_code_last_commit"):
        raise SystemExit("FAIL: 基线快照缺少来源 commit / sha256，无法追溯")
    if any(not p.startswith("/api/") for p in data["c_routes"] + data["vue_endpoints"]):
        raise SystemExit("FAIL: 基线快照含非法端点")
    return data


# 按需求有意移除的能力（密保问题整套），基线保留历史事实、不做篡改，
# 但对照时登记为「已移除」，使其不产生 FAIL，同时在输出中显式列示便于审查。
REMOVED_BY_DESIGN_ROUTES = {
    "/api/security/status",
    "/api/security/setup",
    "/api/security/questions",
    "/api/security/verify",
    "/api/security/reset-password",
    "/api/security/factory-reset",
}
REMOVED_BY_DESIGN_COMPONENTS = {"SecuritySetup.vue"}


def main() -> int:
    failures: list[str] = []
    baseline = load_baseline()
    src = baseline["source"]

    c_routes = set(baseline["c_routes"])
    vue_endpoints = set(baseline["vue_endpoints"])
    vue_components = set(baseline["vue_components"])
    menu_ids = set(baseline["menu_ids"])

    print("基线快照: tests/contract/v2_baseline.json")
    print(f"  来源 commit {src['old_code_last_commit'][:12]} | 旧源码 {src['old_file_count']} 文件 | sha256 {src['old_source_sha256'][:16]}…")
    print(f"  旧源码已删除，本快照为功能对照唯一基线（生成于 HEAD {src['snapshot_created_at_head'][:12]}）")
    print("-" * 72)

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
    c_removed = sorted(p for p in c_routes if p in REMOVED_BY_DESIGN_ROUTES and not covered(p))
    c_miss = sorted(p for p in c_routes
                    if not covered(p) and p not in REMOVED_BY_DESIGN_ROUTES)
    print("(a) 旧 C 后端路由 -> 新契约/Rust 路由")
    print(f"    旧路由 {len(c_routes)} 条 | 未覆盖 {len(c_miss)} 条 {c_miss or '（无）'}")
    if c_removed:
        print(f"    按需求有意移除 {len(c_removed)} 条: {', '.join(c_removed)}")
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
    # 双向核对：映射表不得虚构旧组件，也不得漏掉快照里的旧组件
    # 有意移除的组件（随密保功能整体删除）不计入缺失，单独列示。
    comp_removed = sorted(vue_components & REMOVED_BY_DESIGN_COMPONENTS)
    for comp in sorted(vue_components - PAGE_MAP.keys() - REMOVED_BY_DESIGN_COMPONENTS):
        comp_miss.append(f"{comp}(未建立映射)")
    for comp in sorted(PAGE_MAP.keys() - vue_components):
        comp_miss.append(f"{comp}(映射表多出/基线无此组件)")
    for comp, target in PAGE_MAP.items():
        targets = [t.strip() for t in target.split(" + ")]
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
    print(f"    基线组件 {len(vue_components)} 个 / 映射 {len(PAGE_MAP)} 项 | 缺失 {len(comp_miss)} 项 {comp_miss or '（无）'}")
    if comp_removed:
        print(f"    按需求有意移除 {len(comp_removed)} 个: {', '.join(comp_removed)}")
    if comp_miss:
        failures.append(f"组件映射缺失: {comp_miss}")

    # ---- (d) 旧菜单 -> 新页面 ----
    menu_miss = []
    for mid in sorted(menu_ids - MENU_MAP.keys()):
        menu_miss.append(f"{mid}(未建立映射)")
    for mid in sorted(MENU_MAP.keys() - menu_ids):
        menu_miss.append(f"{mid}(映射表多出/基线无此菜单)")
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
