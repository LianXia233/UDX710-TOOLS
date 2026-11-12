/**
 * 通用模态框：基于原生 <dialog>，支持三种形态（见 dialog-variant.js）。
 *
 *  variant: 'auto'（默认，手机 Bottom Sheet / 桌面 Centered）| 'center' |
 *           'bottom-sheet' | 'side-sheet'（Right Side Sheet）
 *  content / footer 支持 Node、字符串、函数或按钮描述数组；文本一律 textContent。
 *  返回的 api 提供 close() / body / foot / setTitle() / setVariant()。
 */
import { el, icon, button } from '../utils/dom.js';
import { t } from '../i18n/index.js';
import {
  resolveVariant, applyVariant, addGrabber, dismissDialog
} from './dialog-variant.js';

export function openModal({
  title = '',
  content = null,
  footer = null,
  width = '',
  className = '',
  variant = 'auto',
  dismissible = true,
  onClose = null
} = {}) {
  const resolved = resolveVariant(variant);
  const dialog = el('dialog', { class: `modal ${className}`.trim() });
  applyVariant(dialog, resolved);
  // 用 CSS 变量传宽度，避免内联 max-width 与 sheets.css 的定位规则打架
  if (width) dialog.style.setProperty('--dlg-width', String(width));

  const titleEl = el('h3', { class: 'modal__title', text: String(title) });
  const closeBtn = el('button', {
    type: 'button', class: 'modal__x', 'aria-label': t('common.close')
  }, icon('x'));
  const head = el('div', { class: 'modal__head' }, titleEl, closeBtn);
  const body = el('div', { class: 'modal__body' });
  const foot = el('div', { class: 'modal__foot' });
  const panel = el('div', { class: 'modal__panel' }, head, body);
  dialog.appendChild(panel);

  let closed = false;
  const api = {
    dialog, body, foot,
    variant: resolved,
    close() { dismissDialog(dialog); },
    setTitle(text) { titleEl.textContent = String(text); },
    setVariant(next) {
      for (const v of ['center', 'bottom-sheet', 'side-sheet']) {
        dialog.classList.remove(`dlg--${v}`);
      }
      const r = resolveVariant(next);
      dialog.classList.add(`dlg--${r}`);
      dialog.dataset.variant = r;
      api.variant = r;
      syncGrabber(r);
    }
  };

  let grabber = null;
  function syncGrabber(v) {
    const need = v === 'bottom-sheet';
    if (need && !grabber) grabber = addGrabber(panel, () => api.close());
    else if (!need && grabber) { grabber.remove(); grabber = null; }
  }
  syncGrabber(resolved);

  closeBtn.addEventListener('click', () => api.close());
  dialog.addEventListener('cancel', (ev) => {
    // ESC：统一走退场动画；dismissible=false 时完全拦截
    ev.preventDefault();
    if (dismissible) api.close();
  });
  dialog.addEventListener('close', () => {
    if (closed) return;
    closed = true;
    if (dialog.parentNode) dialog.parentNode.removeChild(dialog);
    if (onClose) onClose();
  });
  dialog.addEventListener('click', (ev) => {
    if (dismissible && ev.target === dialog) api.close();
  });

  // 内容
  if (typeof content === 'function') {
    content(body, api);
  } else if (content instanceof Node) {
    body.appendChild(content);
  } else if (content != null) {
    body.appendChild(el('p', { text: String(content) }));
  }

  // 底部操作区
  if (footer) {
    if (typeof footer === 'function') {
      footer(foot, api);
    } else if (footer instanceof Node) {
      foot.appendChild(footer);
    } else if (Array.isArray(footer)) {
      for (const item of footer) {
        const btn = button(item.label, {
          variant: item.variant || 'ghost',
          icon: item.icon || null,
          onClick: () => {
            if (item.onClick) item.onClick(api);
            if (item.closeOnClick) api.close();
          }
        });
        if (item.disabled) btn.disabled = true;
        foot.appendChild(btn);
      }
    }
    if (foot.childNodes.length) panel.appendChild(foot);
  }

  document.body.appendChild(dialog);
  try {
    dialog.showModal();
  } catch (e) {
    dialog.setAttribute('open', '');
  }
  // 首个焦点给关闭按钮，避免键盘焦点落在背景页面
  try { closeBtn.focus(); } catch (e) { /* 忽略 */ }
  return api;
}

export default openModal;
