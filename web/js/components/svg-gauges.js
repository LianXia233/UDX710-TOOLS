/**
 * svg-gauges.js —— 专业级动态 SVG 仪表盘、5G 射频全息拓扑与高速流速可视化组件。
 *
 * 遵循严格 CSP：纯原生 DOM / SVG 命名空间 API 构建，零内联脚本与 innerHTML。
 * 纯真实数据接入，支持平滑过渡与全分辨率响应式缩放。
 */
import { el, SVG_NS } from '../utils/dom.js';
import { formatRate } from '../utils/format.js';

let gaugeCounter = 0;

/**
 * 5G 射频全息拓扑视窗 (5G Holographic Cellular Radar)
 * 全景动态 SVG，展示模组核心、多重调频波形载波、基站脉冲与物理信道 HUD。
 */
export function createCellularHologramHero() {
  gaugeCounter += 1;
  const idPrefix = `holo-${gaugeCounter}`;
  const W = 800;
  const H = 150;

  const svg = el('svg:svg', {
    class: 'cellular-hologram-svg',
    viewBox: `0 0 ${W} ${H}`,
    preserveAspectRatio: 'xMidYMid meet',
    role: 'img',
    'aria-label': '5G 射频全息拓扑链路'
  });

  const defs = el('svg:defs');

  // 渐变定义
  const cyanGrad = el('svg:linearGradient', { id: `${idPrefix}-cyan`, x1: '0%', y1: '0%', x2: '100%', y2: '0%' },
    el('svg:stop', { offset: '0%', 'stop-color': '#06b6d4', 'stop-opacity': '0.9' }),
    el('svg:stop', { offset: '100%', 'stop-color': '#3b82f6', 'stop-opacity': '0.9' })
  );
  const blueGrad = el('svg:linearGradient', { id: `${idPrefix}-blue`, x1: '0%', y1: '0%', x2: '100%', y2: '0%' },
    el('svg:stop', { offset: '0%', 'stop-color': '#2563eb', 'stop-opacity': '0.85' }),
    el('svg:stop', { offset: '100%', 'stop-color': '#10b981', 'stop-opacity': '0.85' })
  );

  // 辉光滤镜
  const glow = el('svg:filter', { id: `${idPrefix}-glow`, x: '-20%', y: '-20%', width: '140%', height: '140%' },
    el('svg:feGaussianBlur', { stdDeviation: '3', result: 'coloredBlur' }),
    el('svg:feMerge', {},
      el('svg:feMergeNode', { in: 'coloredBlur' }),
      el('svg:feMergeNode', { in: 'SourceGraphic' })
    )
  );

  defs.append(cyanGrad, blueGrad, glow);
  svg.appendChild(defs);

  // ① 背景微网格标尺
  const gridGroup = el('svg:g', { stroke: 'var(--border)', 'stroke-opacity': '0.25', 'stroke-width': '0.8' });
  for (let x = 60; x < W; x += 60) {
    gridGroup.appendChild(el('svg:line', { x1: x, y1: 10, x2: x, y2: H - 10, 'stroke-dasharray': '2 6' }));
  }
  for (let y = 30; y < H; y += 30) {
    gridGroup.appendChild(el('svg:line', { x1: 40, y1: y, x2: W - 40, y2: y, 'stroke-dasharray': '2 6' }));
  }
  svg.appendChild(gridGroup);

  // ② 动态双正弦载波束 (Carrier Waves)
  const wave1Path = 'M 90 75 Q 220 25, 380 75 T 710 75';
  const wave2Path = 'M 90 75 Q 240 125, 400 75 T 710 75';

  const waveGroup = el('svg:g', { fill: 'none', 'stroke-linecap': 'round' });
  const wave1 = el('svg:path', {
    d: wave1Path, stroke: `url(#${idPrefix}-cyan)`, 'stroke-width': '2.2', filter: `url(#${idPrefix}-glow)`
  });
  const wave2 = el('svg:path', {
    d: wave2Path, stroke: `url(#${idPrefix}-blue)`, 'stroke-width': '1.8', 'stroke-opacity': '0.75'
  });
  waveGroup.append(wave1, wave2);

  // 穿梭数据光子 (Data Photons)
  const photon1 = el('svg:circle', { r: '4', fill: '#06b6d4', filter: `url(#${idPrefix}-glow)` });
  photon1.appendChild(el('svg:animateMotion', { dur: '2.2s', repeatCount: 'indefinite', path: wave1Path }));

  const photon2 = el('svg:circle', { r: '3.5', fill: '#10b981', filter: `url(#${idPrefix}-glow)` });
  photon2.appendChild(el('svg:animateMotion', { dur: '2.8s', repeatCount: 'indefinite', path: wave2Path }));

  waveGroup.append(photon1, photon2);
  svg.appendChild(waveGroup);

  // ③ 左侧节点：UDX710 5G 模组核心 (Device Node, x=90, y=75)
  const deviceGroup = el('svg:g', { transform: 'translate(90, 75)' });
  const pulseRing = el('svg:circle', { r: '16', fill: 'none', stroke: '#38bdf8', 'stroke-width': '1.5' },
    el('svg:animate', { attributeName: 'r', values: '16;44', dur: '2.5s', repeatCount: 'indefinite' }),
    el('svg:animate', { attributeName: 'opacity', values: '0.8;0', dur: '2.5s', repeatCount: 'indefinite' })
  );
  const rotRing = el('svg:circle', {
    r: '26', fill: 'none', stroke: 'var(--primary)',
    'stroke-width': '1.5', 'stroke-dasharray': '4 8', 'stroke-opacity': '0.65'
  },
    el('svg:animateTransform', {
      attributeName: 'transform', type: 'rotate', from: '0', to: '360', dur: '12s', repeatCount: 'indefinite'
    })
  );
  const deviceCore = el('svg:circle', {
    r: '17', fill: 'var(--primary)', filter: `url(#${idPrefix}-glow)`, stroke: '#ffffff', 'stroke-width': '2'
  });
  const deviceText = el('svg:text', {
    x: '0', y: '4', 'text-anchor': 'middle', fill: '#ffffff',
    'font-size': '9.5', 'font-weight': '900', 'font-family': 'var(--mono)', text: '5G'
  });
  const deviceSub = el('svg:text', {
    x: '0', y: '36', 'text-anchor': 'middle', fill: 'var(--text-muted)',
    'font-size': '10.5', 'font-weight': '700', text: 'UDX710 模组'
  });
  deviceGroup.append(pulseRing, rotRing, deviceCore, deviceText, deviceSub);
  svg.appendChild(deviceGroup);

  // ④ 右侧节点：蜂窝基站铁塔 (Base Station Node, x=710, y=75)
  const towerGroup = el('svg:g', { transform: 'translate(710, 75)' });
  // 基站脉冲波纹
  const towerPulse = el('svg:circle', { r: '14', fill: 'none', stroke: '#2563eb', 'stroke-width': '1.5' },
    el('svg:animate', { attributeName: 'r', values: '14;40', dur: '2.5s', begin: '1.2s', repeatCount: 'indefinite' }),
    el('svg:animate', { attributeName: 'opacity', values: '0.75;0', dur: '2.5s', begin: '1.2s', repeatCount: 'indefinite' })
  );
  const towerBase = el('svg:circle', {
    r: '16', fill: 'var(--surface-elevated)', stroke: 'var(--primary)', 'stroke-width': '2'
  });
  const towerDot = el('svg:circle', { r: '5', fill: '#2563eb' });
  const towerSub = el('svg:text', {
    x: '0', y: '36', 'text-anchor': 'middle', fill: 'var(--text-muted)',
    'font-size': '10.5', 'font-weight': '700', text: '5G 服务基站'
  });
  towerGroup.append(towerPulse, towerBase, towerDot, towerSub);
  svg.appendChild(towerGroup);

  // ⑤ 顶部与底部 HUD 徽标层 (HTML 浮层，确保字体锐利)
  const carrierBadge = el('span', { class: 'holo-badge holo-badge--carrier', text: '蜂窝网络 · 探测中' });
  const saBadge = el('span', { class: 'holo-badge holo-badge--sa', text: '5G NR SA 独立组网' });

  const rsrpTag = el('span', { class: 'holo-stat-pill', text: 'RSRP: — dBm' });
  const sinrTag = el('span', { class: 'holo-stat-pill', text: 'SINR: — dB' });
  const rsrqTag = el('span', { class: 'holo-stat-pill', text: 'RSRQ: — dB' });
  const cellTag = el('span', { class: 'holo-stat-pill', text: 'PCI: — · ARFCN: —' });

  const hudTop = el('div', { class: 'holo-hud holo-hud--top' }, carrierBadge, saBadge);
  const hudBottom = el('div', { class: 'holo-hud holo-hud--bottom' }, rsrpTag, sinrTag, rsrqTag, cellTag);

  const container = el('div', { class: 'cellular-hologram-wrap' }, svg, hudTop, hudBottom);

  return {
    el: container,
    update({ carrier, networkType, band, rsrp, sinr, rsrq, arfcn, pci }) {
      const mode = (networkType || band || '5G NR').toUpperCase();
      const op = carrier || '中国移动 CMCC';
      carrierBadge.textContent = `${op} · ${band || mode}`;

      if (networkType && networkType.toLowerCase().includes('lte')) {
        saBadge.textContent = '4G LTE 网络已连接';
        deviceText.textContent = '4G';
      } else {
        saBadge.textContent = '5G NR SA 独立组网 · 物理层就绪';
        deviceText.textContent = '5G';
      }

      rsrpTag.textContent = rsrp != null ? `RSRP: ${rsrp} dBm` : 'RSRP: —';
      sinrTag.textContent = sinr != null ? `SINR: ${Number(sinr) > 0 ? `+${sinr}` : sinr} dB` : 'SINR: —';
      rsrqTag.textContent = rsrq != null ? `RSRQ: ${rsrq} dB` : 'RSRQ: —';
      cellTag.textContent = `PCI: ${pci != null ? pci : '—'} · ARFCN: ${arfcn != null ? arfcn : '—'}`;

      const numRsrp = Number(rsrp);
      if (Number.isFinite(numRsrp) && numRsrp !== 0) {
        if (numRsrp >= -85) {
          deviceCore.setAttribute('fill', '#059669');
          rsrpTag.style.color = '#059669';
        } else if (numRsrp >= -105) {
          deviceCore.setAttribute('fill', '#2563eb');
          rsrpTag.style.color = 'var(--primary)';
        } else {
          deviceCore.setAttribute('fill', '#d97706');
          rsrpTag.style.color = 'var(--warning)';
        }
      }
    }
  };
}

