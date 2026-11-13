/**
 * APN 配置：模式（自动/手动）+ 模板 CRUD + 应用 / 清除。
 * 大表格用 data-table 分页；表单使用原生控件与 Constraint Validation。
 */
import { el, button, field, selectField, switchField, toolbar } from '../utils/dom.js';
import { fieldOf, safeText, asArray } from '../utils/format.js';

let ctx = null;
let templateTable = null;
let templateSelect = null;
let templates = [];

function templatesFrom(res) {
  return asArray(res, ['templates']);
}

function configFrom(res) {
  const data = fieldOf(res, 'data', res) || {};
  return data && typeof data === 'object' ? data : {};
}

export function render(container, context) {
  ctx = context;
  templates = [];
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // 模式配置
  const cfgCard = ctx.card({ title: 'APN 模式', subtitle: '接入点模式与策略选择', span: 5, icon: 'globe' });
  const mode = selectField({
    label: '模式', name: 'apnmode', options: [
      { value: '0', label: '自动（系统管理）' }, { value: '1', label: '手动（使用模板）' }
    ]
  });
  templateSelect = selectField({ label: '绑定模板', name: 'template', options: [{ value: '0', label: '（未选择）' }] });
  const autoStart = switchField({ label: '开机自动应用', checked: false });
  const saveCfg = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      const manual = mode.input.value === '1';
      const tid = Number(templateSelect.input.value) || 0;
      if (manual && tid <= 0) { ctx.toast.error('请先选择一个模板'); return; }
      try {
        await ctx.api('/api/apn/config', {
          method: 'POST',
          body: { mode: manual ? 1 : 0, template_id: manual ? tid : 0, auto_start: manual && autoStart.input.checked ? 1 : 0 },
          signal: ctx.signal
        });
        if (manual) {
          await ctx.api('/api/apn/apply', { method: 'POST', body: { template_id: tid }, signal: ctx.signal });
        } else {
          await ctx.api('/api/apn/clear', { method: 'POST', signal: ctx.signal });
        }
        ctx.toast.success(ctx.t('common.saved'));
        loadConfig();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  cfgCard.body.append(mode.wrap, templateSelect.wrap, autoStart.wrap, saveCfg,
    el('p', { class: 'field__hint', text: '自动模式下系统将自动匹配运营商 APN；手动模式使用自定义模板。' }));
  grid.appendChild(cfgCard.el);

  // 模板列表
  const tplCard = ctx.card({ title: 'APN 模板', subtitle: '自定义运营商网络接入点配置', span: 7, icon: 'list' });
  tplCard.head.querySelector('.card__actions').appendChild(
    button('新建模板', { variant: 'primary sm', icon: 'plus', onClick: () => openEditor(null) }));
  templateTable = ctx.dataTable({
    columns: [
      { label: '名称', render: (t) => safeText(t.name) },
      { label: 'APN', render: (t) => safeText(t.apn) },
      { label: '用户名', render: (t) => safeText(t.username, '—') },
      { label: '协议', render: (t) => safeText(t.protocol, '—') },
      {
        label: ctx.t('common.actions'), align: 'right', render: (t) => [
          button('应用', {
            variant: 'ghost sm', onClick: async () => {
              try {
                await ctx.api('/api/apn/apply', {
                  method: 'POST', body: { template_id: t.id }, signal: ctx.signal
                });
                ctx.toast.success(ctx.t('common.applied'));
              } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
            }
          }),
          button('', { variant: 'ghost sm icon', icon: 'pencil', title: ctx.t('common.edit'), onClick: () => openEditor(t) }),
          button('', {
            variant: 'danger sm icon', icon: 'trash', title: ctx.t('common.delete'),
            onClick: async () => {
              const ok = await ctx.confirm({ message: `删除模板「${safeText(t.name)}」？`, danger: true });
              if (!ok || t.id == null) return;
              try {
                await ctx.api(`/api/apn/templates/${encodeURIComponent(t.id)}`, { method: 'DELETE', signal: ctx.signal });
                ctx.toast.success(ctx.t('common.deleted'));
                loadTemplates();
              } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
            }
          })
        ]
      }
    ], rows: [], emptyText: '暂无模板'
  });
  tplCard.body.appendChild(templateTable.el);
  grid.appendChild(tplCard.el);

  function openEditor(existing) {
    const f = {
      name: field({ label: '模板名称', name: 'name', value: existing ? (existing.name || '') : '' }),
      apn: field({ label: 'APN', name: 'apn', value: existing ? (existing.apn || '') : '' }),
      username: field({ label: '用户名（可选）', name: 'username', value: existing ? (existing.username || '') : '' }),
      password: field({ label: '密码（可选）', name: 'password', value: existing ? (existing.password || '') : '' }),
      auth: selectField({ label: '认证方式', name: 'auth_type', value: existing ? (existing.auth_type || 'none') : 'none', options: [
        { value: 'none', label: '无认证' }, { value: 'pap', label: 'PAP' }, { value: 'chap', label: 'CHAP' }
      ] }),
      protocol: selectField({ label: '协议', name: 'protocol', value: existing ? (existing.protocol || 'ipv4') : 'ipv4', options: [
        { value: 'ipv4', label: 'IPv4' }, { value: 'ipv6', label: 'IPv6' }, { value: 'dual', label: 'IPv4/IPv6' }
      ] })
    };
    ctx.modal({
      variant: 'auto',
      title: existing ? '编辑模板' : '新建模板',
      content: (body) => {
        for (const key of Object.keys(f)) body.appendChild(f[key].wrap);
      },
      footer: [
        {
          label: ctx.t('common.save'), variant: 'primary', icon: 'save',
          onClick: async (m) => {
            if (!f.name.input.value.trim() || !f.apn.input.value.trim()) {
              ctx.toast.error('模板名称与 APN 不能为空'); return;
            }
            const payload = {
              name: f.name.input.value.trim(), apn: f.apn.input.value.trim(),
              username: f.username.input.value, password: f.password.input.value,
              auth_type: f.auth.input.value, protocol: f.protocol.input.value
            };
            try {
              if (existing && existing.id != null) {
                await ctx.api(`/api/apn/templates/${encodeURIComponent(existing.id)}`, {
                  method: 'PUT', body: payload, signal: ctx.signal
                });
              } else {
                await ctx.api('/api/apn/templates', { method: 'POST', body: payload, signal: ctx.signal });
              }
              ctx.toast.success(ctx.t('common.saved'));
              m.close();
              loadTemplates();
            } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
          }
        },
        { label: ctx.t('common.cancel'), variant: 'ghost', closeOnClick: true }
      ]
    });
  }

  async function loadTemplates() {
    try {
      const res = await ctx.api('/api/apn/templates', { signal: ctx.signal });
      templates = templatesFrom(res);
      templateTable.setRows(templates);
      // 同步模板下拉（先保留当前选中值）
      const selected = templateSelect.input.value;
      const options = [{ value: '0', label: '（未选择）' }].concat(
        templates.map((t) => ({ value: String(t.id), label: safeText(t.name) })));
      templateSelect.input.replaceChildren();
      for (const o of options) {
        templateSelect.input.appendChild(el('option', { value: o.value, text: o.label }));
      }
      templateSelect.input.value = selected;
    } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
  }

  async function loadConfig() {
    try {
      const res = await ctx.api('/api/apn/config', { signal: ctx.signal });
      const c = configFrom(res);
      mode.input.value = String(c.mode || 0);
      autoStart.input.checked = Number(c.auto_start) === 1;
      if (c.template_id != null) templateSelect.input.value = String(c.template_id);
    } catch (e) { /* 忽略 */ }
  }

  loadTemplates();
  loadConfig();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  templateTable = null;
  templateSelect = null;
  templates = [];
}
