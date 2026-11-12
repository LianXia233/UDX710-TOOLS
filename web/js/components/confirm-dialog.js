/**
 * 确认对话框：基于原生 <dialog>，返回 Promise<boolean>，支持三种形态。
 * 危险操作（重启 / 关机 / 恢复出厂 / 切 USB / 发短信 / 锁频段）统一走这里。
 * variant 默认 'auto'：手机为 Bottom Sheet、桌面为 Centered Dialog。
 * 可选 requireText：要求用户输入指定短语后才能确认（用于极高风险操作）。
 * 文本一律 textContent，不注入 HTML。
 */
import { el, icon } from '../utils/dom.js';
import { t } from '../i18n/index.js';
import {
  resolveVariant, applyVariant, addGrabber, dismissDialog
} from './dialog-variant.js';

export function confirmDialog({
  title = t('confirm.title'),
  message = '',
  confirmText = t('confirm.ok'),
  cancelText = t('confirm.cancel'),
  danger = false,
  requireText = null,
  detail = null,
  variant = 'auto',
  dismissible = true
} = {}) {
  return new Promise((resolve) => {
    const dialog = el('dialog', { class: `confirm${danger ? ' confirm--danger' : ''}` });
    applyVariant(dialog, resolveVariant(variant));
    let settled = false;
    let result = false;

    function cleanup() {
      if (dialog.parentNode) dialog.parentNode.removeChild(dialog);
      resolve(result);
    }

    function finish(value) {
      if (settled) return;
      settled = true;
      result = !!value;
      dismissDialog(dialog, cleanup);
    }

    const head = el('div', { class: 'confirm__head' },
      el('span', { class: `confirm__icon${danger ? ' is-danger' : ''}` },
        icon(danger ? 'alert-triangle' : 'help-circle')),
      el('h3', { class: 'confirm__title', text: String(title) }));

    const body = el('div', { class: 'confirm__body' });
    if (message) body.appendChild(el('p', { class: 'confirm__message', text: String(message) }));
    if (detail) body.appendChild(detail);

    let confirmBtn;
    if (requireText != null && requireText !== '') {
      const word = String(requireText);
      const hint = el('p', {
        class: 'confirm__hint',
        text: `请输入「${word}」以确认：`
      });
      const input = el('input', { class: 'input', type: 'text', autocomplete: 'off', placeholder: word });
      body.appendChild(hint);
      body.appendChild(input);
      // 输入正确前禁用确认按钮
      const checker = () => { if (confirmBtn) confirmBtn.disabled = input.value !== word; };
      input.addEventListener('input', checker);
      input.addEventListener('change', checker);
      dialog.__requireInput = input;
      dialog.__requireWord = word;
    }

    const cancelBtn = el('button', {
      type: 'button', class: 'btn btn--ghost', text: String(cancelText)
    });
    cancelBtn.addEventListener('click', () => finish(false));
    confirmBtn = el('button', {
      type: 'button',
      class: `btn ${danger ? 'btn--danger' : 'btn--primary'}`,
      text: String(confirmText)
    });
    if (dialog.__requireInput) confirmBtn.disabled = true;
    confirmBtn.addEventListener('click', () => finish(true));

    const foot = el('div', { class: 'confirm__foot' }, cancelBtn, confirmBtn);
    const panel = el('div', { class: 'confirm__panel' }, head, body, foot);
    dialog.appendChild(panel);

    // Bottom Sheet 的拖拽把手
    if (dialog.dataset.variant === 'bottom-sheet') {
      addGrabber(panel, () => finish(false));
    }

    // ESC / 关闭事件：dismissible=false 时拦截 ESC，仅允许按钮决策
    dialog.addEventListener('cancel', (ev) => {
      ev.preventDefault();
      if (dismissible) finish(false);
    });
    dialog.addEventListener('close', () => finish(false));
    dialog.addEventListener('click', (ev) => {
      if (dismissible && ev.target === dialog) finish(false);
    });

    document.body.appendChild(dialog);
    try {
      dialog.showModal();
    } catch (e) {
      dialog.setAttribute('open', '');
    }
    if (dialog.__requireInput) dialog.__requireInput.focus();
    else confirmBtn.focus();
  });
}

/** 便捷封装：危险操作确认（默认带警告样式）。 */
export function dangerConfirm(message, opts = {}) {
  return confirmDialog({ danger: true, message, ...opts });
}

export default confirmDialog;