/**
 * 高密度紧凑型流速弧形仪表 (Speed Arc Gauge)
 * 180° 半环弧度，动态指针点与渐变光环，适用于实时下载/上传流速。
 */
export function createSpeedArcGauge({
  label = '',
  unit = 'KB/s',
  color = '#2563eb',
  trackColor = 'rgba(37, 99, 235, 0.12)'
} = {}) {
  gaugeCounter += 1;
  const idPrefix = `arc-${gaugeCounter}`;
  const W = 130;
  const H = 82;
  const cx = W / 2;
  const cy = 72;
  const r = 48;
  const stroke = 6.5;
  const arcLength = Math.PI * r;

  const svg = el('svg:svg', {
    class: 'speed-arc-svg',
    viewBox: `0 0 ${W} ${H}`,
    width: W,
    height: H,
    role: 'img',
    'aria-label': `${label} 流速仪表`
  });

  const defs = el('svg:defs');
  const grad = el('svg:linearGradient', { id: `${idPrefix}-grad`, x1: '0%', y1: '0%', x2: '100%', y2: '0%' },
    el('svg:stop', { offset: '0%', 'stop-color': color, 'stop-opacity': '0.35' }),
    el('svg:stop', { offset: '100%', 'stop-color': color, 'stop-opacity': '1' })
  );
  defs.appendChild(grad);
  svg.appendChild(defs);

  // 背景轨道弧 (180°)
  const bgArc = el('svg:path', {
    d: `M ${cx - r} ${cy} A ${r} ${r} 0 0 1 ${cx + r} ${cy}`,
    fill: 'none', stroke: trackColor, 'stroke-width': stroke, 'stroke-linecap': 'round'
  });

  // 前景进度弧
  const progressArc = el('svg:path', {
    d: `M ${cx - r} ${cy} A ${r} ${r} 0 0 1 ${cx + r} ${cy}`,
    fill: 'none', stroke: `url(#${idPrefix}-grad)`, 'stroke-width': stroke,
    'stroke-linecap': 'round',
    'stroke-dasharray': arcLength,
    'stroke-dashoffset': arcLength,
    style: { transition: 'stroke-dashoffset 0.45s ease' }
  });

  // 指针当前点
  const pointerDot = el('svg:circle', {
    cx: cx - r, cy, r: '4', fill: '#ffffff', stroke: color, 'stroke-width': '2',
    style: { display: 'none', transition: 'all 0.45s ease' }
  });

  svg.append(bgArc, progressArc, pointerDot);

  return {
    el: svg,
    update(ratio) {
      const clampRatio = Math.max(0, Math.min(1, ratio));
      const offset = arcLength * (1 - clampRatio);
      progressArc.setAttribute('stroke-dashoffset', offset.toFixed(2));

      if (clampRatio > 0.02) {
        const angle = Math.PI * (1 - clampRatio);
        const px = cx + r * Math.cos(angle);
        const py = cy - r * Math.sin(angle);
        pointerDot.setAttribute('cx', px.toFixed(1));
        pointerDot.setAttribute('cy', py.toFixed(1));
        pointerDot.style.display = 'block';
      } else {
        pointerDot.style.display = 'none';
      }
    }
  };
}

