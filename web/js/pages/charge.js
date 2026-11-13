/**
 * 充电控制：充电策略配置（起止电量）、手动开启 / 停止充电、电池状态展示。
 * 表单使用原生 Constraint Validation（起充电量必须小于停止电量）。
 */
import { el, button, field, switchField } from '../utils/dom.js';
import { fieldOf, safeText, formatPercent } from '../utils/format.js';

let ctx = null;

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const statusCard = ctx.card({ title: '电池状态', subtitle: '/api/charge/config', span: 5, icon: 'battery' });
  const rows = {};
  const kv = (label, key) => {
    const v = el('span', { class: 'kv__v', text: '—' });
    rows[key] = v;
    return el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: label }), v);
  };
  statusCard.body.append(
    kv('电量', 'level'), kv('状态', 'status'), kv('健康', 'health'),
    kv('电压', 'voltage'), kv('电流', 'current'), kv('温度', 'temperature'));
  const levelBar = el('span', { style: { width: '0%' } });
  statusCard.body.appendChild(el('div', { class: 'progress', style: { marginTop: '12px' } }, levelBar));
  grid.appendChild(statusCard.el);

  const ctrlCard = ctx.card({ title: '充电策略', span: 7, icon: 'zap' });
  const enabled = switchField({ label: '启用充电限制', checked: false });
  const start = field({ label: '低于此电量开始充电（%）', name: 'start', type: 'number', value: '20', attrs: { min: '1', max: '99' } });
  const stop = field({ label: '高于此电量停止充电（%）', name: 'stop', type: 'number', value: '80', attrs: { min: '2', max: '100' } });
  const save = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      const s = Number(start.input.value);
      const t = Number(stop.input.value);
      if (!start.input.checkValidity() || !stop.input.checkValidity()) {
        start.input.reportValidity(); stop.input.reportValidity(); return;
      }
      if (s >= t) { ctx.toast.error('开始充电电量必须小于停止充电电量'); return; }
      try {
        await ctx.api('/api/charge/config', {
          method: 'POST',
          body: { enabled: enabled.input.checked, startThreshold: s, stopThreshold: t },
          signal: ctx.signal
        });
        ctx.toast.success(ctx.t('common.saved'));
        load();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const onBtn = button('手动开启充电', {
    variant: 'outline', icon: 'zap',
    onClick: async () => { try { await ctx.api('/api/charge/on', { method: 'POST', signal: ctx.signal }); ctx.toast.success(ctx.t('common.saved')); load(); } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); } }
  });
  const offBtn = button('手动停止充电', {
    variant: 'outline', icon: 'power',
    onClick: async () => { try { await ctx.api('/api/charge/off', { method: 'POST', signal: ctx.signal }); ctx.toast.success(ctx.t('common.saved')); load(); } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); } }
  });
  ctrlCard.body.append(el('div', { class: 'form-grid' }, enabled.wrap, start.wrap, stop.wrap),
    el('div', { class: 'row' }, save, onBtn, offBtn),
    el('p', { class: 'field__hint', text: '当电量低于开始阈值时自动充电，充至停止阈值时停止，可延长电池寿命。' }));
  grid.appendChild(ctrlCard.el);

  async function load() {
    try {
      const res = await ctx.api('/api/charge/config', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const set = (k, v) => { if (rows[k]) rows[k].textContent = safeText(v); };
      // supported=false 表示设备没有 /sys/class/power_supply（本机为模组形态），
      // 此时显示「本机无电池」而不是一排破折号，避免被误判为读取失败。
      if (o.supported === false) {
        for (const k of ['level', 'status', 'health', 'voltage', 'current', 'temperature']) {
          set(k, '本机无电池');
        }
        levelBar.style.width = '0%';
      } else {
        const level = Number(fieldOf(o, 'level', fieldOf(o, 'capacity', 0))) || 0;
        set('level', formatPercent(level));
        levelBar.style.width = `${Math.min(100, level)}%`;
        set('status', fieldOf(o, 'status', fieldOf(o, 'charging')));
        set('health', fieldOf(o, 'health'));
        set('voltage', fieldOf(o, 'voltage'));
        set('current', fieldOf(o, 'current'));
        set('temperature', fieldOf(o, 'temperature'));
      }
      enabled.input.checked = !!(o.enabled !== undefined ? o.enabled : res.enabled);
      if (o.startThreshold != null) start.input.value = o.startThreshold;
      if (o.stopThreshold != null) stop.input.value = o.stopThreshold;
    } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
  }

  load();
  ctx.interval(load, 15000);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
}
