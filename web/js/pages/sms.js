/**
 * 短信管理：收件箱 / 发件箱 / 新建短信 / 配置。
 *  - 大表格用 data-table 分页；发短信属于危险操作，统一走 confirm-dialog 确认。
 *  - 配置包含短信存储、Webhook 转发（含测试与日志）、短信接收修复。
 */
import { el, icon, button, field, switchField, toolbar, loadingState } from '../utils/dom.js';
import { fieldOf, safeText, asArray, truncate, formatDate } from '../utils/format.js';
import { constrain, isPhone } from '../utils/validation.js';

let ctx = null;
let tabContent = null;
let inboxTable = null;
let sentTable = null;

function smsList(res, key) {
  return asArray(res, ['sms', 'messages', key]);
}

function viewDetail(row) {
  ctx.modal({
    title: '短信详情',
    content: (body) => {
      body.appendChild(el('p', { class: 'kv' },
        el('span', { class: 'kv__k', text: '发件人' }),
        el('span', { class: 'kv__v', text: safeText(row.sender || row.address || row.number) })));
      body.appendChild(el('p', { class: 'muted', text: formatDate(row.time || row.date || row.timestamp) }));
      body.appendChild(el('pre', {
        class: 'mono',
        style: { whiteSpace: 'pre-wrap', wordBreak: 'break-word', marginTop: '12px' },
        text: safeText(row.content || row.body || row.message, '')
      }));
    },
    footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
  });
}

async function loadInbox() {
  try {
    const res = await ctx.api('/api/sms', { signal: ctx.signal });
    inboxTable.setRows(smsList(res, 'inbox'));
  } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
}

async function loadSent() {
  try {
    const res = await ctx.api('/api/sms/sent', { signal: ctx.signal });
    sentTable.setRows(smsList(res, 'sent'));
  } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
}

function inboxActions(row) {
  const id = row.id != null ? row.id : row.index;
  return [
    button('', { variant: 'ghost sm icon', icon: 'eye', title: ctx.t('common.view'), onClick: () => viewDetail(row) }),
    button('', {
      variant: 'danger sm icon', icon: 'trash', title: ctx.t('common.delete'),
      onClick: async () => {
        const ok = await ctx.confirm({ message: '删除这条短信？', danger: true });
        if (!ok || id == null) return;
        try {
          await ctx.api(`/api/sms/${encodeURIComponent(id)}`, { method: 'DELETE', signal: ctx.signal });
          ctx.toast.success(ctx.t('common.deleted'));
          loadInbox();
        } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
      }
    })
  ];
}

function sentActions(row) {
  const id = row.id != null ? row.id : row.index;
  return [
    button('', {
      variant: 'danger sm icon', icon: 'trash', title: ctx.t('common.delete'),
      onClick: async () => {
        const ok = await ctx.confirm({ message: '删除这条发送记录？', danger: true });
        if (!ok || id == null) return;
        try {
          await ctx.api(`/api/sms/sent/${encodeURIComponent(id)}`, { method: 'DELETE', signal: ctx.signal });
          ctx.toast.success(ctx.t('common.deleted'));
          loadSent();
        } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
      }
    })
  ];
}

function showInbox() {
  tabContent.replaceChildren();
  tabContent.appendChild(toolbar(
    button(ctx.t('common.refresh'), { variant: 'ghost', icon: 'refresh', onClick: loadInbox })
  ));
  inboxTable = ctx.dataTable({
    columns: [
      { label: '发件人', render: (r) => safeText(r.sender || r.address || r.number) },
      { label: '时间', render: (r) => formatDate(r.time || r.date || r.timestamp) },
      { label: '内容', render: (r) => truncate(r.content || r.body || r.message, 60) },
      { label: ctx.t('common.actions'), align: 'right', render: inboxActions }
    ], rows: [], emptyText: '暂无短信'
  });
  tabContent.appendChild(inboxTable.el);
  loadInbox();
}