/**
 * 紧凑型系统遥测环 (Mini Ring Gauge)
 * 适用于 CPU / 内存 / 温度微瓦片。
 */
export function createMiniRing({ size = 44, strokeWidth = 4, color = 'var(--primary)' } = {}) {
  const r = (size - strokeWidth) / 2;
  const circumference = 2 * Math.PI * r;

  const svg = el('svg:svg', {
    class: 'mini-ring-svg',
    viewBox: `0 0 ${size} ${size}`,
    width: size,
    height: size
  });

  const bg = el('svg:circle', {
    cx: size / 2, cy: size / 2, r,
    fill: 'none', stroke: 'var(--border)', 'stroke-width': strokeWidth, 'stroke-opacity': '0.4'
  });

  const fg = el('svg:circle', {
    cx: size / 2, cy: size / 2, r,
    fill: 'none', stroke: color, 'stroke-width': strokeWidth,
    'stroke-linecap': 'round',
    'stroke-dasharray': circumference,
    'stroke-dashoffset': circumference,
    transform: `rotate(-90 ${size / 2} ${size / 2})`,
    style: { transition: 'stroke-dashoffset 0.5s ease' }
  });

  svg.append(bg, fg);

  return {
    el: svg,
    update(percent, customColor) {
      const p = Math.max(0, Math.min(100, Number(percent) || 0));
      const offset = circumference * (1 - p / 100);
      fg.setAttribute('stroke-dashoffset', offset.toFixed(2));
      if (customColor) fg.setAttribute('stroke', customColor);
    }
  };
}

