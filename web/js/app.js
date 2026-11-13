/**
 * 应用入口：装配 store / router / auth / i18n / 主题 / 实时连接，注册所有页面，
 * 挂载导航，订阅 auth-required 跳转登录，并提供每个页面使用的统一上下文 ctx。
 *
 * 单页入口，纯原生 ES Modules，无框架、无运行时模板编译、无 eval / new Function。
 */
import * as i18n from './i18n/index.js';
import api, { ApiError } from './core/api.js';
import * as auth from './core/auth.js';
import router from './core/router.js';
import * as store from './core/store.js';
import * as events from './core/events.js';
import { realtime } from './core/websocket.js';
import * as watchdog from './core/watchdog.js';
import { registerAction } from './core/plugin-renderer.js';
import { toast } from './components/toast.js';
import confirmDialog, { dangerConfirm } from './components/confirm-dialog.js';
import openModal from './components/modal.js';
import createDataTable from './components/data-table.js';
import { statusBadge, autoBadge, statusLight, toneFor } from './components/status-badge.js';
import * as dom from './utils/dom.js';
import * as format from './utils/format.js';
import * as validation from './utils/validation.js';

// ---- 页面注册 ----
import * as login from './pages/login.js';
import * as dashboard from './pages/dashboard.js';
import * as modem from './pages/modem.js';
import * as sms from './pages/sms.js';
import * as network from './pages/network.js';
import * as traffic from './pages/traffic.js';
import * as plugins from './pages/plugins.js';
import * as system from './pages/system.js';
import * as terminal from './pages/terminal.js';
import * as settings from './pages/settings.js';
import * as apn from './pages/apn.js';
import * as usb from './pages/usb.js';
import * as ipv6Proxy from './pages/ipv6-proxy.js';
import * as rathole from './pages/rathole.js';
import * as update from './pages/update.js';
import * as atDebug from './pages/at-debug.js';
import * as charge from './pages/charge.js';

const PAGES = {
  login, dashboard, modem, sms, network, traffic, plugins, system, terminal,
  settings, apn, usb, ipv6Proxy, rathole, update, atDebug, charge
};

// ---- 导航配置（分组 + 页面 id + 图标 + i18n key） ----
const NAV = [
  { group: 'nav.groupOverview', items: [
    { id: 'dashboard', label: 'nav.dashboard', icon: 'gauge' },
    { id: 'modem', label: 'nav.modem', icon: 'signal' }
  ] },
  { group: 'nav.groupNetwork', items: [
    { id: 'network', label: 'nav.network', icon: 'network' },
    { id: 'traffic', label: 'nav.traffic', icon: 'chart' },
    { id: 'sms', label: 'nav.sms', icon: 'mail' },
    { id: 'apn', label: 'nav.apn', icon: 'globe' }
  ] },
  { group: 'nav.groupServices', items: [
    { id: 'ipv6Proxy', label: 'nav.ipv6Proxy', icon: 'shuffle' },
    { id: 'rathole', label: 'nav.rathole', icon: 'server' },
    { id: 'plugins', label: 'nav.plugins', icon: 'puzzle' }
  ] },
  { group: 'nav.groupSystem', items: [
    { id: 'system', label: 'nav.system', icon: 'cpu' },
    { id: 'update', label: 'nav.update', icon: 'download' },
    { id: 'settings', label: 'nav.settings', icon: 'settings' },
    { id: 'usb', label: 'nav.usb', icon: 'usb' },
    { id: 'charge', label: 'nav.charge', icon: 'battery' }
  ] },
  { group: 'nav.groupTools', items: [
    { id: 'terminal', label: 'nav.terminal', icon: 'terminal' },
    { id: 'atDebug', label: 'nav.atDebug', icon: 'code' }
  ] }
];

const NAV_LABELS = {};
for (const group of NAV) {
  for (const item of group.items) NAV_LABELS[item.id] = item.label;
}

// ---- 统一上下文工厂：每个页面激活时由 router 调用 ----
function makeContext(scope, params) {
  return {
    // 基础服务
    t: (key, p) => i18n.t(key, p),
    api, ApiError, auth, store, events,
    ws: realtime,
    toast,
    confirm: confirmDialog,
    dangerConfirm,
    modal: openModal,
    dataTable: createDataTable,
    statusBadge, autoBadge, statusLight, toneFor,
    // DOM / 工具（展开 dom 模块的所有命名导出）
    ...dom,
    format,
    validation,
    // 导航
    navigate: (id, p) => router.navigate(id, p),
    params: params || {},
    // 生命周期（绑定到当前页面作用域，destroy 时统一清理）
    scope,
    signal: scope.signal,
    onCleanup: scope.onCleanup,
    on: scope.on,
    interval: scope.interval,
    timeout: scope.timeout,
    destroyScope: () => scope.destroy(),
    // 订阅便捷封装（自动随页面销毁退订）
    subscribe(key, fn, owner) {
      const un = store.subscribe(key, fn, owner);
      scope.onCleanup(un);
      return un;
    },
    wsSubscribe(topic, fn, owner) {
      const un = realtime.subscribe(topic, fn, owner);
      scope.onCleanup(un);
      return un;
    },
    eventsOn(type, fn, owner) {
      const un = events.on(type, fn, owner);
      scope.onCleanup(un);
      return un;
    }
  };
}

