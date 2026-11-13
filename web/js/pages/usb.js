/**
 * USB 模式：查看当前模式、永久/临时切换、热切换（立即生效）。
 * 「切 USB」属于危险操作，所有切换均走 confirm-dialog（热切换还需输入确认短语）。
 */
import { el, button, selectField, switchField } from '../utils/dom.js';
import { fieldOf, safeText } from '../utils/format.js';

const MODES = [
  { value: 'rndis', label: 'RNDIS（网络共享，兼容 Windows）' },
  { value: 'ecm', label: 'ECM（以太网）' },
  { value: 'ncm', label: 'NCM（仅测试）' },
  { value: 'acm', label: 'ACM（串口）' },
  { value: 'mtp', label: 'MTP（文件传输）' },
  { value: 'none', label: '禁用 USB 功能' }
];

let ctx = null;

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const currentCard = ctx.card({ title: '当前 USB 模式', subtitle: '查看与切换终端 USB 复合设备功能', span: 12, icon: 'usb' });
  const currentLabel = el('span', { class: 'badge badge--primary', text: '读取中…' });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: load });
  currentCard.head.querySelector('.card__actions').appendChild(refreshBtn);
  currentCard.body.append(
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '当前模式' }), currentLabel),
    el('div', { class: 'alert alert--warning', text: '切换 USB 模式可能导致设备断网、IP 不分配；请确保了解恢复方法后再操作。' }));
  grid.appendChild(currentCard.el);

  const switchCard = ctx.card({ title: '切换 USB 模式', subtitle: '配置目标协议与生效策略', span: 6, icon: 'usb' });
  const mode = selectField({ label: '目标模式', name: 'usbmode', options: MODES });
  const permanent = switchField({ label: '永久生效（重启后仍保留）', checked: false });
  const doSwitch = button('切换模式', {
    variant: 'danger', icon: 'usb',
    onClick: async () => {
      const ok = await ctx.confirm({
        title: '切换 USB 模式',
        message: `确定切换到 ${mode.input.value} 模式吗？USB 连接可能短暂断开。`,
        danger: true
      });
      if (!ok) return;
      try {
        await ctx.api('/api/usb/mode', {
          method: 'POST',
          body: { mode: mode.input.value, permanent: permanent.input.checked },
          signal: ctx.signal
        });
        ctx.toast.success(ctx.t('common.saved'));
        load();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const hotSwitch = button('热切换（立即生效）', {
    variant: 'danger', icon: 'zap',
    onClick: async () => {
      const ok = await ctx.confirm({
        title: '热切换确认',
        message: `即将热切换到 ${mode.input.value}，USB 连接会短暂断开（约 2 秒），确定继续？`,
        danger: true, requireText: 'SWITCH'
      });
      if (!ok) return;
      try {
        await ctx.api('/api/usb-advance', {
          method: 'POST', body: { mode: mode.input.value }, signal: ctx.signal
        });
        ctx.toast.success(ctx.t('common.saved'));
        load();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  switchCard.body.append(mode.wrap, permanent.wrap,
    el('div', { class: 'row' }, doSwitch, hotSwitch));
  grid.appendChild(switchCard.el);

  // 可折叠说明卡片，收起冗余提示文本
  const tipCard = (ctx.collapsibleCard || ctx.card)({
    title: '模式说明与兼容性指南',
    span: 6,
    icon: 'info',
    badge: '点击展开'
  });
  tipCard.body.appendChild(el('ul', { class: 'muted' },
    el('li', { text: 'RNDIS：兼容性好，适合 Windows / OpenWrt / Padavan 路由器。' }),
    el('li', { text: 'ECM/NCM：兼容性差，不建议在 Windows 设备使用。' }),
    el('li', { text: '永久模式重启后仍生效；临时模式再次重启恢复默认。' }),
    el('li', { text: '热切换立即生效但会短暂断开 USB 连接。' })));
  grid.appendChild(tipCard.el);

  async function load() {
    try {
      const res = await ctx.api('/api/usb/mode', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const v = (res && typeof res === 'object' && res.mode !== undefined) ? res.mode : o.mode;
      currentLabel.textContent = safeText(v, '未知');
      mode.input.value = String(v || 'rndis');
    } catch (e) { currentLabel.textContent = '未知'; }
  }

  load();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
}
