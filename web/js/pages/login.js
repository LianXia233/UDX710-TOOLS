/**
 * 登录页：仅登录。密保问题功能已按需求整体移除，不再提供自助找回密码。
 * 仅在未登录时可达（由 app.js 的路由守卫保证）。
 * 安全：密码不写入 localStorage（长期凭证不持久化）；登录态由 auth.js 写入 sessionStorage。
 */
import { el, icon, button, setBusy } from '../utils/dom.js';

let ctx = null;
let bodyEl = null;

export function render(container, context) {
  ctx = context;

  const screen = el('div', { class: 'login-screen' });
  const cardEl = el('div', { class: 'login-card' });
  const head = el('div', { class: 'login-card__head' },
    el('div', { class: 'login-card__logo' }, icon('wifi')),
    el('h1', { class: 'login-card__title', text: ctx.t('app.title') }),
    el('p', { class: 'login-card__subtitle', text: ctx.t('login.subtitle') }));
  bodyEl = el('div', { class: 'login-card__body' });
  const foot = el('div', { class: 'login-card__foot' },
    icon('shield'),
    el('span', { text: ctx.t('login.hint') }));
  cardEl.append(head, bodyEl, foot);
  screen.appendChild(cardEl);
  container.appendChild(screen);

  draw();
}

function draw() {
  if (!bodyEl) return;
  bodyEl.replaceChildren();
  drawLogin();
}

function drawLogin() {
  const form = el('form', { class: 'login-form' });
  const pwd = el('input', {
    class: 'input', type: 'password', name: 'password', required: true,
    placeholder: ctx.t('login.passwordPlaceholder'),
    autocomplete: 'current-password', style: { flex: '1' }
  });
  const eyeBtn = button('', {
    variant: 'ghost icon', icon: 'eye', title: ctx.t('login.password'),
    onClick: () => {
      const isPwd = pwd.type === 'password';
      pwd.type = isPwd ? 'text' : 'password';
      const use = eyeBtn.querySelector('use');
      if (use) use.setAttribute('href', `/icons/sprite.svg#icon-${isPwd ? 'eye-off' : 'eye'}`);
    }
  });
  const inputRow = el('div', { class: 'row' }, pwd, eyeBtn);
  const submit = button(ctx.t('login.submit'), { variant: 'primary block', type: 'submit', icon: 'lock' });

  form.append(
    el('div', { class: 'field' },
      el('label', { class: 'field__label', text: ctx.t('login.password') }), inputRow),
    submit
  );
  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (!form.reportValidity()) return;
    setBusy(submit, true);
    try {
      await ctx.auth.login(pwd.value, ctx.signal);
      ctx.toast.success(ctx.t('login.loginSuccess'));
      ctx.navigate('dashboard');
    } catch (err) {
      ctx.toast.error((err && err.message) || ctx.t('login.loginFailed'));
    } finally {
      setBusy(submit, false);
    }
  });

  bodyEl.append(form,
    el('p', { class: 'field__hint', text: '凭证仅保存在当前会话（sessionStorage），不会持久化到 localStorage。' }));
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  bodyEl = null;
}
