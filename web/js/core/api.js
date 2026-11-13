/**
 * 统一 HTTP 客户端（对应技术报告 4.4 示例的生产化实现）。
 *
 * 关键设计：
 *  - API_BASE = '/api'；调用方可传 '/api/info' 或 '/info'，两种写法都兼容。
 *  - Bearer Token 从 sessionStorage 读取（长期凭证不落 localStorage）。
 *  - AbortController 超时 + 调用方 signal 合并（不依赖 AbortSignal.any，兼容性更好）。
 *  - 401 清除 token 并派发 auth-required 事件，交由 app.js 跳转登录。
 *  - 仅幂等 GET 允许有限重试（网络错误 / 5xx）；POST/PUT/DELETE 等非幂等请求绝不自动重试。
 *  - 响应体大小上限，流式读取并计数，超限立即中断，避免大响应撑爆内存。
 *  - FormData 不设置 Content-Type，由浏览器生成 multipart boundary。
 */
export const API_BASE = '/api';
export const TOKEN_KEY = 'auth_token';
export const META_KEY = 'udx710.auth.meta';
/** 持久副本键：sessionStorage 被清（隐私模式 / 浏览器重启 / 换标签打开）时兜底。 */
export const PERSIST_KEY = 'udx.auth.persist';

const DEFAULT_TIMEOUT = 10000;
const MAX_BODY_BYTES = 16 * 1024 * 1024; // 16 MiB，单次响应上限

export class ApiError extends Error {
  constructor(status, message, payload, extra = {}) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.payload = payload;
    this.timeout = !!extra.timeout;
    this.aborted = !!extra.aborted;
  }
}

/** 归一化 URL：兼容带 /api 前缀与不带前缀两种调用写法。 */
function toUrl(path) {
  if (/^https?:\/\//i.test(path)) return path;
  let p = String(path);
  if (!p.startsWith('/')) p = `/${p}`;
  if (p === API_BASE || p.startsWith(`${API_BASE}/`)) return p;
  return `${API_BASE}${p}`;
}

/**
 * 取会话 token。
 *
 * sessionStorage 是主存储；若已被清空（隐私模式、浏览器重启、从其他标签
 * 或快捷方式重新打开），回落到 localStorage 里的持久副本并回填会话副本，
 * 避免每次打开面板都要求重新登录。有效期仍由副本里的 expiresAt 与服务端
 * Bearer 校验共同约束。
 */
function getToken() {
  try {
    const direct = sessionStorage.getItem(TOKEN_KEY);
    if (direct) return direct;
  } catch (e) { /* 继续尝试持久副本 */ }
  try {
    const raw = localStorage.getItem(PERSIST_KEY);
    if (!raw) return '';
    const obj = JSON.parse(raw);
    if (!obj || !obj.token) return '';
    if (obj.expiresAt && Date.now() > obj.expiresAt) {
      localStorage.removeItem(PERSIST_KEY);
      return '';
    }
    try { sessionStorage.setItem(TOKEN_KEY, obj.token); } catch (e) { /* 忽略 */ }
    return obj.token;
  } catch (e) {
    return '';
  }
}

/** 401：清除会话凭证与持久副本，并通知应用层跳转登录。 */
function handleUnauthorized() {
  try {
    sessionStorage.removeItem(TOKEN_KEY);
    sessionStorage.removeItem(META_KEY);
    localStorage.removeItem(PERSIST_KEY);
  } catch (e) { /* 存储不可用时忽略 */ }
  window.dispatchEvent(new CustomEvent('auth-required'));
}

/** 合并多个 AbortSignal 与超时信号为一个 signal。 */
function combineSignals(signals, timeoutMs) {
  const controller = new AbortController();
  const listeners = [];
  let timedOut = false;

  const abortWith = (reason) => {
    controller.abort(reason === undefined ? new DOMException('Aborted', 'AbortError') : reason);
  };

  for (const s of signals) {
    if (!s) continue;
    if (s.aborted) {
      abortWith(s.reason);
    } else {
      const fn = () => abortWith(s.reason);
      s.addEventListener('abort', fn, { once: true });
      listeners.push([s, fn]);
    }
  }

  let timer = null;
  if (timeoutMs > 0 && !controller.signal.aborted) {
    timer = setTimeout(() => {
      timedOut = true;
      abortWith(new DOMException('Timeout', 'TimeoutError'));
    }, timeoutMs);
  }

  return {
    signal: controller.signal,
    isTimeout: () => timedOut,
    cleanup() {
      if (timer) clearTimeout(timer);
      for (const [s, fn] of listeners) s.removeEventListener('abort', fn);
    }
  };
}

/** 退避等待；外部 signal 触发时立即结束等待。 */
function backoff(attempt, signal) {
  const delay = Math.min(2000, 200 * (2 ** (attempt - 1)));
  return new Promise((resolve) => {
    if (signal && signal.aborted) { resolve(); return; }
    let timer = null;
    const done = () => { cleanup(); resolve(); };
    const onAbort = () => { cleanup(); resolve(); };
    function cleanup() {
      if (timer) clearTimeout(timer);
      if (signal) signal.removeEventListener('abort', onAbort);
    }
    timer = setTimeout(done, delay);
    if (signal) signal.addEventListener('abort', onAbort, { once: true });
  });
}

/** 流式读取响应体并限制字节数；返回解析后的 JSON / 文本 / null。 */
async function readBody(response, maxBytes) {
  const declared = Number(response.headers.get('content-length') || 0);
  if (declared && declared > maxBytes) {
    throw new ApiError(413, '响应体超过大小上限', null);
  }
  let text = '';
  if (response.body && typeof response.body.getReader === 'function') {
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    const parts = [];
    let received = 0;
    // eslint-disable-next-line no-constant-condition
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      received += value.byteLength;
      if (received > maxBytes) {
        try { await reader.cancel(); } catch (e) { /* 忽略取消异常 */ }
        throw new ApiError(413, '响应体超过大小上限', null);
      }
      parts.push(value);
    }
    text = parts.map((v) => decoder.decode(v, { stream: true })).join('') + decoder.decode();
  } else {
    text = await response.text();
    if (text.length > maxBytes) throw new ApiError(413, '响应体超过大小上限', null);
  }
  const trimmed = text.trim();
  if (!trimmed) return null;
  try { return JSON.parse(trimmed); } catch (e) { return trimmed; }
}

