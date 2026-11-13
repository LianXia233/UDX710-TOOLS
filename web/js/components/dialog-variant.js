/**
 * 弹窗形态（Dialog Variants）统一解析与生命周期。
 *
 * 三种形态：
 *  - center        Centered Modal Dialog：桌面居中对话框
 *  - bottom-sheet  Modal Bottom Sheet：从底部滑入，带拖拽把手，手机默认
 *  - side-sheet    Right Side Sheet：从右侧滑入、全高，适合日志 / 详情 / 长表单
 *  - auto          按视口宽度自动选择（<=760px -> bottom-sheet，否则 center）
 *
 * 优先级：页面显式 variant > 「设置」页用户偏好 > auto 视口规则。
 * 窄屏下 side-sheet 自动退化为 bottom-sheet，避免内容被压成一条。
 *
 * 严格 CSP：不使用 innerHTML / 内联脚本；仅 DOM API 与 CSSOM（element.style）。
 */
import { el } from '../utils/dom.js';

export const SHEET_BREAKPOINT = 760;
export const VARIANTS = ['center', 'bottom-sheet', 'side-sheet'];
const PREF_KEY = 'udx.dialog.variant';

function readStore() {
  try {
    return typeof sessionStorage !== 'undefined' ? sessionStorage : null;
  } catch (e) {
    return null;
  }
}

/** 用户偏好：'auto' | 'center' | 'bottom-sheet' | 'side-sheet'。 */
export function getVariantPreference() {
  const s = readStore();
  if (!s) return 'auto';
  const v = s.getItem(PREF_KEY);
  return v && (v === 'auto' || VARIANTS.includes(v)) ? v : 'auto';
}

export function setVariantPreference(value) {
  const s = readStore();
  if (!s) return;
  const v = value === 'auto' || VARIANTS.includes(value) ? value : 'auto';
  if (v === 'auto') s.removeItem(PREF_KEY);
  else s.setItem(PREF_KEY, v);
}

function isNarrow() {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia(`(max-width: ${SHEET_BREAKPOINT}px)`).matches;
}

/** 解析最终形态。 */
export function resolveVariant(preferred = 'auto') {
  const p = String(preferred || 'auto');
  const pref = getVariantPreference();
  let variant;
  if (p !== 'auto' && VARIANTS.includes(p)) variant = p;
  else if (pref !== 'auto' && VARIANTS.includes(pref)) variant = pref;
  else variant = isNarrow() ? 'bottom-sheet' : 'center';
  if (variant === 'side-sheet' && isNarrow()) variant = 'bottom-sheet';
  return variant;
}

/** 给 <dialog> 打上形态类，供 sheets.css 定位与动画。 */
export function applyVariant(dialog, variant) {
  dialog.classList.add('dlg', `dlg--${variant}`);
  dialog.dataset.variant = variant;
}

/**
 * Bottom Sheet 顶部把手：点击或下滑超过阈值关闭。
 * 拖动位移写入 CSS 变量 --sheet-drag，由 CSS 负责 transform。
 */
export function addGrabber(panel, onDismiss) {
  const bar = el('button', {
    type: 'button',
    class: 'dlg__grabber',
    'aria-label': '向下拖动或点击关闭',
    title: '向下拖动或点击关闭'
  }, el('span', { class: 'dlg__grabber-bar' }));
  panel.insertBefore(bar, panel.firstChild);

  let startY = 0;
  let dragging = false;
  const owner = () => panel.closest('dialog');

  bar.addEventListener('touchstart', (e) => {
    const t = e.touches && e.touches[0];
    if (!t) return;
    startY = t.clientY;
    dragging = true;
    const dlg = owner();
    if (dlg) dlg.classList.add('is-dragging');
  }, { passive: true });

  bar.addEventListener('touchmove', (e) => {
    if (!dragging) return;
    const t = e.touches && e.touches[0];
    if (!t) return;
    const dy = Math.max(0, t.clientY - startY);
    const dlg = owner();
    if (dlg) dlg.style.setProperty('--sheet-drag', `${dy}px`);
  }, { passive: true });

  const end = () => {
    if (!dragging) return;
    dragging = false;
    const dlg = owner();
    const dy = dlg ? parseFloat(dlg.style.getPropertyValue('--sheet-drag')) || 0 : 0;
    if (dlg) {
      dlg.classList.remove('is-dragging');
      dlg.style.removeProperty('--sheet-drag');
    }
    if (dy > 110 && typeof onDismiss === 'function') onDismiss();
  };
  bar.addEventListener('touchend', end, { passive: true });
  bar.addEventListener('touchcancel', end, { passive: true });
  bar.addEventListener('click', () => {
    if (typeof onDismiss === 'function') onDismiss();
  });
  return bar;
}

