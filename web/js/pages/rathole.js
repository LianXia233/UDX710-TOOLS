/**
 * Rathole 内网穿透：客户端配置 / 穿透服务 CRUD / 启停 / 运行状态 / 日志 / 服务端配置 / 开机自启。
 *
 * 安全约定：
 *  - 穿透密钥（token）不在列表、日志、服务端配置弹窗中回显；
 *  - 编辑服务时密钥输入框留空表示「保持原值」，仅当用户主动输入才写入；
 *  - 所有请求走 ctx.api（自动附带 Token、AbortSignal、超时），写操作由后端做权限校验。
 */
import { el, button, field, switchField } from '../utils/dom.js';
import { fieldOf, safeText, asArray } from '../utils/format.js';

let ctx = null;
let serviceTable = null;

function post(path, body) {
  return ctx.api(path, { method: 'POST', body, signal: ctx.signal });
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // ---------------- 运行状态 ----------------
  const statusCard = ctx.card({ title: '运行状态', span: 6, icon: 'activity' });
  const statusLabel = el('span', { class: 'badge badge--muted', text: '读取中…' });
  const hint = el('span', { class: 'kv__v', text: '—' });
  const startBtn = button(ctx.t('common.start'), {
    variant: 'primary', icon: 'play', onClick: () => action('start')
  });
  const stopBtn = button(ctx.t('common.stop'), {
    variant: 'danger', icon: 'power', onClick: () => action('stop')
  });
  const refreshBtn = button(ctx.t('common.refresh'), {
    variant: 'outline', icon: 'refresh', onClick: () => { loadStatus(); loadServices(); }
  });
  statusCard.body.append(
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '进程状态' }), statusLabel),
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '说明' }), hint),
    el('div', { class: 'row' }, startBtn, stopBtn, refreshBtn)
  );
  grid.appendChild(statusCard.el);

  // ---------------- 客户端配置 ----------------
  const cfgCard = ctx.card({ title: '客户端配置', span: 6, icon: 'settings' });
  const serverAddr = field({
    label: '服务端地址', name: 'server_addr', placeholder: 'host:port / https://...'
  });
  const secret = field({
    label: '穿透密钥', name: 'token', type: 'password', placeholder: '留空表示不修改'
  });
  const enabled = switchField({ label: '启用 Rathole', checked: false });
  const autoStart = switchField({ label: '开机自启', checked: false });
  autoStart.input.addEventListener('change', () => {
    post('/api/rathole/autostart', { auto_start: autoStart.input.checked ? 1 : 0 })
      .then(() => ctx.toast.success(ctx.t('common.saved')))
      .catch(() => {
        autoStart.input.checked = !autoStart.input.checked;
        ctx.toast.error(ctx.t('common.requestFailed'));
      });
  });
  const saveCfg = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      const addr = serverAddr.input.value.trim();
      if (addr && !/^[A-Za-z0-9._:\-]+(\/[^\s]*)?$/.test(addr) && !/^https?:\/\/[^\s]+$/.test(addr)) {
        ctx.toast.error('服务端地址格式不合法');
        return;
      }
      const body = {
        server_addr: addr,
        enabled: enabled.input.checked ? 1 : 0,
        auto_start: autoStart.input.checked ? 1 : 0
      };
      // 密钥为空时不覆盖已有值
      if (secret.input.value) body.token = secret.input.value;
      try {
        await post('/api/rathole/config', body);
        secret.input.value = '';
        ctx.toast.success(ctx.t('common.saved'));
      } catch (e) {
        ctx.toast.error(ctx.t('common.requestFailed'));
      }
    }
  });
  const serverCfgBtn = button('服务端配置', {
    variant: 'ghost', icon: 'file-text', onClick: openServerConfig
  });
  const logsBtn = button('运行日志', { variant: 'ghost', icon: 'list', onClick: openLogs });
  cfgCard.body.append(
    serverAddr.wrap, secret.wrap, enabled.wrap, autoStart.wrap,
    el('div', { class: 'row' }, saveCfg, serverCfgBtn, logsBtn)
  );
  grid.appendChild(cfgCard.el);

  // ---------------- 穿透服务 ----------------
  const serviceCard = ctx.card({ title: '穿透服务', span: 12, icon: 'server' });
  serviceCard.head.querySelector('.card__actions').appendChild(
    button('添加服务', { variant: 'primary sm', icon: 'plus', onClick: () => openEditor(null) })
  );
  serviceTable = ctx.dataTable({
    columns: [
      { label: '名称', render: (r) => safeText(r.name != null ? r.name : r.service_name, '—') },
      { label: '本地地址', render: (r) => safeText(r.local_addr != null ? r.local_addr : r.localAddr, '—') },
      {
        label: '状态',
        render: (r) => ctx.statusBadge(
          r.enabled ? ctx.t('status.enabled') : ctx.t('status.disabled'),
          r.enabled ? 'ok' : 'muted'
        )
      },
      {
        label: ctx.t('common.actions'), align: 'right', render: (r) => [
          button('', {
            variant: 'ghost sm icon', icon: 'pencil', title: ctx.t('common.edit'),
            onClick: () => openEditor(r)
          }),
          button('', {
            variant: 'danger sm icon', icon: 'trash', title: ctx.t('common.delete'),
            onClick: async () => {
              const ok = await ctx.confirm({ message: '删除此穿透服务？', danger: true });
              if (!ok || r.id == null) return;
              try {
                await ctx.api(`/api/rathole/services/${encodeURIComponent(r.id)}`, {
                  method: 'DELETE', signal: ctx.signal
                });
                ctx.toast.success(ctx.t('common.deleted'));
                loadServices();
              } catch (e) {
                ctx.toast.error(ctx.t('common.requestFailed'));
              }
            }
          })
        ]
      }
    ],
    rows: [],
    emptyText: '暂无穿透服务'
  });
  serviceCard.body.appendChild(serviceTable.el);
  grid.appendChild(serviceCard.el);

  // ---------------- 内部函数 ----------------
  function action(name) {
    post(`/api/rathole/${name}`, {}).then((res) => {
      const o = fieldOf(res, 'data', res) || {};
      ctx.toast.success(ctx.t('common.saved'));
      if (o.device_verified === false) {
        hint.textContent = '运行态已记录，真机需验证 rathole 进程';
      }
      loadStatus();
    }).catch((e) => ctx.toast.error((e && e.message) || ctx.t('common.requestFailed')));
  }

  function openEditor(existing) {
    const f = {
      name: field({
        label: '服务名称', name: 'name',
        value: existing ? safeText(existing.name != null ? existing.name : existing.service_name, '') : ''
      }),
      token: field({
        label: '穿透密钥', name: 'token', type: 'password',
        placeholder: existing ? '留空表示不修改' : '必填'
      }),
      localAddr: field({
        label: '本地地址', name: 'local_addr', placeholder: '127.0.0.1:8080',
        value: existing ? safeText(existing.local_addr != null ? existing.local_addr : existing.localAddr, '') : ''
      }),
      enabled: switchField({
        label: '启用服务', checked: existing ? !!existing.enabled : true
      })
    };
    ctx.modal({
      variant: 'auto',
      title: existing ? '编辑穿透服务' : '添加穿透服务',
      content: (b) => { for (const k of Object.keys(f)) b.appendChild(f[k].wrap); },
      footer: [
        {
          label: ctx.t('common.save'), variant: 'primary', icon: 'save',
          onClick: async (m) => {
            const name = f.name.input.value.trim();
            const localAddr = f.localAddr.input.value.trim();
            if (!name || !localAddr) {
              ctx.toast.error('服务名称与本地地址不能为空');
              return;
            }
            const payload = {
              name,
              local_addr: localAddr,
              enabled: f.enabled.input.checked ? 1 : 0
            };
            const secretValue = f.token.input.value;
            if (secretValue) payload.token = secretValue;
            else if (!existing) {
              ctx.toast.error('新增服务必须填写穿透密钥');
              return;
            }
            try {
              if (existing && existing.id != null) {
                await ctx.api(`/api/rathole/services/${encodeURIComponent(existing.id)}`, {
                  method: 'PUT', body: payload, signal: ctx.signal
                });
              } else {
                await post('/api/rathole/services', payload);
              }
              ctx.toast.success(ctx.t('common.saved'));
              m.close();
              loadServices();
            } catch (e) {
              ctx.toast.error(ctx.t('common.requestFailed'));
            }
          }
        },
        { label: ctx.t('common.cancel'), variant: 'ghost', closeOnClick: true }
      ]
    });
  }

  async function openLogs() {
    try {
      const res = await ctx.api('/api/rathole/logs', { signal: ctx.signal });
      const rows = asArray(res, ['logs']);
      const lines = rows.map((r) => {
        if (typeof r === 'string') return safeText(r);
        const at = Number(r.at || 0);
        const when = at ? new Date(at > 1e12 ? at : at * 1000).toLocaleString() : '';
        return safeText(`${when} ${r.action || JSON.stringify(r)}`);
      });
      ctx.modal({
        variant: 'side-sheet',
        title: 'Rathole 运行日志',
        content: (b) => b.appendChild(el('pre', {
          class: 'mono',
          style: { whiteSpace: 'pre-wrap', maxHeight: '50vh', overflow: 'auto' },
          text: lines.join('\n') || '暂无记录'
        })),
        footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
      });
    } catch (e) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  async function openServerConfig() {
    try {
      const res = await ctx.api('/api/rathole/server-config', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const cfg = o.server_config != null ? o.server_config : (o.config != null ? o.config : o);
      // 服务端配置片段可能含密钥：仅展示结构，不回显 token 明文
      const shown = JSON.stringify(redact(cfg), null, 2);
      ctx.modal({
        variant: 'side-sheet',
        title: '服务端配置片段',
        content: (b) => b.appendChild(el('pre', {
          class: 'mono',
          style: { whiteSpace: 'pre-wrap', maxHeight: '50vh', overflow: 'auto' },
          text: shown
        })),
        footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
      });
    } catch (e) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  function redact(value) {
    if (Array.isArray(value)) return value.map(redact);
    if (value && typeof value === 'object') {
      const out = {};
      for (const [k, v] of Object.entries(value)) {
        out[k] = /token|secret|password|key/i.test(k) && v ? '***' : redact(v);
      }
      return out;
    }
    return value;
  }

  async function loadStatus() {
    try {
      const res = await ctx.api('/api/rathole/status', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const running = !!(o.running !== undefined ? o.running : res.running);
      statusLabel.textContent = running ? ctx.t('status.running') : ctx.t('status.stopped');
      statusLabel.className = `badge ${running ? 'badge--ok' : 'badge--muted'}`;
      if (o.pid != null) hint.textContent = `PID ${safeText(o.pid)}`;
    } catch (e) {
      statusLabel.textContent = '未知';
    }
  }

  async function loadServices() {
    try {
      const res = await ctx.api('/api/rathole/services', { signal: ctx.signal });
      serviceTable.setRows(asArray(res, ['services']));
    } catch (e) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  (async () => {
    try {
      const res = await ctx.api('/api/rathole/config', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      serverAddr.input.value = o.server_addr || o.server || res.server_addr || '';
      const on = (v) => v === true || v === 1 || v === '1' || v === 'true';
      enabled.input.checked = on(o.enabled !== undefined ? o.enabled : res.enabled);
      autoStart.input.checked = on(o.auto_start !== undefined ? o.auto_start : res.auto_start);
    } catch (e) { /* 首次进入允许为空 */ }
  })();

  loadStatus();
  loadServices();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  serviceTable = null;
}
