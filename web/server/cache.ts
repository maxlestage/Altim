/**
 * Memory cache with request dedup: one upstream query at a time per key,
 * even if many visitors arrive together; last good value served during an outage.
 */
type Entry<T> = { at: number; value?: T; pending?: Promise<T> };
const store = new Map<string, Entry<unknown>>();
const STALE_MAX = 10 * 60_000;

export async function cached<T>(key: string, ttlMs: number, load: () => Promise<T>): Promise<T> {
  const entry = (store.get(key) ?? { at: 0 }) as Entry<T>;
  if (entry.value !== undefined && Date.now() - entry.at < ttlMs) return entry.value;
  if (!entry.pending) {
    entry.pending = load()
      .then((value) => {
        entry.value = value;
        entry.at = Date.now();
        return value;
      })
      .finally(() => (entry.pending = undefined));
    store.set(key, entry);
  }
  try {
    return await entry.pending!;
  } catch (e) {
    if (entry.value !== undefined && Date.now() - entry.at < STALE_MAX) return entry.value;
    throw e;
  }
}

export function clearCache() {
  store.clear();
}
