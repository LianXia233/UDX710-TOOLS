/**
 * 系统更新：版本 / 检查更新 / 上传固件（FormData）/ 远程下载 / 解压 / 安装。
 * 安装属于高风险操作：走 confirm-dialog 确认；上传使用 FormData，由 api.js 自动处理边界。
 */
import { el, button, field, toolbar } from '../utils/dom.js';
import { fieldOf, safeText } from '../utils/format.js';

let ctx = null;
let logEl = null;
// check 阶段记录的 SHA256SUMS 地址，download 时透传给后端自动发现摘要。
let lastSha256Url = '';

function log(text) {
  if (!logEl) return;
  logEl.textContent += `${text}\n`;
  logEl.scrollTop = logEl.scrollHeight;
}

export function render(container, context) {
  ctx = context;
  const grid = el('div', { class: 'page-grid' });
  container.appendChild(grid);

  const versionCard = ctx.card({ title: '当前版本与官方仓库', span: 5, icon: 'download' });
  const versionLabel = el('span', { class: 'badge badge--primary', text: '读取中…' });
  const repoLink = el('a', {
    href: 'https://github.com/LianXia233/UDX710-TOOLS',
    target: '_blank',
    rel: 'noopener noreferrer',
    style: { fontSize: '12px', color: 'var(--primary)', textDecoration: 'none', fontWeight: '700' },
    text: 'LianXia233/UDX710-TOOLS ↗'
  });
  const checkResult = el('div', { class: 'muted', style: { margin: '8px 0', fontSize: '12px', lineHeight: '1.6' }, text: '接入 GitHub 官方仓库 Releases / Commits 在线自动构建源' });
  const checkBtn = button('检查 GitHub 最新版本', {
    variant: 'primary', icon: 'refresh',
    onClick: async () => {
      log('正在查询 GitHub 仓库最新版本与构建产物…');
      try {
        const res = await ctx.api('/api/update/check', { signal: ctx.signal });
        const o = fieldOf(res, 'data', res) || {};
        const cur = fieldOf(o, 'current_version', '—');
        const latest = fieldOf(o, 'latest_version', fieldOf(o, 'version', ''));
        const hasUpdate = Boolean(o.update_available);
        const notes = fieldOf(o, 'release_notes', '');
        const dlUrl = fieldOf(o, 'download_url', '');
        const shaUrl = fieldOf(o, 'sha256_url', '');
        lastSha256Url = typeof shaUrl === 'string' ? shaUrl : '';

        versionLabel.textContent = `v${cur}`;
        if (hasUpdate) {
          checkResult.innerHTML = `<strong>发现新版本：</strong><span class="badge badge--ok">${safeText(latest)}</span><br><span style="color:var(--text-muted);">${safeText(notes)}</span>`;
          if (dlUrl) {
            urlInput.input.value = dlUrl;
          }
        } else {
          checkResult.innerHTML = `当前已是最新版本：<span class="badge badge--ok">${safeText(latest || cur)}</span><br><span style="color:var(--text-muted);">已同步官方仓库 main 分支最新功能</span>`;
        }
        log(`检查完毕：当前版本 v${cur}，最新版本 ${latest || cur}。`);
      } catch (e) { log(`检查失败：${(e && e.message) || e}`); }
    }
  });
  versionCard.body.append(
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '当前运行版本' }), versionLabel),
    el('div', { class: 'kv' }, el('span', { class: 'kv__k', text: '官方更新仓库' }), repoLink),
    checkResult, checkBtn);
  grid.appendChild(versionCard.el);

  const pkgCard = ctx.card({ title: '更新包', span: 7, icon: 'cloud-upload' });
  const urlInput = field({ label: '远程更新包 URL', name: 'url', placeholder: 'https://... (CI Release 的 .tar.gz 或纯二进制)' });
  const fileInput = el('input', { type: 'file', accept: '.zip,.tar.gz,.bin', class: 'input' });
  const urlBtn = button('下载并安装', {
    variant: 'primary', icon: 'cloud-download',
    onClick: async () => {
      const url = urlInput.input.value.trim();
      if (!url) { ctx.toast.error('请输入更新包 URL'); return; }
      try {
        log('下载更新包…（摘要将自动从 Release 的 SHA256SUMS 获取）');
        const body = { url };
        if (lastSha256Url) body.sha256_url = lastSha256Url;
        await ctx.api('/api/update/download', { method: 'POST', body, signal: ctx.signal });
        await extractAndInstall();
      } catch (e) { log(`下载失败：${(e && e.message) || e}`); }
    }
  });
  const uploadBtn = button('上传并安装', {
    variant: 'outline', icon: 'upload',
    onClick: async () => {
      const file = fileInput.files && fileInput.files[0];
      if (!file) { ctx.toast.error('请选择 .tar.gz / .zip / .bin 更新包'); return; }
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

  const logCard = (ctx.collapsibleCard || ctx.card)({
    title: '更新执行日志控制台',
    subtitle: '固件传输、校验与刷写详细输出',
    span: 12,
    icon: 'list',
    open: false,
    badge: '查看日志'
  });
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
      const exRes = await ctx.api('/api/update/extract', { method: 'POST', signal: ctx.signal });
      const ex = fieldOf(exRes, 'data', exRes) || {};
      if (ex.extracted) {
        log(`已从归档解出 ${safeText(ex.extracted)}（${(ex.binary_size || 0)} 字节），未执行包内脚本。`);
      }
      const ok = await ctx.confirm({
        title: '安装更新', message: '解压完成，确定开始安装吗？安装后服务将自动重启，期间面板短暂不可用。', danger: true
      });
      if (!ok) { log('已取消安装'); return; }
      log('执行安装（原子替换 server，失败自动回滚）…');
      const res = await ctx.api('/api/update/install', { method: 'POST', signal: ctx.signal, timeout: 60000 });
      log(`安装完成：${safeText(fieldOf(res, 'message', '成功'), '')}`);
      log('服务即将自动重启，等待恢复…');
      ctx.toast.success('安装完成，等待服务重启');
      await waitServiceRecovery();
    } catch (e) {
      log(`更新失败：${(e && e.message) || e}`);
      ctx.toast.error(ctx.t('common.requestFailed'));
    }
  }

  // 安装后服务会退出并由看门狗拉起（约 15-40s）；轮询免认证的
  // /api/health 直到恢复，恢复后刷新版本徽标。超时 120s 则提示手动刷新。
  async function waitServiceRecovery() {
    const deadline = Date.now() + 120000;
    let up = false;
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 5000));
      try {
        await fetch('/api/health', { cache: 'no-store' });
        up = true;
        break;
      } catch (e) { /* 未恢复，继续等待 */ }
    }
    if (up) {
      log('服务已恢复。');
      try {
        const res = await ctx.api('/api/update/version', { signal: ctx.signal });
        const v = safeText(fieldOf(res, 'version', fieldOf(res, 'data', res)), '');
        if (v) versionLabel.textContent = v.startsWith('v') ? v : `v${v}`;
      } catch (e) { /* 版本读取失败不影响结论 */ }
      ctx.toast.success('服务已恢复，更新生效');
    } else {
      log('等待超时：服务未在 120s 内恢复，请检查设备侧 panel_watchdog 日志（/tmp/panel_watchdog.log）后手动刷新。');
      ctx.toast.error('服务恢复超时，请手动刷新页面确认');
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
