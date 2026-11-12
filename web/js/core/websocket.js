/**
 * WebSocket 实时连接。
 *  - ManagedSocket：单条连接，指数退避重连、订阅表、状态回调、受控 close/清理。
 *  - realtime：状态推送单例（/api/ws 为主，连续失败后降级到 /api/events 的 SSE）。
 *    连接失败不影响页面（页面同时保留 HTTP 轮询作为数据来源）。
 *
 * 认证说明：浏览器 WebSocket / EventSource 无法设置 Authorization 头，
 * 因此把会话 token 作为查询参数附加（同源、短连接、不写入日志）；
 * Rust 后端升级握手时从 query 读取并校验。交互式终端同样走这条路径。
 */
import * as auth from './auth.js';

function tokenQuery(url) {
  const token = auth.getToken();
  if (!token) return url;
  const sep = url.includes('?') ? '&' : '?';
  return `${url}${sep}token=${encodeURIComponent(token)}`;
}

export class ManagedSocket {
  constructor(url, options = {}) {
    this.url = url;
    this.options = {
      protocols: [],
      reconnect: true,
      baseDelay: 500,
      maxDelay: 15000,
      withToken: true,
      ...options
    };
    this._subs = new Map();       // topic -> Set<fn>
    this._statusFns = new Set();  // Set<fn(status, detail)>
    this._ws = null;
    this._timer = null;
    this._retries = 0;
    this._closed = false;
    this._status = 'idle';
  }

  _buildUrl() {
    return this.options.withToken ? tokenQuery(this.url) : this.url;
  }

  _setStatus(status, detail) {
    this._status = status;
    for (const fn of Array.from(this._statusFns)) {
      try { fn(status, detail); } catch (e) { /* 订阅者异常隔离 */ }
    }
  }

  /** 订阅状态变化，返回取消订阅函数。 */
  onStatus(fn) {
    if (typeof fn !== 'function') return () => {};
    this._statusFns.add(fn);
    return () => this._statusFns.delete(fn);
  }

  /** 按消息 topic（type/topic 字段）订阅，返回取消订阅函数。 */
  subscribe(topic, fn) {
    if (typeof fn !== 'function') return () => {};
    let set = this._subs.get(topic);
    if (!set) { set = new Set(); this._subs.set(topic, set); }
    set.add(fn);
    return () => {
      set.delete(fn);
      if (set.size === 0) this._subs.delete(topic);
    };
  }

  on(type, fn) { return this.subscribe(type, fn); }

  connect() {
    if (this._closed) return;
    this._setStatus('connecting');
    let ws;
    try {
      ws = this.options.protocols.length
        ? new WebSocket(this._buildUrl(), this.options.protocols)
        : new WebSocket(this._buildUrl());
    } catch (err) {
      this._scheduleReconnect(err);
      return;
    }
    this._ws = ws;

    ws.addEventListener('open', () => {
      this._retries = 0;
      this._setStatus('open');
      if (this.options.onOpen) {
        try { this.options.onOpen(ws); } catch (e) { /* 忽略 */ }
      }
    });
    ws.addEventListener('message', (ev) => this._handleMessage(ev));
    ws.addEventListener('error', (ev) => {
      this._setStatus('error', ev);
      if (this.options.onError) {
        try { this.options.onError(ev); } catch (e) { /* 忽略 */ }
      }
    });
    ws.addEventListener('close', (ev) => {
      this._ws = null;
      this._setStatus('closed', ev);
      if (this.options.onClose) {
        try { this.options.onClose(ev); } catch (e) { /* 忽略 */ }
      }
      if (!this._closed && this.options.reconnect) this._scheduleReconnect();
    });
  }

  _handleMessage(ev) {
    let parsed = null;
    if (typeof ev.data === 'string') {
      try { parsed = JSON.parse(ev.data); } catch (e) { parsed = null; }
    }
    const detail = parsed != null ? parsed : ev.data;
    const topic = (parsed && (parsed.type || parsed.topic)) || 'message';
    if (this.options.onJSON) {
      try { this.options.onJSON(detail, topic, ev.data); } catch (e) { /* 忽略 */ }
    }
    if (this.options.onRaw) {
      try { this.options.onRaw(ev.data); } catch (e) { /* 忽略 */ }
    }
    const topics = new Set([topic, 'message', '*']);
    for (const t of topics) this._dispatch(t, detail, ev.data);
  }

  _dispatch(topic, detail, raw) {
    const set = this._subs.get(topic);
    if (!set) return;
    for (const fn of Array.from(set)) {
      try { fn(detail, topic, raw); } catch (e) { /* 订阅者异常隔离 */ }
    }
  }

  send(data) {
    if (!this._ws || this._ws.readyState !== WebSocket.OPEN) return false;
    const text = typeof data === 'string' ? data : JSON.stringify(data);
    try { this._ws.send(text); return true; } catch (e) { return false; }
  }

