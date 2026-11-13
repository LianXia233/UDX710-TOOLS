/**
 * 系统监控（Dashboard）：
 *  - 系统信息 / 信号 / 流量 / 电池 概览卡片；
 *  - 原生 SVG 趋势图（无图表库），仅周期性更新 path 的 d 属性，不做全量重绘；
 *  - 流量 / CPU / 信号实时采样只保留最近 300 个点（固定窗口，避免内存无限增长）；
 *  - 同时支持 HTTP 轮询与 /api/ws 状态推送（连接失败时轮询仍可用，页面不受影响）。
 */
import { el, icon, card, button } from '../utils/dom.js';
import {
  formatBytes, formatUptime, formatPercent, formatRate, safeText, fieldOf
} from '../utils/format.js';

const MAX_POINTS = 300;
const CHART_W = 600;
const CHART_H = 160;

let ctx = null;
const cpuSeries = [];
const trafficSeries = [];
const signalSeries = [];
let lastTotalBytes = 0;
let chartNodes = null;

function push(series, value) {
  series.push(Number(value) || 0);
  if (series.length > MAX_POINTS) series.shift();
}

function last(series) {
  return series.length ? series[series.length - 1] : 0;
}

function buildPath(series, min, max) {
  if (series.length < 2) return '';
  const range = (max - min) || 1;
  const step = CHART_W / (series.length - 1);
  const points = [];
  for (let i = 0; i < series.length; i += 1) {
    const x = i * step;
    const raw = (series[i] - min) / range;
    const y = CHART_H - Math.max(0, Math.min(1, raw)) * CHART_H;
    points.push(`${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`);
  }
  return points.join(' ');
}

function updateChart() {
  if (!chartNodes) return;
  const line = buildPath(cpuSeries, 0, 100);
  chartNodes.lineCpu.setAttribute('d', line);
  chartNodes.area.setAttribute('d', line ? `${line} L${CHART_W},${CHART_H} L0,${CHART_H} Z` : '');
  const tMax = Math.max(1, ...trafficSeries);
  chartNodes.lineTraffic.setAttribute('d', buildPath(trafficSeries, 0, tMax));
  chartNodes.label.textContent =
    `CPU ${last(cpuSeries).toFixed(0)}% · 流量 ${formatRate(last(trafficSeries))} · 信号 ${last(signalSeries).toFixed(0)} dBm`;
}

function makeChart() {
  const svg = el('svg:svg', {
    class: 'sparkline', viewBox: `0 0 ${CHART_W} ${CHART_H}`,
    preserveAspectRatio: 'none', role: 'img', 'aria-label': 'CPU 与流量趋势'
  });
  for (let i = 0; i <= 4; i += 1) {
    const y = i * (CHART_H / 4);
    svg.appendChild(el('svg:line', { x1: 0, y1: y, x2: CHART_W, y2: y, class: 'chart-grid' }));
  }
  const area = el('svg:path', { class: 'chart-area', d: '' });
  const lineCpu = el('svg:path', { class: 'chart-line', d: '' });
  const lineTraffic = el('svg:path', { class: 'chart-line chart-line--alt', d: '' });
  const label = el('svg:text', { x: 6, y: 14, class: 'chart-text', text: '' });
  svg.append(area, lineCpu, lineTraffic, label);
  chartNodes = { area, lineCpu, lineTraffic, label };
  return svg;
}

function kvRow(label) {
  const value = el('span', { class: 'kv__v' });
  return {
    el: el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: label }), value),
    set(v) { value.textContent = safeText(v); }
  };
}

