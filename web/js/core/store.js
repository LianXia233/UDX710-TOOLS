/**
 * 轻量集中式 Store：get / set / subscribe / unsubscribe。
 * 用普通对象 + 订阅集合实现，避免全局状态散落与隐式耦合。
 * 支持按 key 订阅与全量订阅（key === '*'），并可按 owner 批量退订。
 */
const state = Object.create(null);
const subscribers = new Map(); // key -> Map<fn, owner|undefined>

export function get(key) {
  return state[key];
}

/** 设置状态并通知订阅者；值为对象时做浅比较，相同则不触发。 */
export function set(key, value) {
  const prev = state[key];
  state[key] = value;
  notify(key, value, prev);
  notify('*', { key, value, prev }, null);
}

export function subscribe(key, fn, owner) {
  if (typeof fn !== 'function') return () => {};
  let bucket = subscribers.get(key);
  if (!bucket) {
    bucket = new Map();
    subscribers.set(key, bucket);
  }
  bucket.set(fn, owner);
  return () => unsubscribe(key, fn);
}

export function unsubscribe(key, fn) {
  const bucket = subscribers.get(key);
  if (!bucket) return;
  bucket.delete(fn);
  if (bucket.size === 0) subscribers.delete(key);
}

/** 移除某 owner 的所有订阅；不传 owner 时清空全部订阅。 */
export function clear(owner) {
  if (owner === undefined) {
    subscribers.clear();
    return;
  }
  for (const [key, bucket] of subscribers) {
    for (const [fn, o] of bucket) {
      if (o === owner) bucket.delete(fn);
    }
    if (bucket.size === 0) subscribers.delete(key);
  }
}

function notify(key, value, prev) {
  const bucket = subscribers.get(key);
  if (!bucket) return;
  for (const fn of Array.from(bucket.keys())) {
    try { fn(value, prev); } catch (e) { /* 订阅者异常隔离 */ }
  }
}

export default { get, set, subscribe, unsubscribe, clear };
