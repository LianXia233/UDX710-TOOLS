/**
 * IPv6 端口转发：配置 / 转发规则 CRUD / 服务控制（start/stop/restart）/ 定时推送 / 日志。
 * 大表格用 data-table 分页；规则表单使用原生校验（端口 1-65535）。
 */
import { el, button, field, switchField, toolbar } from '../utils/dom.js';
import { fieldOf, safeText, asArray } from '../utils/format.js';
import { isPortValid } from '../utils/validation.js';

let ctx = null;
let ruleTable = null;

function post(path, body) {
  return ctx.api(path, { method: 'POST', body, signal: ctx.signal });
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // 状态
  const statusCard = ctx.card({ title: '运行状态', span: 6, icon: 'activity' });
  const statusLabel = el('span', { class: 'badge badge--muted', text: '读取中…' });
  const ipv6Label = el('span', { class: 'kv__v', text: '—' });
  const startBtn = button(ctx.t('common.start'), { variant: 'primary', icon: 'play', onClick: () => action('start') });
  const stopBtn = button(ctx.t('common.stop'), { variant: 'danger', icon: 'power', onClick: () => action('stop') });
  const restartBtn = button(ctx.t('common.restart'), { variant: 'outline', icon: 'refresh', onClick: () => action('restart') });
  statusCard.body.append(
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '状态' }), statusLabel),
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: 'IPv6 地址' }), ipv6Label),
    el('div', { class: 'row' }, startBtn, stopBtn, restartBtn));
  grid.appendChild(statusCard.el);

  // 配置
  const cfgCard = ctx.card({ title: '转发配置', span: 6, icon: 'settings' });
  const sendEnabled = switchField({ label: '启用定时推送', checked: false });
  const webhookUrl = field({ label: 'Webhook URL', name: 'url', placeholder: 'https://...' });
  const interval = field({ label: '发送间隔（分钟）', name: 'interval', type: 'number', value: '30', attrs: { min: '1' } });
  const saveCfg = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      try {
        await post('/api/ipv6-proxy/config', {
          send_enabled: sendEnabled.input.checked,
          webhook_url: webhookUrl.input.value,
          send_interval: Number(interval.input.value) || 30
        });
        ctx.toast.success(ctx.t('common.saved'));
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const sendNow = button('立即发送', { variant: 'outline', icon: 'send', onClick: () => action('send') });
  const testBtn = button(ctx.t('common.test'), { variant: 'ghost', icon: 'send', onClick: () => action('test') });
  const logsBtn = button('发送日志', {
    variant: 'ghost', icon: 'list',
    onClick: async () => {
      try {
        const res = await ctx.api('/api/ipv6-proxy/send-logs', { signal: ctx.signal });
        const rows = asArray(res, ['logs']);
        ctx.modal({
          variant: 'side-sheet',
          title: 'IPv6 推送日志',
          content: (b) => b.appendChild(el('pre', {
            class: 'mono', style: { whiteSpace: 'pre-wrap', maxHeight: '50vh', overflow: 'auto' },
            text: rows.map((r) => safeText(typeof r === 'string' ? r : JSON.stringify(r))).join('\n') || '暂无记录'
          })),
          footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
        });
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  cfgCard.body.append(sendEnabled.wrap, webhookUrl.wrap, interval.wrap,
    el('div', { class: 'row' }, saveCfg, sendNow, testBtn, logsBtn));
  grid.appendChild(cfgCard.el);

  // 规则
  const ruleCard = ctx.card({ title: '转发规则', span: 12, icon: 'shuffle' });
  ruleCard.head.querySelector('.card__actions').appendChild(
    button('添加规则', { variant: 'primary sm', icon: 'plus', onClick: () => openEditor(null) }));
  ruleTable = ctx.dataTable({
    columns: [
      { label: '本地端口', render: (r) => safeText(r.localPort != null ? r.localPort : r.local_port) },
      { label: 'IPv6 端口', render: (r) => safeText(r.ipv6Port != null ? r.ipv6Port : r.ipv6_port) },
      { label: '状态', render: (r) => ctx.statusBadge(r.enabled ? ctx.t('status.enabled') : ctx.t('status.disabled'), r.enabled ? 'ok' : 'muted') },
      {
        label: ctx.t('common.actions'), align: 'right', render: (r) => [
          button('', { variant: 'ghost sm icon', icon: 'pencil', title: ctx.t('common.edit'), onClick: () => openEditor(r) }),
          button('', {
            variant: 'danger sm icon', icon: 'trash', title: ctx.t('common.delete'),
            onClick: async () => {
              const ok = await ctx.confirm({ message: '删除此转发规则？', danger: true });
              if (!ok || r.id == null) return;
              try {
                await ctx.api(`/api/ipv6-proxy/rules/${encodeURIComponent(r.id)}`, { method: 'DELETE', signal: ctx.signal });
                ctx.toast.success(ctx.t('common.deleted'));
                loadRules();
              } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
            }
          })
        ]
      }
    ], rows: [], emptyText: '暂无转发规则'
  });
  ruleCard.body.appendChild(ruleTable.el);
  grid.appendChild(ruleCard.el);

  function action(name) {
    post(`/api/ipv6-proxy/${name}`, {}).then(() => {
      ctx.toast.success(ctx.t('common.saved'));
      loadStatus();
    }).catch((e) => ctx.toast.error((e && e.message) || ctx.t('common.requestFailed')));
  }

  function openEditor(existing) {
    const f = {
      local: field({ label: '本地端口', name: 'local', value: existing ? safeText(existing.localPort != null ? existing.localPort : existing.local_port, '') : '' }),
      ipv6: field({ label: 'IPv6 端口', name: 'ipv6', value: existing ? safeText(existing.ipv6Port != null ? existing.ipv6Port : existing.ipv6_port, '') : '' }),
      enabled: switchField({ label: '启用规则', checked: existing ? !!existing.enabled : true })
    };
    ctx.modal({
      variant: 'auto',
      title: existing ? '编辑规则' : '添加规则',
      content: (b) => { for (const k of Object.keys(f)) b.appendChild(f[k].wrap); },
      footer: [
        {
          label: ctx.t('common.save'), variant: 'primary', icon: 'save',
          onClick: async (m) => {
            const localPort = Number(f.local.input.value);
            const ipv6Port = Number(f.ipv6.input.value);
            if (!isPortValid(localPort) || !isPortValid(ipv6Port)) {
              ctx.toast.error('端口必须为 1-65535'); return;
            }
            const payload = { localPort, ipv6Port, enabled: f.enabled.input.checked };
            try {
              if (existing && existing.id != null) {
                await ctx.api(`/api/ipv6-proxy/rules/${encodeURIComponent(existing.id)}`, {
                  method: 'PUT', body: payload, signal: ctx.signal
                });
              } else {
                await post('/api/ipv6-proxy/rules', payload);
              }
              ctx.toast.success(ctx.t('common.saved'));
              m.close();
              loadRules();
            } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
          }
        },
        { label: ctx.t('common.cancel'), variant: 'ghost', closeOnClick: true }
      ]
    });
  }

  async function loadStatus() {
    try {
      const res = await ctx.api('/api/ipv6-proxy/status', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const running = !!(o.running !== undefined ? o.running : res.running);
      statusLabel.textContent = running ? ctx.t('status.running') : ctx.t('status.stopped');
      statusLabel.className = `badge ${running ? 'badge--ok' : 'badge--muted'}`;
      ipv6Label.textContent = safeText(o.ipv6 || o.address || res.ipv6, '—');
    } catch (e) { statusLabel.textContent = '未知'; }
  }

  async function loadRules() {
    try {
      const res = await ctx.api('/api/ipv6-proxy/rules', { signal: ctx.signal });
      ruleTable.setRows(asArray(res, ['rules']));
    } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
  }

  (async () => {
    try {
      const res = await ctx.api('/api/ipv6-proxy/config', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      sendEnabled.input.checked = !!(o.send_enabled !== undefined ? o.send_enabled : res.send_enabled);
      webhookUrl.input.value = o.webhook_url || res.webhook_url || '';
      if (o.send_interval != null) interval.input.value = o.send_interval;
    } catch (e) { /* 忽略 */ }
  })();

  loadStatus();
  loadRules();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  ruleTable = null;
}
