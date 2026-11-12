/**
 * 插件商城：
 *  - 用 core/plugin-renderer.js 渲染「受限 JSON UI」，只解释受支持 type，
 *    bind 只读值、action 仅调用宿主动作白名单，不执行任意 JS、不注入 HTML。
 *  - 明确说明：旧 Vue 模板 / 任意 JS 插件在新前端不再执行（见第 07 章）。
 *  - 提供插件列表 / 上传 / 删除 / 清空 / 存储查看，以及脚本管理（CRUD）。
 */
import { el, icon, button, field, loadingState, setBusy, toolbar } from '../utils/dom.js';
import { fieldOf, safeText, asArray } from '../utils/format.js';
import { renderPluginUI, SUPPORTED_TYPES } from '../core/plugin-renderer.js';

// 内置受限 JSON UI 示例：即使插件内容是旧版 JS，也只渲染这个声明式界面。
const DEMO_UI = {
  schema_version: 1,
  id: 'builtin.status',
  title: '设备状态（受限 JSON UI 示例）',
  permissions: ['device.status.read'],
  view: {
    type: 'column',
    children: [
      { type: 'text', label: '设备名称', bind: 'modem.name' },
      { type: 'status', label: '连接状态', bind: 'modem.connected' },
      { type: 'badge', bind: 'modem.network', tone: 'info' },
      { type: 'divider' },
      { type: 'row', children: [
        { type: 'input', label: '备注', name: 'note', bind: 'form.note', placeholder: '仅本地表单值' },
        { type: 'select', label: '模式', name: 'mode', bind: 'form.mode', options: [
          { value: 'auto', label: '自动' }, { value: 'manual', label: '手动' }
        ] },
        { type: 'button', label: '刷新状态', action: 'device.status.refresh' },
        { type: 'button', label: '重启（未授权）', action: 'device.reboot' }
      ] },
      { type: 'table', columns: [
        { label: '项目', key: 'name' }, { label: '值', key: 'value' }
      ], rowsBind: 'rows' }
    ]
  }
};

let ctx = null;
let tabContent = null;
let pluginTable = null;
let scriptTable = null;

function demoData() {
  return {
    modem: { name: 'UDX710', connected: true, network: '5G NR' },
    form: { note: '', mode: 'auto' },
    rows: [
      { name: '运营商', value: '自动' },
      { name: '网络制式', value: 'NR/LTE' },
      { name: '采样窗口', value: '300 点' }
    ]
  };
}

function pluginListFrom(res) {
  return asArray(res, ['plugins']);
}

function scriptListFrom(res) {
  return asArray(res, ['scripts']);
}

// ---- 插件列表 ----
async function loadPlugins() {
  pluginTable.setRows([]);
  tabContent.appendChild(loadingState());
  try {
    const res = await ctx.api('/api/plugins', { signal: ctx.signal });
    const list = pluginListFrom(res);
    tabContent.replaceChildren();
    tabContent.appendChild(pluginTable.el);
    pluginTable.setRows(list);
  } catch (err) {
    tabContent.replaceChildren(el('p', { class: 'text-danger', text: ctx.t('common.requestFailed') }));
  }
}

