/**
 * 状态徽标 / 状态灯。
 * 所有文本通过 textContent 写入，绝不注入 HTML。
 */
import { el } from '../utils/dom.js';
import { safeText } from '../utils/format.js';

const VALID_TONES = new Set(['neutral', 'ok', 'success', 'error', 'danger', 'warning', 'info', 'primary', 'muted']);

/** 依据常见布尔/状态值推断语义色调。 */
export function toneFor(value) {
  if (value === true || value === 1) return 'ok';
  if (value === false || value === 0) return 'error';
  const v = String(value == null ? '' : value).toLowerCase();
  if (['ok', 'success', 'online', 'connected', 'running', 'up', 'enabled', 'active',
    'good', 'charged', 'full', 'unlocked', 'passed'].includes(v)) return 'ok';
  if (['error', 'failed', 'offline', 'disconnected', 'stopped', 'down', 'disabled',
    'inactive', 'poor', 'dead', 'locked'].includes(v)) return 'error';
  if (['warning', 'busy', 'syncing', 'connecting', 'fair', 'charging', 'unknown'].includes(v)) return 'warning';
  return 'muted';
}

/** 文本徽标。tone 不在允许集合时回退 neutral。 */
export function statusBadge(text, tone = 'neutral') {
  const useTone = VALID_TONES.has(tone) ? tone : 'neutral';
  return el('span', {
    class: `badge badge--${useTone}`,
    text: safeText(text, '—')
  });
}

/** 依据值自动生成色调的徽标。 */
export function autoBadge(text, value) {
  return statusBadge(text, toneFor(value));
}

/** 状态灯：圆点 + 文案，文案缺省时用状态值。 */
export function statusLight(value, label) {
  const tone = toneFor(value);
  const text = label != null
    ? label
    : safeText(value == null ? '未知' : (typeof value === 'boolean' ? (value ? '正常' : '异常') : value), '未知');
  return el('span', { class: 'status-light' },
    el('i', { class: `dot dot--${tone}` }),
    el('span', { class: 'status-light__label', text }));
}

export default { toneFor, statusBadge, autoBadge, statusLight };
