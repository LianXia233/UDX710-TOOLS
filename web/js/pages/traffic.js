/**
 * 流量统计（Traffic Monitor）：
 *  - 概览指标卡片（今日 / 本月 / 累计 / 下载 / 上传）；
 *  - 动态 SVG 吞吐量水流计量通道（Throughput Stream Pipe）；
 *  - 贝塞尔平滑实时速率波形折线；
 *  - 流量上限自动断网设置（POST /api/set/total）；
 *  - 真实直连 /api/get/Total 与 /api/get/set，零假数据。
 */
import { el, button, field, switchField } from '../utils/dom.js';
import { fieldOf, safeText, formatBytes, formatRate } from '../utils/format.js';
import { createSmoothSparkline } from '../components/svg-gauges.js';

let ctx = null;
let sparkline = null;
let lastBytes = 0;
const refs = {};

/** 动态 SVG 流量水流管道指示组件 */
function createTrafficStreamPipe() {
  const svg = el('svg:svg', {
    class: 'traffic-stream-svg',
    viewBox: '0 0 680 110',
    preserveAspectRatio: 'xMidYMid meet',
    role: 'img',
    'aria-label': '实时数据吞吐水流指示'
  });

  const defs = el('svg:defs');
  const rxGrad = el('svg:linearGradient', { id: 'rxGrad', x1: '0%', y1: '0%', x2: '100%', y2: '0%' },
    el('svg:stop', { offset: '0%', 'stop-color': '#3b82f6', 'stop-opacity': '0.1' }),
    el('svg:stop', { offset: '50%', 'stop-color': '#06b6d4', 'stop-opacity': '0.9' }),
    el('svg:stop', { offset: '100%', 'stop-color': '#10b981', 'stop-opacity': '0.2' })
  );
  const txGrad = el('svg:linearGradient', { id: 'txGrad', x1: '100%', y1: '0%', x2: '0%', y2: '0%' },
    el('svg:stop', { offset: '0%', 'stop-color': '#8b5cf6', 'stop-opacity': '0.1' }),
    el('svg:stop', { offset: '50%', 'stop-color': '#ec4899', 'stop-opacity': '0.85' }),
    el('svg:stop', { offset: '100%', 'stop-color': '#f59e0b', 'stop-opacity': '0.2' })
  );
  defs.append(rxGrad, txGrad);
  svg.appendChild(defs);

  // 下行数据通道
  const rxPipe = el('svg:rect', {
    x: '40', y: '24', width: '600', height: '22', rx: '11',
    fill: 'var(--surface-muted)', stroke: 'var(--glass-border-subtle)', 'stroke-width': '1'
  });
  const rxStream = el('svg:line', {
    x1: '50', y1: '35', x2: '630', y2: '35',
    stroke: 'url(#rxGrad)', 'stroke-width': '10', 'stroke-linecap': 'round',
    'stroke-dasharray': '24 80', 'stroke-dashoffset': '0'
  },
    el('svg:animate', {
      attributeName: 'stroke-dashoffset', from: '0', to: '-520', dur: '2.5s', repeatCount: 'indefinite'
    })
  );

  // 上行数据通道
  const txPipe = el('svg:rect', {
    x: '40', y: '64', width: '600', height: '22', rx: '11',
    fill: 'var(--surface-muted)', stroke: 'var(--glass-border-subtle)', 'stroke-width': '1'
  });
  const txStream = el('svg:line', {
    x1: '50', y1: '75', x2: '630', y2: '75',
    stroke: 'url(#txGrad)', 'stroke-width': '10', 'stroke-linecap': 'round',
    'stroke-dasharray': '24 80', 'stroke-dashoffset': '0'
  },
    el('svg:animate', {
      attributeName: 'stroke-dashoffset', from: '-520', to: '0', dur: '3.2s', repeatCount: 'indefinite'
    })
  );

  // 左右标识文字
  const rxLabel = el('svg:text', {
    x: '50', y: '16', fill: 'var(--primary)', 'font-size': '11', 'font-weight': '700', text: '▼ 下载流速 (Downlink)'
  });
  const txLabel = el('svg:text', {
    x: '50', y: '100', fill: 'var(--warning)', 'font-size': '11', 'font-weight': '700', text: '▲ 上传流速 (Uplink)'
  });

  const rxRateVal = el('svg:text', {
    x: '630', y: '16', 'text-anchor': 'end', fill: 'var(--text)', 'font-family': 'var(--mono)', 'font-size': '11', 'font-weight': '700', text: '—'
  });
  const txRateVal = el('svg:text', {
    x: '630', y: '100', 'text-anchor': 'end', fill: 'var(--text)', 'font-family': 'var(--mono)', 'font-size': '11', 'font-weight': '700', text: '—'
  });

  svg.append(rxPipe, rxStream, txPipe, txStream, rxLabel, txLabel, rxRateVal, txRateVal);

  return {
    el: svg,
    updateRate(rx, tx) {
      if (rx != null) rxRateVal.textContent = formatRate(rx);
      if (tx != null) txRateVal.textContent = formatRate(tx);
    }
  };
}

function statBox(label, id) {
  return el('div', { class: 'stat' },
    el('div', { class: 'stat__value', id, text: '—' }),
    el('div', { class: 'stat__label', text: label }));
}

