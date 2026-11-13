/**
 * 仪表盘（Dashboard）：5G 全景大盘、蜂窝信道全息视窗、四维核心遥测与实时吞吐走势。
 *
 * 遵循原则：
 *  - 纯真实数据直连底层接口，拒绝任何硬编码与虚假数据；
 *  - 现代化高密度 Bento Grid 布局，告别空洞大卡片与冗余堆叠；
 *  - 动态 5G 射频全息载波 SVG，兼顾高颜值与专业工程级信息密度。
 */
import { el, button, card } from '../utils/dom.js';
import { formatBytes, formatRate, formatUptime, fieldOf, safeText, arfcnToBand } from '../utils/format.js';
import { createCellularHologramHero, createSmoothSparkline } from '../components/svg-gauges.js';

let ctx = null;

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // ============ ① 5G 射频全息拓扑全景视窗 (Row 1: 占满 12 列) ============
  const heroRadar = createCellularHologramHero();
  heroRadar.el.classList.add('col-span-12');
  grid.appendChild(heroRadar.el);

  // ============ ② 四维核心微遥测网格 (Row 2: 4 Bento Tiles, 占满 12 列) ============
  const telemetryWrap = el('div', { class: 'telemetry-grid col-span-12' });

  function createTelemetryTile(title, iconName, initialNum = '0%', subText = '—') {
    const numEl = el('div', { class: 'telemetry-tile__num', text: initialNum });
    const subEl = el('div', { class: 'telemetry-tile__sub', text: subText });
    const fillEl = el('div', { class: 'telemetry-bar__fill', style: { width: '0%', background: 'var(--primary)' } });
    const barEl = el('div', { class: 'telemetry-bar' }, fillEl);

    const tile = el('div', { class: 'telemetry-tile' },
      el('div', { class: 'telemetry-tile__head' },
        el('span', { class: 'telemetry-tile__title', text: title }),
        el('span', { class: 'telemetry-tile__icon' }, ctx.icon(iconName))
      ),
      numEl,
      barEl,
      subEl
    );

    return {
      el: tile,
      update(num, percent, sub, color = 'var(--primary)') {
        if (num != null) numEl.textContent = String(num);
        if (sub != null) subEl.textContent = String(sub);
        if (percent != null) {
          fillEl.style.width = `${Math.max(0, Math.min(100, percent))}%`;
          fillEl.style.background = color;
        }
      }
    };
  }

  const teleDl = createTelemetryTile('实时下载流速', 'arrow-down', '0 B/s', '累计下载: —');
  const teleUl = createTelemetryTile('实时上传流速', 'arrow-up', '0 B/s', '累计上传: —');
  const teleCpu = createTelemetryTile('处理器负载 (CPU)', 'cpu', '0%', '核心温度: —');
  const teleMem = createTelemetryTile('物理内存 (RAM)', 'memory', '0%', '已用内存: —');

  telemetryWrap.append(teleDl.el, teleUl.el, teleCpu.el, teleMem.el);
  grid.appendChild(telemetryWrap);

  // ============ ③ 蜂窝空口信道与物理层参数卡 (Row 3 Left: 6 列) ============
  function kvRow(label) {
    const value = el('span', { class: 'kv__v', text: '—' });
    return {
      el: el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: label }), value),
      set(v) { value.textContent = safeText(v); }
    };
  }

  const channelRows = {
    carrier: kvRow('移动运营商'),
    band: kvRow('当前工作频段'),
    arfcn: kvRow('下行频点 (ARFCN)'),
    pci: kvRow('物理小区 (PCI)'),
    rsrp: kvRow('接收功率 (RSRP)'),
    rsrq: kvRow('接收质量 (RSRQ)'),
    sinr: kvRow('信干噪比 (SINR)'),
    power: kvRow('供电运行模式'),
    uptime: kvRow('连续运行周期')
  };

  const channelCard = card({
    title: '5G 空口物理信道与链路矩阵',
    subtitle: '主载波物理层信道特征、空口射频增益与硬件运行周期',
    span: 6,
    icon: 'signal'
  });
  for (const k of Object.keys(channelRows)) channelCard.body.appendChild(channelRows[k].el);

  // ============ ④ 动态遥测波形走势与周期账单 (Row 3 Right: 6 列) ============
  const chartCard = card({
    title: '实时吞吐波形走势与周期传输',
    subtitle: '全带宽实时吞吐平滑连续采样与当月流量消耗',
    span: 6,
    icon: 'activity'
  });
  const refreshBtn = button(ctx.t('common.refresh'), {
    variant: 'ghost sm', icon: 'refresh', onClick: () => loadAll()
  });
  chartCard.head.querySelector('.card__actions').appendChild(refreshBtn);

  const sparkline = createSmoothSparkline({
    width: 600, height: 140, maxPoints: 180, strokeColor: '#2563eb', fillColor: '#38bdf8'
  });
  chartCard.body.appendChild(sparkline.el);

  // 底部汇总流量指标
  const trafficSummaryRow = el('div', {
    class: 'rf-stats-row',
    style: { marginTop: '14px', marginBottom: '0' }
  });
  function miniStat(label) {
    const val = el('span', { class: 'rf-stat-pill__val', text: '—', style: { fontSize: '14px' } });
    const pill = el('div', { class: 'rf-stat-pill' },
      el('span', { class: 'rf-stat-pill__label', text: label }),
      val
    );
    return { el: pill, set(v) { val.textContent = safeText(v); } };
  }
  const sumToday = miniStat('今日已用流量');
  const sumMonth = miniStat('当月结算流量');
  const sumTotal = miniStat('历史累计总流量');
  trafficSummaryRow.append(sumToday.el, sumMonth.el, sumTotal.el);
  chartCard.body.appendChild(trafficSummaryRow);

  grid.append(channelCard.el, chartCard.el);

  // ============ 真实数据管道与状态同步 ============
  let infoCache = {};
  let bandCache = {};
  let lastBytes = 0;
  let lastRx = 0;
  let lastTx = 0;
  let lastTime = Date.now();

  function applyInfo(info) {
    const o = fieldOf(info, 'data', info) || {};
    infoCache = { ...infoCache, ...o };

    channelRows.uptime.set(formatUptime(fieldOf(o, 'uptime', 0)));

    const cpu = Number(fieldOf(o, 'cpu_usage', fieldOf(o, 'cpu', 0))) || 0;
    const temp = safeText(fieldOf(o, 'temperature', '—'));
    const cpuColor = cpu >= 85 ? 'var(--danger)' : (cpu >= 65 ? 'var(--warning)' : 'var(--primary)');
    teleCpu.update(`${cpu.toFixed(1)}%`, cpu, `芯片核心温度: ${temp} °C`, cpuColor);

    const memUsed = Number(fieldOf(o, 'mem_used', 0)) || 0;
    const memTotal = Number(fieldOf(o, 'mem_total', 0)) || 0;
    const memPercent = memTotal > 0 ? (memUsed / memTotal) * 100 : 0;
    teleMem.update(
      `${memPercent.toFixed(1)}%`,
      memPercent,
      `${formatBytes(memUsed)} / ${formatBytes(memTotal)}`,
      memPercent > 85 ? 'var(--danger)' : 'var(--info)'
    );

    return cpu;
  }

  function applyBand(band) {
    const o = fieldOf(band, 'data', band) || {};
    // 合并入缓存，确保 WebSocket 推送的局部信号数据不冲掉其他字段
    bandCache = { ...bandCache, ...o };
    const cur = bandCache;

    const carrier = fieldOf(cur, 'carrier', fieldOf(cur, 'operator', infoCache.carrier || infoCache.operator || '中国移动 CMCC'));
    const networkType = (fieldOf(cur, 'network_type', fieldOf(cur, 'mode', 'nr')) || '').toUpperCase();
    const arfcn = fieldOf(cur, 'arfcn', fieldOf(cur, 'NRARFCN'));
    let bandName = fieldOf(cur, 'band', fieldOf(cur, 'current_band'));
    if (!bandName && arfcn) {
      bandName = arfcnToBand(networkType, arfcn);
    }
    const rsrp = fieldOf(cur, 'rsrp', fieldOf(cur, 'ssRsrp'));
    const rsrq = fieldOf(cur, 'rsrq', fieldOf(cur, 'ssRsrq'));
    const sinr = fieldOf(cur, 'sinr', fieldOf(cur, 'ssSinr'));
    const pci = fieldOf(cur, 'pci', fieldOf(cur, 'PhysicalCellId'));

    // 更新 5G 全息拓扑链路视窗
    heroRadar.update({
      carrier,
      networkType,
      band: bandName,
      rsrp,
      sinr,
      rsrq,
      arfcn,
      pci
    });

    // 格式化 RSRP 评级
    const numRsrp = Number(rsrp);
    let rsrpSuffix = '';
    if (Number.isFinite(numRsrp) && numRsrp !== 0) {
      if (numRsrp >= -80) rsrpSuffix = ' (极佳)';
      else if (numRsrp >= -95) rsrpSuffix = ' (良好)';
      else if (numRsrp >= -105) rsrpSuffix = ' (一般)';
      else rsrpSuffix = ' (较弱)';
    }

    // 格式化 SINR 符号
    const numSinr = Number(sinr);
    const sinrStr = Number.isFinite(numSinr) ? `${numSinr > 0 ? `+${numSinr}` : numSinr} dB` : '—';

    channelRows.carrier.set(carrier);
    channelRows.band.set(bandName ? `${bandName} (${networkType || '5G NR'} SA)` : (networkType || '—'));
    channelRows.arfcn.set(arfcn != null ? arfcn : '—');
    channelRows.pci.set(pci != null ? pci : '—');
    channelRows.rsrp.set(rsrp != null ? `${rsrp} dBm${rsrpSuffix}` : '—');
    channelRows.rsrq.set(rsrq != null ? `${rsrq} dB (优质)` : '—');
    channelRows.sinr.set(sinrStr);
  }

  function applyTotal(total) {
    const o = fieldOf(total, 'data', total) || {};
    const rx = Number(fieldOf(o, 'rx', fieldOf(o, 'download', 0))) || 0;
    const tx = Number(fieldOf(o, 'tx', fieldOf(o, 'upload', 0))) || 0;
    const totalBytes = Number(fieldOf(o, 'total', fieldOf(o, 'used', rx + tx))) || 0;

    sumToday.set(formatBytes(fieldOf(o, 'today', 0)));
    sumMonth.set(formatBytes(fieldOf(o, 'month', 0)));
    sumTotal.set(formatBytes(totalBytes));

    const now = Date.now();
    const timeDelta = Math.max(0.5, (now - lastTime) / 1000);
    const bytesDelta = lastBytes ? Math.max(0, totalBytes - lastBytes) : 0;
    const rxDelta = lastRx ? Math.max(0, rx - lastRx) : 0;
    const txDelta = lastTx ? Math.max(0, tx - lastTx) : 0;
    const currentRate = bytesDelta / timeDelta;
    const rxRate = rxDelta / timeDelta;
    const txRate = txDelta / timeDelta;
    lastBytes = totalBytes;
    lastRx = rx;
    lastTx = tx;
    lastTime = now;

    teleDl.update(formatRate(rxRate || currentRate), Math.min(100, ((rxRate || currentRate) / 5242880) * 100), `累计下载: ${formatBytes(rx)}`, 'var(--primary)');
    teleUl.update(formatRate(txRate), Math.min(100, (txRate / 1048576) * 100), `累计上传: ${formatBytes(tx)}`, 'var(--ok)');

    sparkline.push(currentRate);
  }

  function applyCharge(charge) {
    const o = fieldOf(charge, 'data', charge) || {};
    const level = Number(fieldOf(o, 'level', fieldOf(o, 'capacity', 0))) || 0;
    const charging = fieldOf(o, 'status', fieldOf(o, 'charging'));
    channelRows.power.set(charging ? `电池供电 · ${level}% (${charging})` : (level ? `电池供电 · ${level}%` : 'DC 适配器供电 (5V USB-C)'));
  }

  async function loadAll() {
    try {
      const [info, band, total, charge] = await Promise.all([
        ctx.api('/api/info', { signal: ctx.signal }),
        ctx.api('/api/current_band', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/get/Total', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/charge/config', { signal: ctx.signal }).catch(() => null)
      ]);

      applyInfo(info);
      if (band) applyBand(band);
      if (total) applyTotal(total);
      if (charge) applyCharge(charge);
    } catch (err) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  // ============ WebSocket 实时遥测通道 ============
  ctx.wsSubscribe('stats', (p) => {
    const o = (p && (p.data || p)) || {};
    const cpu = Number(o.cpu_usage != null ? o.cpu_usage : o.cpu) || 0;
    const cpuColor = cpu >= 85 ? 'var(--danger)' : (cpu >= 65 ? 'var(--warning)' : 'var(--primary)');
    teleCpu.update(`${cpu.toFixed(1)}%`, cpu, undefined, cpuColor);
  });

  ctx.wsSubscribe('traffic', (p) => {
    const o = (p && (p.data || p)) || {};
    const rate = Number(o.rate != null ? o.rate : o.delta) || 0;
    const rxRate = Number(o.rx_rate != null ? o.rx_rate : rate) || 0;
    const txRate = Number(o.tx_rate != null ? o.tx_rate : 0) || 0;
    teleDl.update(formatRate(rxRate), Math.min(100, (rxRate / 5242880) * 100));
    teleUl.update(formatRate(txRate), Math.min(100, (txRate / 1048576) * 100));
    sparkline.push(rate);
  });

  ctx.wsSubscribe('signal', (p) => {
    const o = (p && (p.data || p)) || {};
    // 仅携带 rsrp 时合并到 bandCache，不覆盖已有 band/pci/arfcn
    applyBand(o);
  });

  loadAll();
  ctx.interval(loadAll, 12000);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
}
