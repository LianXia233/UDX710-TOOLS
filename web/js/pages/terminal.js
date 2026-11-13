/**
 * Web 终端（保留 Web 任意 Shell 能力）：
 *  - 交互式会话 WebSocket /api/terminal/ws：
 *      客户端发 { type:"input", data:"..." } 或 { type:"resize", cols, rows }；
 *      服务端推 { type:"output", data } / { type:"exit", code } / { type:"error", message }。
 *  - 一次性命令 POST /api/shell {command} -> {stdout, stderr, exit_code}。
 *  - 完整终端 UI：输出区、命令输入、历史上下键、清屏、连接状态、断线重连提示。
 *
 * 安全：页面仅在登录后可达（app.js 路由守卫），前端不做任何权限绕过；
 * 顶部显著标注「管理员诊断功能 · 仅限授权账号 · 所有命令将被审计」。
 */
import { el, icon, button, setBusy } from '../utils/dom.js';
import { fieldOf, safeText } from '../utils/format.js';
import { ManagedSocket } from '../core/websocket.js';

const MAX_LINES = 500;
let ctx = null;
let socket = null;
let screenEl = null;
let statusDot = null;
let statusText = null;
let noticeEl = null;
const history = [];
let historyIndex = 0;

function stripAnsi(text) {
  return String(text == null ? '' : text).replace(/\x1b\[[0-9;]*[A-Za-z]/g, '');
}

function appendText(text, cls) {
  if (!screenEl) return;
  const clean = stripAnsi(text);
  const parts = clean.split('\n');
  for (const part of parts) {
    screenEl.appendChild(el('div', {
      class: `terminal__line${cls ? ` ${cls}` : ''}`, text: part
    }));
  }
  while (screenEl.childNodes.length > MAX_LINES) {
    screenEl.removeChild(screenEl.firstChild);
  }
  screenEl.scrollTop = screenEl.scrollHeight;
}

function appendSystem(text) { appendText(text, 'terminal__line--system'); }
function appendError(text) { appendText(text, 'terminal__line--error'); }

function clearScreen() {
  if (screenEl) screenEl.replaceChildren();
}

function setStatus(kind, text) {
  if (statusDot) statusDot.className = `dot dot--${kind}`;
  if (statusText) statusText.textContent = text;
  if (noticeEl) noticeEl.style.display = (kind === 'ok') ? 'none' : 'block';
}

function sendResize() {
  if (!socket || !screenEl) return;
  const rect = screenEl.getBoundingClientRect();
  const cols = Math.max(20, Math.floor(rect.width / 8));
  const rows = Math.max(10, Math.floor(rect.height / 18));
  socket.send({ type: 'resize', cols, rows });
}

function onMessage(detail) {
  if (detail == null) return;
  if (typeof detail !== 'object') { appendText(String(detail)); return; }
  switch (detail.type) {
    case 'output': appendText(detail.data != null ? detail.data : ''); break;
    case 'exit': appendSystem(`进程退出，exit_code = ${safeText(detail.code, '?')}`); break;
    case 'error': appendError(detail.message || '终端错误'); break;
    case 'connected':
    case 'ready': appendSystem('交互式会话已就绪'); break;
    default: appendSystem(safeText(detail));
  }
}

export function render(container, context) {
  ctx = context;
  history.length = 0;
  historyIndex = 0;

  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // 显著安全标注
  const banner = el('div', { class: 'alert alert--danger terminal__banner col-span-12' },
    icon('alert-triangle'),
    el('strong', { text: '管理员诊断功能 · 仅限授权账号 · 所有命令将被审计' }),
    el('span', { text: ' 本页仅在登录后可用；交互式终端与单次命令均由服务端记录审计日志。' }));
  grid.appendChild(banner);

  // 交互式终端
  const termCard = ctx.card({ title: '交互式终端', subtitle: '实时安全 Shell 会话', span: 12, icon: 'terminal' });
  statusDot = el('i', { class: 'dot dot--warning' });
  statusText = el('span', { text: ctx.t('app.connecting') });
  const statusLight = el('span', { class: 'terminal__status' }, statusDot, statusText);
  const clearBtn = button(ctx.t('common.clear'), { variant: 'ghost sm', icon: 'x', onClick: clearScreen });
  const bar = el('div', { class: 'terminal__bar' },
    el('div', { class: 'row' },
      el('span', { class: 'terminal__dots' },
        el('i', { style: { background: '#ef4444' } }),
        el('i', { style: { background: '#f59e0b' } }),
        el('i', { style: { background: '#22c55e' } })),
      el('span', { class: 'terminal__bar-title', text: 'Shell' })),
    el('div', { class: 'row' }, statusLight, clearBtn));

  screenEl = el('div', { class: 'terminal__screen', tabindex: '0', 'aria-label': '终端输出' });
  noticeEl = el('div', {
    class: 'terminal__notice',
    text: '连接断开，正在指数退避重连；重连期间命令不会发送。'
  });
  const prompt = el('span', { class: 'terminal__prompt', text: '$' });
  const input = el('input', {
    class: 'terminal__input', type: 'text', autocomplete: 'off',
    spellcheck: false, placeholder: '输入命令并回车；↑/↓ 历史；Ctrl+L 清屏；Ctrl+C 中断'
  });
  const inputRow = el('div', { class: 'terminal__input-row' }, prompt, input);

  const terminal = el('div', { class: 'terminal' }, bar, noticeEl, screenEl, inputRow);
  termCard.body.appendChild(terminal);
  grid.appendChild(termCard.el);

  input.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') {
      ev.preventDefault();
      const cmd = input.value;
      input.value = '';
      if (!cmd) return;
      history.push(cmd);
      historyIndex = history.length;
      if (!socket || socket.status !== 'open') {
        appendError('未连接，命令未发送。请等待重连完成。');
        return;
      }
      appendText(`$ ${cmd}`);
      socket.send({ type: 'input', data: `${cmd}\n` });
    } else if (ev.key === 'ArrowUp') {
      ev.preventDefault();
      if (history.length) {
        historyIndex = Math.max(0, historyIndex - 1);
        input.value = history[historyIndex] || '';
      }
    } else if (ev.key === 'ArrowDown') {
      ev.preventDefault();
      if (history.length) {
        historyIndex = Math.min(history.length, historyIndex + 1);
        input.value = history[historyIndex] || '';
      }
    } else if (ev.key === 'l' && ev.ctrlKey) {
      ev.preventDefault();
      clearScreen();
    } else if (ev.key === 'c' && ev.ctrlKey) {
      ev.preventDefault();
      if (socket && socket.status === 'open') socket.send({ type: 'input', data: '\u0003' });
    }
  });
  screenEl.addEventListener('click', () => input.focus());

  // 建立交互式会话
  socket = new ManagedSocket('/api/terminal/ws', {
    withToken: true,
    onOpen: () => {
      setStatus('ok', ctx.t('app.online'));
      appendSystem('终端会话已就绪');
      sendResize();
      input.focus();
    },
    onJSON: onMessage,
    onClose: () => setStatus('error', ctx.t('app.offline')),
    onStatus: (s, detail) => {
      if (s === 'connecting') setStatus('warning', ctx.t('app.connecting'));
      else if (s === 'open') setStatus('ok', ctx.t('app.online'));
      else if (s === 'reconnecting') {
        const n = detail && detail.attempt ? `（第 ${detail.attempt} 次）` : '';
        setStatus('warning', `${ctx.t('app.connecting')} ${n}`);
      } else if (s === 'closed' || s === 'error' || s === 'failed') {
        setStatus('error', ctx.t('app.offline'));
      }
    }
  });
  ctx.onCleanup(() => { if (socket) socket.close(); });
  socket.connect();
  ctx.on(window, 'resize', sendResize);

  // 一次性命令（可选折叠卡片）
  const shellCard = (ctx.collapsibleCard || ctx.card)({
    title: '单次命令快速执行',
    subtitle: '独立进程即时诊断执行',
    span: 12,
    icon: 'code',
    badge: '点击展开'
  });
  const shellInput = el('input', { class: 'input', placeholder: '例如：uname -a', style: { flex: '1' } });
  const shellOut = el('pre', {
    class: 'mono',
    style: { background: 'var(--surface-muted)', padding: '12px', borderRadius: '8px', overflow: 'auto', maxHeight: '220px', whiteSpace: 'pre-wrap' },
    text: '（输出将显示在这里）'
  });
  const runBtn = button('执行', {
    variant: 'primary', icon: 'play',
    onClick: async () => {
      const cmd = shellInput.value.trim();
      if (!cmd) return;
      setBusy(runBtn, true);
      try {
        const res = await ctx.api('/api/shell', {
          method: 'POST', body: { command: cmd }, signal: ctx.signal
        });
        const stdout = fieldOf(res, 'stdout', '');
        const stderr = fieldOf(res, 'stderr', '');
        const code = fieldOf(res, 'exit_code', fieldOf(res, 'code', ''));
        shellOut.textContent =
          `exit_code: ${safeText(code, '?')}\n${safeText(stdout, '')}` +
          (stderr ? `\n[stderr]\n${safeText(stderr)}` : '');
      } catch (err) {
        shellOut.textContent = `执行失败：${(err && err.message) || err}`;
      } finally {
        setBusy(runBtn, false);
      }
    }
  });
  shellCard.body.append(
    el('div', { class: 'row' }, shellInput, runBtn),
    shellOut
  );
  grid.appendChild(shellCard.el);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy(); // abort 请求、移除 resize 监听、关闭 socket
  if (socket) { socket.close(); socket = null; }
  ctx = null;
  screenEl = null;
  statusDot = null;
  statusText = null;
  noticeEl = null;
}