function showSent() {
  tabContent.replaceChildren();
  tabContent.appendChild(toolbar(
    button(ctx.t('common.refresh'), { variant: 'ghost', icon: 'refresh', onClick: loadSent })
  ));
  sentTable = ctx.dataTable({
    columns: [
      { label: '收件人', render: (r) => safeText(r.recipient || r.number || r.to) },
      { label: '时间', render: (r) => formatDate(r.time || r.date || r.timestamp) },
      { label: '内容', render: (r) => truncate(r.content || r.message, 60) },
      { label: ctx.t('common.actions'), align: 'right', render: sentActions }
    ], rows: [], emptyText: '暂无发送记录'
  });
  tabContent.appendChild(sentTable.el);
  loadSent();
}

function showCompose() {
  tabContent.replaceChildren();
  const cardEl = ctx.card({ title: '新建短信', span: 12, icon: 'mail' });
  const form = el('form', {});
  const recipientField = field({ label: '收件人', name: 'recipient', placeholder: '请输入手机号码' });
  const recipient = recipientField.input;
  constrain(recipient, { required: true, pattern: '^\\+?[0-9\\-\\s]{5,20}$', message: '请输入有效号码' });
  const content = el('textarea', { class: 'input', name: 'content', rows: 5, required: true, placeholder: '短信内容' });
  const sendBtn = button(ctx.t('common.send'), { variant: 'primary', type: 'submit', icon: 'send' });
  form.append(
    recipientField.wrap,
    el('div', { class: 'field' }, el('label', { class: 'field__label', text: '内容' }), content),
    sendBtn
  );
  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    if (!form.reportValidity()) return;
    if (!isPhone(recipient.value)) { ctx.toast.error(ctx.t('common.invalidInput')); return; }
    // 发短信为危险操作：必须确认
    const ok = await ctx.confirm({
      title: ctx.t('common.confirmOperation'),
      message: `确定发送短信到 ${recipient.value} 吗？`,
      danger: true, confirmText: ctx.t('common.send')
    });
    if (!ok) return;
    try {
      await ctx.api('/api/sms/send', {
        method: 'POST', body: { recipient: recipient.value.trim(), content: content.value },
        signal: ctx.signal
      });
      ctx.toast.success(ctx.t('common.sent'));
      form.reset();
    } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
  });
  cardEl.body.appendChild(form);
  tabContent.appendChild(cardEl.el);
}