/**
 * 射频指标线性量程条 (RF Range Bar)
 * 包含极佳/良好/一般/较弱的彩色量程刻度与指针光标。
 */
export function createRfRangeBar({
  min = -140,
  max = -44,
  label = 'RSRP',
  unit = 'dBm'
} = {}) {
  const valEl = el('span', { class: 'rf-bar__val', text: '—' });
  const tagEl = el('span', { class: 'rf-bar__tag badge badge--ok', text: '' });
  tagEl.style.display = 'none';

  const cursorEl = el('div', { class: 'rf-bar__cursor', style: { left: '0%' } });
  const trackEl = el('div', { class: 'rf-bar__track' }, cursorEl);

  const root = el('div', { class: 'rf-bar-card' },
    el('div', { class: 'rf-bar__head' },
      el('span', { class: 'rf-bar__label', text: label }),
      tagEl
    ),
    el('div', { class: 'rf-bar__val-row' },
      valEl,
      el('span', { class: 'rf-bar__unit', text: unit })
    ),
    trackEl
  );

  return {
    el: root,
    set(val, tagText = '', tagType = 'ok') {
      if (val != null) {
        valEl.textContent = String(val);
        const num = Number(val);
        if (Number.isFinite(num)) {
          const ratio = Math.max(0, Math.min(100, ((num - min) / (max - min)) * 100));
          cursorEl.style.left = `${ratio}%`;
          cursorEl.style.display = 'block';
        } else {
          cursorEl.style.display = 'none';
        }
      } else {
        valEl.textContent = '—';
        cursorEl.style.display = 'none';
      }

      if (tagText) {
        tagEl.textContent = tagText;
        tagEl.className = `rf-bar__tag badge badge--${tagType}`;
        tagEl.style.display = 'inline-flex';
      } else {
        tagEl.style.display = 'none';
      }
    }
  };
}

