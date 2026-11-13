/**
 * 流量统计：概览 / 使用进度 / 流量限制 / 清除统计 / 实时趋势。
 *  - 实时采样只保留最近 300 个点，原生 SVG 绘制趋势（不引入图表库）。
 *  - 清除统计等破坏性操作走 confirm-dialog。
 */
import { el, button, field, switchField, loadingState } from '../utils/dom.js';
import { fieldOf, safeText, formatBytes, formatRate, formatPercent } from '../utils/format.js';

const MAX_POINTS = 300;
const W = 600;
const H = 140;

let ctx = null;
const rateSeries = [];
let lastBytes = 0;
let lineEl = null;
let areaEl = null;
let labelEl = null;
let refs = {};

function pushRate(v) {
  rateSeries.push(Number(v) || 0);
  if (rateSeries.length > MAX_POINTS) rateSeries.shift();
}

function pathOf(series, min, max) {
  if (series.length < 2) return '';
  const range = (max - min) || 1;
  const step = W / (series.length - 1);
  const out = [];
  for (let i = 0; i < series.length; i += 1) {
    const y = H - Math.max(0, Math.min(1, (series[i] - min) / range)) * H;
    out.push(`${i === 0 ? 'M' : 'L'}${(i * step).toFixed(1)},${y.toFixed(1)}`);
  }
  return out.join(' ');
}

function drawChart() {
  if (!lineEl) return;
  const max = Math.max(1, ...rateSeries);
  const d = pathOf(rateSeries, 0, max);
  lineEl.setAttribute('d', d);
  if (areaEl) areaEl.setAttribute('d', d ? `${d} L${W},${H} L0,${H} Z` : '');
  if (labelEl) labelEl.textContent = `当前速率 ${formatRate(rateSeries.length ? rateSeries[rateSeries.length - 1] : 0)} · 采样 ${rateSeries.length}/${MAX_POINTS}`;
}

function makeChart() {
  const svg = el('svg:svg', { class: 'sparkline', viewBox: `0 0 ${W} ${H}`, preserveAspectRatio: 'none' });
  for (let i = 0; i <= 3; i += 1) {
    svg.appendChild(el('svg:line', { x1: 0, y1: i * (H / 3), x2: W, y2: i * (H / 3), class: 'chart-grid' }));
  }
  areaEl = el('svg:path', { class: 'chart-area', d: '' });
  lineEl = el('svg:path', { class: 'chart-line', d: '' });
  labelEl = el('svg:text', { x: 6, y: 14, class: 'chart-text', text: '' });
  svg.append(areaEl, lineEl, labelEl);
  return svg;
}

function stat(label, id) {
  return el('div', { class: 'stat' },
    el('div', { class: 'stat__value', id, text: '—' }),
    el('div', { class: 'stat__label', text: label }));
}

