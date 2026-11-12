/**
 * 系统设置：账户与会话信息 / 修改密码 / 外观（主题、语言）/ 退出登录。
 * 主题与语言偏好存 localStorage（非凭证）；会话 token 仅存 sessionStorage。
 */
import * as i18n from '../i18n/index.js';
import { el, button, field } from '../utils/dom.js';
import { formatDate, safeText } from '../utils/format.js';
import { passwordsMatch } from '../utils/validation.js';
import {
  getVariantPreference, setVariantPreference, resolveVariant
} from '../components/dialog-variant.js';

const THEME_KEY = 'udx710.theme';

let ctx = null;

function currentTheme() {
  try { return localStorage.getItem(THEME_KEY); } catch (e) { return null; }
}

function setTheme(value) {
  try { localStorage.setItem(THEME_KEY, value); } catch (e) { /* 忽略 */ }
  document.documentElement.setAttribute('data-theme', value);
  const btn = document.getElementById('theme-btn');
  if (btn) {
    const use = btn.querySelector('use');
    if (use) use.setAttribute('href', `/icons/sprite.svg#icon-${value === 'dark' ? 'sun' : 'moon'}`);
  }
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // 账户与会话
  const info = ctx.auth.getTokenInfo();
  const accountCard = ctx.card({ title: '账户与会话', span: 6, icon: 'shield' });
  const kvRow = (k, v) => el('div', { class: 'kv' },
    el('span', { class: 'kv__k', text: k }), el('span', { class: 'kv__v', text: safeText(v) }));
  accountCard.body.append(
    kvRow('登录状态', ctx.auth.isAuthenticated() ? ctx.t('status.active') : ctx.t('status.inactive')),
    kvRow('会话签发', info.issuedAt ? formatDate(info.issuedAt) : '—'),
    kvRow('过期时间', info.expiresAt ? formatDate(info.expiresAt) : '跟随服务端'),
    el('p', { class: 'field__hint', text: '凭证保存在 sessionStorage，关闭标签页即失效；不写入 localStorage。' }));
  grid.appendChild(accountCard.el);

  // 修改密码
  const pwdCard = ctx.card({ title: '修改密码', subtitle: 'POST /api/auth/password', span: 6, icon: 'key' });
  const oldPwd = field({ label: '当前密码', name: 'old_password', type: 'password', placeholder: '当前密码' });
  const newPwd = field({ label: '新密码', name: 'new_password', type: 'password', placeholder: '至少 4 位' });
  const confirmPwd = field({ label: '确认新密码', name: 'confirm_password', type: 'password', placeholder: '再次输入' });
  const form = el('form', {});
  const submit = button(ctx.t('common.save'), { variant: 'primary', type: 'submit', icon: 'save' });
  form.append(oldPwd.wrap, newPwd.wrap, confirmPwd.wrap, submit);
  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (!form.reportValidity()) return;
    if (!passwordsMatch(newPwd.input.value, confirmPwd.input.value)) {
      ctx.toast.error(ctx.t('login.mismatch'));
      return;
    }
    try {
      await ctx.auth.changePassword(oldPwd.input.value, newPwd.input.value, ctx.signal);
      ctx.toast.success(ctx.t('login.changeSuccess'));
      form.reset();
    } catch (e) {
      ctx.toast.error((e && e.message) || ctx.t('login.changeFailed'));
    }
  });
  pwdCard.body.appendChild(form);
  grid.appendChild(pwdCard.el);

  // 外观
  const lookCard = ctx.card({ title: '外观与语言', span: 6, icon: 'settings' });
  const langBtn = button(`${ctx.t('app.language')}：${i18n.getLocale() === 'zh-CN' ? '中文' : 'English'}`, {
    variant: 'outline', icon: 'globe',
    onClick: () => { i18n.toggleLocale(); }
  });
  const themeBtn = button(`${ctx.t('app.theme')}：${currentTheme() === 'dark' ? ctx.t('app.darkMode') : ctx.t('app.lightMode')}`, {
    variant: 'outline', icon: 'sun',
    onClick: (ev) => {
      const next = currentTheme() === 'dark' ? 'light' : 'dark';
      setTheme(next);
      ev.currentTarget.querySelector('.btn__label').textContent =
        `${ctx.t('app.theme')}：${next === 'dark' ? ctx.t('app.darkMode') : ctx.t('app.lightMode')}`;
    }
  });
  lookCard.body.append(el('div', { class: 'row' }, langBtn, themeBtn),
    el('p', { class: 'field__hint', text: '切换语言将立即刷新导航与当前页面。' }));

  // 弹窗形态：三种样式可在任意屏幕上强制切换并即时预览
  const variantOptions = [
    ['auto', '自动（手机底部抽屉 / 桌面居中）'],
    ['center', 'Centered Modal Dialog · 居中对话框'],
    ['bottom-sheet', 'Modal Bottom Sheet · 底部抽屉'],
    ['side-sheet', 'Right Side Sheet · 右侧抽屉']
  ];
  const variantSelect = el('select', {
    class: 'select', id: 'dialog-variant', 'aria-label': '弹窗样式'
  }, ...variantOptions.map(([value, label]) => {
    const opt = el('option', { value, text: label });
    if (value === getVariantPreference()) opt.selected = true;
    return opt;
  }));
  variantSelect.addEventListener('change', () => {
    setVariantPreference(variantSelect.value);
    ctx.toast.success(`弹窗样式：${variantOptions.find(([v]) => v === variantSelect.value)[1]}`);
  });
  const previewBtn = button('预览弹窗', {
    variant: 'outline', icon: 'eye',
    onClick: () => {
      const chosen = variantSelect.value;
      ctx.modal({
        title: `弹窗预览 · ${resolveVariant(chosen)}`,
        variant: chosen,
        content: (body) => body.append(
          el('p', { text: '这是三种弹窗形态的实时预览：桌面默认居中对话框，手机默认底部抽屉（可下滑关闭），日志与详情类页面使用右侧抽屉。' }),
          el('p', { class: 'muted', text: '窄屏下右侧抽屉会自动退化为底部抽屉，保证内容宽度。' })
        ),
        footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
      });
    }
  });
  lookCard.body.append(
    el('div', { class: 'field' },
      el('label', { class: 'field__label', text: '弹窗样式' }), variantSelect),
    el('div', { class: 'row' }, previewBtn),
    el('p', { class: 'field__hint', text: '偏好保存在 sessionStorage，关闭标签页后恢复自动。' })
  );
  grid.appendChild(lookCard.el);

  // 退出登录
  const logoutCard = ctx.card({ title: '会话管理', span: 6, icon: 'log-out' });
  const logoutBtn = button(ctx.t('app.logout'), {
    variant: 'danger', icon: 'log-out',
    onClick: async () => {
      const ok = await ctx.confirm({
        title: ctx.t('app.logout'), message: ctx.t('app.confirmLogoutMsg'), danger: true
      });
      if (!ok) return;
      await ctx.auth.logout();
      ctx.toast.success(ctx.t('app.logout'));
      ctx.navigate('login');
    }
  });
  logoutCard.body.append(el('p', { class: 'muted', text: '退出后需要重新输入密码才能访问管理界面。' }), logoutBtn);
  grid.appendChild(logoutCard.el);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
}