  _scheduleReconnect(err) {
    if (this._closed || !this.options.reconnect) {
      this._setStatus('failed', err);
      return;
    }
    this._retries += 1;
    const delay = Math.min(this.options.maxDelay, this.options.baseDelay * (2 ** (this._retries - 1)));
    this._setStatus('reconnecting', { attempt: this._retries, delay });
    this._timer = setTimeout(() => {
      this._timer = null;
      this.connect();
    }, delay);
  }

  /** 关闭连接并清空订阅/状态回调；由页面 destroy 时调用。幂等。 */
  close() {
    if (this._closed) return;
    this._closed = true;
    if (this._timer) { clearTimeout(this._timer); this._timer = null; }
    if (this._ws) {
      try { this._ws.close(1000, 'page leave'); } catch (e) { /* 忽略 */ }
      this._ws = null;
    }
    this._subs.clear();
    this._statusFns.clear();
    this._setStatus('closed');
  }

  get status() { return this._status; }
}

/** 状态推送单例：WS 优先，失败降级 SSE；对外暴露统一订阅接口。 */
function createRealtime() {
  const subs = new Map();      // topic -> Map<fn, owner>
  const statusFns = new Set();
  const FALLBACK_THRESHOLD = 2;
  let ws = null;
  let sse = null;
  let started = false;
  let mode = 'ws';
  let failures = 0;
  let status = 'idle';

  function setStatus(s, detail) {
    status = s;
    for (const fn of Array.from(statusFns)) {
      try { fn(s, detail); } catch (e) { /* 忽略 */ }
    }
  }

  function dispatch(detail, topic, raw) {
    const topics = new Set([topic, 'message', '*']);
    for (const t of topics) {
      const bucket = subs.get(t);
      if (!bucket) continue;
      for (const fn of Array.from(bucket.keys())) {
        try { fn(detail, topic, raw); } catch (e) { /* 忽略 */ }
      }
    }
  }

  function startSSE() {
    if (sse || typeof EventSource === 'undefined') {
      setStatus('failed');
      return;
    }
    try {
      sse = new EventSource(tokenQuery('/api/events'));
      sse.addEventListener('open', () => setStatus('open'));
      sse.addEventListener('message', (ev) => {
        let parsed = null;
        try { parsed = JSON.parse(ev.data); } catch (e) { parsed = null; }
        const detail = parsed != null ? parsed : ev.data;
        const topic = (parsed && (parsed.type || parsed.topic)) || 'message';
        dispatch(detail, topic, ev.data);
      });
      sse.addEventListener('error', () => setStatus('reconnecting'));
    } catch (e) {
      setStatus('failed', e);
    }
  }

  function connect() {
    if (started) return;
    started = true;
    mode = 'ws';
    failures = 0;
    ws = new ManagedSocket('/api/ws', {
      onJSON: (detail, topic, raw) => dispatch(detail, topic, raw),
      onStatus: (s) => {
        if (mode !== 'ws') return;
        if (s === 'open') {
          failures = 0;
          setStatus('open');
        } else if (s === 'reconnecting' || s === 'error' || s === 'closed') {
          failures += 1;
          setStatus('reconnecting');
          if (failures >= FALLBACK_THRESHOLD) {
            // 连续失败：关闭 WS，切换到 SSE 降级通道，页面本身不受影响
            mode = 'sse';
            if (ws) { ws.close(); ws = null; }
            startSSE();
          }
        }
      }
    });
    setStatus('connecting');
    ws.connect();
  }

  function subscribe(topic, fn, owner) {
    if (typeof fn !== 'function') return () => {};
    let bucket = subs.get(topic);
    if (!bucket) { bucket = new Map(); subs.set(topic, bucket); }
    bucket.set(fn, owner);
    return () => {
      bucket.delete(fn);
      if (bucket.size === 0) subs.delete(topic);
    };
  }

  /** 按 owner 清理订阅（页面离开时）。 */
  function clear(owner) {
    if (owner === undefined) { subs.clear(); return; }
    for (const [topic, bucket] of subs) {
      for (const [fn, o] of bucket) {
        if (o === owner) bucket.delete(fn);
      }
      if (bucket.size === 0) subs.delete(topic);
    }
  }

  function onStatus(fn) {
    if (typeof fn !== 'function') return () => {};
    statusFns.add(fn);
    return () => statusFns.delete(fn);
  }

  function close() {
    started = false;
    if (ws) { ws.close(); ws = null; }
    if (sse) { sse.close(); sse = null; }
    subs.clear();
    statusFns.clear();
    setStatus('closed');
  }

  return {
    connect, close, subscribe, onStatus, clear,
    getStatus: () => status,
    send: (data) => (ws ? ws.send(data) : false)
  };
}

export const realtime = createRealtime();
export default realtime;
