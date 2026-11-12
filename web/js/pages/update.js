/**
 * 系统更新：版本 / 检查更新 / 上传固件（FormData）/ 远程下载 / 解压 / 安装。
 * 安装属于高风险操作：走 confirm-dialog 确认；上传使用 FormData，由 api.js 自动处理边界。
 */
import { el, button, field, toolbar } from '../utils/dom.js';
import { fieldOf, safeText } from '../utils/format.js';

let ctx = null;
let logEl = null;

function log(text) {
  if (!logEl) return;
  logEl.textContent += `${text}\n`;
  logEl.scrollTop = logEl.scrollHeight;
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const versionCard = ctx.card({ title: '当前版本', span: 5, icon: 'download' });
  const versionLabel = el('span', { class: 'badge badge--primary', text: '读取中…' });
  const checkResult = el('p', { class: 'muted', text: '' });
  const checkBtn = button('检查更新', {
    variant: 'outline', icon: 'refresh',
    onClick: async () => {
      log('检查最新版本…');
      try {
        const res = await ctx.api('/api/update/check', { signal: ctx.signal });
        const o = fieldOf(res, 'data', res) || {};
        const latest = fieldOf(o, 'version', fieldOf(o, 'latest', ''));
        checkResult.textContent = latest ? `发现新版本：${safeText(latest)}` : '当前已是最新版本';
        log(checkResult.textContent);
      } catch (e) { log(`检查失败：${(e && e.message) || e}`); }
    }
  });
  versionCard.body.append(el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '版本' }), versionLabel),
    checkResult, checkBtn);
  grid.appendChild(versionCard.el);

  const pkgCard = ctx.card({ title: '更新包', span: 7, icon: 'cloud-upload' });
  const urlInput = field({ label: '远程更新包 URL', name: 'url', placeholder: 'https://...' });
  const fileInput = el('input', { type: 'file', accept: '.zip', class: 'input' });
  const urlBtn = button('下载并安装', {
    variant: 'primary', icon: 'cloud-download',
    onClick: async () => {
      const url = urlInput.input.value.trim();
      if (!url) { ctx.toast.error('请输入更新包 URL'); return; }
      try {
        log('下载更新包…');
        await ctx.api('/api/update/download', { method: 'POST', body: { url }, signal: ctx.signal });
        await extractAndInstall();
      } catch (e) { log(`下载失败：${(e && e.message) || e}`); }
    }
  });
  const uploadBtn = button('上传并安装', {
    variant: 'outline', icon: 'upload',
    onClick: async () => {
      const file = fileInput.files && fileInput.files[0];
      if (!file) { ctx.toast.error('请选择 .zip 更新包'); return; }
      const form = new FormData();
      form.append('file', file);
      try {
        log(`上传 ${file.name}…`);
        await ctx.api('/api/update/upload', { method: 'POST', body: form, signal: ctx.signal });
        await extractAndInstall();
      } catch (e) { log(`上传失败：${(e && e.message) || e}`); }
    }
  });
  pkgCard.body.append(
    el('div', { class: 'alert alert--danger', text: '更新过程中请勿断电或关闭页面；失败将按服务端策略回滚。' }),
    urlInput.wrap,
    el('div', { class: 'field' }, el('label', { class: 'field__label', text: '本地 ZIP 更新包' }), fileInput),
    el('div', { class: 'row' }, urlBtn, uploadBtn));
  grid.appendChild(pkgCard.el);

  const logCard = ctx.card({ title: '更新日志', span: 12, icon: 'list' });
  logEl = el('pre', {
    class: 'mono',
    style: { background: '#0b1220', color: '#cde3d0', padding: '12px', borderRadius: '8px', minHeight: '160px', maxHeight: '280px', overflow: 'auto', whiteSpace: 'pre-wrap' },
    text: ''
  });
  const clearLog = button(ctx.t('common.clear'), { variant: 'ghost sm', icon: 'x', onClick: () => { if (logEl) logEl.textContent = ''; } });
  logCard.head.querySelector('.card__actions').appendChild(clearLog);
  logCard.body.appendChild(logEl);
  grid.appendChild(logCard.el);

  async function extractAndInstall() {
    try {
      log('解压更新包…');
      await ctx.api('/api/update/extract', { method: 'POST', signal: ctx.signal });
      const ok = await ctx.confirm({
        title: '安装更新', message: '解压完成，确定开始安装吗？更新过程中设备可能重启。', danger: true
      });
      if (!ok) { log('已取消安装'); return; }
      log('执行安装脚本…');
      const res = await ctx.api('/api/update/install', { method: 'POST', signal: ctx.signal, timeout: 60000 });
      log(`安装完成：${safeText(fieldOf(res, 'message', '成功'), '')}`);
      ctx.toast.success('系统更新成功');
    } catch (e) {
      log(`更新失败：${(e && e.message) || e}`);
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  (async () => {
    try {
      const res = await ctx.api('/api/update/version', { signal: ctx.signal });
      versionLabel.textContent = safeText(fieldOf(res, 'version', fieldOf(res, 'data', res)), '未知');
    } catch (e) { versionLabel.textContent = '未知'; }
  })();
}

export function destroy() {
  if (ctx && ctx.scope) ctx.scope.destroy();
  ctx = null;
  logEl = null;
}
