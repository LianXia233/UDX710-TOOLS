/**
 * DOM 构建工具：函数式、无框架、零依赖。
 *
 * 安全约束：
 *  - 一律使用 createElement / createElementNS + textContent 构建节点；
 *  - 不提供 innerHTML 写入能力（props.html 会被忽略）；
 *  - 不使用模板字符串拼接用户数据到标记中。
 * 组件内的小片段优先用这些函数直接创建 DOM，需要跨页面复用时再考虑 Custom Elements。
 */

export const SVG_NS = 'http://www.w3.org/2000/svg';

// 以属性方式设置会失效、必须以 DOM 属性设置的布尔/值键
const PROP_KEYS = new Set([
  'value', 'checked', 'disabled', 'selected', 'required', 'multiple',
  'readOnly', 'indeterminate'
]);

function appendOne(parent, child) {
  if (child == null || child === false || child === true) return;
  if (Array.isArray(child)) {
    for (const item of child) appendOne(parent, item);
    return;
  }
  if (child instanceof Node) {
    parent.appendChild(child);
    return;
  }
  // 字符串 / 数字一律作为文本节点，绝不解析为标记
  parent.appendChild(document.createTextNode(String(child)));
}

/**
 * 创建元素。
 * @param {string} tag 标签名；前缀 'svg:' 时使用 SVG 命名空间。
 * @param {object} props 属性集合：class/className、text、style、dataset、attrs、on、
 *   value/checked/... 以及任意 HTML/SVG 属性。
 * @param {...any} children 子节点（Node / 字符串 / 数组 / null）。
 */
export function el(tag, props = {}, ...children) {
  let name = tag;
  let node;
  if (tag.startsWith('svg:')) {
    name = tag.slice(4);
    node = document.createElementNS(SVG_NS, name);
  } else {
    node = document.createElement(name);
  }

  const p = props || {};
  for (const key of Object.keys(p)) {
    const value = p[key];
    if (value == null) continue;
    switch (key) {
      case 'class':
      case 'className':
        node.setAttribute('class', String(value));
        break;
      case 'text':
      case 'textContent':
        node.textContent = String(value);
        break;
      case 'html':
        // 明确禁用：不允许注入 HTML 标记
        break;
      case 'style':
        if (typeof value === 'object') Object.assign(node.style, value);
        break;
      case 'dataset':
        if (typeof value === 'object') Object.assign(node.dataset, value);
        break;
      case 'attrs':
        if (typeof value === 'object') {
          for (const k of Object.keys(value)) {
            if (value[k] != null) node.setAttribute(k, String(value[k]));
          }
        }
        break;
      case 'on':
        if (typeof value === 'object') {
          for (const type of Object.keys(value)) {
            if (typeof value[type] === 'function') {
              node.addEventListener(type, value[type]);
            }
          }
        }
        break;
      default:
        if (PROP_KEYS.has(key)) {
          node[key] = value;
        } else {
          node.setAttribute(key, String(value));
        }
    }
  }

  if (p.children != null) appendOne(node, p.children);
  for (const child of children) appendOne(node, child);
  return node;
}

/** 清空容器（replaceChildren 比 innerHTML='' 更安全、更明确）。 */
export function clear(node) {
  if (node && typeof node.replaceChildren === 'function') node.replaceChildren();
}

/** 批量追加子节点（自动展开数组）。 */
export function append(parent, ...nodes) {
  for (const n of nodes) appendOne(parent, n);
  return parent;
}

/** 本地 SVG sprite 图标引用。 */
export function icon(name, className = '') {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('class', `icon ${className}`.trim());
  svg.setAttribute('aria-hidden', 'true');
  const use = document.createElementNS(SVG_NS, 'use');
  use.setAttribute('href', `/icons/sprite.svg#icon-${name}`);
  svg.appendChild(use);
  return svg;
}

