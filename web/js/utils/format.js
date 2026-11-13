/**
 * 格式化工具：纯函数，无副作用，无第三方依赖。
 * 所有返回值都适合放进 textContent，绝不拼接 HTML。
 */

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

/** 字节数格式化（1024 进制）。 */
export function formatBytes(value, digits = 2) {
  const n = Number(value);
  if (!Number.isFinite(n) || n < 0) return '—';
  if (n < 1024) return `${Math.round(n)} B`;
  let i = 0;
  let size = n;
  while (size >= 1024 && i < UNITS.length - 1) {
    size /= 1024;
    i += 1;
  }
  return `${size.toFixed(digits)} ${UNITS[i]}`;
}

/** 每秒速率格式化（字节/秒 -> 可读速率）。 */
export function formatRate(bytesPerSecond, digits = 1) {
  if (!Number.isFinite(Number(bytesPerSecond))) return '—';
  return `${formatBytes(bytesPerSecond, digits)}/s`;
}

/** 秒数 -> "1天 02:03:04" 形式。 */
export function formatUptime(seconds) {
  const s = Math.max(0, Math.floor(Number(seconds) || 0));
  const days = Math.floor(s / 86400);
  const hours = Math.floor((s % 86400) / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  const secs = s % 60;
  const pad = (v) => String(v).padStart(2, '0');
  const clock = `${pad(hours)}:${pad(minutes)}:${pad(secs)}`;
  return days > 0 ? `${days}天 ${clock}` : clock;
}

/** 毫秒/秒时间戳或字符串 -> 本地化时间字符串。 */
export function formatDate(input) {
  if (input == null || input === '') return '—';
  let date;
  if (input instanceof Date) {
    date = input;
  } else if (typeof input === 'number') {
    // 兼容秒级与毫秒级时间戳
    date = new Date(input < 1e12 ? input * 1000 : input);
  } else {
    const numeric = Number(input);
    date = Number.isFinite(numeric) && !Number.isNaN(numeric) && String(input).trim() !== ''
      ? new Date(numeric < 1e12 ? numeric * 1000 : numeric)
      : new Date(input);
  }
  if (Number.isNaN(date.getTime())) return String(input);
  return date.toLocaleString();
}

/** 数值百分比格式化。 */
export function formatPercent(value, digits = 1) {
  const n = Number(value);
  if (!Number.isFinite(n)) return '—';
  return `${n.toFixed(digits)}%`;
}

/** 千分位整数。 */
export function formatNumber(value) {
  const n = Number(value);
  if (!Number.isFinite(n)) return '—';
  return n.toLocaleString();
}

/** 截断长文本（用于日志、短信预览等，避免超长 DOM 文本）。 */
export function truncate(text, max = 120) {
  const str = text == null ? '' : String(text);
  return str.length > max ? `${str.slice(0, max)}…` : str;
}

/** 把任意值安全地转成用于 textContent 的字符串。 */
export function safeText(value, fallback = '—') {
  if (value == null || value === '') return fallback;
  if (typeof value === 'object') {
    try { return JSON.stringify(value); } catch (e) { return fallback; }
  }
  return String(value);
}

/**
 * 兼容旧后端 { Code, Data } 包装与新后端裸对象：提取真实数据体。
 * 仅当存在 Code/code 字段且存在 Data/data 字段时才解包，避免误伤正常的 { data } 业务对象。
 */
export function unwrap(payload) {
  if (payload && typeof payload === 'object' &&
      (payload.Code !== undefined || payload.code !== undefined)) {
    if (payload.Data !== undefined) return payload.Data;
    if (payload.data !== undefined) return payload.data;
  }
  return payload;
}

/** 从响应中尽量提取数组，兼容裸数组与多种包装键。 */
export function asArray(payload, keys = []) {
  if (Array.isArray(payload)) return payload;
  if (!payload || typeof payload !== 'object') return [];
  const candidates = [
    'Data', 'data', 'items', 'list', 'plugins', 'scripts', 'templates',
    'rules', 'services', 'sms', 'messages', 'cells', 'bands', 'interfaces',
    'logs', 'records', 'results', ...keys
  ];
  for (const key of candidates) {
    if (Array.isArray(payload[key])) return payload[key];
  }
  // 再下一层：后端改用 { data: { cells: [...] } } 包装时也能取到。
  // 原实现只看顶层与 Code/Data 解包，一旦加 data 包装，所有列表页会同时变空。
  for (const wrapper of ['Data', 'data']) {
    const inner = payload[wrapper];
    if (inner && typeof inner === 'object' && !Array.isArray(inner)) {
      for (const key of candidates) {
        if (Array.isArray(inner[key])) return inner[key];
      }
    }
  }
  const inner = unwrap(payload);
  if (Array.isArray(inner)) return inner;
  return [];
}

/** 从响应中读取字段，兼容包装体。 */
export function fieldOf(payload, key, fallback = undefined) {
  const obj = unwrap(payload);
  if (obj && typeof obj === 'object' && obj[key] !== undefined) return obj[key];
  if (payload && typeof payload === 'object' && payload[key] !== undefined) return payload[key];
  return fallback;
}

/**
 * 从对象中按白名单路径安全读取值。
 * 拒绝 __proto__ / prototype / constructor 等危险段，且只读取自有属性，
 * 避免原型链访问与意外的 getter 执行。供插件 UI 的 bind 使用。
 */
export function readPath(obj, path) {
  if (!path || typeof path !== 'string') return undefined;
  const BLOCKED = new Set(['__proto__', 'prototype', 'constructor']);
  let node = obj;
  for (const raw of path.split('.')) {
    const key = raw.trim();
    if (!key || BLOCKED.has(key)) return undefined;
    if (node == null || (typeof node !== 'object' && typeof node !== 'function')) return undefined;
    if (!Object.prototype.hasOwnProperty.call(node, key)) return undefined;
    node = node[key];
  }
  return node;
}
