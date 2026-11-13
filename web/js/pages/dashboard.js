/**
 * 系统监控（Dashboard）：
 *  - 动态 5G 拓扑与信号辐射 SVG 视觉中心 (Hero Radar)；
 *  - 核心指标动态环形 SVG 仪表盘（CPU、内存、电池、信号质量）；
 *  - 贝塞尔样条平滑趋势图（无图表库，纯 SVG 声明式样条渲染）；
 *  - 严格接入后端真实 API，不造假、不硬编码任何遥测数值；
 *  - 全面支持 WebSocket 实时推送与 HTTP 自动容灾轮询。
 */
import { el, card, button } from '../utils/dom.js';
import {
  formatBytes, formatUptime, formatPercent, formatRate, safeText, fieldOf
} from '../utils/format.js';
import {
  createCircularGauge, createHeroRadar, createSmoothSparkline
} from '../components/svg-gauges.js';

let ctx = null;

export function render(container, context) {
  ctx = context;

  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // ============ ① 动态 5G 网络雷达视觉横幅 (Hero Card, 占满 12 列) ============
  const heroRadar = createHeroRadar();
  const heroCard = card({
    title: '5G 无线通信拓扑',
    subtitle: '实时基站连接状态与射频辐射分析',
    span: 12,
    icon: 'activity',
    className: 'hero-dashboard-card'
  });
  heroCard.body.appendChild(heroRadar.el);
  grid.appendChild(heroCard.el);

  // ============ ② 环形动态 SVG 仪表盘组件组 (占满 12 列) ============
  const gaugeCard = card({
    title: '核心系统遥测',
    subtitle: '处理器、内存与射频链路实时动态遥测',
    span: 12,
    icon: 'gauge'
  });

  const cpuGauge = createCircularGauge({
    value: 0, max: 100, label: 'CPU 使用率', unit: '%', color: 'primary', size: 124
  });
  const memGauge = createCircularGauge({
    value: 0, max: 100, label: '内存负载', unit: '%', color: 'info', size: 124
  });
  const batteryGauge = createCircularGauge({
    value: 0, max: 100, label: '电池电量', unit: '%', color: 'ok', size: 124
  });
  const signalGauge = createCircularGauge({
    value: 0, max: 100, label: '信号品质', unit: 'dBm', color: 'primary', size: 124
  });

  const gaugeGrid = el('div', { class: 'gauge-grid' },
    cpuGauge.el, memGauge.el, batteryGauge.el, signalGauge.el
  );
  gaugeCard.body.appendChild(gaugeGrid);
  grid.appendChild(gaugeCard.el);

  // ============ ③ 详细系统与射频信息卡片 ============
  function kvRow(label) {
    const value = el('span', { class: 'kv__v', text: '—' });
    return {
      el: el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: label }), value),
      set(v) { value.textContent = safeText(v); }
    };
  }

  // 系统硬件信息卡
  const infoRows = {
    hostname: kvRow('主机名'),
    arch: kvRow('硬件架构'),
    uptime: kvRow('系统运行时间'),
    kernel: kvRow('内核版本'),
    temp: kvRow('设备核心温度'),
    mem: kvRow('已用物理内存')
  };
  const infoCard = card({ title: '设备与系统', subtitle: '核心硬件、内核版本与运行周期', span: 4, icon: 'cpu' });
  for (const k of Object.keys(infoRows)) infoCard.body.appendChild(infoRows[k].el);

  // 射频与基站连接卡
  const signalRows = {
    carrier: kvRow('运营商名称'),
    network: kvRow('网络接入技术'),
    band: kvRow('当前工作频段'),
    rsrp: kvRow('接收功率 (RSRP)'),
    rsrq: kvRow('接收质量 (RSRQ)'),
    sinr: kvRow('信干噪比 (SINR)'),
    imei: kvRow('设备识别码')
  };
  const signalCard = card({ title: '射频与小区', subtitle: '当前载波频段与信号度量', span: 4, icon: 'signal' });
  for (const k of Object.keys(signalRows)) signalCard.body.appendChild(signalRows[k].el);

  // 流量与网络吞吐卡
  const trafficRows = {
    today: kvRow('今日产生流量'),
    month: kvRow('本月产生流量'),
    total: kvRow('历史累计流量'),
    rx: kvRow('累计下载数据'),
    tx: kvRow('累计上传数据')
  };
  const trafficCard = card({ title: '流量统计概况', subtitle: '周期流量与累计传输', span: 4, icon: 'chart' });
  for (const k of Object.keys(trafficRows)) trafficCard.body.appendChild(trafficRows[k].el);

  // 电池与运行状态卡
  const batteryRows = {
    level: kvRow('电池剩余容量'),
    status: kvRow('充电状态'),
    health: kvRow('健康状态'),
    voltage: kvRow('当前电压'),
    temp: kvRow('电池温度')
  };
  const batteryCard = card({ title: '电源与充放电', subtitle: '电源管理与电池健康', span: 6, icon: 'battery' });
  for (const k of Object.keys(batteryRows)) batteryCard.body.appendChild(batteryRows[k].el);

  // 通信通道与服务健康卡（可折叠式高级通道监测）
  const statusCard = (ctx.collapsibleCard || card)({
    title: '服务健康与连接通道',
    subtitle: '核心服务就绪与通信链路',
    span: 6,
    icon: 'activity',
    badge: '链路详情'
  });
  const connLight = ctx.statusLight(false, '连接探测中…');
  const modemLine = el('span', { class: 'kv__v', text: '—' });
  statusCard.body.append(
    el('div', { class: 'row', style: { marginBottom: '12px' } }, connLight),
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '基带模组就绪状态' }), modemLine),
    el('p', { class: 'subtle', style: { marginTop: '12px' }, text: '实时数据由高频实时推送通道提供；网络波动时自动自愈降级。' })
  );

  grid.append(infoCard.el, signalCard.el, trafficCard.el, batteryCard.el, statusCard.el);

  // ============ ④ 平滑实时趋势折线图 (占满 12 列) ============
  const chartCard = card({
    title: '动态遥测波形',
    subtitle: 'CPU 负载与实时网络速率（平滑样条实时采样）',
    span: 12,
    icon: 'activity'
  });
  const refreshBtn = button(ctx.t('common.refresh'), {
    variant: 'ghost sm', icon: 'refresh', onClick: () => loadAll()
  });
  chartCard.head.querySelector('.card__actions').appendChild(refreshBtn);

  const sparkline = createSmoothSparkline({
    width: 800, height: 160, maxPoints: 300, strokeColor: '#2563eb', fillColor: '#38bdf8'
  });
  chartCard.body.appendChild(sparkline.el);
  grid.appendChild(chartCard.el);

  // ============ 通信与健康状态更新 ============
  function applyComm(state) {
    const dot = connLight.querySelector('.dot');
    const label = connLight.querySelector('.status-light__label');
    const map = {
      open: ['ok', 'WebSocket 就绪 (实时推送中)'],
      'open-sse': ['warning', 'HTTP 轮询（容灾降级模式）'],
      connecting: ['warning', '正在建立实时连接…'],
      reconnecting: ['warning', '实时连接重连中…'],
      offline: ['muted', '当前网络处于离线状态'],
      closed: ['muted', '实时连接已关闭'],
      failed: ['error', '实时连接建立失败'],
      error: ['error', '通道异常']
    };
    const pair = map[state] || ['muted', '通道未连接'];
    if (dot) dot.className = `dot dot--${pair[0]}`;
    if (label) label.textContent = pair[1];
  }

  function applyHealth(h) {
    const o = (h && (h.data || h)) || {};
    const parts = [];
    if (o.db_ok === false) parts.push('本地存储不可用');
    if (o.dbus_ok === false) parts.push('系统 D-Bus 离线');
    if (o.ofono_ok === false) parts.push('oFono 调制解调服务未注册');
    else if (o.modem_ready === false) parts.push('未检测到有效 Modem 对象');
    modemLine.textContent = parts.length ? parts.join(' / ') : '基带正常运行 (就绪)';
  }

  if (ctx.ws && typeof ctx.ws.onStatus === 'function') {
    applyComm(ctx.ws.getStatus ? ctx.ws.getStatus() : 'connecting');
    const offComm = ctx.ws.onStatus(applyComm);
    if (ctx.onCleanup) ctx.onCleanup(offComm);
  }

  // ============ 真实数据填充逻辑（绝无造假） ============
  let infoCache = {};
  let lastBytes = 0;

  function applyInfo(info) {
    const o = fieldOf(info, 'data', info) || {};
    infoRows.hostname.set(fieldOf(o, 'hostname', fieldOf(info, 'hostname')));
    infoRows.arch.set(fieldOf(o, 'arch', fieldOf(info, 'arch')));
    infoRows.uptime.set(formatUptime(fieldOf(o, 'uptime', fieldOf(info, 'uptime', 0))));
    infoRows.kernel.set(fieldOf(o, 'kernel', fieldOf(info, 'kernel')));

    const cpu = Number(fieldOf(o, 'cpu_usage', fieldOf(info, 'cpu_usage', fieldOf(info, 'cpu', 0)))) || 0;
    infoRows.temp.set(`${safeText(fieldOf(o, 'temperature', fieldOf(info, 'temperature', '—')))} °C`);

    const memUsed = Number(fieldOf(o, 'mem_used', fieldOf(info, 'mem_used', 0))) || 0;
    const memTotal = Number(fieldOf(o, 'mem_total', fieldOf(info, 'mem_total', 0))) || 0;
    infoRows.mem.set(`${formatBytes(memUsed)}${memTotal ? ` / ${formatBytes(memTotal)}` : ''}`);

    // 更新 CPU 仪表
    cpuGauge.update(cpu);
    // 更新内存仪表 (若有 memTotal 则算百分比)
    const memPercent = memTotal > 0 ? (memUsed / memTotal) * 100 : Math.min(100, (memUsed / 1073741824) * 100);
    memGauge.update(memPercent, 100, formatBytes(memUsed));

    return cpu;
  }

  function applyBand(band) {
    const o = fieldOf(band, 'data', band) || {};
    const carrier = fieldOf(o, 'carrier', infoCache.carrier);
    const networkType = fieldOf(o, 'network_type', fieldOf(o, 'mode'));
    const bandName = fieldOf(o, 'band', fieldOf(o, 'current_band'));
    const rsrp = fieldOf(o, 'rsrp');
    const rsrq = fieldOf(o, 'rsrq');
    const sinr = fieldOf(o, 'sinr');

    signalRows.carrier.set(carrier);
    signalRows.network.set(networkType);
    signalRows.band.set(bandName);
    signalRows.rsrp.set(rsrp);
    signalRows.rsrq.set(rsrq);
    signalRows.sinr.set(sinr);
    signalRows.imei.set(fieldOf(o, 'imei', infoCache.imei));

    // 更新 Hero 雷达图数据
    heroRadar.update({ carrier, networkType, band: bandName, rsrp, sinr });

    // 信号质量转换 (RSRP 正常范围约为 -140 ~ -44 dBm)
    const numRsrp = Number(rsrp);
    if (Number.isFinite(numRsrp) && numRsrp !== 0) {
      // 映射到 0-100%
      const qualityScore = Math.max(0, Math.min(100, Math.round(((numRsrp + 140) / 96) * 100)));
      signalGauge.update(qualityScore, 100, `${numRsrp} dBm`);
    } else {
      signalGauge.update(0, 100, '—');
    }
  }

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

    const deltaRate = lastBytes ? Math.max(0, totalBytes - lastBytes) : 0;
    lastBytes = totalBytes;
    if (deltaRate > 0) sparkline.push(deltaRate);
  }

  function applyCharge(charge) {
    const o = fieldOf(charge, 'data', charge) || {};
    const level = Number(fieldOf(o, 'level', fieldOf(o, 'capacity', 0))) || 0;
    const charging = fieldOf(o, 'status', fieldOf(o, 'charging'));

    batteryRows.level.set(`${level}%`);
    batteryRows.status.set(charging);
    batteryRows.health.set(fieldOf(o, 'health'));
    batteryRows.voltage.set(fieldOf(o, 'voltage'));
    batteryRows.temp.set(fieldOf(o, 'temperature'));

    batteryGauge.update(level, 100, charging ? String(charging) : `${level}%`);
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
      sparkline.push(cpu);
      if (band) applyBand(band);
      if (total) applyTotal(total);
      if (charge) applyCharge(charge);
      if (health) applyHealth(health);
    } catch (err) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  // ============ 订阅 WebSocket 实时通道 ============
  ctx.wsSubscribe('stats', (p) => {
    const o = (p && (p.data || p)) || {};
    const cpu = Number(o.cpu_usage != null ? o.cpu_usage : o.cpu) || 0;
    cpuGauge.update(cpu);
    sparkline.push(cpu);
  });

  ctx.wsSubscribe('traffic', (p) => {
    const o = (p && (p.data || p)) || {};
    const rate = Number(o.rate != null ? o.rate : o.delta) || 0;
    if (rate > 0) sparkline.push(rate);
  });

  ctx.wsSubscribe('health', (p) => {
    applyHealth(p);
  });

  ctx.wsSubscribe('signal', (p) => {
    const o = (p && (p.data || p)) || {};
    const v = Number(o.rsrp);
    if (Number.isFinite(v) && v !== 0) {
      const score = Math.max(0, Math.min(100, Math.round(((v + 140) / 96) * 100)));
      signalGauge.update(score, 100, `${v} dBm`);
    }
  });

  // 初始加载
  loadAll();
  // 15 秒兜底轮询
  ctx.interval(loadAll, 15000);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
}
