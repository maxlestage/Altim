//! Memory cache with request dedup: one upstream query at a time per key, even if many visitors arrive
//! together; last good value served during an outage (same rules as `web/server/cache.ts`).
use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, LazyLock, Mutex};

use futures::FutureExt;
use futures::future::{BoxFuture, Shared};

use crate::http::{Error, Result};
use crate::js::now_ms;

type AnyArc = Arc<dyn Any + Send + Sync>;
type Pending = Shared<BoxFuture<'static, std::result::Result<AnyArc, Error>>>;

#[derive(Default)]
struct Entry {
    at: i64,
    value: Option<AnyArc>,
    pending: Option<Pending>,
    /// Insertion order, to drop the oldest keys first.
    seq: u64,
}

struct Store {
    map: HashMap<String, Entry>,
    seq: u64,
}

static STORE: LazyLock<Mutex<Store>> = LazyLock::new(|| Mutex::new(Store { map: HashMap::new(), seq: 0 }));
const STALE_MAX: i64 = 10 * 60_000;
/// Bound on the number of keys (every symbol asked creates some): the oldest ones are dropped beyond it.
const MAX_KEYS: usize = 5_000;
/// Keys holding years of candles or a full per-asset report weigh far more than the others: a tighter bound for
/// them, so walking many symbols cannot fill the dyno's memory before `MAX_KEYS` is reached.
const HEAVY_PREFIXES: [&str; 6] = ["long:", "strategies:", "anomalies:", "why:", "okx:", "extras:"];
const MAX_HEAVY_KEYS: usize = 400;

fn heavy(key: &str) -> bool {
    HEAVY_PREFIXES.iter().any(|p| key.starts_with(p))
}

/// Value of `key`, loaded with `load` when older than `ttl_ms`. The load runs on its own task: a visitor who
/// leaves does not cancel it for the others.
pub async fn cached<T, F, Fut>(key: &str, ttl_ms: i64, load: F) -> Result<Arc<T>>
where
    T: Send + Sync + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>> + Send + 'static,
{
    // The type is part of the key: two modules choosing the same name can never read each other's values (the
    // TypeScript server had that bug: the guard's headlines and the news page both used "news:crypto:BTC").
    let typed = format!("{key}#{}", std::any::type_name::<T>());
    let key = typed.as_str();
    let pending = {
        let mut store = STORE.lock().unwrap();
        if let Some(e) = store.map.get(key) {
            if let Some(v) = &e.value {
                if now_ms() - e.at < ttl_ms {
                    if let Ok(t) = v.clone().downcast::<T>() {
                        return Ok(t);
                    }
                }
            }
        }
        let existing = store.map.get(key).and_then(|e| e.pending.clone());
        match existing {
            Some(p) => p,
            None => {
                let fut = load();
                let k = key.to_string();
                let task = tokio::spawn(async move {
                    let r = fut.await.map(|v| Arc::new(v) as AnyArc);
                    let mut store = STORE.lock().unwrap();
                    if let Some(e) = store.map.get_mut(&k) {
                        e.pending = None;
                        if let Ok(v) = &r {
                            e.value = Some(v.clone());
                            e.at = now_ms();
                        }
                    }
                    r
                });
                let shared: Pending = async move { task.await.unwrap_or_else(|e| Err(Error(format!("tâche interrompue : {e}")))) }.boxed().shared();
                if !store.map.contains_key(key) && store.map.len() >= MAX_KEYS {
                    let mut old: Vec<(u64, String)> =
                        store.map.iter().filter(|(_, e)| e.pending.is_none()).map(|(k, e)| (e.seq, k.clone())).collect();
                    old.sort();
                    for (_, k) in old.into_iter().take(MAX_KEYS / 10) {
                        store.map.remove(&k);
                    }
                }
                if heavy(key) && !store.map.contains_key(key) {
                    let mut old: Vec<(u64, String)> =
                        store.map.iter().filter(|(k, e)| heavy(k) && e.pending.is_none()).map(|(k, e)| (e.seq, k.clone())).collect();
                    if old.len() >= MAX_HEAVY_KEYS {
                        old.sort();
                        for (_, k) in old.into_iter().take(MAX_HEAVY_KEYS / 10) {
                            store.map.remove(&k);
                        }
                    }
                }
                store.seq += 1;
                let seq = store.seq;
                let e = store.map.entry(key.to_string()).or_insert_with(|| Entry { seq, ..Default::default() });
                e.pending = Some(shared.clone());
                shared
            }
        }
    };
    match pending.await {
        Ok(v) => v.downcast::<T>().map_err(|_| Error("type de cache incohérent".into())),
        Err(e) => {
            let store = STORE.lock().unwrap();
            if let Some(entry) = store.map.get(key) {
                if let Some(v) = &entry.value {
                    if now_ms() - entry.at < STALE_MAX {
                        if let Ok(t) = v.clone().downcast::<T>() {
                            return Ok(t);
                        }
                    }
                }
            }
            Err(e)
        }
    }
}

/// The value cached under `key` if one is there and younger than `max_age_ms`, without ever loading it (for
/// summaries that use only what other routes already fetched).
pub fn peek<T: Send + Sync + 'static>(key: &str, max_age_ms: i64) -> Option<Arc<T>> {
    let typed = format!("{key}#{}", std::any::type_name::<T>());
    let store = STORE.lock().unwrap();
    let e = store.map.get(&typed)?;
    if now_ms() - e.at > max_age_ms {
        return None;
    }
    e.value.clone()?.downcast::<T>().ok()
}

pub fn clear_cache() {
    STORE.lock().unwrap().map.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn dedup_and_stale() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let load = || async {
            CALLS.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Ok::<_, Error>(42u32)
        };
        let (a, b) = tokio::join!(cached("t:dedup", 60_000, load), cached("t:dedup", 60_000, load));
        assert_eq!((*a.unwrap(), *b.unwrap()), (42, 42));
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
        // Expired (ttl 0) and failing: the last good value is served.
        let v = cached::<u32, _, _>("t:dedup", 0, || async { Err(Error("panne".into())) }).await.unwrap();
        assert_eq!(*v, 42);
        let e = cached::<u32, _, _>("t:none", 0, || async { Err(Error("panne".into())) }).await;
        assert_eq!(e.unwrap_err().0, "panne");
        // Same name, other type: its own entry, never the other one's value.
        let s = cached::<String, _, _>("t:dedup", 60_000, || async { Ok("texte".to_string()) }).await.unwrap();
        assert_eq!(*s, "texte");
        assert_eq!(*cached::<u32, _, _>("t:dedup", 60_000, || async { Ok(7) }).await.unwrap(), 42);
        // Peek: only what is there, of the right type, never a load.
        assert_eq!(peek::<u32>("t:dedup", 60_000).as_deref(), Some(&42));
        assert!(peek::<u64>("t:dedup", 60_000).is_none());
        assert!(peek::<u32>("t:absent", 60_000).is_none());
    }
}