function showConfig() {
  tabContent.replaceChildren();

  // 存储配置
  const storeCard = ctx.card({ title: '短信存储配置', span: 6, icon: 'database' });
  const inboxMax = field({ label: '收件箱最大条数', name: 'inbox_max', type: 'number', value: '100' }).input;
  const outboxMax = field({ label: '发件箱最大条数', name: 'outbox_max', type: 'number', value: '50' }).input;
  constrain(inboxMax, { required: true, min: 10, max: 150, step: 1 });
  constrain(outboxMax, { required: true, min: 1, max: 50, step: 1 });
  const saveStore = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      if (!inboxMax.checkValidity() || !outboxMax.checkValidity()) {
        inboxMax.reportValidity(); outboxMax.reportValidity(); return;
      }
      try {
        await ctx.api('/api/sms/config', {
          method: 'POST',
          body: { inbox_max: Number(inboxMax.value), outbox_max: Number(outboxMax.value) },
          signal: ctx.signal
        });
        ctx.toast.success(ctx.t('common.saved'));
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  storeCard.body.append(inboxMax.closest('.field'), outboxMax.closest('.field'), saveStore);

  // Webhook 配置
  const hookCard = ctx.card({ title: 'Webhook 转发', span: 6, icon: 'link' });
  const enabled = switchField({ label: '启用 Webhook 通知', checked: false }).input;
  const url = field({ label: 'Webhook URL', name: 'url', placeholder: 'https://...' }).input;
  const bodyTa = el('textarea', { class: 'input', rows: 3, placeholder: '请求体，支持 #{sender} #{content} #{time}' });
  const headersTa = el('textarea', { class: 'input', rows: 2, placeholder: 'Content-Type: application/json' });
  const testBtn = button(ctx.t('common.test'), {
    variant: 'ghost', icon: 'send',
    onClick: async () => {
      try {
        await ctx.api('/api/sms/webhook/test', { method: 'POST', signal: ctx.signal });
        ctx.toast.success(ctx.t('common.sent'));
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const logsBtn = button('转发日志', {
    variant: 'ghost', icon: 'list',
    onClick: async () => {
      try {
        const res = await ctx.api('/api/sms/webhook/logs', { signal: ctx.signal });
        const rows = asArray(res, ['logs']);
        ctx.modal({
          title: 'Webhook 转发日志',
          content: (b) => b.appendChild(el('pre', {
            class: 'mono', style: { whiteSpace: 'pre-wrap', maxHeight: '50vh', overflow: 'auto' },
            text: rows.map((r) => safeText(typeof r === 'string' ? r : JSON.stringify(r))).join('\n') || '暂无记录'
          })),
          footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
        });
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const saveHook = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      try {
        await ctx.api('/api/sms/webhook', {
          method: 'POST',
          body: { enabled: enabled.checked, url: url.value, body: bodyTa.value, headers: headersTa.value },
          signal: ctx.signal
        });
        ctx.toast.success(ctx.t('common.saved'));
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  hookCard.body.append(
    enabled.closest('.field'),
    url.closest('.field'),
    el('div', { class: 'field' }, el('label', { class: 'field__label', text: '请求体' }), bodyTa),
    el('div', { class: 'field' }, el('label', { class: 'field__label', text: '请求头' }), headersTa),
    el('div', { class: 'row' }, saveHook, testBtn, logsBtn)
  );

  // 短信接收修复
  const fixCard = ctx.card({ title: '修复短信接收', span: 12, icon: 'activity' });
  const fixSwitch = switchField({ label: '开启短信接收修复（开机自动应用）', checked: false }).input;
  fixCard.body.append(fixSwitch.closest('.field'),
    el('p', { class: 'field__hint', text: '开启后通过 AT 命令修复短信接收问题（R106 专用修复）。' }));
  fixSwitch.addEventListener('change', async () => {
    try {
      await ctx.api('/api/sms/fix', {
        method: 'POST', body: { enabled: fixSwitch.checked }, signal: ctx.signal
      });
      ctx.toast.success(ctx.t('common.saved'));
    } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
  });

  tabContent.append(storeCard.el, hookCard.el, fixCard.el);

  // 载入现有配置
  (async () => {
    try {
      const [cfg, hook, fix] = await Promise.all([
        ctx.api('/api/sms/config', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/sms/webhook', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/sms/fix', { signal: ctx.signal }).catch(() => null)
      ]);
      const c = fieldOf(cfg, 'data', cfg) || {};
      if (c.inbox_max != null) inboxMax.value = c.inbox_max;
      if (c.outbox_max != null) outboxMax.value = c.outbox_max;
      const h = fieldOf(hook, 'data', hook) || {};
      enabled.checked = !!(h.enabled);
      url.value = h.url || '';
      bodyTa.value = h.body || '';
      headersTa.value = h.headers || '';
      const f = fieldOf(fix, 'data', fix) || {};
      fixSwitch.checked = !!(f.enabled);
    } catch (e) { /* 配置读取失败不阻断页面 */ }
  })();
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);
  const cardEl = ctx.card({ title: '短信管理', subtitle: '收发与转发配置', span: 12, icon: 'mail' });
  const tabs = el('div', { class: 'tabs' });
  const defs = [
    ['收件箱', showInbox], ['发件箱', showSent], ['新建短信', showCompose], ['配置', showConfig]
  ];
  const buttons = [];
  for (const [label, fn] of defs) {
    const b = button(label, { variant: 'ghost', className: 'tab' });
    b.addEventListener('click', () => {
      for (const x of buttons) x.classList.remove('is-active');
      b.classList.add('is-active');
      fn();
    });
    buttons.push(b);
    tabs.appendChild(b);
  }
  tabContent = el('div', {});
  cardEl.body.append(tabs, tabContent);
  grid.appendChild(cardEl.el);
  buttons[0].classList.add('is-active');
  showInbox();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  tabContent = null;
  inboxTable = null;
  sentTable = null;
}
