/**
 * AT 命令调试：发送 AT 命令（/api/at）、快捷查询、历史记录（↑/↓）、
 * 清屏、IMEI 修改（构造 AT 命令并走同一接口，带 15 位校验）。
 */
import { el, button, field, selectField, toolbar } from '../utils/dom.js';
import { fieldOf, safeText } from '../utils/format.js';
import { isImei } from '../utils/validation.js';

const QUICK_COMMANDS = [
  'AT', 'ATI', 'AT+CSQ', 'AT+CREG?', 'AT+COPS?', 'AT+CGMI',
  'AT+CIMI', 'AT+ICCID?', 'AT+CPIN?', 'AT^MONSC'
];

let ctx = null;
let output = null;
const history = [];
let historyIndex = 0;

function appendLine(text, cls) {
  if (!output) return;
  output.appendChild(el('div', { class: `terminal__line${cls ? ` ${cls}` : ''}`, text }));
  while (output.childNodes.length > 300) output.removeChild(output.firstChild);
  output.scrollTop = output.scrollHeight;
}

export function render(container, context) {
  ctx = context;
  history.length = 0;
  historyIndex = 0;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const mainCard = ctx.card({ title: 'AT 命令', subtitle: 'POST /api/at', span: 8, icon: 'terminal' });
  const cmdField = field({ label: '命令', name: 'atcmd', placeholder: '输入 AT 命令，如 AT+CSQ' });
  const cmdInput = cmdField.input;
  const sendBtn = button(ctx.t('common.send'), { variant: 'primary', icon: 'send' });
  const clearBtn = button(ctx.t('common.clear'), { variant: 'ghost', icon: 'x' });
  output = el('div', { class: 'terminal__screen', style: { height: '360px' } });

  async function send(command) {
    const cmd = (command != null ? command : cmdInput.value).trim();
    if (!cmd) return;
    history.push(cmd);
    historyIndex = history.length;
    cmdInput.value = '';
    appendLine(`> ${cmd}`);
    try {
      const res = await ctx.api('/api/at', { method: 'POST', body: { command: cmd }, signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const text = typeof o === 'string' ? o
        : (o.result != null ? o.result : (o.output != null ? o.output : JSON.stringify(res, null, 2)));
      appendLine(safeText(text, ''));
    } catch (e) {
      appendLine(`错误：${(e && e.message) || e}`, 'terminal__line--error');
    }
  }

  sendBtn.addEventListener('click', () => send());
  cmdInput.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') { ev.preventDefault(); send(); }
    else if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      if (history.length) { historyIndex = Math.max(0, historyIndex - 1); cmdInput.value = history[historyIndex] || ''; }
    } else if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      if (history.length) { historyIndex = Math.min(history.length, historyIndex + 1); cmdInput.value = history[historyIndex] || ''; }
    }
  });
  clearBtn.addEventListener('click', () => { if (output) output.replaceChildren(); });

  mainCard.body.append(
    el('div', { class: 'row' }, cmdField.wrap, sendBtn, clearBtn),
    el('p', { class: 'field__hint', text: '↑/↓ 浏览历史；命令经服务端串行化执行，避免 AT 通道交错。' }),
    output);
  grid.appendChild(mainCard.el);

  const quickCard = ctx.card({ title: '快捷查询', span: 4, icon: 'list' });
  for (const q of QUICK_COMMANDS) {
    quickCard.body.appendChild(button(q, {
      variant: 'ghost sm', onClick: () => send(q), className: 'at-quick'
    }));
  }
  grid.appendChild(quickCard.el);

  // IMEI 修改
  const imeiCard = ctx.card({ title: 'IMEI 修改', subtitle: '通过 AT 命令写入', span: 12, icon: 'sim-card' });
  const imei = field({ label: '新 IMEI（15 位数字）', name: 'imei', placeholder: '15 位纯数字' }).input;
  const slot = selectField({ label: '卡槽', name: 'slot', options: [
    { value: '1', label: '卡槽 1' }, { value: '2', label: '卡槽 2' }
  ] });
  const imeiBtn = button('确认修改', {
    variant: 'danger', icon: 'pencil',
    onClick: async () => {
      if (!isImei(imei.value)) { ctx.toast.error('IMEI 必须是 15 位纯数字'); return; }
      const ok = await ctx.confirm({ message: `确定将卡槽 ${slot.input.value} 的 IMEI 修改为 ${imei.value}？`, danger: true });
      if (!ok) return;
      const reg = slot.input.value === '2' ? '10' : '7';
      send(`AT+EGMR=1,${reg},"${imei.value}"`);
    }
  });
  imeiCard.body.append(el('div', { class: 'row' }, imei.closest('.field'), slot.wrap, imeiBtn));
  grid.appendChild(imeiCard.el);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  output = null;
  history.length = 0;
}