/** 按钮；variant 支持空格分隔的多个标记，如 'primary sm'、'danger'、'ghost icon-btn'。 */
export function button(label, opts = {}) {
  const {
    variant = '', icon: iconName = null, onClick = null, type = 'button',
    disabled = false, title = '', className = '', name = '', id = ''
  } = opts;
  const classes = ['btn'];
  for (const token of String(variant).split(/\s+/).filter(Boolean)) {
    classes.push(`btn--${token}`);
  }
  if (className) classes.push(className);
  const kids = [];
  if (iconName) kids.push(icon(iconName));
  if (label != null && label !== '') kids.push(el('span', { class: 'btn__label' }, String(label)));
  const node = el('button', {
    type, class: classes.join(' '), title, disabled, name, id
  }, ...kids);
  if (onClick) node.addEventListener('click', onClick);
  return node;
}

/** 过滤副标题中的原始后端 API 接口路径，避免在 UI 暴露诸如 /api/usb/mode 等技术参数 */
function sanitizeSubtitle(sub) {
  if (!sub || typeof sub !== 'string') return '';
  const trimmed = sub.trim();
  if (/^(?:GET|POST|PUT|DELETE)?\s*\/api\//i.test(trimmed)) return '';
  return trimmed.replace(/\s*(?:来自\s*)?(?:GET|POST|PUT|DELETE)?\s*\/api\/[a-zA-Z0-9_\-\/]+/gi, '').trim();
}

/** 卡片：返回 { el, body, head, setTitle }。span 为 12 栏网格中所占列数。 */
export function card({ title = '', subtitle = '', span = 12, icon: iconName = null, actions = [], className = '' } = {}) {
  const headKids = [];
  if (iconName) headKids.push(el('span', { class: 'card__icon' }, icon(iconName)));
  const textWrap = el('div', { class: 'card__titles' });
  const titleEl = title ? el('h3', { class: 'card__title', text: title }) : null;
  if (titleEl) textWrap.appendChild(titleEl);
  const cleanSub = sanitizeSubtitle(subtitle);
  if (cleanSub) textWrap.appendChild(el('p', { class: 'card__subtitle', text: cleanSub }));
  headKids.push(textWrap);
  headKids.push(el('div', { class: 'card__actions' }, ...actions));
  const head = (title || cleanSub || iconName || actions.length)
    ? el('div', { class: 'card__head' }, ...headKids)
    : null;
  const body = el('div', { class: 'card__body' });
  const root = el('section', {
    class: `card col-span-${span} ${className}`.trim()
  }, ...(head ? [head] : []), body);
  return {
    el: root,
    body,
    head,
    setTitle(text) { if (titleEl) titleEl.textContent = text; }
  };
}

/**
 * 卡片组件包装（原可折叠 Accordion）。
 * 根据用户明确要求：“不要折叠文案了”，全部平铺直接呈现，避免折叠隐藏关键参数。
 */
export function collapsibleCard(opts = {}) {
  const { title = '', subtitle = '', span = 12, icon = null, className = '' } = opts;
  const c = card({ title, subtitle, span, icon, className });
  return {
    el: c.el,
    body: c.body,
    head: c.head,
    toggle() {}
  };
}

/** 键值展示行。value 可为 Node 或文本。 */
export function kv(label, value, opts = {}) {
  const { tone = '', mono = false } = opts;
  const v = el('span', {
    class: `kv__v${mono ? ' mono' : ''}${tone ? ` tone-${tone}` : ''}`
  });
  appendOne(v, value);
  return el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: label }), v);
}

/** 带标签的表单控件；control 可传入自定义 input/select/textarea。 */
export function field({ label = '', name = '', type = 'text', value = '', placeholder = '',
  required = false, hint = '', control = null, attrs = {} } = {}) {
  const id = `f-${name || Math.random().toString(36).slice(2, 9)}`;
  const input = control || el('input', {
    id, name, type, value, placeholder, required, class: 'input', ...attrs
  });
  const kids = [];
  if (label) kids.push(el('label', { class: 'field__label', for: id, text: label }));
  kids.push(input);
  if (hint) kids.push(el('p', { class: 'field__hint', text: hint }));
  return { wrap: el('div', { class: 'field' }, ...kids), input };
}

