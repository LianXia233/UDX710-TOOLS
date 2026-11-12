/**
 * 轻量 i18n：无第三方依赖。
 * - 支持 'a.b.c' 形式的嵌套键与 {name} / {0} 插值。
 * - 语言偏好存 localStorage（非凭证，允许持久化），默认简体中文。
 * - onChange() 订阅语言切换，app.js 据此重建导航与当前页面。
 */
import zhCN from './zh-CN.js';
import enUS from './en-US.js';

const MESSAGES = { 'zh-CN': zhCN, 'en-US': enUS };
const FALLBACK = 'zh-CN';
const STORAGE_KEY = 'udx710.locale';
const listeners = new Set();

function readSaved() {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && MESSAGES[saved]) return saved;
  } catch (e) {
    /* localStorage 不可用时忽略 */
  }
  return 'zh-CN';
}

let current = readSaved();

/** 按点路径读取字典值，找不到返回 undefined。 */
function lookup(dict, key) {
  if (!dict || !key) return undefined;
  const parts = key.split('.');
  let node = dict;
  for (const part of parts) {
    if (node == null || typeof node !== 'object') return undefined;
    node = node[part];
  }
  return typeof node === 'string' ? node : undefined;
}

/** 插值：支持 {name} 命名参数与 {0} 位置参数。 */
function interpolate(text, params) {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (match, token) => {
    if (Object.prototype.hasOwnProperty.call(params, token)) {
      return String(params[token]);
    }
    return match;
  });
}

/** 翻译；缺失键回退到中文，再回退到键本身。 */
export function t(key, params) {
  let text = lookup(MESSAGES[current], key);
  if (text === undefined && current !== FALLBACK) {
    text = lookup(MESSAGES[FALLBACK], key);
  }
  if (text === undefined) return key;
  return interpolate(text, params);
}

export function getLocale() {
  return current;
}

export function setLocale(locale) {
  if (!MESSAGES[locale] || locale === current) return;
  current = locale;
  try {
    localStorage.setItem(STORAGE_KEY, locale);
  } catch (e) {
    /* 忽略持久化失败 */
  }
  for (const fn of listeners) {
    try { fn(locale); } catch (e) { /* 单个订阅者异常不影响其他 */ }
  }
}

export function toggleLocale() {
  setLocale(current === 'zh-CN' ? 'en-US' : 'zh-CN');
}

/** 订阅语言变化，返回取消订阅函数。 */
export function onChange(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export default { t, getLocale, setLocale, toggleLocale, onChange };
