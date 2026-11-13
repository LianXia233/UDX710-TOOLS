/**
 * Modem 页：展示设备硬件 / 射频连接 / 频段参数 / 服务与邻区小区信息。
 * 配备动态 SVG 天线与射频链路质量图；所有数据直连后端真实 API。
 */
import { el, button, loadingState } from '../utils/dom.js';
import { fieldOf, safeText, asArray, formatDate } from '../utils/format.js';

let ctx = null;
let cellTable = null;

function kv(label, ref) {
  const v = el('span', { class: 'kv__v', text: '—' });
  ref[label] = v;
  return el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: label }), v);
}

/** 动态 SVG 射频链路与天线诊断图 */
function createAntennaVisualizer() {
  const svg = el('svg:svg', {
    class: 'antenna-svg',
    viewBox: '0 0 540 100',
    preserveAspectRatio: 'xMidYMid meet',
    role: 'img',
    'aria-label': '天线与射频链路状态'
  });

  const defs = el('svg:defs');
  const glow = el('svg:filter', { id: 'antGlow', x: '-20%', y: '-20%', width: '140%', height: '140%' },
    el('svg:feGaussianBlur', { stdDeviation: '2', result: 'b' }),
    el('svg:feMerge', {},
      el('svg:feMergeNode', { in: 'b' }),
      el('svg:feMergeNode', { in: 'SourceGraphic' })
    )
  );
  defs.appendChild(glow);
  svg.appendChild(defs);

  // 背景框
  svg.appendChild(el('svg:rect', {
    x: '4', y: '4', width: '532', height: '92', rx: '16',
    fill: 'var(--surface-muted)', stroke: 'var(--glass-border-subtle)', 'stroke-width': '1'
  }));

  // 左侧天线与信号柱
  const antGroup = el('svg:g', { transform: 'translate(40, 20)' });
  // 5 个动态信号强度柱
  const bars = [];
  const barHeights = [14, 22, 32, 42, 54];
  for (let i = 0; i < 5; i += 1) {
    const bar = el('svg:rect', {
      x: i * 12, y: 56 - barHeights[i], width: '8', height: barHeights[i], rx: '3',
      fill: i < 2 ? 'var(--primary)' : 'var(--border)',
      style: { transition: 'fill 0.3s ease' }
    });
    bars.push(bar);
    antGroup.appendChild(bar);
  }
  svg.appendChild(antGroup);

  // 中间指标标签
  const labelGroup = el('svg:g', { transform: 'translate(130, 36)' });
  const titleText = el('svg:text', {
    x: '0', y: '0', fill: 'var(--text)',
    'font-size': '15', 'font-weight': '700', 'font-family': 'var(--font)',
    text: '5G NR · 射频链路在线'
  });
  const detailText = el('svg:text', {
    x: '0', y: '24', fill: 'var(--text-muted)',
    'font-size': '12', 'font-family': 'var(--mono)',
    text: 'RSRP: — dBm · RSRQ: — dB · SINR: — dB'
  });
  labelGroup.append(titleText, detailText);
  svg.appendChild(labelGroup);

  // 右侧雷达小光晕
  const radarGroup = el('svg:g', { transform: 'translate(490, 50)' });
  const ring1 = el('svg:circle', { r: '8', fill: 'none', stroke: 'var(--primary)', 'stroke-width': '2' },
    el('svg:animate', { attributeName: 'r', values: '8;28', dur: '2.4s', repeatCount: 'indefinite' }),
    el('svg:animate', { attributeName: 'opacity', values: '0.8;0', dur: '2.4s', repeatCount: 'indefinite' })
  );
  const core = el('svg:circle', { r: '6', fill: 'var(--primary)', filter: 'url(#antGlow)' });
  radarGroup.append(ring1, core);
  svg.appendChild(radarGroup);

  return {
    el: svg,
    update(b) {
      const mode = (fieldOf(b, 'network_type', fieldOf(b, 'mode', '5G-MIFI')) || '').toUpperCase();
      const band = fieldOf(b, 'band', fieldOf(b, 'current_band', ''));
      titleText.textContent = `${mode}${band ? ` · ${band}` : ''}`;

      const rsrp = fieldOf(b, 'rsrp');
      const rsrq = fieldOf(b, 'rsrq');
      const sinr = fieldOf(b, 'sinr');
      detailText.textContent = `RSRP: ${safeText(rsrp)} dBm · RSRQ: ${safeText(rsrq)} dB · SINR: ${safeText(sinr)} dB`;

      // 依据真实 RSRP 更新信号柱
      const numRsrp = Number(rsrp);
      let activeBars = 1;
      if (Number.isFinite(numRsrp) && numRsrp !== 0) {
        if (numRsrp >= -80) activeBars = 5;
        else if (numRsrp >= -90) activeBars = 4;
        else if (numRsrp >= -100) activeBars = 3;
        else if (numRsrp >= -115) activeBars = 2;
        else activeBars = 1;
      }
      for (let i = 0; i < 5; i += 1) {
        bars[i].setAttribute('fill', i < activeBars ? 'var(--primary)' : 'var(--border)');
      }
    }
  };
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // 顶部天线链路图
  const antennaVis = createAntennaVisualizer();
  const antennaCard = ctx.card({
    title: '调制解调器射频链路诊断',
    subtitle: '天线增益与基带接收参数动态监测',
    span: 12,
    icon: 'activity'
  });
  antennaCard.body.appendChild(antennaVis.el);
  grid.appendChild(antennaCard.el);

  // 设备硬件信息
  const infoRef = {};
  const infoCard = ctx.card({ title: 'Modem 硬件与固件', subtitle: '蜂窝通信模组硬件信息与唯一标识', span: 6, icon: 'cpu' });
  const infoLabels = ['设备型号', 'IMEI', 'ICCID', '运营商', '固件版本', '序列号', '基带芯片温度', '系统运行时间'];
  for (const l of infoLabels) infoCard.body.appendChild(kv(l, infoRef));

  // 当前射频连接
  const bandRef = {};
  const bandCard = ctx.card({ title: '服务网络与载波连接', subtitle: '主载波与空口射频物理信道参数', span: 6, icon: 'signal' });
  const bandLabels = ['网络制式', '工作频段', '下行频点 (ARFCN)', '物理小区标识 (PCI)', 'RSRP 信号强度', 'RSRQ 信号质量', 'SINR 信干噪比', 'QCI 业务质量等级'];
  for (const l of bandLabels) bandCard.body.appendChild(kv(l, bandRef));
  const bandHint = el('p', { class: 'field__hint', text: '' });
  bandCard.body.appendChild(bandHint);

  // 小区与邻区数据表
  const cellCard = ctx.card({ title: '服务小区与邻区拓扑', subtitle: '主服小区与同频/异频邻区测量矩阵', span: 12, icon: 'network' });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: load });
  cellCard.head.querySelector('.card__actions').appendChild(refreshBtn);
  cellTable = ctx.dataTable({
    columns: [
      { label: '无线技术', render: (r) => safeText(r.type || r.technology) },
      { label: '频段 (Band)', render: (r) => safeText(r.band) },
      { label: '频点 (ARFCN)', render: (r) => safeText(r.arfcn) },
      { label: '小区标识 (PCI)', render: (r) => safeText(r.pci) },
      { label: 'RSRP (dBm)', render: (r) => safeText(r.rsrp) },
      {
        label: '小区状态',
        render: (r) => ctx.statusBadge(
          safeText(r.status || (r.serving ? '主服务小区' : '相邻小区')),
          r.serving ? 'ok' : 'neutral'
        )
      }
    ],
    rows: [],
    emptyText: '暂无邻区扫描数据或基带正在测量中'
  });
  cellCard.body.appendChild(cellTable.el);
  const cellHint = el('p', { class: 'field__hint', text: '' });
  cellCard.body.insertBefore(cellHint, cellTable.el);

  grid.append(infoCard.el, bandCard.el, cellCard.el);

  function setRef(ref, label, value) {
    if (ref[label]) ref[label].textContent = safeText(value);
  }

  async function load() {
    try {
      const [info, band, cells] = await Promise.all([
        ctx.api('/api/info', { signal: ctx.signal }),
        ctx.api('/api/current_band', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/cells', { signal: ctx.signal }).catch(() => null)
      ]);
      const i = fieldOf(info, 'data', info) || {};
      setRef(infoRef, '设备型号', fieldOf(i, 'model', fieldOf(i, 'modem_model')));
      setRef(infoRef, 'IMEI', fieldOf(i, 'imei'));
      setRef(infoRef, 'ICCID', fieldOf(i, 'iccid'));
      setRef(infoRef, '运营商', fieldOf(i, 'carrier'));
      setRef(infoRef, '固件版本', fieldOf(i, 'firmware', fieldOf(i, 'version')));
      setRef(infoRef, '序列号', fieldOf(i, 'serial'));
      setRef(infoRef, '基带芯片温度', `${safeText(fieldOf(i, 'temperature', '—'))} °C`);
      setRef(infoRef, '系统运行时间', formatDate(fieldOf(i, 'uptime', '—')));

      const b = fieldOf(band, 'data', band) || {};
      setRef(bandRef, '网络制式', fieldOf(b, 'network_type', fieldOf(b, 'mode')));
      setRef(bandRef, '工作频段', fieldOf(b, 'band', fieldOf(b, 'current_band')));
      setRef(bandRef, '下行频点 (ARFCN)', fieldOf(b, 'arfcn'));
      setRef(bandRef, '物理小区标识 (PCI)', fieldOf(b, 'pci'));
      setRef(bandRef, 'RSRP 信号强度', `${safeText(fieldOf(b, 'rsrp'))} dBm`);
      setRef(bandRef, 'RSRQ 信号质量', `${safeText(fieldOf(b, 'rsrq'))} dB`);
      setRef(bandRef, 'SINR 信干噪比', `${safeText(fieldOf(b, 'sinr'))} dB`);
      setRef(bandRef, 'QCI 业务质量等级', fieldOf(b, 'qci'));

      antennaVis.update(b);

      bandHint.textContent = b.available === false
        ? (b.reason || '模组未就绪，无法读取当前连接参数')
        : '';

      const rows = asArray(cells, ['cells', 'neighbors']);
      cellTable.setRows(rows);
      cellHint.textContent = rows.length
        ? ''
        : (fieldOf(cells, 'reason', '') || '');
    } catch (err) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  load();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  cellTable = null;
}