/** 下拉选择框。options: [{value,label}] 或字符串数组。 */
export function selectField({ label = '', name = '', options = [], value = '', hint = '', attrs = {} } = {}) {
  const id = `f-${name || Math.random().toString(36).slice(2, 9)}`;
  const select = el('select', { id, name, class: 'input select', ...attrs });
  for (const opt of options) {
    const item = typeof opt === 'object'
      ? { value: opt.value, label: opt.label != null ? opt.label : opt.value }
      : { value: opt, label: opt };
    select.appendChild(el('option', { value: item.value, text: item.label }));
  }
  if (value !== '') select.value = String(value);
  const kids = [];
  if (label) kids.push(el('label', { class: 'field__label', for: id, text: label }));
  kids.push(select);
  if (hint) kids.push(el('p', { class: 'field__hint', text: hint }));
  return { wrap: el('div', { class: 'field' }, ...kids), input: select };
}

/** 开关（原生 checkbox + CSS 呈现）。 */
export function switchField({ label = '', name = '', checked = false, hint = '' } = {}) {
  const input = el('input', { type: 'checkbox', class: 'switch__input', name, checked });
  const wrap = el('label', { class: 'switch' }, input,
    el('span', { class: 'switch__track' }, el('span', { class: 'switch__thumb' })),
    el('span', { class: 'switch__label', text: label }));
  const outer = el('div', { class: 'field' }, wrap);
  if (hint) outer.appendChild(el('p', { class: 'field__hint', text: hint }));
  return { wrap: outer, input };
}

/** 空态。 */
export function emptyState(text, iconName = 'inbox') {
  return el('div', { class: 'empty' },
    icon(iconName, 'empty__icon'),
    el('p', { class: 'empty__text', text }));
}

/** 加载态。 */
export function loadingState(text = '加载中...') {
  return el('div', { class: 'loading' },
    el('span', { class: 'spinner' }),
    el('span', { text }));
}

/** 切换忙碌状态（禁用按钮、加 busy 类）。 */
export function setBusy(node, busy) {
  if (!node) return;
  node.classList.toggle('is-busy', !!busy);
  if ('disabled' in node) node.disabled = !!busy;
}

/** 工具条容器。 */
export function toolbar(...children) {
  return el('div', { class: 'toolbar' }, ...children);
}

/** 行容器（Flex）。 */
export function row(...children) {
  return el('div', { class: 'row' }, ...children);
}

/**
 * 页面作用域：集中管理 AbortController、定时器、全局监听与订阅。
 * 每个页面在 render 时创建，destroy 时统一清理，避免请求/定时器/监听泄漏。
 * 与框架无关，纯原生实现。
 */
export function createScope() {
  const controller = new AbortController();
  const cleanups = [];
  let destroyed = false;

  function onCleanup(fn) {
    if (typeof fn === 'function') cleanups.push(fn);
    return fn;
  }

  return {
    signal: controller.signal,
    abort() { controller.abort(); },
    onCleanup,
    /** 在常驻目标（window/document/store/ws 等）上注册监听，离开页面自动移除。 */
    on(target, type, fn, opts) {
      target.addEventListener(type, fn, opts);
      return onCleanup(() => target.removeEventListener(type, fn, opts));
    },
    interval(fn, ms) {
      const id = setInterval(fn, ms);
      return onCleanup(() => clearInterval(id));
    },
    timeout(fn, ms) {
      const id = setTimeout(fn, ms);
      return onCleanup(() => clearTimeout(id));
    },
    /** 幂等销毁：abort 未完成请求，逆序执行所有清理回调。 */
    destroy() {
      if (destroyed) return;
      destroyed = true;
      controller.abort();
      for (let i = cleanups.length - 1; i >= 0; i -= 1) {
        try { cleanups[i](); } catch (e) { /* 单个清理异常不阻断其余清理 */ }
      }
      cleanups.length = 0;
    }
  };
}
