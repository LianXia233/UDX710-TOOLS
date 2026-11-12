/**
 * 认证与 Token 生命周期。
 *
 * 安全约束：
 *  - 凭证只存 sessionStorage（会话级），不写入 localStorage（避免长期凭证持久化）。
 *  - 记录签发时间 / 过期时间；若服务端返回 expires_in 或 JWT exp，本地到期即失效。
 *  - 真正的安全性由服务端 Bearer 校验负责，本地过期判断仅为体验优化；
 *    服务端 401 时 api.js 会清除凭证并派发 auth-required。
 */
import { api, TOKEN_KEY, META_KEY } from './api.js';
import * as store from './store.js';
import * as events from './events.js';

function readMeta() {
  try { return JSON.parse(sessionStorage.getItem(META_KEY) || '{}'); } catch (e) { return {}; }
}
function writeMeta(meta) {
  try { sessionStorage.setItem(META_KEY, JSON.stringify(meta)); } catch (e) { /* 忽略 */ }
}

/** 仅解析 JWT 的 exp 做本地过期判断，不做验签（验签属于服务端职责）。 */
function decodeExp(token) {
  const parts = String(token || '').split('.');
  if (parts.length !== 3) return 0;
  try {
    let s = parts[1].replace(/-/g, '+').replace(/_/g, '/');
    while (s.length % 4) s += '=';
    const payload = JSON.parse(atob(s));
    return typeof payload.exp === 'number' ? payload.exp : 0;
  } catch (e) {
    return 0;
  }
}

export function getToken() {
  try {
    const token = sessionStorage.getItem(TOKEN_KEY);
    if (!token) return '';
    const meta = readMeta();
    if (meta.expiresAt && Date.now() > meta.expiresAt) {
      clearToken();
      return '';
    }
    return token;
  } catch (e) {
    return '';
  }
}

export function setToken(token, ttlSeconds) {
  if (!token) return '';
  try { sessionStorage.setItem(TOKEN_KEY, token); } catch (e) { /* 忽略 */ }
  const meta = { issuedAt: Date.now() };
  const ttl = Number(ttlSeconds);
  if (ttl > 0) meta.expiresAt = Date.now() + ttl * 1000;
  const exp = decodeExp(token);
  if (exp && (!meta.expiresAt || exp * 1000 < meta.expiresAt)) {
    meta.expiresAt = exp * 1000;
  }
  writeMeta(meta);
  store.set('authenticated', true);
  events.emit('auth:login', { token });
  return token;
}

export function clearToken() {
  try {
    sessionStorage.removeItem(TOKEN_KEY);
    sessionStorage.removeItem(META_KEY);
  } catch (e) { /* 忽略 */ }
  store.set('authenticated', false);
  events.emit('auth:logout', {});
}

export function isAuthenticated() {
  return !!getToken();
}

export function getTokenInfo() {
  const token = getToken();
  const meta = readMeta();
  return {
    hasToken: !!token,
    issuedAt: meta.issuedAt || 0,
    expiresAt: meta.expiresAt || 0,
    expired: !!(meta.expiresAt && Date.now() > meta.expiresAt)
  };
}

/** 登录：成功写入 token，失败抛出错误（api 已处理非 2xx）。 */
export async function login(password, signal) {
  const payload = await api('/api/auth/login', {
    method: 'POST', body: { password }, signal
  });
  const token = payload && (payload.token || (payload.data && payload.data.token));
  if (!token) {
    const msg = payload && (payload.error || payload.message || payload.Error);
    throw new Error(msg || '登录失败：服务端未返回 token');
  }
  const ttl = payload.expires_in || payload.ttl ||
    (payload.data && payload.data.expires_in) || 0;
  setToken(token, ttl);
  return payload;
}

/** 登出：无论请求成功与否都清除本地凭证。 */
export async function logout() {
  try {
    await api('/api/auth/logout', { method: 'POST' });
  } catch (e) {
    /* 登出请求失败也清理本地会话 */
  } finally {
    clearToken();
  }
}

export function status(signal) {
  return api('/api/auth/status', { signal });
}

export function changePassword(old_password, new_password, signal) {
  return api('/api/auth/password', {
    method: 'POST', body: { old_password, new_password }, signal
  });
}

export default {
  getToken, setToken, clearToken, isAuthenticated, getTokenInfo,
  login, logout, status, changePassword
};