// ---- 主题 ----
const THEME_KEY = 'udx710.theme';
function savedTheme() {
  try { return localStorage.getItem(THEME_KEY); } catch (e) { return null; }
}
function effectiveTheme() {
  const s = savedTheme();
  if (s === 'light' || s === 'dark') return s;
  return (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) ? 'dark' : 'light';
}
function applyTheme() {
  const s = savedTheme();
  if (s === 'light' || s === 'dark') document.documentElement.setAttribute('data-theme', s);
  else document.documentElement.removeAttribute('data-theme');
  const btn = document.getElementById('theme-btn');
  if (btn) {
    const dark = effectiveTheme() === 'dark';
    btn.replaceChildren(dom.icon(dark ? 'sun' : 'moon'));
    btn.title = dark ? i18n.t('app.lightMode') : i18n.t('app.darkMode');
  }
}
function toggleTheme() {
  const next = effectiveTheme() === 'dark' ? 'light' : 'dark';
  try { localStorage.setItem(THEME_KEY, next); } catch (e) { /* 忽略 */ }
  applyTheme();
}

// ---- 侧边栏 / 顶部栏 ----
let shellEl;
let sidebarEl;

function closeMobileSidebar() {
  shellEl.classList.remove('sidebar-open');
}

function iconBtn(iconName, title, onClick, id) {
  const b = dom.el('button', {
    type: 'button', class: 'icon-btn', title, id: id || ''
  }, dom.icon(iconName));
  b.addEventListener('click', onClick);
  return b;
}

function setActiveNav(id) {
  for (const item of document.querySelectorAll('#nav .nav__item')) {
    item.classList.toggle('is-active', item.getAttribute('data-id') === id);
  }
}

function buildNav() {
  const nav = document.getElementById('nav');
  nav.replaceChildren();
  for (const group of NAV) {
    nav.appendChild(dom.el('div', { class: 'nav__group-title', text: i18n.t(group.group) }));
    for (const item of group.items) {
      const btn = dom.el('button', {
        type: 'button', class: 'nav__item', 'data-id': item.id,
        title: i18n.t(item.label)
      },
      dom.el('span', { class: 'nav__icon' }, dom.icon(item.icon)),
      dom.el('span', { class: 'nav__label', text: i18n.t(item.label) }));
      btn.addEventListener('click', () => {
        router.navigate(item.id);
        closeMobileSidebar();
      });
      nav.appendChild(btn);
    }
  }
  const current = router.currentRoute();
  if (current) setActiveNav(current.id);
}

function buildSidebarFoot() {
  const foot = document.getElementById('sidebar-foot');
  foot.replaceChildren();
  const btn = dom.el('button', {
    type: 'button', class: 'sidebar__toggle', title: i18n.t('app.collapse')
  }, dom.icon('chevron-left'), dom.el('span', { text: i18n.t('app.collapse') }));
  btn.addEventListener('click', () => {
    const collapsed = sidebarEl.classList.toggle('is-collapsed');
    const iconEl = btn.querySelector('.icon');
    if (iconEl) {
      iconEl.replaceChildren();
      const use = document.createElementNS(dom.SVG_NS, 'use');
      use.setAttribute('href', `/icons/sprite.svg#icon-${collapsed ? 'chevron-right' : 'chevron-left'}`);
      iconEl.appendChild(use);
    }
    const label = btn.querySelector('span');
    if (label) label.textContent = collapsed ? i18n.t('app.expand') : i18n.t('app.collapse');
  });
  foot.appendChild(btn);
}

function setConnection(status) {
  const dot = document.getElementById('conn-dot');
  const label = document.getElementById('conn-label');
  const map = {
    open: ['ok', i18n.t('app.online')],
    connecting: ['warning', i18n.t('app.connecting')],
    reconnecting: ['warning', i18n.t('app.connecting')],
    closed: ['muted', i18n.t('app.offline')],
    failed: ['error', i18n.t('app.offline')],
    error: ['error', i18n.t('app.offline')]
  };
  const [tone, text] = map[status] || ['muted', i18n.t('app.offline')];
  if (dot) dot.className = `dot dot--${tone}`;
  if (label) label.textContent = text;
  store.set('connection', status);
}

function buildTopbar() {
  const actions = document.getElementById('topbar-actions');
  actions.replaceChildren();
  const conn = dom.el('span', { class: 'topbar__status status-light' },
    dom.el('i', { class: 'dot dot--muted', id: 'conn-dot' }),
    dom.el('span', { id: 'conn-label', text: i18n.t('app.connecting') }));

  const langBtn = iconBtn('globe', i18n.t('app.language'), () => i18n.toggleLocale());
  const themeBtn = iconBtn('moon', i18n.t('app.theme'), toggleTheme, 'theme-btn');
  const refreshBtn = iconBtn('refresh', i18n.t('app.refresh'), () => router.reload());
  const logoutBtn = iconBtn('log-out', i18n.t('app.logout'), handleLogout);
  const clock = dom.el('div', { class: 'topbar__clock' },
    dom.el('div', { text: i18n.t('common.time') }),
    dom.el('div', { id: 'clock-time', text: '--:--:--' }));

  actions.append(conn, langBtn, themeBtn, refreshBtn, logoutBtn, clock);
  applyTheme();
  setConnection(realtime.getStatus());
}

