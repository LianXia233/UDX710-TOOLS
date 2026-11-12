/**
 * 全局 Toast：单例容器，自动消失，限制同时可见数量。
 * 文本一律 textContent，不解析 HTML。
 */
import { el } from '../utils/dom.js';

const MAX_VISIBLE = 4;
let host = null;

function ensureHost() {
  if (host) return host;
  host = el('div', {
    class: 'toast-stack', id: 'toast-root', role: 'status', 'aria-live': 'polite'
  });
  document.body.appendChild(host);
  return host;
}

function show(message, type = 'info', duration = 3500) {
  const root = ensureHost();
  const node = el('div', {
    class: `toast toast--${type}`, role: 'alert'
  }, el('span', { class: 'toast__dot' }),
  el('span', { class: 'toast__msg', text: String(message == null ? '' : message) }));
  root.appendChild(node);

  // 超过上限时移除最旧的一条
  while (root.children.length > MAX_VISIBLE) {
    root.removeChild(root.firstChild);
  }

  const timer = setTimeout(() => {
    node.classList.add('toast--leaving');
    setTimeout(() => { if (node.parentNode) node.parentNode.removeChild(node); }, 200);
  }, duration);

  return () => {
    clearTimeout(timer);
    if (node.parentNode) node.parentNode.removeChild(node);
  };
}

export const toast = {
  show,
  success: (msg, d) => show(msg, 'success', d),
  error: (msg, d) => show(msg, 'error', d),
  warning: (msg, d) => show(msg, 'warning', d),
  info: (msg, d) => show(msg, 'info', d)
};

export default toast;
