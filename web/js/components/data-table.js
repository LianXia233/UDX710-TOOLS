/**
 * 数据表格：分页渲染，避免一次生成数千行 DOM。
 * 列支持 { label, field, align, render(row) }；render 返回 Node / Node[] / 文本。
 * 单元格内容一律 textContent / appendChild，不注入 HTML。
 * 返回 { el, setRows, refresh, getRows, destroy }。
 */
import { el } from '../utils/dom.js';
import { safeText } from '../utils/format.js';
import { t } from '../i18n/index.js';

export function createDataTable({
  columns = [],
  rows = [],
  pageSize = 10,
  pageSizeOptions = [10, 25, 50, 100],
  emptyText = null,
  onRowClick = null
} = {}) {
  let allRows = Array.isArray(rows) ? rows.slice() : [];
  let page = 1;
  let size = pageSize;
  const emptyLabel = emptyText || t('common.empty');

  const root = el('div', { class: 'datatable' });
  const wrap = el('div', { class: 'table-wrap' });
  const headRow = el('tr');
  for (const col of columns) {
    headRow.appendChild(el('th', {
      class: col.align ? `align-${col.align}` : '',
      text: safeText(col.label || '', '')
    }));
  }
  const thead = el('thead', {}, headRow);
  const tbody = el('tbody');
  const table = el('table', { class: 'table' }, thead, tbody);
  wrap.appendChild(table);
  const pager = el('div', { class: 'pager' });
  root.appendChild(wrap);
  root.appendChild(pager);

  function totalPages() {
    return Math.max(1, Math.ceil(allRows.length / size));
  }

  function cellContent(td, value) {
    if (value instanceof Node) {
      td.appendChild(value);
    } else if (Array.isArray(value)) {
      for (const item of value) {
        if (item instanceof Node) td.appendChild(item);
        else td.appendChild(document.createTextNode(safeText(item, '')));
      }
    } else {
      td.textContent = safeText(value, '');
    }
  }

  function renderPager(pages) {
    const info = el('span', {
      class: 'pager__info',
      text: `${t('common.total')} ${allRows.length} ${t('common.items')} · ${t('common.page')} ${page} ${t('common.of')} ${pages}`
    });
    const prev = el('button', {
      type: 'button', class: 'btn btn--ghost btn--sm', text: t('common.prev')
    });
    prev.disabled = page <= 1;
    prev.addEventListener('click', () => { if (page > 1) { page -= 1; render(); } });

    const next = el('button', {
      type: 'button', class: 'btn btn--ghost btn--sm', text: t('common.nextPage')
    });
    next.disabled = page >= pages;
    next.addEventListener('click', () => { if (page < pages) { page += 1; render(); } });

    const sizeLabel = el('span', { class: 'pager__label', text: t('common.perPage') });
    const sizeSelect = el('select', { class: 'input select pager__size' });
    for (const opt of pageSizeOptions) {
      sizeSelect.appendChild(el('option', { value: String(opt), text: String(opt) }));
    }
    sizeSelect.value = String(size);
    sizeSelect.addEventListener('change', () => {
      size = Number(sizeSelect.value) || size;
      page = 1;
      render();
    });

    pager.replaceChildren(
      info, el('span', { class: 'pager__spacer' }),
      sizeLabel, sizeSelect, prev, next
    );
  }

  function render() {
    const pages = totalPages();
    if (page > pages) page = pages;
    tbody.replaceChildren();
    const start = (page - 1) * size;
    const slice = allRows.slice(start, start + size);
    if (slice.length === 0) {
      tbody.appendChild(el('tr', {},
        el('td', { colspan: String(columns.length || 1), class: 'table-empty', text: emptyLabel })));
    } else {
      for (const rowData of slice) {
        const tr = el('tr', { class: onRowClick ? 'is-clickable' : '' });
        for (const col of columns) {
          const td = el('td', { class: col.align ? `align-${col.align}` : '' });
          const value = typeof col.render === 'function'
            ? col.render(rowData)
            : (col.field ? rowData[col.field] : '');
          cellContent(td, value);
          tr.appendChild(td);
        }
        if (onRowClick) tr.addEventListener('click', () => onRowClick(rowData));
        tbody.appendChild(tr);
      }
    }
    renderPager(pages);
  }

  render();
  return {
    el: root,
    setRows(newRows) {
      allRows = Array.isArray(newRows) ? newRows.slice() : [];
      page = 1;
      render();
    },
    refresh() { render(); },
    getRows() { return allRows.slice(); },
    destroy() { root.replaceChildren(); }
  };
}

export default createDataTable;
