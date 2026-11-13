/**
 * 登录界面（Login View）：
 *  - 纯净透亮白色毛玻璃美学体系（Glassmorphism Card）；
 *  - 动态 5G 终端与量子护盾动态 SVG 视觉中心；
 *  - 顶部快速外观调节（语言切换 / 亮暗玻璃切换）；
 *  - 原生无缝密码显隐交互、触控防抖与加载态；
 *  - 直连后端真实 /api/auth/login 认证接口，密码仅保存在当前会话 sessionStorage。
 */
import { el, icon, button, setBusy } from '../utils/dom.js';
import * as i18n from '../i18n/index.js';

let ctx = null;
let bodyEl = null;

export function render(container, context) {
  ctx = context;

  const screen = el('div', { class: 'login-screen' });

  // 顶部快捷控制胶囊（语言 + 主题）
  const topControls = el('div', { class: 'login-top-bar' },
    button(i18n.getLocale() === 'zh-CN' ? 'EN' : '中文', {
      variant: 'ghost sm',
      icon: 'globe',
      title: ctx.t('app.language'),
      onClick: () => {
        i18n.toggleLocale();
        ctx.navigate('login');
      }
    }),
    button('', {
      variant: 'ghost sm icon-btn',
      icon: document.documentElement.getAttribute('data-theme') === 'dark' ? 'sun' : 'moon',
      title: ctx.t('app.theme'),
      onClick: (ev) => {
        const cur = document.documentElement.getAttribute('data-theme');
        const next = cur === 'dark' ? 'light' : 'dark';
        document.documentElement.setAttribute('data-theme', next);
        try { localStorage.setItem('udx710.theme', next); } catch (e) { /* 忽略 */ }
        const use = ev.currentTarget.querySelector('use');
        if (use) use.setAttribute('href', `/icons/sprite.svg#icon-${next === 'dark' ? 'sun' : 'moon'}`);
      }
    })
  );

  const cardEl = el('div', { class: 'login-card' });

  // 动态 SVG 英雄视觉区
  const heroImg = el('img', {
    class: 'login-hero-img',
    src: '/icons/login-hero.svg',
    alt: '5G-MIFI 动态安全终端标识',
    style: { display: 'block', margin: '0 auto', maxWidth: '220px', height: 'auto' }
  });

  const head = el('div', { class: 'login-card__head' },
    heroImg,
    el('h1', { class: 'login-card__title', text: ctx.t('app.title') }),
    el('p', { class: 'login-card__subtitle', text: '高性能移动通信与基带管理系统' })
  );

  bodyEl = el('div', { class: 'login-card__body' });

  const foot = el('div', { class: 'login-card__foot' },
    icon('shield'),
    el('span', { text: '端到端会话隔离 · 凭证仅限当前会话内存' })
  );

  cardEl.append(head, bodyEl, foot);
  screen.append(topControls, cardEl);
  container.appendChild(screen);

  draw();
}

function draw() {
  if (!bodyEl) return;
  bodyEl.replaceChildren();

  const form = el('form', { class: 'login-form' });

  const pwdInput = el('input', {
    class: 'input login-pwd-input',
    type: 'password',
    name: 'password',
    required: true,
    placeholder: '请输入管理员密码',
    autocomplete: 'current-password',
    style: { flex: '1', paddingRight: '40px' }
  });

  const eyeBtn = button('', {
    variant: 'ghost icon',
    icon: 'eye',
    title: ctx.t('login.password'),
    className: 'login-eye-btn',
    onClick: () => {
      const isPwd = pwdInput.type === 'password';
      pwdInput.type = isPwd ? 'text' : 'password';
      const use = eyeBtn.querySelector('use');
      if (use) use.setAttribute('href', `/icons/sprite.svg#icon-${isPwd ? 'eye-off' : 'eye'}`);
    }
  });

  const inputWrap = el('div', { class: 'login-input-wrap' },
    el('span', { class: 'login-input-icon' }, icon('lock')),
    pwdInput,
    eyeBtn
  );

  const submitBtn = button('登 录', {
    variant: 'primary block',
    type: 'submit',
    icon: 'power',
    className: 'login-submit-btn'
  });

  form.append(
    el('div', { class: 'field' },
      el('label', { class: 'field__label', text: '访问密码 (Password)' }),
      inputWrap
    ),
    submitBtn
  );

  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (!form.reportValidity()) return;
    setBusy(submitBtn, true);
    try {
      await ctx.auth.login(pwdInput.value, ctx.signal);
      ctx.toast.success(ctx.t('login.loginSuccess'));
      ctx.navigate('dashboard');
    } catch (err) {
      ctx.toast.error((err && err.message) || ctx.t('login.loginFailed'));
    } finally {
      setBusy(submitBtn, false);
    }
  });

  bodyEl.append(form);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  bodyEl = null;
}
