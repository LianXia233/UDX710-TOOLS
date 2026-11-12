/**
 * 前端看门狗：检测「界面主线程假死」与「后端失联/假死」并自愈。
 *
 * - UI 假死：1s 心跳，若回调被阻塞超过阈值才恢复，说明主线程曾卡死；
 *   显示提示条并在宽限期后自动 reload（可取消 / 立即刷新）。
 * - 后端假死：周期探测 /api/health（AbortController 超时），连续失败达阈值
 *   判定失联；后端恢复后自动重建实时连接（WS / SSE 由 realtime 自行降级）。
 * - 页面隐藏时浏览器会节流定时器，此时重置心跳，避免误报。
 * - 不写 localStorage、不注入内联样式，兼容严格 CSP。
 */
import * as auth from './auth.js';
import { realtime } from './websocket.js';

const DEFAULTS = {
  tickIntervalMs: 1000,
  freezeThresholdMs: 8000,
  probeIntervalMs: 15000,
  probeTimeoutMs: 5000,
  probeFailures: 3,
  autoReloadDelayMs: 3000
};

let cfg = { ...DEFAULTS };
let banner = null;
let textEl = null;
let actionBtn = null;
let cancelBtn = null;
let tickTimer = null;
let probeTimer = null;
let reloadTimer = null;
let lastTick = 0;
let failures = 0;
let backendUp = true;
let frozen = false;
let started = false;
let lastStalled = false;
let onAction = null;

const nowMs = () => (typeof performance !== 'undefined' ? performance.now() : Date.now());

function ensureBanner() {
  if (banner) return true;
  banner = document.getElementById('watchdog-banner');
  textEl = document.getElementById('watchdog-banner-text');
  actionBtn = document.getElementById('watchdog-banner-action');
  if (!banner || !textEl || !actionBtn) return false;

  cancelBtn = document.createElement('button');
  cancelBtn.type = 'button';
  cancelBtn.textContent = '暂不刷新';
  cancelBtn.hidden = true;
  cancelBtn.addEventListener('click', () => {
    clearReload();
    hide();
  });
  banner.appendChild(cancelBtn);
  return true;
}

function show(level, text, actionLabel, handler, cancelable = false) {
  if (!ensureBanner()) return;
  banner.dataset.level = level;
  textEl.textContent = text;
  actionBtn.hidden = !actionLabel;
  actionBtn.textContent = actionLabel || '';
  onAction = typeof handler === 'function' ? handler : null;
  cancelBtn.hidden = !cancelable;
  banner.classList.add('is-visible');
}

function hide() {
  if (!banner) return;
  banner.classList.remove('is-visible');
  if (actionBtn) actionBtn.hidden = true;
  if (cancelBtn) cancelBtn.hidden = true;
  onAction = null;
}

function clearReload() {
  if (reloadTimer) {
    clearTimeout(reloadTimer);
    reloadTimer = null;
  }
}

function reload() {
  clearReload();
  try { window.location.reload(); } catch (e) { /* 忽略 */ }
}

// ---------------- UI 主线程假死 ----------------
function tick() {
  const t = nowMs();
  const drift = t - lastTick;
  lastTick = t;
  // 后台标签页会被节流，跳过判定
  if (document.hidden) return;
  if (frozen || drift <= cfg.freezeThresholdMs) return;

  frozen = true;
  show(
    'error',
    `界面曾无响应 ${(drift / 1000).toFixed(1)} 秒，即将自动刷新`,
    '立即刷新',
    reload,
    true
  );
  clearReload();
  reloadTimer = setTimeout(reload, cfg.autoReloadDelayMs);
}

// ---------------- 后端健康探测 ----------------
async function probe() {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), cfg.probeTimeoutMs);
  try {
    const res = await fetch('/api/health', {
      method: 'GET',
      cache: 'no-store',
      signal: controller.signal
    });
    clearTimeout(timer);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const data = await res.json();
    failures = 0;
    // /api/health 暴露后端看门狗状态：stalled 表示后端心跳已超时，
    // action=exit 时 supervisor 正在重启，action=log 时仅告警。
    const stalled = !!(data && data.watchdog && data.watchdog.stalled);
    if (stalled !== lastStalled) {
      lastStalled = stalled;
      if (stalled) {
        show('warning', '后端心跳超时，守护进程正在处理', '', null, false);
      } else if (backendUp && !frozen) {
        hide();
      }
    }
    if (!backendUp) {
      backendUp = true;
      // 后端恢复：重建实时通道（realtime.connect 幂等）
      try { realtime.close(); } catch (e) { /* 忽略 */ }
      if (auth.isAuthenticated()) realtime.connect();
      show('ok', '后端已恢复连接', '', null, false);
      setTimeout(() => {
        if (backendUp && !frozen) hide();
      }, 2600);
    }
    return data;
  } catch (e) {
    clearTimeout(timer);
    failures += 1;
    if (failures >= cfg.probeFailures && backendUp) {
      backendUp = false;
      show(
        'warning',
        `后端连续 ${failures} 次无响应，正在持续重试`,
        '立即重试',
        () => { failures = 0; probe(); },
        false
      );
    }
    return null;
  }
}

function onVisibility() {
  // 回到前台：重置心跳并立刻探测一次，避免累计漂移误报
  if (!document.hidden) {
    lastTick = nowMs();
    probe();
  }
}

function onOnline() {
  failures = 0;
  probe();
}

// ---------------- 公共 API ----------------
export function start(options = {}) {
  if (started) return;
  if (!ensureBanner()) return;
  started = true;
  cfg = { ...DEFAULTS, ...options };
  lastTick = nowMs();
  failures = 0;
  backendUp = true;
  frozen = false;

  actionBtn.addEventListener('click', () => {
    if (onAction) onAction();
  });
  tickTimer = setInterval(tick, cfg.tickIntervalMs);
  probeTimer = setInterval(probe, cfg.probeIntervalMs);
  document.addEventListener('visibilitychange', onVisibility);
  window.addEventListener('online', onOnline);
  probe();
}

export function stop() {
  if (!started) return;
  started = false;
  clearInterval(tickTimer);
  clearInterval(probeTimer);
  clearReload();
  document.removeEventListener('visibilitychange', onVisibility);
  window.removeEventListener('online', onOnline);
  hide();
}

export function getStatus() {
  return {
    started,
    backendUp,
    frozen,
    failures,
    lastTickAgeMs: Math.max(0, nowMs() - lastTick)
  };
}

export default { start, stop, getStatus, probe };