export function render(container, context) {
  ctx = context;
  cpuSeries.length = 0;
  trafficSeries.length = 0;
  signalSeries.length = 0;
  lastTotalBytes = 0;

  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // 系统信息
  const infoRows = {
    hostname: kvRow('主机名'), arch: kvRow('架构'), uptime: kvRow('运行时间'),
    kernel: kvRow('内核'), cpu: kvRow('CPU 使用率'), mem: kvRow('内存使用'),
    temp: kvRow('温度')
  };
  const infoCard = card({ title: '系统信息', subtitle: '来自 /api/info', span: 4, icon: 'cpu' });
  for (const key of Object.keys(infoRows)) infoCard.body.appendChild(infoRows[key].el);

  // 信号 / Modem
  const signalRows = {
    carrier: kvRow('运营商'), network: kvRow('网络类型'), band: kvRow('频段'),
    rsrp: kvRow('RSRP'), rsrq: kvRow('RSRQ'), sinr: kvRow('SINR'),
    imei: kvRow('IMEI')
  };
  const signalCard = card({ title: '信号与连接', subtitle: '当前频段 / 小区', span: 4, icon: 'signal' });
  for (const key of Object.keys(signalRows)) signalCard.body.appendChild(signalRows[key].el);

  // 流量
  const trafficRows = {
    today: kvRow('今日流量'), month: kvRow('本月流量'), total: kvRow('累计流量'),
    rx: kvRow('下载'), tx: kvRow('上传')
  };
  const trafficCard = card({ title: '流量统计', subtitle: '/api/get/Total', span: 4, icon: 'chart' });
  for (const key of Object.keys(trafficRows)) trafficCard.body.appendChild(trafficRows[key].el);

  // 电池
  const batteryRows = {
    level: kvRow('电量'), status: kvRow('状态'), health: kvRow('健康'),
    voltage: kvRow('电压'), temp: kvRow('电池温度')
  };
  const batteryCard = card({ title: '电池', subtitle: '充电与电源', span: 6, icon: 'battery' });
  const statWrap = el('div', { class: 'stat-grid' },
    el('div', { class: 'stat' }, el('div', { class: 'stat__value', id: 'dash-battery', text: '—' }), el('div', { class: 'stat__label', text: '电池电量' })),
    el('div', { class: 'stat' }, el('div', { class: 'stat__value', id: 'dash-cpu', text: '—' }), el('div', { class: 'stat__label', text: 'CPU' })),
    el('div', { class: 'stat stat--ok' }, el('div', { class: 'stat__value', id: 'dash-signal', text: '—' }), el('div', { class: 'stat__label', text: '信号 dBm' })));
  batteryCard.body.append(statWrap);
  for (const key of Object.keys(batteryRows)) batteryCard.body.appendChild(batteryRows[key].el);

  // 状态卡：状态灯由 /api/health 与 WS 推送驱动，不再写死「已连接」。
  const statusCard = card({ title: '运行状态', span: 6, icon: 'activity' });
  const connLight = ctx.statusLight(false, '检测中…');
  const modemLine = el('span', { class: 'kv__v', text: '—' });
  statusCard.body.append(
    el('div', { class: 'row' }, connLight),
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '模组' }), modemLine),
    el('p', { class: 'muted', style: { marginTop: '8px' }, text: '实时推送 /api/ws，断线自动降级为 HTTP 轮询' }),
    el('p', { class: 'muted', style: { marginTop: '4px' }, text: `采样窗口：最近 ${MAX_POINTS} 个点` }));

  /** 依据健康摘要刷新状态灯与模组说明。 */
  function applyHealth(h) {
    const o = (h && (h.data || h)) || {};
    const dot = connLight.querySelector('.dot');
    const label = connLight.querySelector('.status-light__label');
    const okAll = o.status === 'ok' && o.db_ok !== false;
    if (dot) dot.className = `dot dot--${okAll ? 'ok' : 'warning'}`;
    if (label) label.textContent = okAll ? '已连接' : '降级运行';
    const parts = [];
    if (o.db_ok === false) parts.push('数据库异常');
    if (o.dbus_ok === false) parts.push('D-Bus 不可用');
    if (o.ofono_ok === false) parts.push('oFono 未注册');
    else if (o.modem_ready === false) parts.push('无 Modem 对象');
    modemLine.textContent = parts.length ? parts.join(' / ') : '就绪';
  }

  // 趋势图
  const chartCard = card({ title: '实时趋势', subtitle: 'CPU / 流量 / 信号（最近 300 点）', span: 12, icon: 'activity' });
  const refreshBtn = button(ctx.t('common.refresh'), {
    variant: 'ghost sm', icon: 'refresh', onClick: () => loadAll()
  });
  chartCard.head.querySelector('.card__actions').appendChild(refreshBtn);
  chartCard.body.appendChild(makeChart());

  grid.append(infoCard.el, signalCard.el, trafficCard.el, batteryCard.el, statusCard.el, chartCard.el);

  function applyInfo(info) {
    const o = fieldOf(info, 'data', info) || {};
    infoRows.hostname.set(fieldOf(o, 'hostname', fieldOf(info, 'hostname')));
    infoRows.arch.set(fieldOf(o, 'arch', fieldOf(info, 'arch')));
    infoRows.uptime.set(formatUptime(fieldOf(o, 'uptime', fieldOf(info, 'uptime', 0))));
    infoRows.kernel.set(fieldOf(o, 'kernel', fieldOf(info, 'kernel')));
    const cpu = Number(fieldOf(o, 'cpu_usage', fieldOf(info, 'cpu_usage', fieldOf(info, 'cpu', 0)))) || 0;
    infoRows.cpu.set(formatPercent(cpu));
    infoRows.mem.set(formatBytes(fieldOf(o, 'mem_used', fieldOf(info, 'mem_used', 0))));
    infoRows.temp.set(`${safeText(fieldOf(o, 'temperature', fieldOf(info, 'temperature', '—')))} °C`);
    const dashCpu = document.getElementById('dash-cpu');
    if (dashCpu) dashCpu.textContent = formatPercent(cpu);
    return cpu;
  }

  function applyBand(band) {
    const o = fieldOf(band, 'data', band) || {};
    // current_band 现在直接带 carrier；取不到时再回退 /api/info。
    signalRows.carrier.set(fieldOf(o, 'carrier', infoCarry().carrier));
    signalRows.network.set(fieldOf(o, 'network_type', fieldOf(o, 'mode')));
    signalRows.band.set(fieldOf(o, 'band', fieldOf(o, 'current_band')));
    signalRows.rsrp.set(fieldOf(o, 'rsrp'));
    signalRows.rsrq.set(fieldOf(o, 'rsrq'));
    signalRows.sinr.set(fieldOf(o, 'sinr'));
    signalRows.imei.set(fieldOf(o, 'imei', infoCarry().imei));
    const rsrp = Number(fieldOf(o, 'rsrp', 0)) || 0;
    // 无 Modem 时 rsrp 为 null，不推送 0，避免曲线被压成一条直线。
    if (rsrp) push(signalSeries, rsrp);
    const dashSignal = document.getElementById('dash-signal');
    if (dashSignal) dashSignal.textContent = rsrp ? `${rsrp} dBm` : '—';
  }

  // 兜底：部分字段可能只在 /api/info 中
  let infoCache = {};
  function infoCarry() { return infoCache; }

  function applyTotal(total) {
    const o = fieldOf(total, 'data', total) || {};
    const rx = Number(fieldOf(o, 'rx', fieldOf(o, 'download', 0))) || 0;
    const tx = Number(fieldOf(o, 'tx', fieldOf(o, 'upload', 0))) || 0;
    const totalBytes = Number(fieldOf(o, 'total', fieldOf(o, 'used', rx + tx))) || 0;
    trafficRows.today.set(formatBytes(fieldOf(o, 'today', 0)));
    trafficRows.month.set(formatBytes(fieldOf(o, 'month', 0)));
    trafficRows.total.set(formatBytes(totalBytes));
    trafficRows.rx.set(formatBytes(rx));
    trafficRows.tx.set(formatBytes(tx));
    const delta = lastTotalBytes ? Math.max(0, totalBytes - lastTotalBytes) : 0;
    lastTotalBytes = totalBytes;
    push(trafficSeries, delta);
    return totalBytes;
  }

  function applyCharge(charge) {
    const o = fieldOf(charge, 'data', charge) || {};
    const level = Number(fieldOf(o, 'level', fieldOf(o, 'capacity', 0))) || 0;
    batteryRows.level.set(`${level}%`);
    batteryRows.status.set(fieldOf(o, 'status', fieldOf(o, 'charging')));
    batteryRows.health.set(fieldOf(o, 'health'));
    batteryRows.voltage.set(fieldOf(o, 'voltage'));
    batteryRows.temp.set(fieldOf(o, 'temperature'));
    const dashBattery = document.getElementById('dash-battery');
    if (dashBattery) dashBattery.textContent = `${level}%`;
  }

  async function loadAll() {
    try {
      const [info, band, total, charge, health] = await Promise.all([
        ctx.api('/api/info', { signal: ctx.signal }),
        ctx.api('/api/current_band', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/get/Total', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/charge/config', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/health', { signal: ctx.signal }).catch(() => null)
      ]);
      infoCache = fieldOf(info, 'data', info) || info || {};
      const cpu = applyInfo(info);
      push(cpuSeries, cpu);
      applyBand(band);
      applyTotal(total);
      applyCharge(charge);
      if (health) applyHealth(health);
      updateChart();
    } catch (err) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  // 实时推送（WS/SSE）；失败时轮询仍工作
  ctx.wsSubscribe('stats', (p) => {
    const o = p && (p.data || p) || {};
    push(cpuSeries, Number(o.cpu_usage != null ? o.cpu_usage : o.cpu) || 0);
    updateChart();
  });
  ctx.wsSubscribe('traffic', (p) => {
    const o = p && (p.data || p) || {};
    push(trafficSeries, Number(o.rate != null ? o.rate : o.delta) || 0);
    updateChart();
  });
  ctx.wsSubscribe('health', (p) => {
    applyHealth(p);
  });
  ctx.wsSubscribe('signal', (p) => {
    const o = p && (p.data || p) || {};
    const v = Number(o.rsrp);
    // 无 Modem 时后端给 null，跳过以免污染曲线。
    if (!Number.isFinite(v) || v === 0) return;
    push(signalSeries, v);
    updateChart();
  });

  loadAll();
  ctx.interval(loadAll, 5000);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  chartNodes = null;
}
