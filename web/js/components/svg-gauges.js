/**
 * svg-gauges.js —— 专业级动态 SVG 仪表盘、5G 射频拓扑雷达与流速可视化组件。
 *
 * 遵循严格 CSP：纯原生 DOM / SVG 命名空间 API 构建，零内联脚本与 innerHTML。
 * 纯真实数据接入，不捏造任何数值；支持平滑过渡与全分辨率响应式缩放。
 */
import { el, SVG_NS } from '../utils/dom.js';

let gaugeCounter = 0;

/**
 * 创建高精度多弧动态环形遥测仪表 (Precision Multi-Arc Telemetry Gauge)
 * @param {Object} options
 * @param {number} [options.value=0] 当前值
 * @param {number} [options.max=100] 最大值
 * @param {string} [options.label=''] 仪表盘底部标签
 * @param {string} [options.unit='%'] 数值单位
 * @param {string} [options.color='primary'] 颜色主调 ('primary' | 'ok' | 'warning' | 'danger' | 'info' | 'auto')
 * @param {number} [options.size=124] 宽高尺寸 (px)
 * @param {number} [options.strokeWidth=8] 圆环粗细
 */
export function createCircularGauge({
  value = 0,
  max = 100,
  label = '',
  unit = '%',
  color = 'primary',
  size = 124,
  strokeWidth = 8
} = {}) {
  gaugeCounter += 1;
  const idPrefix = `gauge-${gaugeCounter}`;
  const r = (size - strokeWidth * 2.4) / 2;
  const circumference = 2 * Math.PI * r;

  const colorMap = {
    primary: ['#3b82f6', '#2563eb', '#1d4ed8'],
    ok: ['#34d399', '#10b981', '#059669'],
    warning: ['#fbbf24', '#f59e0b', '#d97706'],
    danger: ['#f87171', '#ef4444', '#dc2626'],
    info: ['#38bdf8', '#06b6d4', '#0891b2']
  };

  const svg = el('svg:svg', {
    class: 'svg-gauge',
    viewBox: `0 0 ${size} ${size}`,
    width: size,
    height: size,
    role: 'img',
    'aria-label': `${label}: ${value}${unit}`
  });

  const defs = el('svg:defs');
  const grad = el('svg:linearGradient', {
    id: `${idPrefix}-grad`,
    x1: '0%', y1: '100%', x2: '100%', y2: '0%'
  });

  const stop1 = el('svg:stop', { offset: '0%', 'stop-color': colorMap.primary[0] });
  const stop2 = el('svg:stop', { offset: '65%', 'stop-color': colorMap.primary[1] });
  const stop3 = el('svg:stop', { offset: '100%', 'stop-color': colorMap.primary[2] });
  grad.append(stop1, stop2, stop3);

  const glowFilter = el('svg:filter', {
    id: `${idPrefix}-glow`,
    x: '-20%', y: '-20%', width: '140%', height: '140%'
  },
    el('svg:feGaussianBlur', { stdDeviation: '3', result: 'coloredBlur' }),
    el('svg:feMerge', {},
      el('svg:feMergeNode', { in: 'coloredBlur' }),
      el('svg:feMergeNode', { in: 'SourceGraphic' })
    )
  );

  defs.append(grad, glowFilter);
  svg.appendChild(defs);

  // 外圈微刻度线环 (12 个方向均匀点缀)
  const ticksGroup = el('svg:g', { stroke: 'var(--border)', 'stroke-opacity': '0.35', 'stroke-width': '1.2' });
  const tickRadius = r + strokeWidth * 0.9;
  for (let i = 0; i < 12; i += 1) {
    const angle = (i * 30) * (Math.PI / 180);
    const x1 = (size / 2) + (tickRadius - 3) * Math.cos(angle);
    const y1 = (size / 2) + (tickRadius - 3) * Math.sin(angle);
    const x2 = (size / 2) + tickRadius * Math.cos(angle);
    const y2 = (size / 2) + tickRadius * Math.sin(angle);
    ticksGroup.appendChild(el('svg:line', { x1, y1, x2, y2 }));
  }
  svg.appendChild(ticksGroup);

  // 背景底环 (轻磨砂轨道)
  const bgCircle = el('svg:circle', {
    cx: size / 2,
    cy: size / 2,
    r,
    fill: 'none',
    stroke: 'var(--border)',
    'stroke-width': strokeWidth,
    'stroke-opacity': '0.3'
  });

  // 前景进度环
  const progressCircle = el('svg:circle', {
    cx: size / 2,
    cy: size / 2,
    r,
    fill: 'none',
    stroke: `url(#${idPrefix}-grad)`,
    'stroke-width': strokeWidth,
    'stroke-linecap': 'round',
    'stroke-dasharray': circumference,
    'stroke-dashoffset': circumference,
    transform: `rotate(-90 ${size / 2} ${size / 2})`,
    filter: `url(#${idPrefix}-glow)`,
    style: {
      transition: 'stroke-dashoffset 0.65s cubic-bezier(0.4, 0, 0.2, 1)'
    }
  });

  // 中心数值文本
  const valText = el('svg:text', {
    x: size / 2,
    y: (size / 2) + 1,
    'text-anchor': 'middle',
    'dominant-baseline': 'middle',
    class: 'svg-gauge__val',
    fill: 'var(--text)',
    'font-size': size > 110 ? '21' : '16',
    'font-weight': '800',
    'font-family': 'var(--font)',
    text: String(value)
  });

  // 单位微标
  const unitText = el('svg:text', {
    x: size / 2,
    y: (size / 2) + (size > 110 ? 19 : 15),
    'text-anchor': 'middle',
    class: 'svg-gauge__unit',
    fill: 'var(--text-muted)',
    'font-size': '10.5',
    'font-weight': '600',
    text: unit
  });

  // 底部标签文本
  const labelEl = el('div', { class: 'svg-gauge__label', text: label });

  svg.append(bgCircle, progressCircle, valText, unitText);
  const container = el('div', { class: 'svg-gauge-box' }, svg, labelEl);

  function applyColorTheme(curTone) {
    const stops = colorMap[curTone] || colorMap.primary;
    stop1.setAttribute('stop-color', stops[0]);
    stop2.setAttribute('stop-color', stops[1]);
    stop3.setAttribute('stop-color', stops[2]);
  }

  applyColorTheme(color === 'auto' ? 'primary' : color);

  function setProgress(curVal, curMax = max) {
    const safeMax = Number(curMax) || 100;
    const v = Math.max(0, Math.min(Number(curVal) || 0, safeMax));
    const ratio = v / safeMax;
    const offset = circumference * (1 - ratio);
    progressCircle.setAttribute('stroke-dashoffset', offset.toFixed(2));

    // 自动变色模式 (例如 CPU / 温度达到阈值时由绿转橙转红)
    if (color === 'auto') {
      const pct = ratio * 100;
      if (pct >= 85) applyColorTheme('danger');
      else if (pct >= 65) applyColorTheme('warning');
      else applyColorTheme('ok');
    }

    valText.textContent = typeof curVal === 'number' && !Number.isInteger(curVal)
      ? curVal.toFixed(1)
      : String(curVal != null ? curVal : '—');
  }

  setProgress(value, max);

  return {
    el: container,
    update(nextVal, nextMax = max, extraText) {
      if (extraText) unitText.textContent = extraText;
      setProgress(nextVal, nextMax);
    }
  };
}

