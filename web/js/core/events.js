/**
 * 轻量事件总线：on / off / emit / clear。
 * 支持按 owner 清理——页面离开时用 clear(owner) 一次性移除该页面注册的所有监听，
 * 避免监听器随路由切换累积。
 */
const channels = new Map(); // event -> Map<fn, owner|undefined>

export function on(event, fn, owner) {
  if (typeof fn !== 'function') return () => {};
  let bucket = channels.get(event);
  if (!bucket) {
    bucket = new Map();
    channels.set(event, bucket);
  }
  bucket.set(fn, owner);
  return () => off(event, fn);
}

export function off(event, fn) {
  const bucket = channels.get(event);
  if (!bucket) return;
  bucket.delete(fn);
  if (bucket.size === 0) channels.delete(event);
}

export function emit(event, payload) {
  const bucket = channels.get(event);
  if (!bucket) return;
  // 复制一份，允许回调中安全地 off / on
  for (const fn of Array.from(bucket.keys())) {
    try { fn(payload); } catch (e) { /* 单个订阅者异常不影响其他订阅者 */ }
  }
}

/** 移除某 owner 注册的所有监听；不传 owner 时清空全部。 */
export function clear(owner) {
  if (owner === undefined) {
    channels.clear();
    return;
  }
  for (const [event, bucket] of channels) {
    for (const [fn, o] of bucket) {
      if (o === owner) bucket.delete(fn);
    }
    if (bucket.size === 0) channels.delete(event);
  }
}

export default { on, off, emit, clear };
