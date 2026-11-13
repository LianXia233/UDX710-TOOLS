/**
 * 设备控制（系统）：重启 / 关机 / 清缓存 / 系统时间 / 定时重启 / 手机壳模式。
 *  - reboot / poweroff / 恢复出厂 等不可逆操作统一走 confirm-dialog。
 *  - 手机壳模式需要输入确认短语（requireText）后才可提交。
 */
import { el, button, field, selectField, switchField } from '../utils/dom.js';
import { fieldOf, safeText, formatDate } from '../utils/format.js';

let ctx = null;

function post(path, body) {
  return ctx.api(path, { method: 'POST', body, signal: ctx.signal });
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // 设备控制
  const ctrl = ctx.card({ title: '设备控制', subtitle: '/api/device_control', span: 6, icon: 'power' });
  const rebootBtn = button('重启设备', {
    variant: 'danger', icon: 'refresh',
    onClick: async () => {
      const ok = await ctx.confirm({ title: '重启设备', message: '确定要重启设备吗？设备将在几秒后重启。', danger: true });
      if (!ok) return;
      try { await post('/api/device_control', { action: 'reboot' }); ctx.toast.success('重启命令已发送'); }
      catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const powerBtn = button('设备关机', {
    variant: 'danger', icon: 'power',
    onClick: async () => {
      const ok = await ctx.confirm({ title: '设备关机', message: '确定要关闭设备电源吗？此操作不可撤销。', danger: true });
      if (!ok) return;
      try { await post('/api/device_control', { action: 'poweroff' }); ctx.toast.success('关机命令已发送'); }
      catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const cacheBtn = button('清除系统缓存', {
    variant: 'outline', icon: 'trash',
    onClick: async () => {
      const ok = await ctx.confirm({ message: '清除系统缓存？', danger: false });
      if (!ok) return;
      try { await post('/api/clear_cache', {}); ctx.toast.success(ctx.t('common.saved')); }
      catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  ctrl.body.append(el('div', { class: 'alert alert--danger', text: '重启 / 关机为不可逆操作，请确保重要数据已保存。' }),
    el('div', { class: 'row' }, rebootBtn, powerBtn, cacheBtn));
  grid.appendChild(ctrl.el);

  // 系统时间
  const timeCard = ctx.card({ title: '系统时间', span: 6, icon: 'clock' });
  const timeLabel = el('span', { class: 'kv__v', text: '—' });
  const syncBtn = button('NTP 同步', {
    variant: 'primary', icon: 'refresh',
    onClick: async () => {
      try { await post('/api/set/time', {}); ctx.toast.success('时间同步成功'); loadTime(); }
      catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  timeCard.body.append(el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '当前时间' }), timeLabel), syncBtn);
  grid.appendChild(timeCard.el);

  // 定时重启
  const cronCard = ctx.card({ title: '定时重启', subtitle: '/api/set/reboot', span: 6, icon: 'clock' });
  const days = selectField({
    label: '重启日期', name: 'rday', options: [
      { value: '*', label: '每天' }, { value: '1', label: '周一' }, { value: '2', label: '周二' },
      { value: '3', label: '周三' }, { value: '4', label: '周四' }, { value: '5', label: '周五' },
      { value: '6', label: '周六' }, { value: '0', label: '周日' }
    ]
  });
  const hour = field({ label: '小时', name: 'hour', type: 'number', value: '3', attrs: { min: '0', max: '23' } });
  const minute = field({ label: '分钟', name: 'minute', type: 'number', value: '0', attrs: { min: '0', max: '59' } });
  const cronStatus = el('span', { class: 'muted', text: '读取中…' });
  const saveCron = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      const q = `day=${encodeURIComponent(days.input.value)}&hour=${encodeURIComponent(hour.input.value)}&minute=${encodeURIComponent(minute.input.value)}`;
      try {
        await ctx.api(`/api/set/reboot?${q}`, { signal: ctx.signal });
        ctx.toast.success(ctx.t('common.saved'));
        loadCron();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const clearCron = button('清除定时重启', {
    variant: 'ghost',
    onClick: async () => {
      const ok = await ctx.confirm({ message: '清除定时重启配置？', danger: true });
      if (!ok) return;
      try {
        await ctx.api('/api/claen/cron', { signal: ctx.signal }); // 保留旧拼写
        ctx.toast.success(ctx.t('common.deleted'));
        loadCron();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  cronCard.body.append(
    el('p', { class: 'muted' }, cronStatus),
    el('div', { class: 'form-grid' }, days.wrap, hour.wrap, minute.wrap),
    el('div', { class: 'row' }, saveCron, clearCron));
  grid.appendChild(cronCard.el);

  // 手机壳模式
  const phoneCard = ctx.card({ title: '手机壳模式', subtitle: '/api/phone-case', span: 6, icon: 'phone' });
  const phoneSwitch = switchField({ label: '修复手机壳 USB 无网问题（非手机壳勿开启）', checked: false });
  phoneCard.body.append(
    el('div', { class: 'alert alert--warning', text: '非手机壳设备开启后可能导致 USB 断网 / 网络异常，甚至需要重新刷机恢复。' }),
    phoneSwitch.wrap);
  phoneSwitch.input.addEventListener('change', async () => {
    const next = phoneSwitch.input.checked;
    const ok = await ctx.confirm({
      title: '危险操作', message: '确定切换手机壳模式吗？',
      danger: true, requireText: '已知晓风险'
    });
    if (!ok) { phoneSwitch.input.checked = !next; return; }
    try {
      await post('/api/phone-case', { enabled: next });
      ctx.toast.success(ctx.t('common.saved'));
    } catch (e) {
      ctx.toast.error(ctx.t('common.requestFailed'));
      phoneSwitch.input.checked = !next;
    }
  });
  grid.appendChild(phoneCard.el);

  async function loadTime() {
    try {
      const res = await ctx.api('/api/get/time', { signal: ctx.signal });
      const o = (res && typeof res === 'object') ? res : {};
      // 后端同时给出 time（本地可读）与 timestamp；缺 time 时用 timestamp 兜底格式化，
      // 避免直接把整个对象 JSON.stringify 出来显示。
      let text = o.time;
      if (!text && o.timestamp) {
        const d = new Date(Number(o.timestamp) * 1000);
        text = d.toLocaleString();
      }
      timeLabel.textContent = safeText(text);
    } catch (e) { timeLabel.textContent = '—'; }
  }
  const DAY_NAMES = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];
  async function loadCron() {
    try {
      const res = await ctx.api('/api/get/first-reboot', { signal: ctx.signal });
      const o = (res && typeof res === 'object') ? res : {};
      const enabled = !!o.enabled;
      const dayList = Array.isArray(o.days) ? o.days.map(Number) : [];
      const hh = String(Number(o.hour) || 0).padStart(2, '0');
      const mm = String(Number(o.minute) || 0).padStart(2, '0');
      if (!enabled || !dayList.length) {
        cronStatus.textContent = '当前配置：未启用';
        return;
      }
      const when = dayList.length >= 7
        ? '每天'
        : dayList.map((d) => DAY_NAMES[d] || `周${d}`).join('、');
      cronStatus.textContent = `当前配置：${when} ${hh}:${mm}`;
      // 回填表单，便于在现有配置上修改而不是从零重填
      days.input.value = dayList.length >= 7 ? '*' : String(dayList[0]);
      hour.input.value = String(Number(o.hour) || 0);
      minute.input.value = String(Number(o.minute) || 0);
    } catch (e) { cronStatus.textContent = '当前配置：未设置'; }
  }
  (async () => {
    try {
      const res = await ctx.api('/api/phone-case', { signal: ctx.signal });
      const o = (res && typeof res === 'object') ? (res.enabled !== undefined ? res : (res.data || res.Data || {})) : {};
      phoneSwitch.input.checked = !!(o.enabled);
    } catch (e) { /* 忽略 */ }
  })();

  loadTime();
  loadCron();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
}