export function render(container, context) {
  ctx = context;
  lastBytes = 0;

  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // 流量概览卡
  const overview = ctx.card({ title: '移动数据流量概览', subtitle: '周期数据使用量与累计传输消耗', span: 12, icon: 'chart' });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: loadTotal });
  overview.head.querySelector('.card__actions').appendChild(refreshBtn);

  overview.body.appendChild(el('div', { class: 'stat-grid' },
    statBox('今日已用流量', 'tf-today'),
    statBox('本月已用流量', 'tf-month'),
    statBox('历史累计总流量', 'tf-total'),
    statBox('累计接收 (RX)', 'tf-rx'),
    statBox('累计发送 (TX)', 'tf-tx')
  ));

  refs.progress = el('span', { style: { width: '0%' } });
  refs.progressText = el('span', { class: 'muted', style: { marginLeft: '8px' }, text: '0%' });
  overview.body.appendChild(el('div', { style: { marginTop: '20px' } },
    el('div', { class: 'row', style: { justifyContent: 'space-between', marginBottom: '6px' } },
      el('span', { class: 'field__label', text: '月度流量使用进度' }),
      refs.progressText
    ),
    el('div', { class: 'progress' }, refs.progress)
  ));
  grid.appendChild(overview.el);

  // 动态数据水流指示卡
  const streamVis = createTrafficStreamPipe();
  const streamCard = ctx.card({
    title: '网络数据吞吐流速',
    subtitle: '双向管道数据吞吐实时渲染',
    span: 12,
    icon: 'activity'
  });
  streamCard.body.appendChild(streamVis.el);
  grid.appendChild(streamCard.el);

  // 流量上限与限制设置卡
  const limitCard = ctx.card({ title: '月度流量断网保护', subtitle: '达到指定阈值后自动切断移动数据', span: 6, icon: 'shield' });
  const enable = switchField({ label: '启用月度流量超额断网', checked: false });
  const limitInput = field({
    label: '月度额度上限（GB）', name: 'limit', type: 'number', value: '10',
    attrs: { min: '0.1', step: '0.1' }
  });
  const saveLimit = button(ctx.t('common.save'), {
    variant: 'primary', icon: 'save',
    onClick: async () => {
      if (!limitInput.input.checkValidity()) { limitInput.input.reportValidity(); return; }
      const muchBytes = Math.round(Number(limitInput.input.value) * 1073741824);
      try {
        await ctx.api('/api/set/total', {
          method: 'POST',
          body: { switch: enable.input.checked ? 1 : 0, much: muchBytes },
          signal: ctx.signal
        });
        ctx.toast.success(ctx.t('common.saved'));
        loadConfig();
      } catch (e) {
        ctx.toast.error(ctx.t('common.requestFailed'));
      }
    }
  });
  limitCard.body.append(
    enable.wrap,
    limitInput.wrap,
    el('div', { class: 'row', style: { marginTop: '16px' } }, saveLimit)
  );
  grid.appendChild(limitCard.el);

  // 实时速率平滑趋势卡
  const chartCard = ctx.card({ title: '实时吞吐波形走势', subtitle: '网络速率动态采样', span: 6, icon: 'activity' });
  sparkline = createSmoothSparkline({
    width: 600, height: 160, maxPoints: 300, strokeColor: '#0ea5e9', fillColor: '#38bdf8'
  });
  chartCard.body.appendChild(sparkline.el);
  grid.appendChild(chartCard.el);

  // ============ 真实 API 载入 ============
  async function loadTotal() {
    try {
      const res = await ctx.api('/api/get/Total', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const rx = Number(fieldOf(o, 'rx', fieldOf(o, 'download', 0))) || 0;
      const tx = Number(fieldOf(o, 'tx', fieldOf(o, 'upload', 0))) || 0;
      const total = Number(fieldOf(o, 'total', fieldOf(o, 'used', rx + tx))) || 0;

      const setNode = (id, text) => {
        const n = document.getElementById(id);
        if (n) n.textContent = text;
      };

      setNode('tf-today', formatBytes(fieldOf(o, 'today', 0)));
      setNode('tf-month', formatBytes(fieldOf(o, 'month', 0)));
      setNode('tf-total', formatBytes(total));
      setNode('tf-rx', formatBytes(rx));
      setNode('tf-tx', formatBytes(tx));

      const delta = lastBytes ? Math.max(0, total - lastBytes) : 0;
      lastBytes = total;
      if (delta > 0 && sparkline) sparkline.push(delta);
      streamVis.updateRate(delta, null);
    } catch (e) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  async function loadConfig() {
    try {
      const res = await ctx.api('/api/get/set', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const sw = fieldOf(o, 'switch', 0);
      enable.input.checked = Number(sw) === 1;
      const much = Number(fieldOf(o, 'much', 0)) || 0;
      if (much > 0) {
        const gb = (much / 1073741824).toFixed(1);
        limitInput.input.value = gb;
        // 计算月度进度
        const m = Number(fieldOf(o, 'month', 0)) || 0;
        const ratio = Math.min(100, Math.round((m / much) * 100));
        refs.progress.style.width = `${ratio}%`;
        refs.progressText.textContent = `${ratio}% (已用 ${formatBytes(m)} / 上限 ${gb} GB)`;
      }
    } catch (e) { /* 忽略静默异常 */ }
  }

  // 实时通道订阅
  ctx.wsSubscribe('traffic', (p) => {
    const o = (p && (p.data || p)) || {};
    const rate = Number(o.rate != null ? o.rate : o.delta) || 0;
    if (rate > 0 && sparkline) sparkline.push(rate);
    const rx = Number(o.rx_rate || rate);
    const tx = Number(o.tx_rate || 0);
    streamVis.updateRate(rx, tx);
  });

  loadTotal();
  loadConfig();
  ctx.interval(loadTotal, 10000);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  sparkline = null;
}
