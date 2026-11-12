/**
 * 插件 JSON UI 宿主渲染器（对应技术报告第 07 章「受限插件协议」）。
 *
 * 安全边界（务必保持）：
 *  - 只解释受支持的 type：column/row/text/status/badge/button/table/input/select/divider。
 *  - bind 只通过 readPath 读取白名单路径的值，绝不写入、绝不访问原型链危险键。
 *  - action 只能是宿主注册动作白名单中的名字，未注册动作渲染为禁用按钮。
 *  - 一律 createElement + textContent，绝不执行脚本、不 eval / new Function、不注入 HTML。
 *  - 节点数量与层级有上限，避免恶意描述撑爆 DOM。
 *
 * 旧版 Vue 模板插件（含任意 JS / 运行时模板编译）在新前端中不再执行。
 */
import { el } from '../utils/dom.js';
import { readPath, safeText } from '../utils/format.js';

export const SUPPORTED_TYPES = [
  'column', 'row', 'text', 'status', 'badge', 'button',
  'table', 'input', 'select', 'divider'
];

const SUPPORTED = new Set(SUPPORTED_TYPES);
const SAFE_TONES = new Set(['ok', 'error', 'warning', 'info', 'primary', 'muted']);
const SAFE_INPUT_TYPES = new Set(['text', 'number', 'password', 'email', 'tel']);

/** 宿主注册的动作白名单（全局）。宿主应只注册经过审计的安全动作。 */
const actionWhitelist = new Map();

export function registerAction(name, fn) {
  if (typeof name === 'string' && typeof fn === 'function') actionWhitelist.set(name, fn);
}
export function unregisterAction(name) {
  actionWhitelist.delete(name);
}
export function clearActions() {
  actionWhitelist.clear();
}
export function getActionNames() {
  return Array.from(actionWhitelist.keys());
}

function toneClass(tone) {
  return SAFE_TONES.has(tone) ? tone : 'muted';
}

function boundValue(node, state) {
  if (node.bind) return readPath(state.data, node.bind);
  if (node.value !== undefined) return node.value;
  return node.text;
}

function childrenOf(node) {
  return Array.isArray(node.children) ? node.children : [];
}

function dispatchAction(name, node, state) {
  const fn = state.actions.get(name);
  if (typeof fn !== 'function') return;
  try {
    const result = fn({
      data: state.data,
      form: { ...state.form },
      node,
      refresh: () => { if (state.draw) state.draw(); }
    });
    if (result && typeof result.then === 'function') {
      result.catch(() => { /* 动作内部错误由宿主自行处理 */ });
    }
    if (state.onAction) state.onAction(name, { ok: true });
  } catch (err) {
    if (state.onAction) state.onAction(name, { ok: false, error: err && err.message });
  }
}