/**
 * 现代化交互式平滑吞吐波形走势图 (Smooth Spline Sparkline)
 * - 原生兼容桌面鼠标悬停 (Hover) 与移动端触摸屏 (Touch/Drag) 滑动手势；
 * - 实时标尺刻度：动态适配速率单位 (B/s, KB/s, MB/s)，消除裸露无单位数字；
 * - 悬浮探针 HUD 卡片：随指针动态吸附折线，呈现具体时间与瞬时真实速率。
 */
export function createSmoothSparkline({
  width = 600,
  height = 140,
  maxPoints = 120,
  strokeColor = '#2563eb',
  fillColor = '#3b82f6',
  label = '实时吞吐波形',
  formatter = formatRate
} = {}) {
  const series = [];
  let currentPts = [];

  const svg = el('svg:svg', {
    class: 'sparkline-smooth',
    viewBox: `0 0 ${width} ${height}`,
    preserveAspectRatio: 'none',
    role: 'img',
    'aria-label': '平滑动态吞吐趋势折线图',
    style: {
      touchAction: 'none',
      userSelect: 'none',
      cursor: 'crosshair',
      display: 'block',
      width: '100%',
      height: `${height}px`
    }
  });

  const defs = el('svg:defs');
  const gradId = `spark-grad-${Math.random().toString(36).slice(2, 8)}`;
  const grad = el('svg:linearGradient', { id: gradId, x1: '0%', y1: '0%', x2: '0%', y2: '100%' },
    el('svg:stop', { offset: '0%', 'stop-color': fillColor, 'stop-opacity': '0.38' }),
    el('svg:stop', { offset: '65%', 'stop-color': fillColor, 'stop-opacity': '0.10' }),
    el('svg:stop', { offset: '100%', 'stop-color': fillColor, 'stop-opacity': '0' })
  );

  const glowFilter = el('svg:filter', { id: `${gradId}-glow`, x: '-10%', y: '-20%', width: '120%', height: '140%' },
    el('svg:feGaussianBlur', { stdDeviation: '2', result: 'b' }),
    el('svg:feMerge', {},
      el('svg:feMergeNode', { in: 'b' }),
      el('svg:feMergeNode', { in: 'SourceGraphic' })
    )
  );

  const shadowFilter = el('svg:filter', { id: `${gradId}-shadow`, x: '-20%', y: '-20%', width: '140%', height: '140%' },
    el('svg:feDropShadow', { dx: '0', dy: '3', stdDeviation: '4', 'flood-color': 'rgba(15, 23, 42, 0.18)' })
  );

  defs.append(grad, glowFilter, shadowFilter);
  svg.appendChild(defs);

  // ① 背景微网格标尺
  for (let i = 0; i <= 3; i += 1) {
    const y = i * (height / 3);
    svg.appendChild(el('svg:line', {
      x1: 0, y1: y, x2: width, y2: y,
      stroke: 'var(--border)', 'stroke-opacity': '0.3', 'stroke-width': '1', 'stroke-dasharray': i === 0 || i === 3 ? 'none' : '4 4'
    }));
  }

  // ② 右侧 Y 轴动态速率标尺
  const scaleMax = el('svg:text', {
    x: width - 8, y: 14, fill: 'var(--text-muted)',
    'font-size': '10', 'font-family': 'var(--mono)', 'text-anchor': 'end', text: '—'
  });
  const scaleMid = el('svg:text', {
    x: width - 8, y: Math.round(height / 2) + 4, fill: 'var(--text-muted)',
    'font-size': '10', 'font-family': 'var(--mono)', 'text-anchor': 'end', text: '—'
  });
  const scaleZero = el('svg:text', {
    x: width - 8, y: height - 6, fill: 'var(--text-muted)',
    'font-size': '10', 'font-family': 'var(--mono)', 'text-anchor': 'end', text: formatter(0)
  });
  svg.append(scaleMax, scaleMid, scaleZero);

  // ③ 曲线与渐变区域
  const areaPath = el('svg:path', { fill: `url(#${gradId})`, d: '' });
  const linePath = el('svg:path', {
    fill: 'none', stroke: strokeColor, 'stroke-width': '2.4',
    'stroke-linecap': 'round', 'stroke-linejoin': 'round',
    filter: `url(#${gradId}-glow)`, d: ''
  });

  // ④ 最新数据端点发光信标
  const beaconGroup = el('svg:g', { style: { display: 'none' } });
  const pulseRing = el('svg:circle', {
    r: '4', fill: 'none', stroke: strokeColor, 'stroke-width': '1.5'
  },
    el('svg:animate', { attributeName: 'r', values: '4;14', dur: '1.8s', repeatCount: 'indefinite' }),
    el('svg:animate', { attributeName: 'opacity', values: '0.9;0', dur: '1.8s', repeatCount: 'indefinite' })
  );
  const pulseDot = el('svg:circle', {
    r: '4', fill: '#ffffff', stroke: strokeColor, 'stroke-width': '2.5'
  });
  beaconGroup.append(pulseRing, pulseDot);

  // ⑤ 顶部 HUD 动态文本栏
  const infoText = el('svg:text', {
    x: 8, y: 15, fill: 'var(--text)',
    'font-size': '11', 'font-weight': '650', 'font-family': 'var(--font)', text: ''
  });

  // ⑥ 交互式十字准星垂直探针线 (Crosshair Hairline)
  const crosshairLine = el('svg:line', {
    y1: 0, y2: height, stroke: 'var(--primary, #087cba)', 'stroke-width': '1.2',
    'stroke-dasharray': '3 3', style: { display: 'none', pointerEvents: 'none' }
  });

  // ⑦ 交互式吸附点 (Focus Circle)
  const focusCircle = el('svg:circle', {
    r: '5', fill: '#ffffff', stroke: 'var(--primary, #087cba)', 'stroke-width': '2.5',
    style: { display: 'none', pointerEvents: 'none' }
  });

  // ⑧ 悬浮探针 HUD 卡片 (Interactive Tooltip Pill)
  const tooltipGroup = el('svg:g', {
    style: { display: 'none', pointerEvents: 'none', transition: 'transform 0.08s ease-out' }
  });
  const tooltipBg = el('svg:rect', {
    x: 0, y: 0, width: 142, height: 42, rx: 6,
    fill: 'var(--surface-overlay, #ffffff)',
    stroke: 'var(--primary, #087cba)',
    'stroke-width': '1.2',
    filter: `url(#${gradId}-shadow)`
  });
  const ttTimeText = el('svg:text', {
    x: 10, y: 16, fill: 'var(--text-muted, #5f7a93)',
    'font-size': '9.5', 'font-family': 'var(--mono)'
  });
  const ttValText = el('svg:text', {
    x: 10, y: 32, fill: 'var(--primary, #087cba)',
    'font-size': '12.5', 'font-weight': '750', 'font-family': 'var(--mono)'
  });
  tooltipGroup.append(tooltipBg, ttTimeText, ttValText);

  svg.append(areaPath, linePath, beaconGroup, infoText, crosshairLine, focusCircle, tooltipGroup);

  function buildSpline(pts) {
    if (pts.length < 2) return '';
    let d = `M ${pts[0].x.toFixed(1)} ${pts[0].y.toFixed(1)}`;
    for (let i = 0; i < pts.length - 1; i += 1) {
      const p0 = pts[i === 0 ? i : i - 1];
      const p1 = pts[i];
      const p2 = pts[i + 1];
      const p3 = pts[i + 2] || p2;

      const cp1x = p1.x + (p2.x - p0.x) / 6;
      const cp1y = p1.y + (p2.y - p0.y) / 6;
      const cp2x = p2.x - (p3.x - p1.x) / 6;
      const cp2y = p2.y - (p3.y - p1.y) / 6;

      d += ` C ${cp1x.toFixed(1)} ${cp1y.toFixed(1)}, ${cp2x.toFixed(1)} ${cp2y.toFixed(1)}, ${p2.x.toFixed(1)} ${p2.y.toFixed(1)}`;
    }
    return d;
  }

  function redraw() {
    if (series.length < 2) {
      linePath.setAttribute('d', '');
      areaPath.setAttribute('d', '');
      beaconGroup.style.display = 'none';
      return;
    }
    const maxVal = Math.max(1, ...series);
    const step = width / (series.length - 1);
    const pts = series.map((v, i) => {
      const x = i * step;
      const ratio = Math.max(0, Math.min(1, v / maxVal));
      const y = height - ratio * (height - 24) - 10;
      return { x, y };
    });
    currentPts = pts;

    const d = buildSpline(pts);
    linePath.setAttribute('d', d);
    areaPath.setAttribute('d', `${d} L ${width} ${height} L 0 ${height} Z`);

    const lastPt = pts[pts.length - 1];
    pulseRing.setAttribute('cx', lastPt.x.toFixed(1));
    pulseRing.setAttribute('cy', lastPt.y.toFixed(1));
    pulseDot.setAttribute('cx', lastPt.x.toFixed(1));
    pulseDot.setAttribute('cy', lastPt.y.toFixed(1));
    beaconGroup.style.display = 'block';

    const latest = series[series.length - 1];
    infoText.textContent = `实时: ${formatter(latest)}  ·  峰值: ${formatter(maxVal)}`;

    // 更新右侧刻度
    scaleMax.textContent = formatter(maxVal);
    scaleMid.textContent = formatter(maxVal / 2);
  }

  // 鼠标与触控事件处理（Pointer Events API，自动兼容桌面鼠标与手机/平板触摸屏）
  let leaveTimer = null;
  function handlePointer(ev) {
    if (series.length < 2 || currentPts.length === 0) return;
    if (leaveTimer) { clearTimeout(leaveTimer); leaveTimer = null; }

    const rect = svg.getBoundingClientRect();
    const clientX = ev.clientX;
    const clientY = ev.clientY;
    if (clientX < rect.left || clientX > rect.right) return;

    const xInSvg = Math.max(0, Math.min(width, ((clientX - rect.left) / rect.width) * width));
    const step = width / (series.length - 1);
    const idx = Math.max(0, Math.min(series.length - 1, Math.round(xInSvg / step)));
    const pt = currentPts[idx];
    if (!pt) return;

    const val = series[idx];
    const timeAgoSec = Math.round((series.length - 1 - idx) * 2);

    crosshairLine.setAttribute('x1', pt.x.toFixed(1));
    crosshairLine.setAttribute('x2', pt.x.toFixed(1));
    crosshairLine.style.display = 'block';

    focusCircle.setAttribute('cx', pt.x.toFixed(1));
    focusCircle.setAttribute('cy', pt.y.toFixed(1));
    focusCircle.style.display = 'block';

    ttTimeText.textContent = idx === series.length - 1 ? '● 最新实时采样点' : `● ${timeAgoSec} 秒前采样 (点 #${idx + 1})`;
    ttValText.textContent = `流速: ${formatter(val)}`;

    let ttX = pt.x + 12;
    let ttY = Math.max(6, Math.min(height - 48, pt.y - 22));
    if (ttX + 148 > width) {
      ttX = pt.x - 152;
    }
    tooltipGroup.setAttribute('transform', `translate(${ttX.toFixed(1)}, ${ttY.toFixed(1)})`);
    tooltipGroup.style.display = 'block';
  }

  function handlePointerLeave() {
    if (leaveTimer) clearTimeout(leaveTimer);
    leaveTimer = setTimeout(() => {
      crosshairLine.style.display = 'none';
      focusCircle.style.display = 'none';
      tooltipGroup.style.display = 'none';
    }, 1200);
  }

  svg.addEventListener('pointerenter', handlePointer);
  svg.addEventListener('pointermove', handlePointer);
  svg.addEventListener('pointerdown', handlePointer);
  svg.addEventListener('pointerleave', handlePointerLeave);
  svg.addEventListener('pointerup', handlePointerLeave);
  svg.addEventListener('pointercancel', handlePointerLeave);

  return {
    el: svg,
    push(val) {
      series.push(Number(val) || 0);
      if (series.length > maxPoints) series.shift();
      redraw();
    },
    clear() {
      series.length = 0;
      currentPts = [];
      redraw();
    }
  };
}

/**
 * 基础圆形仪表 (兼容向后调用)
 */
export function createCircularGauge({
  value = 0,
  max = 100,
  label = '',
  unit = '%',
  color = 'primary',
  size = 80,
  strokeWidth = 6
} = {}) {
  const ring = createMiniRing({ size, strokeWidth, color: 'var(--primary)' });
  ring.update((value / max) * 100);
  return {
    el: ring.el,
    update: (v) => ring.update((v / max) * 100)
  };
}

export default {
  createCellularHologramHero,
  createSpeedArcGauge,
  createMiniRing,
  createRfRangeBar,
  createSmoothSparkline,
  createCircularGauge
};