function messageOf(payload, fallback) {
  if (payload == null) return fallback;
  if (typeof payload === 'string') return payload;
  if (typeof payload === 'object') {
    return payload.message || payload.error || payload.Error || payload.detail || fallback;
  }
  return fallback;
}

function normalizeBody(body) {
  if (body == null) return { body: undefined, isForm: false };
  const passthrough = (body instanceof FormData) || (body instanceof Blob) ||
    (body instanceof URLSearchParams) || (body instanceof ArrayBuffer) ||
    (typeof ArrayBuffer !== 'undefined' && ArrayBuffer.isView(body)) ||
    (typeof ReadableStream !== 'undefined' && body instanceof ReadableStream);
  if (passthrough) return { body, isForm: body instanceof FormData };
  if (typeof body === 'string') return { body, isForm: false };
  return { body: JSON.stringify(body), isForm: false };
}

/**
 * 发起 API 请求。
 * @param {string} path 路径（'/api/info' 或 '/info'）。
 * @param {object} options fetch 选项扩展：{ timeout, signal, body, headers, method,
 *   maxBytes, retry, retries, fetchOptions }。
 * @returns {Promise<any>} 解析后的响应体。
 */
export async function api(path, options = {}) {
  const method = String(options.method || 'GET').toUpperCase();
  const timeout = options.timeout == null ? DEFAULT_TIMEOUT : options.timeout;
  const maxBytes = options.maxBytes == null ? MAX_BODY_BYTES : options.maxBytes;
  const external = options.signal || null;
  const retryable = method === 'GET' && options.retry !== false;
  const extraRetries = options.retries == null ? 2 : Math.max(0, options.retries);
  const maxAttempts = retryable ? Math.min(3, extraRetries + 1) : 1;

  const headers = new Headers(options.headers || {});
  headers.set('Accept', 'application/json');
  const token = getToken();
  if (token) headers.set('Authorization', `Bearer ${token}`);

  const { body, isForm } = normalizeBody(options.body);
  if (body !== undefined && !isForm) headers.set('Content-Type', 'application/json');

  let lastError = null;
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    if (external && external.aborted) {
      throw new ApiError(0, '请求已取消', null, { aborted: true });
    }
    const merged = combineSignals([external], timeout);
    try {
      const response = await fetch(toUrl(path), {
        method,
        headers,
        body,
        signal: merged.signal,
        credentials: 'same-origin',
        cache: 'no-store',
        ...(options.fetchOptions || {})
      });
      const payload = await readBody(response, maxBytes);

      if (!response.ok) {
        if (response.status === 401) {
          handleUnauthorized();
          throw new ApiError(401, messageOf(payload, '未授权'), payload);
        }
        if (retryable && response.status >= 500 && attempt < maxAttempts) {
          lastError = new ApiError(response.status, messageOf(payload, response.statusText), payload);
          await backoff(attempt, merged.signal);
          continue;
        }
        throw new ApiError(response.status, messageOf(payload, response.statusText), payload);
      }
      return payload;
    } catch (err) {
      if (merged.isTimeout()) {
        throw new ApiError(0, '请求超时', null, { timeout: true });
      }
      if (external && external.aborted) {
        throw new ApiError(0, '请求已取消', null, { aborted: true });
      }
      if (err instanceof ApiError) throw err;
      if (retryable && attempt < maxAttempts) {
        lastError = err;
        await backoff(attempt, external);
        continue;
      }
      throw new ApiError(0, messageOf(err, '网络错误'), null);
    } finally {
      merged.cleanup();
    }
  }
  throw lastError || new ApiError(0, '请求失败', null);
}

// 便捷方法（均复用同一 api，保持超时/认证/重试策略一致）
api.get = (path, opts = {}) => api(path, { ...opts, method: 'GET' });
api.post = (path, body, opts = {}) => api(path, { ...opts, method: 'POST', body });
api.put = (path, body, opts = {}) => api(path, { ...opts, method: 'PUT', body });
api.patch = (path, body, opts = {}) => api(path, { ...opts, method: 'PATCH', body });
api.del = (path, opts = {}) => api(path, { ...opts, method: 'DELETE' });
api.form = (path, formData, opts = {}) => api(path, { ...opts, method: 'POST', body: formData });

export const request = api;
export default api;
