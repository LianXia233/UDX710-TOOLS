/**
 * APN 配置：模式（自动/手动）+ 预制 APN 快速库 + 模板 CRUD + 应用 / 清除。
 * 支持中国移动、中国联通、中国电信、中国广电及通用互联网预制 APN 一键应用与模板导入。
 */
import { el, button, field, selectField, switchField, toolbar } from '../utils/dom.js';
import { fieldOf, safeText, asArray } from '../utils/format.js';

let ctx = null;
let templateTable = null;
let templateSelect = null;
let templates = [];

export const PRESET_APNS = [
  {
    name: '中国移动 (CMNET - 5G/4G通用)',
    apn: 'cmnet',
    username: '',
    password: '',
    auth_type: 'none',
    protocol: 'dual',
    carrier: '中国移动'
  },
  {
    name: '中国移动 物联网专用 (CMIOT)',
    apn: 'cmiot',
    username: '',
    password: '',
    auth_type: 'none',
    protocol: 'dual',
    carrier: '中国移动'
  },
  {
    name: '中国联通 (3GNET - 5G/4G通用)',
    apn: '3gnet',
    username: '',
    password: '',
    auth_type: 'none',
    protocol: 'dual',
    carrier: '中国联通'
  },
  {
    name: '中国联通 物联网专用 (WONET)',
    apn: 'wonet',
    username: '',
    password: '',
    auth_type: 'none',
    protocol: 'dual',
    carrier: '中国联通'
  },
  {
    name: '中国电信 (CTNET - 5G/4G通用)',
    apn: 'ctnet',
    username: 'ctnet@myhsp.org',
    password: 'vnet.mobi',
    auth_type: 'pap',
    protocol: 'dual',
    carrier: '中国电信'
  },
  {
    name: '中国电信 纯净接入 (CTLTE)',
    apn: 'ctlte',
    username: '',
    password: '',
    auth_type: 'none',
    protocol: 'dual',
    carrier: '中国电信'
  },
  {
    name: '中国广电 (CBNET - 5G专属)',
    apn: 'cbnet',
    username: '',
    password: '',
    auth_type: 'none',
    protocol: 'dual',
    carrier: '中国广电'
  },
  {
    name: '通用互联网 APN (INTERNET)',
    apn: 'internet',
    username: '',
    password: '',
    auth_type: 'none',
    protocol: 'dual',
    carrier: '通用'
  }
];

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

  // ============ ① 预制运营商 APN 快速库 (Row 1: 12 列) ============
  const presetCard = ctx.card({
    title: '预制 APN 快速库',
    subtitle: '三大运营商、中国广电及物联网卡专用接入点，支持一键切换或存入模板',
    span: 12,
    icon: 'globe'
  });

  const presetGrid = el('div', {
    class: 'rf-stats-row',
    style: {
      gridTemplateColumns: 'repeat(auto-fit, minmax(min(220px, 100%), 1fr))',
      gap: '12px'
    }
  });

  for (const item of PRESET_APNS) {
    const chip = el('div', {
      class: 'rf-stat-pill',
      style: {
        padding: '12px 14px',
        justifyContent: 'space-between'
      }
    },
      el('div', {},
        el('div', { class: 'row', style: { justifyContent: 'space-between', marginBottom: '4px' } },
          el('span', { class: 'badge badge--ok', text: item.carrier }),
          el('span', { style: { fontSize: '11px', color: 'var(--text-muted)' }, text: item.protocol.toUpperCase() })
        ),
        el('div', { style: { fontSize: '14px', fontWeight: '800', color: 'var(--text)' }, text: item.name }),
        el('div', { style: { fontSize: '12px', fontFamily: 'var(--mono)', color: 'var(--primary)', marginTop: '2px' }, text: `APN: ${item.apn}` })
      ),
      el('div', { class: 'row', style: { gap: '6px', marginTop: '10px' } },
        button('一键应用', {
          variant: 'primary sm',
          icon: 'check',
          onClick: async () => {
            const ok = await ctx.confirm({
              title: '应用预制 APN',
              message: `确定直接应用「${item.name}」（APN: ${item.apn}）吗？设备将使用此接入点注册蜂窝网络。`
            });
            if (!ok) return;
            try {
              // 自动模式先切手动并写入配置
              await ctx.api('/api/apn/config', {
                method: 'POST',
                body: { mode: 1, auto_start: 1 },
                signal: ctx.signal
              });
              // 寻找或创建对应模板并应用
              let match = templates.find((t) => t.apn === item.apn);
              if (!match) {
                const addRes = await ctx.api('/api/apn/templates', {
                  method: 'POST',
                  body: item,
                  signal: ctx.signal
                });
                const newId = addRes && addRes.id;
                match = { id: newId };
              }
              if (match && match.id) {
                await ctx.api('/api/apn/apply', {
                  method: 'POST',
                  body: { template_id: match.id },
                  signal: ctx.signal
                });
              }
              ctx.toast.success(`已成功应用 ${item.name}`);
              loadTemplates();
              loadConfig();
            } catch (e) {
              ctx.toast.error(ctx.t('common.requestFailed'));
            }
          }
        }),
        button('存入模板', {
          variant: 'ghost sm',
          icon: 'plus',
          onClick: async () => {
            try {
              await ctx.api('/api/apn/templates', {
                method: 'POST',
                body: item,
                signal: ctx.signal
              });
              ctx.toast.success(`已添加「${item.name}」到模板库`);
              loadTemplates();
            } catch (e) {
              ctx.toast.error(ctx.t('common.requestFailed'));
            }
          }
        })
      )
    );
    presetGrid.appendChild(chip);
  }

  presetCard.body.appendChild(presetGrid);
  grid.appendChild(presetCard.el);

  // ============ ② 模式配置 (Row 2 Left: 5 列) ============
  const cfgCard = ctx.card({ title: 'APN 策略模式', subtitle: '接入点模式与策略选择', span: 5, icon: 'settings' });
  const mode = selectField({
    label: '模式', name: 'apnmode', options: [
      { value: '0', label: '自动模式（底层系统自主适配）' },
      { value: '1', label: '手动模式（优先使用指定模板）' }
    ]
  });
  templateSelect = selectField({ label: '指定默认模板', name: 'template', options: [{ value: '0', label: '（未选择）' }] });
  const autoStart = switchField({ label: '开机自动应用该 APN', checked: true });
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
    el('p', { class: 'field__hint', text: '自动模式下系统将根据 SIM 卡信息自动选择运营商 APN；手动模式下严格遵循选定模板。' }));
  grid.appendChild(cfgCard.el);

  // ============ ③ 模板列表 (Row 2 Right: 7 列) ============
  const tplCard = ctx.card({ title: '已存 APN 模板', subtitle: '运营商接入点配置档案', span: 7, icon: 'list' });
  const actionsWrap = tplCard.head.querySelector('.card__actions');

  const importAllBtn = button('一键导入全部预制模板', {
    variant: 'ghost sm',
    icon: 'download',
    onClick: async () => {
      const ok = await ctx.confirm({
        title: '批量导入预制模板',
        message: '将把中国移动、联通、电信、广电及通用 APN 全部写入您的模板列表，确定导入吗？'
      });
      if (!ok) return;
      try {
        for (const item of PRESET_APNS) {
          if (!templates.some((t) => t.apn === item.apn)) {
            await ctx.api('/api/apn/templates', { method: 'POST', body: item, signal: ctx.signal });
          }
        }
        ctx.toast.success('预制模板全部导入完成');
        loadTemplates();
      } catch (e) {
        ctx.toast.error(ctx.t('common.requestFailed'));
      }
    }
  });

  const newBtn = button('新建自定义模板', {
    variant: 'primary sm',
    icon: 'plus',
    onClick: () => openEditor(null)
  });

  actionsWrap.append(importAllBtn, newBtn);

  templateTable = ctx.dataTable({
    columns: [
      { label: '名称', render: (t) => safeText(t.name) },
      { label: 'APN', render: (t) => safeText(t.apn) },
      { label: '用户名', render: (t) => safeText(t.username, '—') },
      { label: '协议', render: (t) => safeText(t.protocol, 'IPv4/IPv6') },
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
              const ok = await ctx.confirm({ message: `确定删除模板「${safeText(t.name)}」？`, danger: true });
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
    ], rows: [], emptyText: '暂无模板，可从上方快捷库一键添加'
  });
  tplCard.body.appendChild(templateTable.el);
  grid.appendChild(tplCard.el);

  function openEditor(existing) {
    const f = {
      name: field({ label: '模板名称', name: 'name', value: existing ? (existing.name || '') : '' }),
      apn: field({ label: 'APN 接入点', name: 'apn', value: existing ? (existing.apn || '') : '' }),
      username: field({ label: '用户名（可选）', name: 'username', value: existing ? (existing.username || '') : '' }),
      password: field({ label: '密码（可选）', name: 'password', value: existing ? (existing.password || '') : '' }),
      auth: selectField({ label: '认证方式', name: 'auth_type', value: existing ? (existing.auth_type || 'none') : 'none', options: [
        { value: 'none', label: '无认证 (默认)' }, { value: 'pap', label: 'PAP' }, { value: 'chap', label: 'CHAP' }
      ] }),
      protocol: selectField({ label: '网络协议', name: 'protocol', value: existing ? (existing.protocol || 'dual') : 'dual', options: [
        { value: 'dual', label: 'IPv4 / IPv6 (双栈推荐)' },
        { value: 'ipv4', label: '仅 IPv4' },
        { value: 'ipv6', label: '仅 IPv6' }
      ] })
    };
    ctx.modal({
      variant: 'auto',
      title: existing ? '编辑 APN 模板' : '新建 APN 模板',
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