/**
 * 跟踪对话框的可视视口与软键盘高度，写入 --dlg-vvh / --dlg-kb。
 *
 * iOS Safari 打开键盘时 layout viewport 不缩放，仅 visualViewport 缩小；
 * 通过底部偏移让 Bottom Sheet 始终贴在键盘上方，并保证居中弹窗不越界。
 * 同时锁定页面滚动，关闭后自动释放。
 */
const openDialogs = new Set();

export function bindDialogLayout(dialog) {
  if (!dialog) return () => {};
  openDialogs.add(dialog);
  document.documentElement.classList.add('dlg-open');

  const sync = () => {
    const vv = typeof window !== 'undefined' ? window.visualViewport : null;
    const vh = vv ? vv.height : window.innerHeight;
    const kb = vv ? Math.max(0, window.innerHeight - vv.height - vv.offsetTop) : 0;
    dialog.style.setProperty('--dlg-vvh', `${Math.round(vh)}px`);
    dialog.style.setProperty('--dlg-kb', `${Math.round(kb)}px`);
  };
  sync();

  let raf = 0;
  const onViewportChange = () => {
    if (typeof cancelAnimationFrame === 'function') cancelAnimationFrame(raf);
    raf = typeof requestAnimationFrame === 'function'
      ? requestAnimationFrame(sync)
      : setTimeout(sync, 16);
  };

  const vv = typeof window !== 'undefined' ? window.visualViewport : null;
  if (vv) {
    vv.addEventListener('resize', onViewportChange);
    vv.addEventListener('scroll', onViewportChange);
  }
  window.addEventListener('orientationchange', onViewportChange);

  let released = false;
  const release = () => {
    if (released) return;
    released = true;
    if (typeof cancelAnimationFrame === 'function') cancelAnimationFrame(raf);
    if (vv) {
      vv.removeEventListener('resize', onViewportChange);
      vv.removeEventListener('scroll', onViewportChange);
    }
    window.removeEventListener('orientationchange', onViewportChange);
    openDialogs.delete(dialog);
    if (!openDialogs.size) document.documentElement.classList.remove('dlg-open');
  };
  dialog.addEventListener('close', release, { once: true });
  return release;
}

/**
 * 带退场动画地关闭 dialog。动画结束（或 240ms 兜底）后调用 dialog.close()，
 * 由各组件自身的 close 事件负责 DOM 清理与 Promise resolve。
 * reduced-motion 或动画被禁用时立即关闭。
 */
export function dismissDialog(dialog, done) {
  const finish = () => {
    if (dialog) {
      try { dialog.close(); } catch (e) { /* 已关闭 */ }
    }
    if (typeof done === 'function') done();
  };
  if (!dialog || !dialog.open) { finish(); return; }
  if (dialog.dataset.closing === '1') return;
  dialog.dataset.closing = '1';

  const reduced = typeof window !== 'undefined' && window.matchMedia
    && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (reduced) { finish(); return; }

  dialog.classList.add('is-closing');
  let fired = false;
  const once = () => {
    if (fired) return;
    fired = true;
    finish();
  };
  dialog.addEventListener('animationend', once, { once: true });
  setTimeout(once, 240);
}

export default { resolveVariant, applyVariant, addGrabber, dismissDialog, getVariantPreference, setVariantPreference, VARIANTS };
