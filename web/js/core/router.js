/**
 * 轻量 hash 路由。
 *  - 页面以 { render(container, ctx), destroy() } 形式注册。
 *  - 每次切换创建新作用域（createScope），先调用上一页 destroy() 再渲染下一页。
 *  - 未知路由回退到 fallback；支持守卫（如未登录跳转登录）与 onChange 回调。
 *  - 页面自身的请求/定时器/监听清理由 ctx.scope 承载，destroy() 中统一销毁。
 */
import { createScope } from '../utils/dom.js';

const pages = new Map();
let cfg = null;
let current = null;
let hashHandler = null;

export function register(id, mod) {
  pages.set(id, mod);
  return router;
}

export function registerAll(map) {
  for (const [id, mod] of Object.entries(map)) pages.set(id, mod);
  return router;
}

export function configure(options) {
  cfg = options;
  return router;
}

function parseHash() {
  const raw = (location.hash || '').replace(/^#\/?/, '');
  const qIndex = raw.indexOf('?');
  const idPart = (qIndex >= 0 ? raw.slice(0, qIndex) : raw).trim();
  const query = qIndex >= 0 ? raw.slice(qIndex + 1) : '';
  const params = {};
  if (query) {
    for (const [k, v] of new URLSearchParams(query)) params[k] = v;
  }
  return { id: idPart || (cfg && cfg.fallback) || 'dashboard', params };
}

function buildHash(id, params) {
  let hash = `#/${id}`;
  if (params && Object.keys(params).length) {
    hash += `?${new URLSearchParams(params).toString()}`;
  }
  return hash;
}

async function renderRoute(id, params, hash) {
  const mod = pages.get(id);
  if (!mod) return;
  const scope = createScope();
  const ctx = cfg.contextFactory
    ? cfg.contextFactory(scope, params)
    : { scope, params };
  cfg.container.replaceChildren();
  try {
    await mod.render(cfg.container, ctx);
  } catch (err) {
    if (cfg.onError) cfg.onError(err, id);
    else throw err;
  }
  current = { id, mod, scope, ctx, params, hash };
  if (cfg.onChange) cfg.onChange(id, ctx);
}

async function activate() {
  if (!cfg) return;
  let { id, params } = parseHash();
  if (!pages.has(id)) id = cfg.fallback || 'dashboard';

  const guardTarget = cfg.guard ? cfg.guard(id) : null;
  if (guardTarget && guardTarget !== id && pages.has(guardTarget)) {
    const target = buildHash(guardTarget);
    if (location.hash !== target) {
      // 改变 hash 会在下一次 hashchange 中重新进入 activate
      location.hash = target;
      return;
    }
    id = guardTarget;
    params = {};
  }
  if (!pages.has(id)) return;
  if (current && current.id === id && current.hash === location.hash) return;

  if (current) {
    try { if (current.mod.destroy) current.mod.destroy(); } catch (e) { /* 忽略 */ }
    current.scope.destroy(); // 幂等兜底：即使页面忘记清理也确保作用域被销毁
    current = null;
  }
  await renderRoute(id, params, location.hash);
}

/** 跳转到指定页面（可附带 query 参数）。 */
export function navigate(id, params) {
  const target = buildHash(id, params);
  if (location.hash === target) {
    activate();
  } else {
    location.hash = target;
  }
}

/** 以全新作用域重渲当前页面（语言 / 主题切换后调用）。 */
export function reload() {
  if (!cfg) return;
  if (!current) { activate(); return; }
  const { id, params, hash } = current;
  try { if (current.mod.destroy) current.mod.destroy(); } catch (e) { /* 忽略 */ }
  current.scope.destroy();
  current = null;
  renderRoute(id, params, hash);
}

export function currentRoute() {
  return current ? { id: current.id, params: current.params } : null;
}

export function start() {
  if (!cfg) throw new Error('router.configure() 必须在 start() 之前调用');
  hashHandler = () => { activate(); };
  window.addEventListener('hashchange', hashHandler);
  activate();
}

export function stop() {
  if (hashHandler) window.removeEventListener('hashchange', hashHandler);
  hashHandler = null;
  if (current) {
    try { if (current.mod.destroy) current.mod.destroy(); } catch (e) { /* 忽略 */ }
    current.scope.destroy();
    current = null;
  }
}

const router = {
  register, registerAll, configure, navigate, reload,
  currentRoute, start, stop
};
export default router;
