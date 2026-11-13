/**
 * Modem 页：只读展示设备 / 连接 / 频段 / 邻区信息。
 * 控制类操作（锁频段、锁小区）集中在 network.js，避免重复入口。
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

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const infoRef = {};
  const infoCard = ctx.card({ title: 'Modem 信息', subtitle: '来自 /api/info', span: 6, icon: 'cpu' });
  const infoLabels = ['设备型号', 'IMEI', 'ICCID', '运营商', '固件版本', '序列号', '温度', 'uptime'];
  for (const l of infoLabels) infoCard.body.appendChild(kv(l, infoRef));

  const bandRef = {};
  const bandCard = ctx.card({ title: '当前连接', subtitle: '/api/current_band', span: 6, icon: 'signal' });
  const bandLabels = ['网络制式', '频段', '频点 ARFCN', 'PCI', 'RSRP', 'RSRQ', 'SINR', 'QCI'];
  for (const l of bandLabels) bandCard.body.appendChild(kv(l, bandRef));
  const bandHint = el('p', { class: 'field__hint', text: '' });
  bandCard.body.appendChild(bandHint);

  const cellCard = ctx.card({ title: '小区 / 邻区', subtitle: '/api/cells', span: 12, icon: 'network' });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: load });
  cellCard.head.querySelector('.card__actions').appendChild(refreshBtn);
  cellTable = ctx.dataTable({
    columns: [
      { label: '类型', render: (r) => safeText(r.type || r.technology) },
      { label: '频段', render: (r) => safeText(r.band) },
      { label: 'ARFCN', render: (r) => safeText(r.arfcn) },
      { label: 'PCI', render: (r) => safeText(r.pci) },
      { label: 'RSRP', render: (r) => safeText(r.rsrp) },
      { label: '状态', render: (r) => ctx.statusBadge(safeText(r.status || (r.serving ? '服务小区' : '邻区')), r.serving ? 'ok' : 'muted') }
    ],
    rows: [],
    emptyText: '暂无小区信息'
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
      setRef(infoRef, '温度', `${safeText(fieldOf(i, 'temperature', '—'))} °C`);
      setRef(infoRef, 'uptime', formatDate(fieldOf(i, 'uptime', '—')));

      const b = fieldOf(band, 'data', band) || {};
      setRef(bandRef, '网络制式', fieldOf(b, 'network_type', fieldOf(b, 'mode')));
      setRef(bandRef, '频段', fieldOf(b, 'band', fieldOf(b, 'current_band')));
      setRef(bandRef, '频点 ARFCN', fieldOf(b, 'arfcn'));
      setRef(bandRef, 'PCI', fieldOf(b, 'pci'));
      setRef(bandRef, 'RSRP', fieldOf(b, 'rsrp'));
      setRef(bandRef, 'RSRQ', fieldOf(b, 'rsrq'));
      setRef(bandRef, 'SINR', fieldOf(b, 'sinr'));
      setRef(bandRef, 'QCI', fieldOf(b, 'qci'));
      bandHint.textContent = b.available === false
        ? (b.reason || '模组未就绪，无法读取当前连接参数')
        : '';

      // 小区表只接受真实小区数据；此前会用频段列表兜底，导致表格里出现
      // 类型/ARFCN/PCI 全空的行，属于错误展示。
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
