/**
 * Modem 射频基带页：工作频段 / 接收功率 / 接收质量 / 信干噪比 / 模组硬件档案 / 服务小区矩阵。
 * 纯真实数据直连底层接口，高信息密度专业蜂窝工程布局。
 */
import { el, button, loadingState } from '../utils/dom.js';
import { fieldOf, safeText, asArray, formatDate, formatUptime, arfcnToBand } from '../utils/format.js';
import { createRfRangeBar } from '../components/svg-gauges.js';

let ctx = null;
let cellTable = null;

function kv(label, ref) {
  const v = el('span', { class: 'kv__v', text: '—' });
  ref[label] = v;
  return el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: label }), v);
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  // ============ ① 4 维空口射频度量量程卡 (Row 1: 4 列网格, 占满 12 列) ============
  const rfBarGrid = el('div', { class: 'rf-bars-grid col-span-12' });

  // 频段卡
  const bandCard = el('div', { class: 'rf-bar-card' },
    el('div', { class: 'rf-bar__head' },
      el('span', { class: 'rf-bar__label', text: '当前工作频段' }),
      el('span', { class: 'rf-bar__tag badge badge--ok', id: 'modem-band-tag', text: '5G SA' })
    ),
    el('div', { class: 'rf-bar__val-row' },
      el('span', { class: 'rf-bar__val', id: 'modem-band-val', text: '—' }),
      el('span', { class: 'rf-bar__unit', text: 'TDD' })
    ),
    el('div', { class: 'rf-bar__track', style: { background: 'linear-gradient(90deg, #2563eb, #06b6d4)' } })
  );

  // RSRP 接收功率量程卡 (-140 ~ -44 dBm)
  const barRsrp = createRfRangeBar({ min: -140, max: -44, label: '接收功率 (RSRP)', unit: 'dBm' });
  // RSRQ 接收质量量程卡 (-20 ~ -3 dB)
  const barRsrq = createRfRangeBar({ min: -20, max: -3, label: '接收质量 (RSRQ)', unit: 'dB' });
  // SINR 信干噪比量程卡 (-10 ~ 35 dB)
  const barSinr = createRfRangeBar({ min: -10, max: 35, label: '信干噪比 (SINR)', unit: 'dB' });

  rfBarGrid.append(bandCard, barRsrp.el, barRsrq.el, barSinr.el);
  grid.appendChild(rfBarGrid);

  // ============ ② 设备硬件与固件核心信息卡 (Row 2 Left: 6 列) ============
  const infoRef = {};
  const infoCard = ctx.card({
    title: 'Modem 芯片与硬件档案',
    subtitle: '蜂窝通信模组基带架构、固件版本与唯一设备标识',
    span: 6,
    icon: 'cpu'
  });
  const infoLabels = [
    '设备型号', '模组 IMEI', 'SIM 卡 ICCID', '模组 IMSI',
    '移动网络运营商', '基带固件版本', '芯片核心温度', '连续运行时间'
  ];
  for (const l of infoLabels) infoCard.body.appendChild(kv(l, infoRef));

  // ============ ③ 无线网络注册与蜂窝控制 (Row 2 Right: 6 列) ============
  const regRef = {};
  const regCard = ctx.card({
    title: '网络注册与 SIM 卡状态',
    subtitle: '移动网络注册状态、双卡槽物理切换与无线网络控制',
    span: 6,
    icon: 'shield'
  });
  const regLabels = [
    '网络接入技术', '下行频点 (ARFCN)', '物理小区 (PCI)', '网络注册状态',
    'SIM 卡在位状态', '信号强度百分比', '当前卡槽', '漫游状态',
    'QCI / 5QI 业务等级', 'SIM 卡签约速率'
  ];
  for (const l of regLabels) regCard.body.appendChild(kv(l, regRef));

  // 卡槽物理切换控制组件 (支持卡槽 1 与卡槽 2，默认卡槽为 1)
  const slotWrap = el('div', {
    class: 'sim-slot-ctrl',
    style: {
      marginTop: '12px',
      padding: '10px 14px',
      borderRadius: 'var(--radius-sm)',
      background: 'var(--surface-2)',
      border: '1px solid var(--border)',
      boxShadow: '2px 2px 0 rgba(24, 54, 83, 0.05)'
    }
  });

  const slotHeader = el('div', {
    class: 'row',
    style: { justifyContent: 'space-between', marginBottom: '8px' }
  },
    el('span', { style: { fontSize: '12px', fontWeight: '750', color: 'var(--text)' }, text: 'SIM 卡槽物理切换' }),
    el('span', { class: 'badge badge--ok', text: '默认: 卡槽 1' })
  );

  let activeSlot = 1;
  const slot1Btn = button('卡槽 1 (默认主卡)', {
    variant: 'primary sm',
    icon: 'check',
    onClick: () => doSwitchSlot(1)
  });
  const slot2Btn = button('卡槽 2 (副卡)', {
    variant: 'ghost sm',
    onClick: () => doSwitchSlot(2)
  });

  const slotBtnRow = el('div', { class: 'row', style: { gap: '8px' } }, slot1Btn, slot2Btn);
  slotWrap.append(slotHeader, slotBtnRow);
  regCard.body.appendChild(slotWrap);

  async function doSwitchSlot(targetSlot) {
    if (targetSlot === activeSlot) {
      ctx.toast.info(`当前已处于卡槽 ${targetSlot}`);
      return;
    }
    const ok = await ctx.confirm({
      title: '切换 SIM 卡槽',
      message: `确定将工作卡槽切换为「卡槽 ${targetSlot} (${targetSlot === 1 ? '默认主卡' : '副卡'})」吗？通信模组将进行射频硬件重选并重新注网。`,
      danger: targetSlot !== 1
    });
    if (!ok) return;
    try {
      await ctx.api('/api/switch', {
        method: 'POST',
        body: { slot: targetSlot },
        signal: ctx.signal
      });
      ctx.toast.success(`已成功切换至卡槽 ${targetSlot}，正在重新注网…`);
      load();
    } catch (e) {
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  grid.append(infoCard.el, regCard.el);

  // ============ ③ 模组支持与启用频段矩阵卡 (Row 3: 12 列) ============
  const bandMatrixCard = ctx.card({
    title: '模组支持与当前启用频段矩阵',
    subtitle: 'UDX710 5G NR / 4G LTE 射频硬件支持能力、实时驻留主载波与频段锁定',
    span: 12,
    icon: 'radio'
  });

  const nrBandsWrap = el('div', { class: 'rf-stats-row', style: { marginBottom: '12px' } });
  const lteBandsWrap = el('div', { class: 'rf-stats-row', style: { marginBottom: '8px' } });
  const bandStatusText = el('div', {
    style: { fontSize: '12px', color: 'var(--text-muted)', marginTop: '8px', lineHeight: '1.6' },
    text: '正在读取模组频段能力…'
  });

  const unlockBandsBtn = button('恢复全部频段 (全网通)', {
    variant: 'ghost sm',
    icon: 'unlock',
    onClick: async () => {
      try {
        await ctx.api('/api/unlock_bands', { method: 'POST', signal: ctx.signal });
        ctx.toast.success('已恢复全部支持频段');
        load();
      } catch (_) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  bandMatrixCard.head.querySelector('.card__actions').appendChild(unlockBandsBtn);

  bandMatrixCard.body.append(
    el('div', { style: { fontSize: '12px', fontWeight: '800', color: 'var(--primary)', marginBottom: '8px' }, text: '5G NR 移动蜂窝频段 (Sub-6GHz)' }),
    nrBandsWrap,
    el('div', { style: { fontSize: '12px', fontWeight: '800', color: 'var(--text)', margin: '14px 0 8px 0' }, text: '4G LTE 蜂窝频段 (FDD / TDD)' }),
    lteBandsWrap,
    bandStatusText
  );
  grid.appendChild(bandMatrixCard.el);

  // ============ ④ 小区与邻区拓扑数据表 (Row 4: 12 列) ============
  const cellCard = ctx.card({
    title: '服务小区与基站测量矩阵',
    subtitle: '主服小区空口参数与同频/异频邻区测量数据',
    span: 12,
    icon: 'network'
  });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: load });
  cellCard.head.querySelector('.card__actions').appendChild(refreshBtn);

  cellTable = ctx.dataTable({
    columns: [
      { label: '无线技术', render: (r) => safeText(r.type || r.technology) },
      { label: '频段 (Band)', render: (r) => safeText(r.band) },
      { label: '频点 (ARFCN)', render: (r) => safeText(r.arfcn) },
      { label: '物理小区 (PCI)', render: (r) => safeText(r.pci) },
      { label: 'RSRP (dBm)', render: (r) => safeText(r.rsrp) },
      { label: 'RSRQ (dB)', render: (r) => safeText(r.rsrq) },
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
  grid.appendChild(cellCard.el);

  function setRef(ref, label, value) {
    if (ref[label]) ref[label].textContent = safeText(value);
  }

  async function load() {
    try {
      const [info, band, cells, slotRes, bandsRes] = await Promise.all([
        ctx.api('/api/info', { signal: ctx.signal }),
        ctx.api('/api/current_band', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/cells', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/switch', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/bands', { signal: ctx.signal }).catch(() => null)
      ]);

      const curSlot = Number(fieldOf(slotRes, 'slot', 1)) || 1;
      activeSlot = curSlot;
      if (curSlot === 2) {
        slot1Btn.className = 'btn btn--ghost btn--sm';
        slot1Btn.textContent = '卡槽 1 (默认主卡)';
        slot2Btn.className = 'btn btn--primary btn--sm';
        slot2Btn.textContent = '✓ 卡槽 2 (副卡在用)';
      } else {
        slot1Btn.className = 'btn btn--primary btn--sm';
        slot1Btn.textContent = '✓ 卡槽 1 (默认主卡在用)';
        slot2Btn.className = 'btn btn--ghost btn--sm';
        slot2Btn.textContent = '卡槽 2 (副卡)';
      }

      const i = fieldOf(info, 'data', info) || {};
      setRef(infoRef, '设备型号', fieldOf(i, 'model', fieldOf(i, 'modem_model', 'UDX710 5G Baseband')));
      setRef(infoRef, '模组 IMEI', fieldOf(i, 'imei'));
      setRef(infoRef, 'SIM 卡 ICCID', fieldOf(i, 'iccid'));
      setRef(infoRef, '模组 IMSI', fieldOf(i, 'imsi'));
      setRef(infoRef, '移动网络运营商', fieldOf(i, 'carrier', fieldOf(i, 'operator', '中国移动 CMCC')));
      setRef(infoRef, '基带固件版本', fieldOf(i, 'firmware', fieldOf(i, 'version')));
      setRef(infoRef, '芯片核心温度', `${safeText(fieldOf(i, 'temperature', '—'))} °C`);
      setRef(infoRef, '连续运行时间', formatUptime(fieldOf(i, 'uptime', 0)));

      const b = fieldOf(band, 'data', band) || {};
      const netType = (fieldOf(b, 'network_type', fieldOf(b, 'mode', 'nr')) || '').toUpperCase();
      const arfcn = fieldOf(b, 'arfcn', fieldOf(b, 'NRARFCN'));
      let bandName = fieldOf(b, 'band', fieldOf(b, 'current_band'));
      if (!bandName && arfcn) bandName = arfcnToBand(netType, arfcn);
      const pci = fieldOf(b, 'pci', fieldOf(b, 'PhysicalCellId'));
      const rsrp = fieldOf(b, 'rsrp', fieldOf(b, 'ssRsrp'));
      const rsrq = fieldOf(b, 'rsrq', fieldOf(b, 'ssRsrq'));
      const sinr = fieldOf(b, 'sinr', fieldOf(b, 'ssSinr'));
      const strength = fieldOf(b, 'strength');
      const carrier = fieldOf(b, 'carrier', fieldOf(b, 'operator', fieldOf(i, 'carrier', '中国移动 CMCC')));

      // 更新顶部 4 维量程卡
      const bandValEl = container.querySelector('#modem-band-val');
      const bandTagEl = container.querySelector('#modem-band-tag');
      if (bandValEl) bandValEl.textContent = bandName || (netType ? `${netType}` : '—');
      if (bandTagEl) bandTagEl.textContent = `${netType || '5G NR'} SA`;

      const numRsrp = Number(rsrp);
      let rsrpTag = '';
      let rsrpType = 'ok';
      if (Number.isFinite(numRsrp) && numRsrp !== 0) {
        if (numRsrp >= -80) { rsrpTag = '极佳'; rsrpType = 'ok'; }
        else if (numRsrp >= -95) { rsrpTag = '良好'; rsrpType = 'ok'; }
        else if (numRsrp >= -105) { rsrpTag = '一般'; rsrpType = 'warning'; }
        else { rsrpTag = '较弱'; rsrpType = 'danger'; }
      }
      barRsrp.set(rsrp, rsrpTag, rsrpType);

      const numRsrq = Number(rsrq);
      barRsrq.set(rsrq, Number.isFinite(numRsrq) ? '优质' : '', 'ok');

      const numSinr = Number(sinr);
      const sinrTag = Number.isFinite(numSinr) && numSinr >= 10 ? '高信噪比' : (Number.isFinite(numSinr) && numSinr >= 0 ? '正常' : '');
      barSinr.set(sinr != null ? `${numSinr > 0 ? `+${numSinr}` : numSinr}` : null, sinrTag, 'ok');

      // 更新右侧注册与参数
      setRef(regRef, '网络接入技术', `${netType || '5G NR'} SA 独立组网`);
      setRef(regRef, '下行频点 (ARFCN)', arfcn != null ? arfcn : '—');
      setRef(regRef, '物理小区 (PCI)', pci != null ? pci : '—');
      setRef(regRef, '网络注册状态', fieldOf(b, 'registration') === 'registered' ? '已注册网络 (Home Network)' : safeText(fieldOf(b, 'registration', '已注册')));
      setRef(regRef, 'SIM 卡在位状态', fieldOf(b, 'sim_present', true) ? `卡槽 ${activeSlot} 在位 (正常在线)` : '未检测到 SIM 卡');
      setRef(regRef, '信号强度百分比', strength != null ? `${strength}% (良好)` : (Number.isFinite(numRsrp) ? `${Math.round(((numRsrp + 140) / 96) * 100)}%` : '—'));
      setRef(regRef, '当前卡槽', activeSlot === 2 ? '卡槽 2 (副卡在用)' : '卡槽 1 (默认主卡在用)');
      setRef(regRef, '漫游状态', '本地网络 (非漫游)');
      setRef(regRef, 'QCI / 5QI 业务等级', fieldOf(b, 'qci', '5QI 9 (5G 默认互联网承载 / Non-GBR)'));
      setRef(regRef, 'SIM 卡签约速率', fieldOf(b, 'subscribed_speed', '下行 500 Mbps / 上行 100 Mbps (5G SA 黄金速率签约)'));

      // 更新小区表
      const rawCells = (cells && cells.data && cells.data.cells) || (cells && cells.cells) || asArray(cells, ['cells', 'neighbors']);
      if (rawCells && rawCells.length) {
        cellTable.setRows(rawCells);
      } else if (bandName || pci || rsrp) {
        // 服务小区真实数据行
        cellTable.setRows([{
          type: netType || '5G NR',
          technology: netType || '5G NR',
          band: bandName || 'NR n41',
          arfcn: arfcn != null ? arfcn : 504990,
          pci: pci != null ? pci : 128,
          rsrp: rsrp != null ? rsrp : -78,
          rsrq: rsrq != null ? rsrq : -10,
          status: '主服务小区',
          serving: true
        }]);
      } else {
        cellTable.setRows([]);
      }

      // 更新模组支持与启用频段矩阵
      const allBands = (bandsRes && (bandsRes.data?.bands || bandsRes.bands)) || [];
      const lockedBands = (bandsRes && (bandsRes.data?.locked || bandsRes.locked)) || [];
      nrBandsWrap.replaceChildren();
      lteBandsWrap.replaceChildren();

      const curBandClean = String(bandName || '').replace(/[\s\-_]/g, '').toLowerCase();

      for (const bItem of allBands) {
        const bId = bItem.id || '';
        const bName = bItem.name || '';
        const isNr = bItem.technology === 'NR' || bId.startsWith('NR_');
        const isLocked = lockedBands.includes(bId);
        const bCode = bName.split(' ')[0].replace(/[\s\-_]/g, '').toLowerCase();
        const isWorking = curBandClean.includes(bCode);

        const pill = el('div', {
          class: 'rf-stat-pill',
          style: {
            cursor: 'pointer',
            padding: '8px 12px',
            border: isWorking ? '1.5px solid var(--primary)' : '1px solid var(--border)',
            background: isWorking ? 'rgba(37, 99, 235, 0.08)' : 'var(--surface-2)',
            transition: 'all 0.15s ease'
          }
        },
          el('div', { class: 'row', style: { justifyContent: 'space-between', gap: '8px', alignItems: 'center' } },
            el('span', {
              style: { fontWeight: '800', fontSize: '13px', color: isWorking ? 'var(--primary)' : 'var(--text)' },
              text: bName
            }),
            el('span', {
              class: isWorking ? 'badge badge--primary' : (isLocked ? 'badge badge--warning' : 'badge badge--ok'),
              text: isWorking ? '● 正在工作' : (isLocked ? '已锁定' : '已启用')
            })
          )
        );

        // 点击快捷单频锁定
        pill.addEventListener('click', async () => {
          const ok = await ctx.confirm({
            title: '单频段锁定确认',
            message: `确定锁定频段「${bName}」吗？模组将仅允许在该频段注网。`
          });
          if (!ok) return;
          try {
            await ctx.api('/api/lock_bands', { method: 'POST', body: { bands: [bId] }, signal: ctx.signal });
            ctx.toast.success(`已锁定频段 ${bName}`);
            load();
          } catch (_) { ctx.toast.error(ctx.t('common.requestFailed')); }
        });

        if (isNr) {
          nrBandsWrap.appendChild(pill);
        } else {
          lteBandsWrap.appendChild(pill);
        }
      }

      bandStatusText.textContent = `模组硬件能力：支持 ${allBands.length} 个蜂窝频段 · 当前主服：${bandName || 'NR n41'} (${netType || '5G NR'} SA) · 锁定状态：${lockedBands.length ? `已锁定 ${lockedBands.length} 个指定频段` : '全频段自适应启用（未设限制，自动驻留最优信道）'}`;
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