/**
 * 创建 5G-MIFI 实用科技射频雷达与拓扑视效 HUD (Hero RF Radar Visualizer)
 * 实时绘制基站信号波束、双向链路粒子穿梭、RSRP 信号量化阶梯与射频指标
 */
export function createHeroRadar() {
  const W = 680;
  const H = 175;
  const svg = el('svg:svg', {
    class: 'hero-radar-svg',
    viewBox: `0 0 ${W} ${H}`,
    preserveAspectRatio: 'xMidYMid meet',
    role: 'img',
    'aria-label': '5G 实时射频拓扑与基站射频分析'
  });

  const defs = el('svg:defs');

  // 发光滤镜
  const glow = el('svg:filter', { id: 'radarGlow', x: '-20%', y: '-20%', width: '140%', height: '140%' },
    el('svg:feGaussianBlur', { stdDeviation: '3', result: 'b' }),
    el('svg:feMerge', {},
      el('svg:feMergeNode', { in: 'b' }),
      el('svg:feMergeNode', { in: 'SourceGraphic' })
    )
  );

  // 上下行流线渐变
  const dlGrad = el('svg:linearGradient', { id: 'dlGrad', x1: '100%', y1: '0%', x2: '0%', y2: '0%' },
    el('svg:stop', { offset: '0%', 'stop-color': '#0891b2', 'stop-opacity': '0.1' }),
    el('svg:stop', { offset: '50%', 'stop-color': '#06b6d4', 'stop-opacity': '0.9' }),
    el('svg:stop', { offset: '100%', 'stop-color': '#3b82f6', 'stop-opacity': '0.2' })
  );

  const ulGrad = el('svg:linearGradient', { id: 'ulGrad', x1: '0%', y1: '0%', x2: '100%', y2: '0%' },
    el('svg:stop', { offset: '0%', 'stop-color': '#3b82f6', 'stop-opacity': '0.2' }),
    el('svg:stop', { offset: '50%', 'stop-color': '#10b981', 'stop-opacity': '0.9' }),
    el('svg:stop', { offset: '100%', 'stop-color': '#059669', 'stop-opacity': '0.1' })
  );

  defs.append(glow, dlGrad, ulGrad);
  svg.appendChild(defs);

  // 背景雷达经纬网线与方位角
  const gridGroup = el('svg:g', { stroke: 'var(--border)', 'stroke-opacity': '0.22', 'stroke-width': '1' });
  for (let x = 60; x < W; x += 80) {
    gridGroup.appendChild(el('svg:line', { x1: x, y1: 8, x2: x, y2: H - 8, 'stroke-dasharray': '3 7' }));
  }
  for (let y = 30; y < H; y += 38) {
    gridGroup.appendChild(el('svg:line', { x1: 20, y1: y, x2: W - 20, y2: y, 'stroke-dasharray': '2 8' }));
  }
  svg.appendChild(gridGroup);

  // 左右数据穿梭路径
  const leftStreamPath = 'M 50 88 C 130 38, 200 138, 340 88';
  const rightStreamPath = 'M 340 88 C 480 38, 550 138, 630 88';

  const streamGroup = el('svg:g', { fill: 'none', 'stroke-linecap': 'round' });
  streamGroup.append(
    el('svg:path', { d: leftStreamPath, stroke: 'url(#ulGrad)', 'stroke-width': '2.2' }),
    el('svg:path', { d: rightStreamPath, stroke: 'url(#dlGrad)', 'stroke-width': '2.2' })
  );

  // 穿梭光子 1 (客户端 <-> 设备)
  const dotLeft = el('svg:circle', { r: '4', fill: '#10b981', filter: 'url(#radarGlow)' });
  dotLeft.appendChild(el('svg:animateMotion', {
    dur: '2.8s', repeatCount: 'indefinite', path: leftStreamPath
  }));

  // 穿梭光子 2 (设备 <-> 基站)
  const dotRight = el('svg:circle', { r: '4', fill: '#06b6d4', filter: 'url(#radarGlow)' });
  dotRight.appendChild(el('svg:animateMotion', {
    dur: '2.4s', repeatCount: 'indefinite', path: rightStreamPath
  }));

  streamGroup.append(dotLeft, dotRight);
  svg.appendChild(streamGroup);

  // 中心 5G 设备核心枢纽 (Hub)
  const hubGroup = el('svg:g', { transform: 'translate(340, 88)' });

  // 脉冲波纹辐射弧 1
  const pulseRing1 = el('svg:circle', { r: '18', fill: 'none', stroke: '#38bdf8', 'stroke-width': '1.8' },
    el('svg:animate', { attributeName: 'r', values: '16;64', dur: '2.6s', repeatCount: 'indefinite' }),
    el('svg:animate', { attributeName: 'opacity', values: '0.85;0', dur: '2.6s', repeatCount: 'indefinite' })
  );
  // 脉冲波纹辐射弧 2 (交替)
  const pulseRing2 = el('svg:circle', { r: '18', fill: 'none', stroke: '#2563eb', 'stroke-width': '1.6' },
    el('svg:animate', { attributeName: 'r', values: '16;64', dur: '2.6s', begin: '1.3s', repeatCount: 'indefinite' }),
    el('svg:animate', { attributeName: 'opacity', values: '0.85;0', dur: '2.6s', begin: '1.3s', repeatCount: 'indefinite' })
  );

  // 旋转科技刻度环
  const orbitRing = el('svg:circle', {
    r: '32', fill: 'none', stroke: 'var(--primary)',
    'stroke-width': '1.8', 'stroke-dasharray': '5 12', 'stroke-opacity': '0.7'
  },
    el('svg:animateTransform', {
      attributeName: 'transform', type: 'rotate', from: '0', to: '360', dur: '14s', repeatCount: 'indefinite'
    })
  );

  // 实体微型设备基座
  const coreBase = el('svg:circle', {
    r: '20', fill: 'var(--primary)', filter: 'url(#radarGlow)', stroke: '#ffffff', 'stroke-width': '2'
  });

  // 设备中心微标文字
  const coreText = el('svg:text', {
    x: '0', y: '4', 'text-anchor': 'middle', fill: '#ffffff',
    'font-size': '10', 'font-weight': '900', 'font-family': 'var(--mono)', text: '5G'
  });

  hubGroup.append(pulseRing1, pulseRing2, orbitRing, coreBase, coreText);
  svg.appendChild(hubGroup);

  // 左侧节点：用户客户端 (WLAN / RNDIS)
  const clientGroup = el('svg:g', { transform: 'translate(50, 88)' });
  clientGroup.append(
    el('svg:circle', { r: '14', fill: 'var(--surface)', stroke: 'var(--border)', 'stroke-width': '1.8' }),
    el('svg:circle', { r: '6', fill: 'var(--ok)' }),
    el('svg:text', {
      x: '0', y: '26', 'text-anchor': 'middle', fill: 'var(--text-muted)',
      'font-size': '10.5', 'font-weight': '600', text: '客户端'
    })
  );
  svg.appendChild(clientGroup);

  // 右侧节点：基站 / 运营商核心网 (Serving Cell)
  const towerGroup = el('svg:g', { transform: 'translate(630, 88)' });
  towerGroup.append(
    el('svg:circle', { r: '14', fill: 'var(--surface)', stroke: 'var(--border)', 'stroke-width': '1.8' }),
    el('svg:circle', { r: '6', fill: 'var(--primary)' }),
    el('svg:text', {
      x: '0', y: '26', 'text-anchor': 'middle', fill: 'var(--text-muted)',
      'font-size': '10.5', 'font-weight': '600', text: '服务基站'
    })
  );
  svg.appendChild(towerGroup);

  // 实时指标浮层徽章
  const carrierText = el('span', { class: 'hero-radar-badge', text: '移动网络 · 探测中' });
  const signalText = el('span', { class: 'hero-radar-stat', text: 'RSRP: — dBm' });
  const qualityText = el('span', { class: 'hero-radar-stat', text: 'SINR: — dB' });

  const overlayBar = el('div', { class: 'hero-radar-overlay' },
    el('div', { class: 'hero-radar-left' }, carrierText),
    el('div', { class: 'hero-radar-right' }, signalText, qualityText)
  );

  const wrapper = el('div', { class: 'hero-radar-wrap' }, svg, overlayBar);

  return {
    el: wrapper,
    update({ carrier, networkType, band, rsrp, sinr }) {
      const modeStr = (networkType || band || '5G NR').toUpperCase();
      const carrierStr = carrier || '蜂窝网络';
      carrierText.textContent = `${carrierStr} · ${modeStr}`;

      signalText.textContent = rsrp != null ? `RSRP: ${rsrp} dBm` : 'RSRP: —';
      qualityText.textContent = sinr != null ? `SINR: ${sinr} dB` : 'SINR: —';

      const numRsrp = Number(rsrp);
      if (Number.isFinite(numRsrp) && numRsrp !== 0) {
        if (numRsrp >= -85) coreBase.setAttribute('stroke', '#10b981');
        else if (numRsrp >= -105) coreBase.setAttribute('stroke', '#3b82f6');
        else coreBase.setAttribute('stroke', '#f59e0b');
      }
    }
  };
}

