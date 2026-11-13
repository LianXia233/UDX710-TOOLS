/**
 * 系统设置：账户与会话信息 / 修改密码 / 外观（白色毛玻璃主题、语言）/
 * 全局弹窗样式（Centered Modal Dialog / Modal Bottom Sheet / Right Side Sheet）/ 退出登录。
 *
 * 凭证严格保存在 sessionStorage，不落 localStorage；外观及弹窗偏好持久化。
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
  const accountCard = ctx.card({ title: '账户安全与会话凭证', span: 6, icon: 'shield' });
  const kvRow = (k, v) => el('div', { class: 'kv' },
    el('span', { class: 'kv__k', text: k }), el('span', { class: 'kv__v', text: safeText(v) }));
  accountCard.body.append(
    kvRow('当前登录状态', ctx.auth.isAuthenticated() ? '已认证 (Active)' : '未登录'),
    kvRow('会话签发时间', info.issuedAt ? formatDate(info.issuedAt) : '—'),
    kvRow('会话有效期限', info.expiresAt ? formatDate(info.expiresAt) : '跟随服务端安全策略'),
    el('p', { class: 'field__hint', text: '安全机制：令牌保存在 sessionStorage，标签页关闭即刻作废，严禁写入未加密本地磁盘。' }));
  grid.appendChild(accountCard.el);

  // 修改管理员密码
  const pwdCard = ctx.card({ title: '修改管理员密码', subtitle: '更新 WebUI 管理控制台访问密码', span: 6, icon: 'key' });
  const oldPwd = field({ label: '当前原密码', name: 'old_password', type: 'password', placeholder: '请输入当前旧密码' });
  const newPwd = field({ label: '新登录密码', name: 'new_password', type: 'password', placeholder: '至少 4 位字符' });
  const confirmPwd = field({ label: '确认新密码', name: 'confirm_password', type: 'password', placeholder: '请再次输入新密码' });
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

  // 外观与主题
  const lookCard = ctx.card({ title: '界面美化与国际化', subtitle: '白色高质感毛玻璃主题切换', span: 6, icon: 'settings' });
  const langBtn = button(`${ctx.t('app.language')}：${i18n.getLocale() === 'zh-CN' ? '简体中文 (zh-CN)' : 'English (en-US)'}`, {
    variant: 'outline', icon: 'globe',
    onClick: () => { i18n.toggleLocale(); }
  });
  const themeBtn = button(`${ctx.t('app.theme')}：${currentTheme() === 'dark' ? '深色玻璃模式' : '亮白毛玻璃模式'}`, {
    variant: 'outline', icon: 'sun',
    onClick: (ev) => {
      const next = currentTheme() === 'dark' ? 'light' : 'dark';
      setTheme(next);
      ev.currentTarget.querySelector('.btn__label').textContent =
        `${ctx.t('app.theme')}：${next === 'dark' ? '深色玻璃模式' : '亮白毛玻璃模式'}`;
    }
  });
  lookCard.body.append(
    el('div', { class: 'row', style: { marginBottom: '12px' } }, langBtn, themeBtn),
    el('p', { class: 'field__hint', text: '支持纯净亮白毛玻璃与深空夜览双重视觉质感，多层柔和光晕，不伤眼。' })
  );
  grid.appendChild(lookCard.el);

  // 全局弹窗样式体系 (Centered Modal Dialog / Modal Bottom Sheet / Right Side Sheet)
  const modalCard = ctx.card({
    title: '全局弹窗样式体系 (Modal Variants)',
    subtitle: '多形态交互架构 · 适配不同屏幕与操作习惯',
    span: 6,
    icon: 'activity'
  });

  const variantOptions = [
    ['auto', '智能自动（移动设备使用 Bottom Sheet / 宽屏使用 Centered Dialog）'],
    ['center', 'Centered Modal Dialog · 居中悬浮对话框'],
    ['bottom-sheet', 'Modal Bottom Sheet · 底部抽屉（带下拉关闭手势）'],
    ['side-sheet', 'Right Side Sheet · 右侧全高抽屉（适宜长日志/深度检查）']
  ];
  const variantSelect = el('select', {
    class: 'select', id: 'dialog-variant', 'aria-label': '全局弹窗形态选择'
  }, ...variantOptions.map(([value, label]) => {
    const opt = el('option', { value, text: label });
    if (value === getVariantPreference()) opt.selected = true;
    return opt;
  }));

  variantSelect.addEventListener('change', () => {
    setVariantPreference(variantSelect.value);
    ctx.toast.success(`全局弹窗偏好已切换为：${variantOptions.find(([v]) => v === variantSelect.value)[1]}`);
  });

  // 三形态一键测试按钮组
  const testCenterBtn = button('居中对话框 (Center)', {
    variant: 'sm outline', icon: 'eye',
    onClick: () => {
      ctx.modal({
        title: 'Centered Modal Dialog · 居中对话框预览',
        variant: 'center',
        content: (body) => body.append(
          el('p', { text: '✨ 这是标准的 Centered Modal Dialog 居中对话框形态：' }),
          el('p', { class: 'muted', style: { marginTop: '8px' }, text: '采用晶透白色毛玻璃面板、柔和圆角（26px）、高深度弥散阴影与微弹跳入场动效。最适合操作确认、提示警告以及标准表单编辑。' })
        ),
        footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
      });
    }
  });

  const testSheetBtn = button('底部抽屉 (Bottom Sheet)', {
    variant: 'sm outline', icon: 'download',
    onClick: () => {
      ctx.modal({
        title: 'Modal Bottom Sheet · 底部抽屉预览',
        variant: 'bottom-sheet',
        content: (body) => body.append(
          el('p', { text: '📱 这是专为触控与移动端优化的 Modal Bottom Sheet 底部抽屉形态：' }),
          el('p', { class: 'muted', style: { marginTop: '8px' }, text: '从屏幕底部平滑升起，支持顶部 Grabber 物理触摸向下滑动关闭，底部自动适配全面屏安全区 (safe-area-inset-bottom)。单手操作极佳！' })
        ),
        footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
      });
    }
  });

  const testSideBtn = button('右侧抽屉 (Right Side Sheet)', {
    variant: 'sm outline', icon: 'chevron-right',
    onClick: () => {
      ctx.modal({
        title: 'Right Side Sheet · 右侧全高抽屉预览',
        variant: 'side-sheet',
        content: (body) => body.append(
          el('p', { text: '📑 这是专为深度排查与长篇阅读打造的 Right Side Sheet 右侧全高抽屉形态：' }),
          el('p', { class: 'muted', style: { marginTop: '8px' }, text: '从屏幕右边缘滑入，全高 (100dvh) 视口，适合浏览系统审计日志、Rathole 穿透状态、AT 历史回显以及细粒度配置清单。' }),
          el('pre', { class: 'mono alert', style: { marginTop: '12px' }, text: '示例日志：\n[INFO] 5G Sub-6GHz carrier aggregated.\n[INFO] RSRP: -78 dBm, SINR: 26 dB\n[INFO] Realtime WebSocket connection healthy.' })
        ),
        footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
      });
    }
  });

  modalCard.body.append(
    el('div', { class: 'field' },
      el('label', { class: 'field__label', text: '全局弹窗策略偏好' }),
      variantSelect
    ),
    el('p', { class: 'field__label', style: { marginTop: '14px', marginBottom: '8px' }, text: '即时预览三种弹窗形态：' }),
    el('div', { class: 'row', style: { gap: '8px', flexWrap: 'wrap' } },
      testCenterBtn, testSheetBtn, testSideBtn
    ),
    el('p', { class: 'subtle', style: { marginTop: '12px' }, text: '三种形态均基于原生 <dialog> 标准，在任意设备上都支持 ESC 关闭、背景点击关闭与流畅退出动画。' })
  );
  grid.appendChild(modalCard.el);

  // 退出登录
  const logoutCard = ctx.card({ title: '会话终结与登出', span: 12, icon: 'log-out' });
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
  logoutCard.body.append(
    el('p', { class: 'muted', style: { marginBottom: '12px' }, text: '登出后将立即注销服务器 Bearer 会话令牌并清除本地临时会话，需要重新输入管理密码方可登录。' }),
    logoutBtn
  );
  grid.appendChild(logoutCard.el);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
}
