/**
 * 登录页：登录 + 「忘记密码 → 密保验证 → 重置密码」流程。
 * 仅在未登录时可达（由 app.js 的路由守卫保证）。
 * 安全：密码不写入 localStorage（长期凭证不持久化）；登录态由 auth.js 写入 sessionStorage。
 */
import { el, icon, button, loadingState, setBusy } from '../utils/dom.js';
import { fieldOf } from '../utils/format.js';

const CONFIRMATION_TEXT = '已知晓风险';
let ctx = null;
let mode = 'login';       // login | loading | notset | verify | reset
let questions = { question1: '', question2: '' };
let answers = { answer1: '', answer2: '' };
let bodyEl = null;

export function render(container, context) {
  ctx = context;
  mode = 'login';
  questions = { question1: '', question2: '' };
  answers = { answer1: '', answer2: '' };

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

function backButton() {
  return button(ctx.t('login.backToLogin'), {
    variant: 'ghost', icon: 'arrow-left',
    onClick: () => { mode = 'login'; draw(); }
  });
}

function draw() {
  if (!bodyEl) return;
  bodyEl.replaceChildren();
  switch (mode) {
    case 'login': drawLogin(); break;
    case 'loading': bodyEl.appendChild(loadingState(ctx.t('common.loading'))); break;
    case 'notset': drawNotSet(); break;
    case 'verify': drawVerify(); break;
    case 'reset': drawReset(); break;
    default: drawLogin();
  }
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
  const forgot = button(ctx.t('login.forgot'), {
    variant: 'ghost', className: 'login-link',
    onClick: () => startRecovery()
  });

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

  bodyEl.append(form, el('div', { class: 'login-forgot' }, forgot),
    el('p', { class: 'field__hint', text: '凭证仅保存在当前会话（sessionStorage），不会持久化到 localStorage。' }));
}

async function startRecovery() {
  mode = 'loading';
  draw();
  try {
    // 先获取密保问题；旧后端返回 { status:'ok', data:{question1,question2} }
    const res = await ctx.api('/api/security/questions', { signal: ctx.signal });
    const data = fieldOf(res, 'data', res) || {};
    const q1 = data.question1 || data.q1 || fieldOf(res, 'question1', '');
    const q2 = data.question2 || data.q2 || fieldOf(res, 'question2', '');
    if (!q1 && !q2) {
      mode = 'notset';
    } else {
      questions = { question1: q1, question2: q2 };
      mode = 'verify';
    }
  } catch (e) {
    // 404 / 未设置 / 网络异常都视为未设置密保，给出明确提示
    mode = 'notset';
  }
  draw();
}

function drawNotSet() {
  bodyEl.append(
    el('div', { class: 'empty' }, icon('alert-triangle', 'empty__icon'),
      el('p', { class: 'empty__text', text: ctx.t('login.securityNotSet') })),
    el('p', { class: 'muted', text: ctx.t('login.securityNotSetDesc') }),
    el('div', { class: 'toolbar' }, backButton())
  );
}

function drawVerify() {
  const form = el('form', {});
  const a1 = el('input', { class: 'input', type: 'text', required: true, placeholder: ctx.t('login.answerPlaceholder') });
  const a2 = el('input', { class: 'input', type: 'text', required: true, placeholder: ctx.t('login.answerPlaceholder') });
  const verifyBtn = button(ctx.t('login.verify'), { variant: 'primary block', type: 'submit' });
  form.append(
    el('h4', { class: 'card__title', text: ctx.t('login.forgotTitle') }),
    el('p', { class: 'muted', text: ctx.t('login.stepVerify') }),
    el('div', { class: 'field' },
      el('label', { class: 'field__label', text: questions.question1 || ctx.t('login.question') }), a1),
    el('div', { class: 'field' },
      el('label', { class: 'field__label', text: questions.question2 || ctx.t('login.question') }), a2),
    verifyBtn
  );
  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (!form.reportValidity()) return;
    answers.answer1 = a1.value.trim();
    answers.answer2 = a2.value.trim();
    setBusy(verifyBtn, true);
    try {
      // 验证接口；若后端未实现（404 等），reset 接口本身会再次校验答案，故容错继续
      try {
        await ctx.api('/api/security/verify', {
          method: 'POST', body: { answer1: answers.answer1, answer2: answers.answer2 },
          signal: ctx.signal
        });
      } catch (verifyErr) {
        // 容错：仅放行「不支持/未找到」，其余错误（如答案错误）仍中断
        if (verifyErr && verifyErr.status && verifyErr.status !== 404 && verifyErr.status !== 405) throw verifyErr;
      }
      mode = 'reset';
      draw();
    } catch (err) {
      ctx.toast.error((err && err.message) || ctx.t('login.verifyFailed'));
    } finally {
      setBusy(verifyBtn, false);
    }
  });
  bodyEl.append(form, el('div', { class: 'toolbar' }, backButton()));
}

function drawReset() {
  const confirmation = el('input', { class: 'input', type: 'text', placeholder: CONFIRMATION_TEXT });
  const resetBtn = button(ctx.t('login.resetPassword'), { variant: 'primary block', icon: 'key' });
  const hint = ctx.t('login.typeToConfirm', { word: CONFIRMATION_TEXT });
  const form = el('form', {});
  form.append(
    el('h4', { class: 'card__title', text: ctx.t('login.forgotTitle') }),
    el('p', { class: 'muted', text: ctx.t('login.stepReset') }),
    el('div', { class: 'alert alert--warning', text: ctx.t('login.resetDesc') }),
    el('div', { class: 'field' },
      el('label', { class: 'field__label', text: hint }), confirmation),
    resetBtn
  );
  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (confirmation.value !== CONFIRMATION_TEXT) {
      ctx.toast.error(ctx.t('login.resetFailed'));
      return;
    }
    setBusy(resetBtn, true);
    try {
      await ctx.api('/api/security/reset-password', {
        method: 'POST',
        body: { answer1: answers.answer1, answer2: answers.answer2, confirm: CONFIRMATION_TEXT },
        signal: ctx.signal
      });
      ctx.toast.success(ctx.t('login.resetSuccess'));
      mode = 'login';
      draw();
    } catch (err) {
      ctx.toast.error((err && err.message) || ctx.t('login.resetFailed'));
    } finally {
      setBusy(resetBtn, false);
    }
  });
  bodyEl.append(form, el('div', { class: 'toolbar' }, backButton()));
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  bodyEl = null;
}