function pluginRowActions(p) {
  const name = p.filename || p.name;
  const run = button('查看 UI', {
    variant: 'ghost sm', icon: 'play',
    onClick: () => openPluginUI(p)
  });
  const storage = button('存储', {
    variant: 'ghost sm', icon: 'database',
    onClick: () => openStorage(name)
  });
  const del = button('', {
    variant: 'danger sm icon', icon: 'trash', title: ctx.t('common.delete'),
    onClick: async () => {
      const ok = await ctx.confirm({
        title: ctx.t('common.delete'), message: `确定删除插件「${safeText(name)}」吗？`, danger: true
      });
      if (!ok) return;
      try {
        await ctx.api(`/api/plugins/${encodeURIComponent(name)}`, { method: 'DELETE', signal: ctx.signal });
        ctx.toast.success(ctx.t('common.deleted'));
        loadPlugins();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  return [run, storage, del];
}

function openPluginUI(plugin) {
  const content = plugin.content || '';
  let parsed = null;
  try { parsed = JSON.parse(content); } catch (e) { parsed = null; }
  const isJsonUi = parsed && parsed.view;
  const descriptor = isJsonUi ? parsed : DEMO_UI;
  const data = demoData();

  ctx.modal({
    variant: 'side-sheet',
    title: `插件 UI · ${safeText(plugin.name || plugin.filename || '')}`,
    width: '560px',
    content: (body) => {
      if (!isJsonUi) {
        body.appendChild(el('div', { class: 'alert alert--warning',
          text: '该插件为旧版 Vue/JS 模板或内容非 JSON UI：新前端不再执行任意 JS，已自动切换为内置的受限 JSON UI 示例。' }));
      }
      body.appendChild(el('p', { class: 'field__hint',
        text: `支持的组件类型：${SUPPORTED_TYPES.join('、')}。未注册动作（如 device.reboot）将渲染为禁用按钮。` }));
      const host = el('div', { class: 'card', style: { boxShadow: 'none' } });
      body.appendChild(host);
      const renderer = renderPluginUI(host, descriptor, {
        data,
        actions: {
          'device.status.refresh': async () => {
            try {
              const info = await ctx.api('/api/info', { signal: ctx.signal });
              data.modem.name = safeText(fieldOf(info, 'hostname', fieldOf(info, 'host', 'UDX710')));
              renderer.update(data);
              ctx.toast.success(ctx.t('common.saved'));
            } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
          }
        },
        onAction: (name, result) => {
          if (!result.ok) ctx.toast.error(`动作 ${name} 执行失败`);
        }
      });
    },
    footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
  });
}

async function openStorage(name) {
  let stored = {};
  try {
    stored = await ctx.api(`/api/plugins/storage/${encodeURIComponent(name)}`, { signal: ctx.signal });
  } catch (e) {
    ctx.toast.error(ctx.t('common.requestFailed'));
    return;
  }
  const obj = fieldOf(stored, 'data', stored) || {};
  const ta = el('textarea', { class: 'input', rows: 12, value: JSON.stringify(obj, null, 2) });
  ctx.modal({
    variant: 'side-sheet',
    title: `插件存储 · ${safeText(name)}`,
    content: (body) => body.appendChild(ta),
    footer: [
      {
        label: ctx.t('common.save'), variant: 'primary', icon: 'save',
        onClick: async () => {
          let payload;
          try { payload = JSON.parse(ta.value); } catch (e) {
            ctx.toast.error('存储内容必须是合法 JSON'); return;
          }
          try {
            await ctx.api(`/api/plugins/storage/${encodeURIComponent(name)}`, {
              method: 'POST', body: payload, signal: ctx.signal
            });
            ctx.toast.success(ctx.t('common.saved'));
          } catch (err) { ctx.toast.error(ctx.t('common.requestFailed')); }
        }
      },
      {
        label: ctx.t('common.clear'), variant: 'danger',
        onClick: async () => {
          const ok = await ctx.confirm({ message: '清空该插件的存储数据？', danger: true });
          if (!ok) return;
          try {
            await ctx.api(`/api/plugins/storage/${encodeURIComponent(name)}`, {
              method: 'DELETE', signal: ctx.signal
            });
            ta.value = '{}';
            ctx.toast.success(ctx.t('common.deleted'));
          } catch (err) { ctx.toast.error(ctx.t('common.requestFailed')); }
        }
      },
      { label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }
    ]
  });
}

function openUpload() {
  const nameInput = el('input', { class: 'input', placeholder: '插件名称（可选）' });
  const contentTa = el('textarea', { class: 'input', rows: 10, placeholder: '粘贴受限 JSON UI 描述（含 view 字段）…' });
  const fileInput = el('input', { type: 'file', accept: '.json,.js', class: 'input' });
  fileInput.addEventListener('change', async () => {
    const file = fileInput.files && fileInput.files[0];
    if (!file) return;
    contentTa.value = await file.text();
    if (!nameInput.value) nameInput.value = file.name.replace(/\.(json|js)$/i, '');
  });
  ctx.modal({
    variant: 'auto',
    title: '上传插件',
    content: (body) => body.append(
      el('div', { class: 'field' }, el('label', { class: 'field__label', text: '选择文件' }), fileInput),
      el('div', { class: 'field' }, el('label', { class: 'field__label', text: '名称' }), nameInput),
      el('div', { class: 'field' }, el('label', { class: 'field__label', text: 'JSON UI 内容' }), contentTa)
    ),
    footer: [
      {
        label: ctx.t('common.upload'), variant: 'primary', icon: 'upload',
        onClick: async (m) => {
          const content = contentTa.value.trim();
          if (!content) { ctx.toast.error('内容不能为空'); return; }
          const name = nameInput.value.trim() || `plugin_${Date.now()}`;
          try {
            await ctx.api('/api/plugins', {
              method: 'POST', body: { name, content }, signal: ctx.signal
            });
            ctx.toast.success(ctx.t('common.saved'));
            m.close();
            loadPlugins();
          } catch (err) { ctx.toast.error(ctx.t('common.requestFailed')); }
        }
      },
      { label: ctx.t('common.cancel'), variant: 'ghost', closeOnClick: true }
    ]
  });
}

function showPlugins() {
  tabContent.replaceChildren();
  const actions = toolbar(
    button('上传插件', { variant: 'primary', icon: 'upload', onClick: openUpload }),
    button(ctx.t('common.refresh'), { variant: 'ghost', icon: 'refresh', onClick: loadPlugins }),
    button('清空全部', {
      variant: 'danger', icon: 'trash',
      onClick: async () => {
        const ok = await ctx.confirm({ message: '确定删除所有插件？此操作不可恢复。', danger: true });
        if (!ok) return;
        try {
          await ctx.api('/api/plugins/all', { method: 'DELETE', signal: ctx.signal });
          ctx.toast.success(ctx.t('common.deleted'));
          loadPlugins();
        } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
      }
    })
  );
  tabContent.appendChild(actions);

  pluginTable = ctx.dataTable({
    columns: [
      { label: '名称', render: (p) => safeText(p.name || p.filename) },
      { label: '版本', render: (p) => safeText(p.version) },
      { label: '作者', render: (p) => safeText(p.author) },
      { label: '描述', render: (p) => safeText(p.description, '—') },
      { label: ctx.t('common.actions'), align: 'right', render: (p) => pluginRowActions(p) }
    ],
    rows: [],
    emptyText: '暂无插件，点击「上传插件」添加'
  });
  tabContent.appendChild(pluginTable.el);
  loadPlugins();
}

// ---- 脚本管理 ----
async function loadScripts() {
  try {
    const res = await ctx.api('/api/scripts', { signal: ctx.signal });
    scriptTable.setRows(scriptListFrom(res));
  } catch (e) {
    ctx.toast.error(ctx.t('common.requestFailed'));
  }
}

function openScriptEditor(existing) {
  const nameInput = el('input', { class: 'input', value: existing ? (existing.name || '') : '', placeholder: '脚本名，如 test.sh' });
  if (existing) nameInput.disabled = true;
  const contentTa = el('textarea', { class: 'input', rows: 12, value: existing ? (existing.content || '') : '#!/bin/sh\n' });
  ctx.modal({
    variant: 'side-sheet',
    title: existing ? '编辑脚本' : '新建脚本',
    content: (body) => body.append(
      el('div', { class: 'field' }, el('label', { class: 'field__label', text: '名称' }), nameInput),
      el('div', { class: 'field' }, el('label', { class: 'field__label', text: '内容' }), contentTa)
    ),
    footer: [
      {
        label: ctx.t('common.save'), variant: 'primary', icon: 'save',
        onClick: async (m) => {
          const name = nameInput.value.trim();
          const content = contentTa.value;
          if (!name || !content) { ctx.toast.error('名称与内容不能为空'); return; }
          try {
            if (existing) {
              await ctx.api(`/api/scripts/${encodeURIComponent(name)}`, {
                method: 'PUT', body: { content }, signal: ctx.signal
              });
            } else {
              await ctx.api('/api/scripts', {
                method: 'POST', body: { name, content }, signal: ctx.signal
              });
            }
            ctx.toast.success(ctx.t('common.saved'));
            m.close();
            loadScripts();
          } catch (err) { ctx.toast.error(ctx.t('common.requestFailed')); }
        }
      },
      { label: ctx.t('common.cancel'), variant: 'ghost', closeOnClick: true }
    ]
  });
}

function showScripts() {
  tabContent.replaceChildren();
  tabContent.appendChild(toolbar(
    button('新建脚本', { variant: 'primary', icon: 'plus', onClick: () => openScriptEditor(null) }),
    button(ctx.t('common.refresh'), { variant: 'ghost', icon: 'refresh', onClick: loadScripts })
  ));
  scriptTable = ctx.dataTable({
    columns: [
      { label: '名称', render: (s) => safeText(s.name) },
      { label: '大小', render: (s) => `${safeText(s.size, '—')} B` },
      {
        label: ctx.t('common.actions'), align: 'right', render: (s) => [
          button('', { variant: 'ghost sm icon', icon: 'pencil', title: ctx.t('common.edit'), onClick: () => openScriptEditor(s) }),
          button('', {
            variant: 'danger sm icon', icon: 'trash', title: ctx.t('common.delete'),
            onClick: async () => {
              const ok = await ctx.confirm({ message: `删除脚本「${safeText(s.name)}」？`, danger: true });
              if (!ok) return;
              try {
                await ctx.api(`/api/scripts/${encodeURIComponent(s.name)}`, { method: 'DELETE', signal: ctx.signal });
                ctx.toast.success(ctx.t('common.deleted'));
                loadScripts();
              } catch (err) { ctx.toast.error(ctx.t('common.requestFailed')); }
            }
          })
        ]
      }
    ],
    rows: [],
    emptyText: '暂无脚本'
  });
  tabContent.appendChild(scriptTable.el);
  loadScripts();
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const info = el('div', { class: 'alert alert--info col-span-12' },
    icon('shield'),
    el('strong', { text: '插件系统已改为受限协议：' }),
    el('span', { text: '旧 Vue 模板 / 任意 JS 插件不再执行；宿主只解释声明式 JSON UI（受支持组件 + 动作白名单），不执行脚本、不注入 HTML。' }));
  grid.appendChild(info);

  const cardEl = ctx.card({ title: '插件与脚本', subtitle: 'JSON UI 宿主渲染', span: 12, icon: 'puzzle' });
  const tabs = el('div', { class: 'tabs' });
  const tabPlugins = button('插件', { variant: 'ghost', className: 'tab is-active' });
  const tabScripts = button('脚本', { variant: 'ghost', className: 'tab' });
  tabs.append(tabPlugins, tabScripts);
  tabContent = el('div', {});
  cardEl.body.append(tabs, tabContent);
  grid.appendChild(cardEl.el);

  tabPlugins.addEventListener('click', () => {
    tabPlugins.classList.add('is-active');
    tabScripts.classList.remove('is-active');
    showPlugins();
  });
  tabScripts.addEventListener('click', () => {
    tabScripts.classList.add('is-active');
    tabPlugins.classList.remove('is-active');
    showScripts();
  });

  showPlugins();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  tabContent = null;
  pluginTable = null;
  scriptTable = null;
}
