/**
 * 流量统计：展示今日 / 本月 / 累计流量，双向流速仪表，月度超额断网保护配置。
 * 纯真实数据直连底层接口，高密度现代化双向流速控制台。
 */
import { el, button, field, switchField } from '../utils/dom.js';
import { formatBytes, formatRate, fieldOf, safeText } from '../utils/format.js';
import { createSmoothSparkline, createSpeedArcGauge } from '../components/svg-gauges.js';

let ctx = null;
let sparkline = null;
let lastBytes = 0;
let lastRx = 0;
let lastTx = 0;
let lastTime = Date.now();
let peakDl = 0;
let peakUl = 0;
const refs = {};

export function render(container, context) {
  ctx = context;
  lastBytes = 0;
  lastRx = 0;
  lastTx = 0;
  lastTime = Date.now();
  peakDl = 0;
  peakUl = 0;

  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // ============ ① 实时双向流速仪表站 (Row 1: 占满 12 列) ============
  const throughputWrap = el('div', { class: 'throughput-grid col-span-12' });

  // 下行仪表卡 (Downlink)
  const dlArc = createSpeedArcGauge({ label: '下行流速', color: '#2563eb' });
  const dlRateNum = el('span', { class: 'speed-card__rate', text: '0' });
  const dlRateUnit = el('span', { class: 'speed-card__unit', text: 'B/s' });
  const dlPeakText = el('span', { class: 'badge badge--neutral', text: '峰值: 0 B/s' });
  const dlRxTotal = el('span', { class: 'speed-card__total-item' }, '下行累计 (RX): ', el('strong', { id: 'speed-dl-total', text: '0 B' }));
  const dlTodayTotal = el('span', { class: 'speed-card__total-item' }, '今日已用: ', el('strong', { id: 'speed-dl-today', text: '0 B' }));

  const dlCard = el('div', { class: 'speed-card speed-card--down' },
    el('div', { class: 'speed-card__head' },
      el('span', { class: 'speed-card__label' },
        el('span', { style: { color: 'var(--primary)', fontWeight: '900' }, text: '▼' }),
        '下行下载实时流速 (Downlink)'
      ),
      dlPeakText
    ),
    el('div', { class: 'row', style: { alignItems: 'center', justifyContent: 'space-between', marginTop: '6px' } },
      el('div', { class: 'speed-card__rate-row' }, dlRateNum, dlRateUnit),
      dlArc.el
    ),
    el('div', { class: 'speed-card__totals' }, dlRxTotal, dlTodayTotal)
  );

  // 上行仪表卡 (Uplink)
  const ulArc = createSpeedArcGauge({ label: '上行流速', color: '#059669', trackColor: 'rgba(5, 150, 105, 0.12)' });
  const ulRateNum = el('span', { class: 'speed-card__rate', text: '0' });
  const ulRateUnit = el('span', { class: 'speed-card__unit', text: 'B/s' });
  const ulPeakText = el('span', { class: 'badge badge--neutral', text: '峰值: 0 B/s' });
  const ulTxTotal = el('span', { class: 'speed-card__total-item' }, '上行累计 (TX): ', el('strong', { id: 'speed-ul-total', text: '0 B' }));
  const ulMonthTotal = el('span', { class: 'speed-card__total-item' }, '当月结算: ', el('strong', { id: 'speed-ul-month', text: '0 B' }));

  const ulCard = el('div', { class: 'speed-card speed-card--up' },
    el('div', { class: 'speed-card__head' },
      el('span', { class: 'speed-card__label' },
        el('span', { style: { color: 'var(--ok)', fontWeight: '900' }, text: '▲' }),
        '上行上传实时流速 (Uplink)'
      ),
      ulPeakText
    ),
    el('div', { class: 'row', style: { alignItems: 'center', justifyContent: 'space-between', marginTop: '6px' } },
      el('div', { class: 'speed-card__rate-row' }, ulRateNum, ulRateUnit),
      ulArc.el
    ),
    el('div', { class: 'speed-card__totals' }, ulTxTotal, ulMonthTotal)
  );

  throughputWrap.append(dlCard, ulCard);
  grid.appendChild(throughputWrap);

  // ============ ② 移动数据流量概览卡 (Row 2 Left: 6 列) ============
  function statBox(label, id) {
    return el('div', { class: 'stat' },
      el('div', { class: 'stat__value', id, text: '—' }),
      el('div', { class: 'stat__label', text: label }));
  }

  const overview = ctx.card({
    title: '移动蜂窝数据流量统计',
    subtitle: '结算周期内数据传输消耗与全周期累计量',
    span: 6,
    icon: 'chart'
  });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: loadTotal });
  const clearBtn = button('清空统计', {
    variant: 'danger sm', icon: 'trash',
    onClick: async () => {
      const ok = await ctx.confirm({
        title: '清空蜂窝流量统计',
        message: '确定要将今日与当月已用流量归零吗？此操作将重置统计基线。',
        danger: true
      });
      if (!ok) return;
      try {
        await ctx.api('/api/set/total', {
          method: 'POST',
          body: { action: 'clear', clear: true },
          signal: ctx.signal
        });
        ctx.toast.success('流量统计已清空');
        loadTotal();
      } catch (_) {
        ctx.toast.error(ctx.t('common.requestFailed'));
      }
    }
  });
  overview.head.querySelector('.card__actions').append(clearBtn, refreshBtn);

  overview.body.appendChild(el('div', { class: 'stat-grid' },
    statBox('今日已用流量', 'tf-today'),
    statBox('本月已用流量', 'tf-month'),
    statBox('历史累计总流量', 'tf-total'),
    statBox('累计下载 (RX)', 'tf-rx'),
    statBox('累计上传 (TX)', 'tf-tx')
  ));

  refs.progress = el('span', { style: { width: '0%', background: 'linear-gradient(90deg, #2563eb, #06b6d4)' } });
  refs.progressText = el('span', { class: 'muted', style: { marginLeft: '8px', fontSize: '11.5px' }, text: '0%' });
  overview.body.appendChild(el('div', { style: { marginTop: '16px' } },
    el('div', { class: 'row', style: { justifyContent: 'space-between', marginBottom: '6px' } },
      el('span', { class: 'field__label', text: '月度流量使用进度与配额' }),
      refs.progressText
    ),
    el('div', { class: 'progress' }, refs.progress)
  ));
  grid.appendChild(overview.el);

  // ============ ③ 月度流量超额断网保护卡 (Row 2 Right: 6 列) ============
  const limitCard = ctx.card({
    title: '月度流量超额断网保护',
    subtitle: '达到指定阈值后自动切断移动蜂窝数据',
    span: 6,
    icon: 'shield'
  });
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

  // ============ ④ 实时速率平滑趋势卡 (Row 3: 12 列) ============
  const chartCard = ctx.card({
    title: '动态遥测吞吐波形走势',
    subtitle: '下行实时传输速率平滑波形采样',
    span: 12,
    icon: 'activity'
  });
  sparkline = createSmoothSparkline({
    width: 800, height: 160, maxPoints: 300, strokeColor: '#0ea5e9', fillColor: '#38bdf8'
  });
  chartCard.body.appendChild(sparkline.el);
  grid.appendChild(chartCard.el);

  // ============ 速率分解显示辅助 ============
  function updateSpeedMeter(dlBytesPerSec, ulBytesPerSec = 0) {
    const formattedDl = formatRate(dlBytesPerSec);
    const partsDl = formattedDl.split(' ');
    if (partsDl.length === 2) {
      dlRateNum.textContent = partsDl[0];
      dlRateUnit.textContent = partsDl[1];
    } else {
      dlRateNum.textContent = formattedDl;
      dlRateUnit.textContent = '';
    }

    if (dlBytesPerSec > peakDl) {
      peakDl = dlBytesPerSec;
      dlPeakText.textContent = `峰值: ${formatRate(peakDl)}`;
    }

    // 更新弧度 (按 10MB/s 标定)
    dlArc.update(Math.min(1, dlBytesPerSec / 10485760));

    const formattedUl = formatRate(ulBytesPerSec);
    const partsUl = formattedUl.split(' ');
    if (partsUl.length === 2) {
      ulRateNum.textContent = partsUl[0];
      ulRateUnit.textContent = partsUl[1];
    } else {
      ulRateNum.textContent = formattedUl;
      ulRateUnit.textContent = '';
    }

    if (ulBytesPerSec > peakUl) {
      peakUl = ulBytesPerSec;
      ulPeakText.textContent = `峰值: ${formatRate(peakUl)}`;
    }

    ulArc.update(Math.min(1, ulBytesPerSec / 5242880));
  }

  function setStat(id, v) {
    const elNode = container.querySelector(`#${id}`);
    if (elNode) elNode.textContent = safeText(v);
  }

  async function loadTotal() {
    try {
      const res = await ctx.api('/api/get/Total', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      const rx = Number(fieldOf(o, 'rx', fieldOf(o, 'download', 0))) || 0;
      const tx = Number(fieldOf(o, 'tx', fieldOf(o, 'upload', 0))) || 0;
      const total = Number(fieldOf(o, 'total', fieldOf(o, 'used', rx + tx))) || 0;
      const today = Number(fieldOf(o, 'today', 0)) || 0;
      const month = Number(fieldOf(o, 'month', 0)) || 0;

      setStat('tf-today', formatBytes(today));
      setStat('tf-month', formatBytes(month));
      setStat('tf-total', formatBytes(total));
      setStat('tf-rx', formatBytes(rx));
      setStat('tf-tx', formatBytes(tx));

      const dlTotal = container.querySelector('#speed-dl-total');
      const dlToday = container.querySelector('#speed-dl-today');
      const ulTotal = container.querySelector('#speed-ul-total');
      const ulMonth = container.querySelector('#speed-ul-month');
      if (dlTotal) dlTotal.textContent = formatBytes(rx);
      if (dlToday) dlToday.textContent = formatBytes(today);
      if (ulTotal) ulTotal.textContent = formatBytes(tx);
      if (ulMonth) ulMonth.textContent = formatBytes(month);

      const now = Date.now();
      const timeDelta = Math.max(0.5, (now - lastTime) / 1000);
      const bytesDelta = lastBytes ? Math.max(0, total - lastBytes) : 0;
      const rxDelta = lastRx ? Math.max(0, rx - lastRx) : 0;
      const txDelta = lastTx ? Math.max(0, tx - lastTx) : 0;
      const currentRate = bytesDelta / timeDelta;
      const rxRate = rxDelta / timeDelta;
      const txRate = txDelta / timeDelta;
      lastBytes = total;
      lastRx = rx;
      lastTx = tx;
      lastTime = now;

      updateSpeedMeter(rxRate || currentRate, txRate);
      if (sparkline) sparkline.push(currentRate);

      const much = Number(fieldOf(refs.cfg, 'much', 0)) || 0;
      if (much > 0) {
        const pct = Math.min(100, Math.round((month / much) * 100));
        refs.progress.style.width = `${pct}%`;
        refs.progressText.textContent = `${pct}%（${formatBytes(month)} / ${formatBytes(much)}）`;
      } else {
        refs.progress.style.width = '0%';
        refs.progressText.textContent = '未设月度上限';
      }
    } catch (e) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  async function loadConfig() {
    try {
      const res = await ctx.api('/api/get/total/config', { signal: ctx.signal });
      const o = fieldOf(res, 'data', res) || {};
      refs.cfg = o;
      enable.input.checked = Boolean(o.switch === 1 || o.enabled === true);
      const muchBytes = Number(fieldOf(o, 'much', fieldOf(o, 'limit', 0))) || 0;
      limitInput.input.value = muchBytes > 0 ? (muchBytes / 1073741824).toFixed(1) : '10';
    } catch (_) {}
  }

  // WebSocket 实时流速广播
  ctx.wsSubscribe('traffic', (p) => {
    const o = (p && (p.data || p)) || {};
    const rate = Number(o.rate != null ? o.rate : o.delta) || 0;
    const rxRate = Number(o.rx_rate != null ? o.rx_rate : rate) || 0;
    const txRate = Number(o.tx_rate != null ? o.tx_rate : 0) || 0;
    updateSpeedMeter(rxRate, txRate);
    if (sparkline) sparkline.push(rate);
  });

  loadConfig().then(loadTotal);
  ctx.interval(loadTotal, 10000);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  sparkline = null;
}
