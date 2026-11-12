/**
 * 密保中心：状态 / 设置密保问题 / 验证 / 重置密码 / 恢复出厂设置。
 * 恢复出厂设置属于最高风险操作：走 confirm-dialog + requireText 二次确认。
 */
import { el, button, field, selectField } from '../utils/dom.js';
import { fieldOf, safeText } from '../utils/format.js';

const PRESET_QUESTIONS = [
  '你的出生地是？', '你小时候最好的朋友叫什么名字？', '你最喜欢的电影是？',
  '你母亲的名字是？', '你第一只宠物的名字是？', '你小学的名字是？'
];

let ctx = null;
let statusLabel = null;
let setupBox = null;
let dangerBox = null;

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const statusCard = ctx.card({ title: '密保状态', span: 12, icon: 'shield' });
  statusLabel = el('span', { class: 'badge badge--muted', text: '读取中…' });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: load });
  statusCard.head.querySelector('.card__actions').appendChild(refreshBtn);
  statusCard.body.append(el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '当前状态' }), statusLabel));
  grid.appendChild(statusCard.el);

  setupBox = el('div', { class: 'col-span-6' });
  dangerBox = el('div', { class: 'col-span-6' });
  grid.append(setupBox, dangerBox);

  load();
}

function questionSelect(label, name) {
  return selectField({
    label, name, options: PRESET_QUESTIONS.map((q) => ({ value: q, label: q }))
  });
}

function drawSetup() {
  const cardEl = ctx.card({ title: '设置密保问题', subtitle: '设置后不可更改（除非恢复出厂）', span: 12, icon: 'key' });
  const q1 = questionSelect('密保问题一', 'q1');
  const a1 = field({ label: '答案一', name: 'a1', placeholder: '请输入答案' });
  const q2 = questionSelect('密保问题二', 'q2');
  const a2 = field({ label: '答案二', name: 'a2', placeholder: '请输入答案' });
  const save = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      const v1 = a1.input.value.trim();
      const v2 = a2.input.value.trim();
      if (!v1 || !v2 || q1.input.value === q2.input.value) {
        ctx.toast.error('请完整填写且两个问题不能相同'); return;
      }
      try {
        await ctx.api('/api/security/setup', {
          method: 'POST',
          body: { question1: q1.input.value, answer1: v1, question2: q2.input.value, answer2: v2 },
          signal: ctx.signal
        });
        ctx.toast.success(ctx.t('common.saved'));
        load();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  cardEl.body.append(el('div', { class: 'alert alert--warning', text: '请牢记密保答案，遗忘后将无法找回密码。' }),
    q1.wrap, a1.wrap, q2.wrap, a2.wrap, save);
  setupBox.replaceChildren(cardEl.el);
}

function drawDanger() {
  const cardEl = ctx.card({ title: '危险操作', span: 12, icon: 'alert-triangle' });
  const verifyBtn = button('验证密保', {
    variant: 'outline', icon: 'shield', onClick: openVerify
  });
  const resetBtn = button('重置密码', {
    variant: 'outline', icon: 'key', onClick: openReset
  });
  const factoryBtn = button('恢复出厂设置', {
    variant: 'danger', icon: 'alert-triangle', onClick: openFactoryReset
  });
  cardEl.body.append(
    el('div', { class: 'alert alert--danger', text: '恢复出厂将清除所有数据与配置（网络、短信等），且无法恢复。' }),
    el('div', { class: 'row' }, verifyBtn, resetBtn, factoryBtn));
  dangerBox.replaceChildren(cardEl.el);
}

function answerFields() {
  const a1 = field({ label: '密保答案一', name: 'answer1', placeholder: '答案一' });
  const a2 = field({ label: '密保答案二', name: 'answer2', placeholder: '答案二' });
  return { a1, a2, values: () => ({ answer1: a1.input.value.trim(), answer2: a2.input.value.trim() }) };
}

function openVerify() {
  const f = answerFields();
  ctx.modal({
    title: '验证密保',
    content: (b) => b.append(f.a1.wrap, f.a2.wrap),
    footer: [
      {
        label: ctx.t('common.submit'), variant: 'primary',
        onClick: async (m) => {
          try {
            await ctx.api('/api/security/verify', { method: 'POST', body: f.values(), signal: ctx.signal });
            ctx.toast.success('验证通过');
            m.close();
          } catch (e) { ctx.toast.error(ctx.t('login.verifyFailed')); }
        }
      },
      { label: ctx.t('common.cancel'), variant: 'ghost', closeOnClick: true }
    ]
  });
}

function openReset() {
  const f = answerFields();
  const confirmInput = field({ label: '请输入「已知晓风险」以确认', name: 'confirm' });
  ctx.modal({
    title: '重置密码为默认值',
    content: (b) => b.append(
      el('p', { class: 'alert alert--warning', text: '验证通过后密码将重置为 admin。' }),
      f.a1.wrap, f.a2.wrap, confirmInput.wrap),
    footer: [
      {
        label: ctx.t('common.confirm'), variant: 'primary',
        onClick: async (m) => {
          if (confirmInput.input.value !== '已知晓风险') { ctx.toast.error('确认文字不正确'); return; }
          try {
            await ctx.api('/api/security/reset-password', {
              method: 'POST', body: { ...f.values(), confirm: '已知晓风险' }, signal: ctx.signal
            });
            ctx.toast.success(ctx.t('login.resetSuccess'));
            m.close();
          } catch (e) { ctx.toast.error(ctx.t('login.resetFailed')); }
        }
      },
      { label: ctx.t('common.cancel'), variant: 'ghost', closeOnClick: true }
    ]
  });
}

function openFactoryReset() {
  const f = answerFields();
  ctx.confirm({
    title: '恢复出厂设置',
    message: '此操作将清除所有数据与配置且无法恢复，确定继续？',
    danger: true, requireText: 'FACTORY'
  }).then(async (ok) => {
    if (!ok) return;
    ctx.modal({
      title: '验证密保以恢复出厂',
      content: (b) => b.append(f.a1.wrap, f.a2.wrap),
      footer: [
        {
          label: '确认恢复出厂', variant: 'danger',
          onClick: async (m) => {
            try {
              await ctx.api('/api/security/factory-reset', {
                method: 'POST', body: { ...f.values(), confirm: 'FACTORY' }, signal: ctx.signal
              });
              ctx.toast.success('恢复出厂成功，页面即将刷新');
              m.close();
            } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
          }
        },
        { label: ctx.t('common.cancel'), variant: 'ghost', closeOnClick: true }
      ]
    });
  });
}

async function load() {
  try {
    const res = await ctx.api('/api/security/status', { signal: ctx.signal });
    const o = fieldOf(res, 'data', res) || {};
    const isSet = !!(o.is_set !== undefined ? o.is_set : res.is_set);
    statusLabel.textContent = isSet ? '已设置密保' : '未设置密保';
    statusLabel.className = `badge ${isSet ? 'badge--ok' : 'badge--warning'}`;
    setupBox.replaceChildren();
    if (!isSet) drawSetup();
    drawDanger();
  } catch (e) {
    statusLabel.textContent = '状态未知';
    drawDanger();
  }
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  statusLabel = null;
  setupBox = null;
  dangerBox = null;
}
