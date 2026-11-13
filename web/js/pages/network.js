/**
 * 网络管理（含高级网络与网络接口监控）：
 *  - 基础：数据连接 / 漫游 / 飞行模式 / 网络模式 / SIM 卡槽；
 *  - 高级：频段列表与锁定（危险操作走 confirm-dialog）、小区锁定 / 解锁；
 *  - 接口：/api/netif/list 列表、监听开关、实时统计查看。
 */
import { el, button, switchField, selectField, toolbar, loadingState } from '../utils/dom.js';
import { fieldOf, safeText, asArray, formatBytes } from '../utils/format.js';

let ctx = null;
let bandBox = null;
let cellTable = null;
let cellHint = null;
let netifTable = null;

function post(path, body) {
  return ctx.api(path, { method: 'POST', body, signal: ctx.signal });
}

async function toggleWithConfirm(message, path, body, control, checkedAfter) {
  const ok = await ctx.confirm({ message, danger: true });
  if (!ok) { control.checked = !checkedAfter; return; }
  try {
    await post(path, body);
    ctx.toast.success(ctx.t('common.saved'));
  } catch (e) {
    ctx.toast.error(ctx.t('common.requestFailed'));
    control.checked = !checkedAfter;
  }
}

// ---- 基础控制 ----
function buildBasic(grid) {
  const cardEl = ctx.card({ title: '基础网络控制', span: 6, icon: 'network' });

  const data = switchField({ label: '移动数据', checked: false });
  const roaming = switchField({ label: '数据漫游', checked: false });
  const airplane = switchField({ label: '飞行模式（关闭所有无线连接）', checked: false });

  data.input.addEventListener('change', async () => {
    try { await post('/api/data', { active: data.input.checked }); }
    catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); data.input.checked = !data.input.checked; }
  });
  roaming.input.addEventListener('change', async () => {
    try { await post('/api/roaming', { allowed: roaming.input.checked }); }
    catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); roaming.input.checked = !roaming.input.checked; }
  });
  airplane.input.addEventListener('change', () => {
    toggleWithConfirm('切换飞行模式会断开所有无线连接，确定继续？',
      '/api/airplane_mode', { enabled: airplane.input.checked }, airplane.input, airplane.input.checked);
  });

  const modeSel = selectField({
    label: '网络模式', name: 'netmode', options: [
      { value: 'auto', label: '5G/LTE Auto' },
      { value: 'nr5g', label: 'NR 5G Only' },
      { value: 'nsa', label: 'NSA Only' },
      { value: 'sa', label: 'SA Only' },
      { value: 'lte', label: 'LTE Only' }
    ]
  });
  const setMode = button(ctx.t('common.apply'), {
    variant: 'primary',
    onClick: async () => {
      try { await post('/api/set_network', { mode: modeSel.input.value }); ctx.toast.success(ctx.t('common.saved')); }
      catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  // 卡槽：本机以单卡模式运行（oFono NUM_SIM_SLOTS=1），固定卡槽 1，
  // 不提供切换入口，避免误切导致 Modem 掉线。
  const slotRow = el('div', { class: 'row' },
    el('span', { class: 'badge badge--muted', text: '卡槽 1' }),
    el('span', { class: 'muted', text: '本机为单卡配置，不支持切换卡槽' }));

  cardEl.body.append(
    data.wrap, roaming.wrap, airplane.wrap,
    el('div', { class: 'row' }, modeSel.wrap, setMode),
    el('p', { class: 'field__hint', text: 'SIM 卡槽' }), slotRow
  );
  grid.appendChild(cardEl.el);

  // 载入初始状态（四项全部回显，此前模式与飞行模式只有 POST 没有 GET）
  (async () => {
    try {
      const [d, r, nm, ap] = await Promise.all([
        ctx.api('/api/data', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/roaming', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/get/network_mode', { signal: ctx.signal }).catch(() => null),
        ctx.api('/api/airplane_mode', { signal: ctx.signal }).catch(() => null)
      ]);
      data.input.checked = !!(fieldOf(d, 'active', fieldOf(d, 'data', d)));
      roaming.input.checked = !!(fieldOf(r, 'allowed', fieldOf(r, 'data', r)));
      const curMode = fieldOf(nm, 'mode', '');
      if (curMode) modeSel.input.value = curMode;
      airplane.input.checked = !!fieldOf(ap, 'enabled', false);
    } catch (e) { /* 忽略 */ }
  })();
}

// ---- 高级网络：频段 ----
async function loadBands() {
  bandBox.replaceChildren(loadingState());
  try {
    const [bands, current] = await Promise.all([
      ctx.api('/api/bands', { signal: ctx.signal }),
      ctx.api('/api/current_band', { signal: ctx.signal }).catch(() => null)
    ]);
    bandBox.replaceChildren();
    const list = asArray(bands, ['bands']);
    const cur = fieldOf(current, 'data', current) || {};
    const checks = [];
    for (const b of list) {
      const value = (b && typeof b === 'object') ? (b.id != null ? b.id : b.band) : b;
      const label = (b && typeof b === 'object') ? safeText(b.name || b.band || value) : safeText(b);
      const sw = switchField({ label, checked: false });
      sw.input.dataset.band = String(value);
      checks.push(sw.input);
      bandBox.appendChild(sw.wrap);
    }
    if (!list.length) {
      bandBox.appendChild(el('p', { class: 'muted', text: fieldOf(bands, 'reason', '') || '暂无频段数据' }));
    } else if (fieldOf(bands, 'available', true) === false) {
      // 后端标注 available=false 时说明清单是内置候选而非设备上报，
      // 明确告知用户，避免误以为读到了模组真实能力。
      bandBox.insertBefore(
        el('p', { class: 'field__hint', text: fieldOf(bands, 'reason', '') || '以下为内置候选频段，设备未上报实际支持列表' }),
        bandBox.firstChild);
    }

    bandBox.appendChild(el('div', { class: 'toolbar', style: { marginTop: '12px' } },
      el('span', { class: 'muted', text: `当前频段：${safeText(fieldOf(cur, 'band', '—'))}` }),
      button('锁定所选频段', {
        variant: 'danger', icon: 'lock',
        onClick: async () => {
          const selected = checks.filter((c) => c.checked).map((c) => c.dataset.band);
          if (!selected.length) { ctx.toast.error('请至少选择一个频段'); return; }
          const ok = await ctx.confirm({
            title: '锁定频段', message: `确定锁定 ${selected.length} 个频段吗？错误的锁定可能导致无法注册网络。`,
            danger: true, requireText: 'LOCK'
          });
          if (!ok) return;
          try {
            await post('/api/lock_bands', { bands: selected });
            ctx.toast.success(ctx.t('status.locked'));
          } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
        }
      }),
      button('解锁全部频段', {
        variant: 'ghost',
        onClick: async () => {
          const ok = await ctx.confirm({ message: '解锁所有频段？', danger: true });
          if (!ok) return;
          try { await post('/api/unlock_bands', {}); ctx.toast.success(ctx.t('status.unlocked')); }
          catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
        }
      })
    ));
  } catch (e) {
    bandBox.replaceChildren(el('p', { class: 'text-danger', text: ctx.t('common.requestFailed') }));
  }
}

// ---- 高级网络：小区 ----
async function loadCells() {
  try {
    const res = await ctx.api('/api/cells', { signal: ctx.signal });
    const rows = asArray(res, ['cells', 'neighbors']);
    cellTable.setRows(rows);
    if (cellHint) {
      cellHint.textContent = rows.length ? '' : (fieldOf(res, 'reason', '') || '');
    }
  } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
}

function buildAdvanced(grid) {
  const bandCard = ctx.card({ title: '频段锁定', subtitle: '/api/bands', span: 6, icon: 'signal' });
  const refreshBands = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: loadBands });
  bandCard.head.querySelector('.card__actions').appendChild(refreshBands);
  bandBox = el('div', { style: { maxHeight: '320px', overflow: 'auto' } });
  bandCard.body.appendChild(bandBox);
  grid.appendChild(bandCard.el);

  const cellCard = ctx.card({ title: '小区锁定', subtitle: '/api/cells', span: 6, icon: 'network' });
  const unlockBtn = button('解锁小区', {
    variant: 'ghost sm',
    onClick: async () => {
      const ok = await ctx.confirm({ message: '解锁当前小区锁定？', danger: true });
      if (!ok) return;
      try { await post('/api/unlock_cell', {}); ctx.toast.success(ctx.t('status.unlocked')); }
      catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
    }
  });
  cellCard.head.querySelector('.card__actions').appendChild(unlockBtn);
  cellTable = ctx.dataTable({
    columns: [
      { label: '制式', render: (r) => safeText(r.technology || r.type) },
      { label: '频段', render: (r) => safeText(r.band) },
      { label: 'ARFCN', render: (r) => safeText(r.arfcn) },
      { label: 'PCI', render: (r) => safeText(r.pci) },
      {
        label: ctx.t('common.actions'), align: 'right', render: (r) => [button('锁定', {
          variant: 'danger sm',
          onClick: async () => {
            const ok = await ctx.confirm({ message: `锁定到 PCI ${safeText(r.pci)}？`, danger: true });
            if (!ok) return;
            try {
              await post('/api/lock_cell', {
                technology: r.technology || r.type || '',
                arfcn: String(r.arfcn || ''), pci: String(r.pci || '')
              });
              ctx.toast.success(ctx.t('status.locked'));
            } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
          }
        })]
      }
    ], rows: [], emptyText: '暂无小区数据'
  });
  cellCard.body.appendChild(cellTable.el);
  cellHint = el('p', { class: 'field__hint', text: '' });
  cellCard.body.insertBefore(cellHint, cellTable.el);
  grid.appendChild(cellCard.el);

  loadBands();
  loadCells();
}