function buildNode(node, state, depth) {
  if (depth > state.maxDepth) {
    return el('span', { class: 'plugin-unsupported', text: '[节点层级超出上限]' });
  }
  state.nodes += 1;
  if (state.nodes > state.maxNodes) {
    return el('span', { class: 'plugin-unsupported', text: '[节点数量超出上限]' });
  }
  if (!node || typeof node !== 'object') {
    return el('span', { text: safeText(node, '') });
  }
  if (!SUPPORTED.has(node.type)) {
    return el('span', {
      class: 'plugin-unsupported',
      text: `[不支持的组件类型: ${safeText(node.type, 'unknown')}]`
    });
  }

  switch (node.type) {
    case 'column':
      return el('div', { class: 'plugin-col' },
        childrenOf(node).slice(0, state.maxNodes).map((c) => buildNode(c, state, depth + 1)));

    case 'row':
      return el('div', { class: 'plugin-row' },
        childrenOf(node).slice(0, state.maxNodes).map((c) => buildNode(c, state, depth + 1)));

    case 'text': {
      const value = boundValue(node, state);
      const valueEl = el('span', { class: 'plugin-value', text: safeText(value, '') });
      if (node.label) {
        return el('div', { class: 'plugin-line' },
          el('span', { class: 'plugin-label', text: safeText(node.label) }), valueEl);
      }
      return el('p', { class: 'plugin-text' }, valueEl);
    }

    case 'status': {
      const value = boundValue(node, state);
      const on = value === true || value === 1 ||
        ['ok', 'connected', 'online', 'running', 'up', 'enabled', 'active'].includes(String(value).toLowerCase());
      const tone = on ? 'ok' : (value === false || value === 0 ? 'error' : 'warning');
      return el('span', { class: 'plugin-status' },
        el('i', { class: `dot dot--${tone}` }),
        el('span', { text: safeText(node.label || (on ? '正常' : '异常')) }));
    }

    case 'badge': {
      const value = boundValue(node, state);
      return el('span', { class: `badge badge--${toneClass(node.tone)}` },
        safeText(value, safeText(node.label, '')));
    }

    case 'button': {
      const actionName = typeof node.action === 'string' ? node.action : '';
      const allowed = actionName && state.actions.has(actionName);
      const btn = el('button', {
        type: 'button',
        class: 'btn btn--primary',
        disabled: !allowed,
        title: allowed ? '' : '动作未注册或不在白名单中'
      }, safeText(node.label || '按钮'));
      if (allowed) {
        btn.addEventListener('click', () => dispatchAction(actionName, node, state));
      }
      return btn;
    }

    case 'table': {
      const columns = Array.isArray(node.columns) ? node.columns : [];
      let rows = [];
      if (node.rowsBind) {
        const r = readPath(state.data, node.rowsBind);
        if (Array.isArray(r)) rows = r;
      } else if (Array.isArray(node.rows)) {
        rows = node.rows;
      }
      rows = rows.slice(0, node.maxRows || 100);
      const head = el('thead');
      const headRow = el('tr');
      for (const col of columns) {
        headRow.appendChild(el('th', { text: safeText(col.label || col.key || col.bind || '') }));
      }
      head.appendChild(headRow);
      const body = el('tbody');
      if (rows.length === 0) {
        body.appendChild(el('tr', {},
          el('td', { colspan: String(columns.length || 1) }, '暂无数据')));
      } else {
        for (const rowData of rows) {
          const tr = el('tr');
          for (const col of columns) {
            const value = col.bind
              ? readPath(rowData, col.bind)
              : (col.key ? rowData[col.key] : '');
            tr.appendChild(el('td', { text: safeText(value, '') }));
          }
          body.appendChild(tr);
        }
      }
      return el('div', { class: 'table-wrap' }, el('table', { class: 'table' }, head, body));
    }

    case 'input': {
      const name = node.name || node.bind || '';
      const initial = boundValue(node, state);
      const inputType = SAFE_INPUT_TYPES.has(node.inputType) ? node.inputType : 'text';
      const input = el('input', {
        class: 'input', type: inputType,
        placeholder: safeText(node.placeholder || '', ''),
        value: initial == null ? '' : String(initial)
      });
      if (name) state.form[name] = input.value;
      input.addEventListener('input', () => {
        if (name) state.form[name] = input.value;
      });
      const kids = [];
      if (node.label) kids.push(el('label', { class: 'field__label', text: safeText(node.label) }));
      kids.push(input);
      return el('div', { class: 'field' }, ...kids);
    }

    case 'select': {
      const name = node.name || node.bind || '';
      const select = el('select', { class: 'input select' });
      const options = Array.isArray(node.options) ? node.options : [];
      for (const opt of options) {
        const item = (opt && typeof opt === 'object')
          ? { value: opt.value, label: opt.label != null ? opt.label : opt.value }
          : { value: opt, label: opt };
        select.appendChild(el('option', {
          value: String(item.value == null ? '' : item.value),
          text: safeText(item.label, '')
        }));
      }
      const initial = boundValue(node, state);
      if (initial != null) select.value = String(initial);
      if (name) state.form[name] = select.value;
      select.addEventListener('change', () => {
        if (name) state.form[name] = select.value;
      });
      const kids = [];
      if (node.label) kids.push(el('label', { class: 'field__label', text: safeText(node.label) }));
      kids.push(select);
      return el('div', { class: 'field' }, ...kids);
    }

    case 'divider':
      return el('hr', { class: 'plugin-divider' });

    default:
      return el('span', { class: 'plugin-unsupported', text: '[未处理的组件]' });
  }
}

/**
 * 渲染插件 UI 描述。
 * @param {HTMLElement} container 宿主容器。
 * @param {object} descriptor ui.json 对象（含 view；也允许直接传 view 节点）。
 * @param {object} options { data, actions, onAction, maxNodes, maxDepth }。
 * @returns {{el, update, destroy, info}}
 */
export function renderPluginUI(container, descriptor, options = {}) {
  const root = el('div', { class: 'plugin-ui' });
  container.replaceChildren(root);

  const hasWrapper = descriptor && descriptor.view;
  const view = hasWrapper ? descriptor.view : (descriptor && descriptor.type ? descriptor : null);

  // 合并全局白名单与本次宿主提供的动作，形成该渲染实例的唯一动作来源
  const actionMap = new Map(actionWhitelist);
  if (options.actions && typeof options.actions === 'object') {
    for (const key of Object.keys(options.actions)) {
      if (typeof options.actions[key] === 'function') actionMap.set(key, options.actions[key]);
    }
  }

  const state = {
    data: options.data || {},
    form: {},
    actions: actionMap,
    onAction: options.onAction,
    maxNodes: options.maxNodes || 400,
    maxDepth: options.maxDepth || 12,
    nodes: 0,
    draw: null
  };

  function draw() {
    state.nodes = 0;
    root.replaceChildren();
    if (hasWrapper) {
      if (descriptor.title) {
        root.appendChild(el('h4', { class: 'plugin-ui__title', text: safeText(descriptor.title) }));
      }
      if (Array.isArray(descriptor.permissions) && descriptor.permissions.length) {
        root.appendChild(el('p', {
          class: 'plugin-ui__perm',
          text: `声明权限：${descriptor.permissions.map((p) => safeText(p, '')).join('、')}`
        }));
      }
    }
    if (view) {
      root.appendChild(buildNode(view, state, 0));
    } else {
      root.appendChild(el('p', {
        class: 'plugin-ui__empty',
        text: '该插件未提供可渲染的 UI 描述（view）。'
      }));
    }
  }
  state.draw = draw;
  draw();

  return {
    el: root,
    update(newData) {
      state.data = newData || {};
      draw();
    },
    destroy() {
      root.replaceChildren();
    },
    info: {
      id: (descriptor && descriptor.id) || '',
      schemaVersion: (descriptor && descriptor.schema_version) || 0,
      permissions: (descriptor && descriptor.permissions) || []
    }
  };
}

export default { SUPPORTED_TYPES, registerAction, unregisterAction, clearActions, getActionNames, renderPluginUI };