/**
 * 创建流畅贝塞尔平滑趋势折线图 (Smooth Spline Sparkline)
 */
export function createSmoothSparkline({
  width = 600,
  height = 140,
  maxPoints = 120,
  strokeColor = '#2563eb',
  fillColor = '#3b82f6'
} = {}) {
  const series = [];
  const svg = el('svg:svg', {
    class: 'sparkline-smooth',
    viewBox: `0 0 ${width} ${height}`,
    preserveAspectRatio: 'none',
    role: 'img',
    'aria-label': '平滑动态趋势折线图'
  });

  const defs = el('svg:defs');
  const gradId = `spark-grad-${Math.random().toString(36).slice(2, 8)}`;
  const grad = el('svg:linearGradient', { id: gradId, x1: '0%', y1: '0%', x2: '0%', y2: '100%' },
    el('svg:stop', { offset: '0%', 'stop-color': fillColor, 'stop-opacity': '0.32' }),
    el('svg:stop', { offset: '65%', 'stop-color': fillColor, 'stop-opacity': '0.08' }),
    el('svg:stop', { offset: '100%', 'stop-color': fillColor, 'stop-opacity': '0' })
  );

  const glowFilter = el('svg:filter', { id: `${gradId}-glow`, x: '-10%', y: '-20%', width: '120%', height: '140%' },
    el('svg:feGaussianBlur', { stdDeviation: '2', result: 'b' }),
    el('svg:feMerge', {},
      el('svg:feMergeNode', { in: 'b' }),
      el('svg:feMergeNode', { in: 'SourceGraphic' })
    )
  );

  defs.append(grad, glowFilter);
  svg.appendChild(defs);

  // 横向刻度线
  for (let i = 0; i <= 3; i += 1) {
    const y = i * (height / 3);
    svg.appendChild(el('svg:line', {
      x1: 0, y1: y, x2: width, y2: y,
      stroke: 'var(--border)', 'stroke-opacity': '0.28', 'stroke-width': '1'
    }));
  }

  // 曲线与区域填充
  const areaPath = el('svg:path', { fill: `url(#${gradId})`, d: '' });
  const linePath = el('svg:path', {
    fill: 'none', stroke: strokeColor, 'stroke-width': '2.2',
    'stroke-linecap': 'round', 'stroke-linejoin': 'round',
    filter: `url(#${gradId}-glow)`, d: ''
  });

  const pulseDot = el('svg:circle', {
    r: '4', fill: '#ffffff', stroke: strokeColor, 'stroke-width': '2',
    cx: width, cy: height, style: { display: 'none' }
  });

  const infoText = el('svg:text', {
    x: 8, y: 14, fill: 'var(--text-muted)',
    'font-size': '10.5', 'font-family': 'var(--mono)', text: ''
  });

  svg.append(areaPath, linePath, pulseDot, infoText);

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
      pulseDot.style.display = 'none';
      return;
    }
    const maxVal = Math.max(1, ...series);
    const step = width / (series.length - 1);
    const pts = series.map((v, i) => {
      const x = i * step;
      const ratio = Math.max(0, Math.min(1, v / maxVal));
      const y = height - ratio * (height - 14) - 7;
      return { x, y };
    });

    const d = buildSpline(pts);
    linePath.setAttribute('d', d);
    areaPath.setAttribute('d', `${d} L ${width} ${height} L 0 ${height} Z`);

    const lastPt = pts[pts.length - 1];
    pulseDot.setAttribute('cx', lastPt.x.toFixed(1));
    pulseDot.setAttribute('cy', lastPt.y.toFixed(1));
    pulseDot.style.display = 'block';

    const latest = series[series.length - 1];
    infoText.textContent = `实时: ${latest.toFixed(1)} · 峰值: ${maxVal.toFixed(1)} (${series.length}/${maxPoints})`;
  }

  return {
    el: svg,
    push(val) {
      series.push(Number(val) || 0);
      if (series.length > maxPoints) series.shift();
      redraw();
    },
    clear() {
      series.length = 0;
      redraw();
    }
  };
}

export default {
  createCircularGauge,
  createHeroRadar,
  createSmoothSparkline
};