// ---- 网络接口 ----
async function loadNetif() {
  try {
    const res = await ctx.api('/api/netif/list', { signal: ctx.signal });
    netifTable.setRows(asArray(res, ['interfaces', 'netif']));
  } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
}

function buildNetif(grid) {
  const cardEl = ctx.card({ title: '网络接口监控', subtitle: '/api/netif/list', span: 12, icon: 'activity' });
  const refreshBtn = button(ctx.t('common.refresh'), { variant: 'ghost sm', icon: 'refresh', onClick: loadNetif });
  cardEl.head.querySelector('.card__actions').appendChild(refreshBtn);
  netifTable = ctx.dataTable({
    columns: [
      { label: '接口', render: (r) => safeText(r.name || r.interface) },
      { label: 'MAC', render: (r) => safeText(r.mac) },
      { label: 'IPv4', render: (r) => safeText(r.ipv4 || r.ip) },
      { label: '状态', render: (r) => ctx.statusBadge(r.up ? ctx.t('status.up') : ctx.t('status.down'), r.up ? 'ok' : 'muted') },      {
        label: ctx.t('common.actions'), align: 'right', render: (r) => {
          const name = r.name || r.interface;
          // 后端字段是 monitored，此前读 monitor/monitoring 导致开关恒为关闭。
          const monitor = switchField({ label: '监听', checked: !!(r.monitored || r.monitor || r.monitoring) });
          monitor.input.addEventListener('change', async () => {
            try {
              await post('/api/netif/monitor', { interface: name, enabled: monitor.input.checked });
              ctx.toast.success(ctx.t('common.saved'));
            } catch (e) {
              ctx.toast.error(ctx.t('common.requestFailed'));
              monitor.input.checked = !monitor.input.checked;
            }
          });
          const stats = button('统计', {
            variant: 'ghost sm',
            onClick: async () => {
              try {
                const res = await post('/api/netif/stats', { interface: name });
                const s = fieldOf(res, 'data', res) || {};
                ctx.modal({
                  variant: 'side-sheet',
                  title: `接口统计 · ${safeText(name)}`,
                  content: (b) => b.append(
                    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '接收' }), el('span', { class: 'kv__v', text: formatBytes(fieldOf(s, 'rx_bytes', fieldOf(s, 'rx', 0))) })),
                    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '发送' }), el('span', { class: 'kv__v', text: formatBytes(fieldOf(s, 'tx_bytes', fieldOf(s, 'tx', 0))) }))
                  ),
                  footer: [{ label: ctx.t('common.close'), variant: 'ghost', closeOnClick: true }]
                });
              } catch (e) { ctx.toast.error(ctx.t('common.requestFailed')); }
            }
          });
          return [monitor.wrap, stats];
        }
      }
    ], rows: [], emptyText: '未检测到网络接口'
  });
  cardEl.body.appendChild(netifTable.el);
  grid.appendChild(cardEl.el);
  loadNetif();
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);
  buildBasic(grid);
  buildAdvanced(grid);
  buildNetif(grid);
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  bandBox = null;
  cellTable = null;
  netifTable = null;
}