function updateClock() {
  const node = document.getElementById('clock-time');
  if (node) node.textContent = new Date().toLocaleTimeString();
}

function setTopbarTitle(id) {
  const titleEl = document.getElementById('topbar-title');
  const subEl = document.getElementById('topbar-subtitle');
  const label = NAV_LABELS[id];
  if (titleEl) titleEl.textContent = label ? i18n.t(label) : (id === 'login' ? i18n.t('login.title') : id);
  if (subEl) subEl.textContent = i18n.t('app.subtitle');
}

function applyStaticI18n() {
  document.getElementById('brand-title').textContent = i18n.t('app.title');
  document.getElementById('brand-subtitle').textContent = i18n.t('app.subtitle');
  document.getElementById('menu-btn').setAttribute('aria-label', i18n.t('app.menu'));
  document.title = `${i18n.t('app.title')} · ${i18n.t('app.subtitle')}`;
}

// ---- 认证 ----
function guard(id) {
  const authed = auth.isAuthenticated();
  if (!authed && id !== 'login') return 'login';
  if (authed && id === 'login') return 'dashboard';
  return null;
}

async function checkAuth() {
  if (!auth.isAuthenticated()) {
    store.set('authenticated', false);
    return;
  }
  try {
    const res = await auth.status();
    // 兼容新旧字段：Rust 后端返回 authenticated，旧版/代理可能返回 logged_in。
    if (res && (res.logged_in === false || res.authenticated === false)) {
      auth.clearToken();
      store.set('authenticated', false);
    } else {
      store.set('authenticated', true);
    }
  } catch (e) {
    // 网络异常时保留本地 token，保证离线可用性；401 时 api.js 会派发 auth-required
  }
}

function handleAuthRequired() {
  auth.clearToken();
  toast.warning(i18n.t('toast.unauthorized'));
  realtime.close();
  if (!router.currentRoute() || router.currentRoute().id !== 'login') {
    router.navigate('login');
  }
}

async function handleLogout() {
  const ok = await confirmDialog({
    title: i18n.t('app.logout'),
    message: i18n.t('app.confirmLogoutMsg'),
    confirmText: i18n.t('common.confirm'),
    danger: true
  });
  if (!ok) return;
  await auth.logout();
  realtime.close();
  toast.success(i18n.t('app.logout'));
  router.navigate('login');
}

// ---- 插件宿主动作白名单（少量通用安全动作；plugins.js 会追加更多） ----
function registerPluginActions() {
  registerAction('device.status.refresh', () => {
    toast.info(i18n.t('app.refresh'));
  });
  registerAction('ui.toast', ({ node }) => {
    toast.info((node && node.message) || '插件动作');
  });
}

// ---- 启动 ----
async function boot() {
  shellEl = document.getElementById('shell');
  sidebarEl = document.getElementById('sidebar');

  applyTheme();
  applyStaticI18n();
  buildNav();
  buildSidebarFoot();
  buildTopbar();
  updateClock();
  setInterval(updateClock, 1000);
  registerPluginActions();

  document.getElementById('menu-btn').addEventListener('click', () => {
    shellEl.classList.toggle('sidebar-open');
  });
  document.getElementById('sidebar-backdrop').addEventListener('click', closeMobileSidebar);

  // 实时状态连接状态 -> 顶部状态灯
  realtime.onStatus(setConnection);
  // 登录/登出事件 -> 建立/关闭实时连接
  events.on('auth:login', () => realtime.connect());
  events.on('auth:logout', () => realtime.close());
  // 401 -> 跳转登录
  window.addEventListener('auth-required', handleAuthRequired);
  // 语言切换 -> 重建静态文案并刷新当前页面
  i18n.onChange(() => {
    applyStaticI18n();
    buildNav();
    buildTopbar();
    setTopbarTitle((router.currentRoute() || {}).id || 'dashboard');
    router.reload();
  });

  router.registerAll(PAGES);
  router.configure({
    container: document.getElementById('app'),
    fallback: 'dashboard',
    guard,
    contextFactory: makeContext,
    onChange(id) {
      setActiveNav(id);
      setTopbarTitle(id);
    },
    onError(err, id) {
      // 页面渲染异常不应导致整个应用崩溃
      console.error(`[app] 页面 ${id} 渲染失败`, err);
      toast.error(i18n.t('common.requestFailed'));
    }
  });

  await checkAuth();
  router.start();
  if (auth.isAuthenticated()) realtime.connect();

  // 看门狗：UI 主线程假死检测 + /api/health 后端失联探测与实时通道自愈。
  watchdog.start();
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', boot, { once: true });
} else {
  boot();
}

export default { boot, makeContext };