export function render(container, context) {
  ctx = context;
  rateSeries.length = 0;
  lastBytes = 0;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const overview = ctx.card({ title: '流量概览', subtitle: '/api/get/Total', span: 12, icon: 'chart' });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: loadTotal });
  overview.head.querySelector('.card__actions').appendChild(refreshBtn);
  overview.body.appendChild(el('div', { class: 'stat-grid' },
    stat('今日流量', 'tf-today'), stat('本月流量', 'tf-month'),
    stat('累计流量', 'tf-total'), stat('下载', 'tf-rx'), stat('上传', 'tf-tx')));
  refs.progress = el('span', { style: { width: '0%' } });
  overview.body.appendChild(el('div', { style: { marginTop: '16px' } },
    el('p', { class: 'muted', text: '流量使用进度' }),
    el('div', { class: 'progress' }, refs.progress)));
  grid.appendChild(overview.el);

  // 限制设置
  const limitCard = ctx.card({ title: '流量限制设置', subtitle: '达到上限后自动断网', span: 6, icon: 'shield' });
  const enable = switchField({ label: '启用流量限制', checked: false });
  const limit = field({ label: '每月上限（GB）', name: 'limit', type: 'number', value: '10', attrs: { min: '1', step: '0.1' } });
  const saveLimit = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      if (!limit.input.checkValidity()) { limit.input.reportValidity(); return; }
      const much = Math.round(Number(limit.input.value) * 1073741824);
      try {
        await ctx.api('/api/set/total', {
          method: 'POST', body: { switch: enable.input.checked ? 1 : 0, much },
          signal: ctx.signal
        });
        ctx.toast.success(ctx.t('common.saved'));
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  const clearBtn = button('清除流量统计', {
    variant: 'danger', icon: 'trash',
    onClick: async () => {
      const ok = await ctx.confirm({
        title: ctx.t('common.confirmOperation'),
        message: '清除所有流量统计数据？此操作不可恢复。', danger: true
      });
      if (!ok) return;
      try {
        await ctx.api('/api/set/total', { method: 'POST', body: {}, signal: ctx.signal });
        ctx.toast.success(ctx.t('common.deleted'));
        loadTotal();
      } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  limitCard.body.append(enable.wrap, limit.wrap,
    el('div', { class: 'row' }, saveLimit, clearBtn));
  grid.appendChild(limitCard.el);

  // 使用说明
  const tipCard = ctx.card({ title: '说明', span: 6, icon: 'info' });
  tipCard.body.appendChild(el('ul', { class: 'muted' },
    el('li', { text: '达到流量上限后自动断网，建议预留 10%~20% 余量。' }),
    el('li', { text: '高速下载时统计可能有几 MB 误差，属正常现象。' }),
    el('li', { text: '清除统计后数据需约 5 分钟重新显示。' })));
  grid.appendChild(tipCard.el);

  // 实时趋势
  const chartCard = ctx.card({ title: '实时流量趋势', subtitle: `最近 ${MAX_POINTS} 个采样点`, span: 12, icon: 'activity' });
  chartCard.body.appendChild(makeChart());
  grid.appendChild(chartCard.el);

  async function loadTotal() {
    try {
      const res = await ctx.api('/api/get/Total', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const rx = Number(fieldOf(o, 'rx', fieldOf(o, 'download', 0))) || 0;
      const tx = Number(fieldOf(o, 'tx', fieldOf(o, 'upload', 0))) || 0;
      const total = Number(fieldOf(o, 'total', fieldOf(o, 'used', rx + tx))) || 0;
      setText('tf-today', formatBytes(fieldOf(o, 'today', 0)));
      setText('tf-month', formatBytes(fieldOf(o, 'month', 0)));
      setText('tf-total', formatBytes(total));
      setText('tf-rx', formatBytes(rx));
      setText('tf-tx', formatBytes(tx));
      const limitBytes = Number(fieldOf(o, 'limit', fieldOf(o, 'much', 0))) || 0;
      const pct = limitBytes > 0 ? Math.min(100, (total / limitBytes) * 100) : 0;
      if (refs.progress) refs.progress.style.width = `${pct.toFixed(1)}%`;
      const delta = lastBytes ? Math.max(0, total - lastBytes) : 0;
      lastBytes = total;
      pushRate(delta);
      drawChart();
    } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
  }

  function setText(id, text) {
    const node = document.getElementById(id);
    if (node) node.textContent = text;
  }

  // 载入限制配置
  (async () => {
    try {
      const res = await ctx.api('/api/get/set', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const on = Number(fieldOf(o, 'switch', 0)) === 1;
      enable.input.checked = on;
      const much = Number(fieldOf(o, 'much', 0)) || 0;
      if (much > 0) limit.input.value = String(Math.round(much / 1073741824 * 10) / 10);
    } catch (e) { /* 忽略 */ }
  })();

  ctx.wsSubscribe('traffic', (p) => {
    const o = (p && (p.data || p)) || {};
    pushRate(Number(o.rate != null ? o.rate : o.delta) || 0);
    drawChart();
  });

  loadTotal();
  // 速率已由 WS 推送，这里只做兜底轮询，放宽到 15 秒。
  ctx.interval(loadTotal, 15000);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  lineEl = null;
  areaEl = null;
  labelEl = null;
  refs = {};
}
