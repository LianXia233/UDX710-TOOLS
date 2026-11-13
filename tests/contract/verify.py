#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
UDX710-TOOLS API 契约静态校验脚本（纯 Python 3 标准库，无第三方依赖）。

用法：  python3 tests/contract/verify.py
退出码：0 = 全部校验通过；1 = 存在失败项。

校验项：
  (a) tests/contract/api_endpoints.json 中 path 唯一；每条含 path / methods /
      auth / permission / request / response / notes 七个字段；
  (b) 从 tests/contract/v2_baseline.json 读取 v2.x C 后端的
      mg_match(hm->uri, mg_str("..."), NULL) 路由基线（旧源码已删除，
      快照含来源 commit 与合并 sha256），把 /* 归一化为 /:id，
      逐个确认能在契约中找到对应条目。契约里的 Axum 占位符
      (/:id /:name /:service_id /:rule_id) 统一归一化为 /:id 以便跨命名比对。
      允许 GET / POST 共用同一 path（按归一化 path 集合比对，天然支持）。
      容错：以 // 注释掉的路由（如 /api/apn）不计入校验，单独列出；
  (c) 所有写接口（POST / PUT / DELETE / WS）auth 不为 public，
      且 /api/device_control 不得免认证；
      例外：登录与忘记密码恢复流程是协议上必须免认证的写入口，
      由 PUBLIC_WRITE_EXEMPT 显式列出并单独打印，便于审查。

输出：按功能域分组的统计表、未匹配项、注释路由、契约新增项。
"""
from __future__ import annotations

import json
import re
import sys
from collections import Counter, OrderedDict
from pathlib import Path

# 以脚本自身位置定位仓库根，保证从任意 cwd 调用都能工作
HERE = Path(__file__).resolve().parent          # <root>/tests/contract
ROOT = HERE.parent.parent                       # <root>
CONTRACT_FILE = HERE / "api_endpoints.json"
# v2.x 旧 C/Vue 源码已删除，路由/端点/组件/菜单基线固化在本快照中
BASELINE_FILE = HERE / "v2_baseline.json"

# 写方法集合；WebSocket 交互式终端按任务要求计入写接口
WRITE_METHODS = {"POST", "PUT", "DELETE", "PATCH", "WS"}

# 协议上必须免认证的写入口（仅登录本身）。
# 密保问答与自助重置密码已按需求整体移除，不再有免认证的恢复入口。
PUBLIC_WRITE_EXEMPT = {
    "/api/auth/login",
}

# 按需求有意移除的能力：密保问题（问答 / 设置 / 自助重置 / 恢复出厂）。
# 这些端点是 v2.x 的历史事实（基线快照如实保留，不做篡改），
# 但当前实现已整体删除，因此 (b) 未匹配项命中本表时放行并单独列示，
# 便于审查「移除是有意为之」而非实现遗漏。
REMOVED_BY_DESIGN = {
    "/api/security/status",
    "/api/security/setup",
    "/api/security/questions",
    "/api/security/verify",
    "/api/security/reset-password",
    "/api/security/factory-reset",
}

REQUIRED_FIELDS = {
    "path", "methods", "auth", "permission", "request", "response", "notes"
}
VALID_AUTH = {"required", "public", "public-get"}

# 功能域归类：按顺序匹配，精确路径规则放在可能冲突的前缀之前
DOMAIN_RULES = [
    ("/api/auth/", "认证 auth"),
    ("/api/sms", "短信 sms"),
    ("/api/update/", "OTA 更新"),
    ("/api/charge/", "充电控制"),
    ("/api/rathole/", "内网穿透 rathole"),
    ("/api/ipv6-proxy/", "IPv6 代理"),
    ("/api/netif/", "网络接口 netif"),
    ("/api/apn", "APN 配置"),
    ("/api/plugins", "插件与存储"),
    ("/api/scripts", "脚本管理"),
    ("/api/shell", "Shell 诊断"),
    ("/api/terminal/ws", "Web 终端 WS"),
    ("/api/events", "实时推送 SSE"),
    ("/api/get/Total", "流量统计"),
    ("/api/get/set", "流量统计"),
    ("/api/set/total", "流量统计"),
    ("/api/get/time", "系统时间"),
    ("/api/set/time", "系统时间"),
    ("/api/get/first-reboot", "定时重启"),
    ("/api/set/reboot", "定时重启"),
    ("/api/claen/cron", "定时重启"),
    ("/api/bands", "高级网络 radio"),
    ("/api/lock_bands", "高级网络 radio"),
    ("/api/unlock_bands", "高级网络 radio"),
    ("/api/cells", "高级网络 radio"),
    ("/api/lock_cell", "高级网络 radio"),
    ("/api/unlock_cell", "高级网络 radio"),
    ("/api/current_band", "高级网络 radio"),
    ("/api/set_network", "网络配置"),
    ("/api/switch", "网络配置"),
    ("/api/airplane_mode", "网络配置"),
    ("/api/at", "AT 调试"),
    ("/api/usb", "USB 模式"),
    ("/api/data", "数据/漫游"),
    ("/api/roaming", "数据/漫游"),
    ("/api/phone-case", "手机壳模式"),
    ("/api/info", "系统信息"),
    ("/api/serial", "系统信息"),
    ("/api/key", "系统信息"),
    ("/api/device_control", "设备控制"),
    ("/api/clear_cache", "设备控制"),
]


def normalize(path: str) -> str:
    """把 C 通配 /* 归一化为 /:id（任务 b），并把契约里的任意 Axum 占位符
    (/:name /:service_id /:rule_id /:id) 统一成 /:id，实现跨命名比对。"""
    p = path.replace("/*", "/:id")
    p = re.sub(r"/:[A-Za-z_][A-Za-z0-9_]*", "/:id", p)
    return p


def classify(path: str) -> str:
    for prefix, name in DOMAIN_RULES:
        if path == prefix or path.startswith(prefix):
            return name
    return "其他"


def disp_width(s: str) -> int:
    """终端显示宽度：中文等全角字符按 2 计。"""
    return sum(2 if ord(ch) > 127 else 1 for ch in s)


def pad(s: str, width: int) -> str:
    return s + " " * max(0, width - disp_width(s))


def main() -> int:
    errors = []

    try:
        with CONTRACT_FILE.open(encoding="utf-8") as f:
            endpoints = json.load(f)
    except (OSError, json.JSONDecodeError) as exc:
        print("无法读取契约文件 %s: %s" % (CONTRACT_FILE, exc))
        return 1
    try:
        baseline = json.loads(BASELINE_FILE.read_text(encoding="utf-8"))
        c_active = baseline["c_mg_routes_active"]
        c_commented = baseline["c_mg_routes_commented"]
        baseline_src = baseline["source"]
    except (OSError, json.JSONDecodeError, KeyError) as exc:
        print("无法读取 v2.x 基线快照 %s: %s" % (BASELINE_FILE, exc))
        return 1
    # 空基线必须拒绝，避免门禁在快照损坏时被"假通过"
    if (not isinstance(c_active, list) or not c_active
            or not isinstance(c_commented, list)
            or baseline.get("schema") != 1
            or not baseline_src.get("old_source_sha256")):
        print("v2.x 基线快照内容不完整或 schema 不匹配，拒绝放行")
        return 1

    # ---- (a) path 唯一 + 字段完整性 + auth 取值 ----
    paths = [e.get("path", "<missing>") for e in endpoints]
    dup = [p for p, c in Counter(paths).items() if c > 1]
    if dup:
        errors.append("(a) path 重复: %s" % ", ".join(sorted(dup)))
    for i, e in enumerate(endpoints):
        missing = REQUIRED_FIELDS - set(e)
        extra = set(e) - REQUIRED_FIELDS
        if missing or extra:
            errors.append("(a) 第 %d 条字段异常: 缺=%s 多=%s"
                          % (i, sorted(missing), sorted(extra)))
        if e.get("auth") not in VALID_AUTH:
            errors.append("(a) 第 %d 条 auth 取值非法: %s" % (i, e.get("auth")))

    # ---- (b) C 路由覆盖（基线来自快照）----
    c_norm_set = sorted({normalize(u) for u in c_active})
    contract_norm = {}
    for e in endpoints:
        contract_norm.setdefault(normalize(e["path"]), e["path"])
    unmatched_raw = [n for n in c_norm_set if n not in contract_norm]
    removed_norm = {normalize(p) for p in REMOVED_BY_DESIGN}
    removed_hit = [n for n in unmatched_raw if n in removed_norm]
    unmatched = [n for n in unmatched_raw if n not in removed_norm]
    if unmatched:
        errors.append("(b) C 路由未能在契约中找到: %s" % ", ".join(unmatched))
    if removed_hit:
        print("(b) 按需求有意移除、不再实现的路由 %d 条: %s"
              % (len(removed_hit), ", ".join(sorted(removed_hit))))

    # ---- (c) 写接口认证 ----
    write_count = 0
    exempt_used = []
    for e in endpoints:
        writes = [m for m in e.get("methods", []) if m in WRITE_METHODS]
        if not writes:
            continue
        write_count += 1
        if e.get("auth") == "public":
            if e["path"] in PUBLIC_WRITE_EXEMPT:
                exempt_used.append(e["path"])
            else:
                errors.append("(c) 写接口 auth=public 且非豁免: %s (%s)"
                              % (e["path"], "/".join(writes)))
        # public-get: 语义为「仅 GET 免认证」，写方法仍需认证，视为合规
        # required   : 全部方法需认证，合规
    dc = next((e for e in endpoints if e.get("path") == "/api/device_control"), None)
    if dc is None:
        errors.append("(c) 契约缺少 /api/device_control 条目")
    elif dc.get("auth") != "required":
        errors.append("(c) /api/device_control 不得免认证, 当前 auth=%s"
                      % dc.get("auth"))

    # ---- 统计 ----
    stats = OrderedDict()
    for e in endpoints:
        dom = classify(e["path"])
        s = stats.setdefault(dom, Counter())
        s["total"] += 1
        if any(m in WRITE_METHODS for m in e.get("methods", [])):
            s["write"] += 1
        s[e.get("auth", "?")] += 1

    c_dom = Counter()
    for n in c_norm_set:
        p = contract_norm.get(n)
        c_dom[classify(p) if p else "未匹配"] += 1

    commented_norm = {normalize(u) for u in c_commented}
    extra_new = [e["path"] for e in endpoints
                 if normalize(e["path"]) not in set(c_norm_set)
                 and normalize(e["path"]) not in commented_norm]
    extra_compat = [e["path"] for e in endpoints
                    if normalize(e["path"]) in commented_norm]

    # ---- 输出 ----
    line = "=" * 88
    print(line)
    print(" UDX710-TOOLS API 契约静态校验")
    print(line)
    print(" 契约文件   : %s" % CONTRACT_FILE.relative_to(ROOT))
    print(" 基线快照   : %s" % BASELINE_FILE.relative_to(ROOT))
    print(" 基线来源   : commit %s | 旧源码 %s 文件 | sha256 %s…"
          % (baseline_src["old_code_last_commit"][:12],
             baseline_src["old_file_count"],
             baseline_src["old_source_sha256"][:16]))
    print(" 契约端点数 : %d" % len(endpoints))
    print(" C 有效路由 : %d (归一化去重后 %d)"
          % (len(c_active), len(c_norm_set)))
    print(" C 注释路由 : %d (已容错跳过)" % len(c_commented))
    print(" 写接口数   : %d" % write_count)
    print("-" * 88)

    ok_a = not dup and all(
        not (REQUIRED_FIELDS - set(e)) and not (set(e) - REQUIRED_FIELDS)
        and e.get("auth") in VALID_AUTH for e in endpoints)
    ok_b = not unmatched
    ok_c = not any(x.startswith("(c)") for x in errors)
    print(" (a) path 唯一 / 字段 / auth 取值 : %s (%d 条, 重复 %d)"
          % ("PASS" if ok_a else "FAIL", len(endpoints), len(dup)))
    print(" (b) C 路由覆盖                    : %s (%d/%d 命中, 未匹配 %d)"
          % ("PASS" if ok_b else "FAIL",
             len(c_norm_set) - len(unmatched), len(c_norm_set), len(unmatched)))
    print(" (c) 写接口认证 / device_control   : %s (写 %d, 协议豁免 %d, device_control=%s)"
          % ("PASS" if ok_c else "FAIL", write_count, len(exempt_used),
             dc.get("auth") if dc else "缺失"))
    print("-" * 88)
    print(" 按功能域统计")
    header = ["域", "端点", "写", "required", "pub-get", "public", "C路由", "未命中"]
    widths = [30, 6, 5, 10, 9, 8, 7, 8]
    print(" " + "".join(pad(h, w) for h, w in zip(header, widths)))
    print(" " + "-" * sum(widths))
    for dom, s in stats.items():
        miss = sum(1 for n in unmatched
                   if classify(contract_norm.get(n, "")) == dom)
        row = [dom, s["total"], s["write"], s["required"],
               s["public-get"], s["public"], c_dom.get(dom, 0), miss]
        print(" " + "".join(pad(str(v), w) for v, w in zip(row, widths)))
    tot = Counter()
    for s in stats.values():
        tot.update(s)
    trow = ["合计", tot["total"], tot["write"], tot["required"],
            tot["public-get"], tot["public"], len(c_norm_set), len(unmatched)]
    print(" " + "-" * sum(widths))
    print(" " + "".join(pad(str(v), w) for v, w in zip(trow, widths)))
    print("-" * 88)
    print(" 未匹配项 (C 有 / 契约无)   : %s"
          % (", ".join(unmatched) if unmatched else "无"))
    print(" C 注释路由 (容错跳过)     : %s"
          % (", ".join(sorted(set(c_commented))) if c_commented else "无"))
    print(" 契约新增 (C 无对应路由)   : %s"
          % (", ".join(extra_new) if extra_new else "无"))
    print(" 注释路由的兼容别名条目     : %s"
          % (", ".join(extra_compat) if extra_compat else "无"))
    print(" 协议豁免的公开写入口       : %s"
          % (", ".join(sorted(exempt_used)) if exempt_used else "无"))
    print(line)

    if errors:
        print(" 结果: FAIL (退出码 1)，失败明细：")
        for e in errors:
            print("   - %s" % e)
        return 1
    print(" 结果: PASS (退出码 0)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
