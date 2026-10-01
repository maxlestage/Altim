//! Browser side of the decision cache ("altim.decision.v1", decision.ts) and of the configuration changes
//! ("altim.configChanges.v1", config-changes.ts): same keys and JSON as the TypeScript, the logic in
//! `altim_core::web::decision`. The cache (up to 40 decisions) is mirrored in memory, parsed once per entry, and
//! re-read when another tab writes it (`storage` event), so the Radar's badges do not re-parse it at every render.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use altim_core::types::Kind;
use altim_core::web::decision::cache::{self, CACHE_KEY, CachedDecision, RECENT_MS};
use altim_core::web::decision::config_changes::{CONFIG_KEY, ConfigState, ConfigTransition, apply_decision, parse_state};
use altim_core::web::decision::doc::DecisionDoc;
use serde_json::{Map, Value};
use yew::prelude::*;

use crate::state::{Store, local_get, local_set, now, use_store};

struct Mirror {
    all: Map<String, Value>,
    parsed: HashMap<String, Option<Rc<CachedDecision>>>,
}

thread_local! {
    static CACHE: RefCell<Option<Mirror>> = const { RefCell::new(None) };
    /// Bumped when a decision is cached (the Radar's badges and sort re-render).
    pub static DECISIONS: Store<u64> = Store::new(0);
    /// Configuration state, with the stored text it was read from (`readConfigState`'s cache).
    static CONFIG: RefCell<Option<(Option<String>, Rc<ConfigState>)>> = const { RefCell::new(None) };
    pub static CONFIG_SEEN: Store<u64> = Store::new(0);
    static LISTENING: RefCell<Option<gloo::events::EventListener>> = const { RefCell::new(None) };
}

/// Another tab changed the cache: read it again at the next use.
fn listen() {
    LISTENING.with(|l| {
        if l.borrow().is_some() {
            return;
        }
        let Some(w) = web_sys::window() else { return };
        *l.borrow_mut() = Some(gloo::events::EventListener::new(&w, "storage", |e| {
            let key = js_sys::Reflect::get(e, &"key".into()).ok().and_then(|k| k.as_string());
            if key.as_deref().is_none_or(|k| k == CACHE_KEY) {
                CACHE.with(|c| *c.borrow_mut() = None);
                DECISIONS.with(|s| s.set(*s.get() + 1));
            }
            if key.as_deref().is_none_or(|k| k == CONFIG_KEY) {
                CONFIG_SEEN.with(|s| s.set(*s.get() + 1));
            }
        }));
    });
}

fn with_mirror<T>(f: impl FnOnce(&mut Mirror) -> T) -> T {
    listen();
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let m = c.get_or_insert_with(|| Mirror { all: cache::read_all(local_get(CACHE_KEY).as_deref()), parsed: HashMap::new() });
        f(m)
    })
}

/// `cachedDecision`: the last decision kept for this asset.
pub fn cached(kind: Kind, symbol: &str) -> Option<Rc<CachedDecision>> {
    let key = format!("{}:{symbol}", kind.as_str());
    with_mirror(|m| {
        if let Some(p) = m.parsed.get(&key) {
            return p.clone();
        }
        let p = cache::entry(&m.all, kind, symbol).map(Rc::new);
        m.parsed.insert(key, p.clone());
        p
    })
}

/// The cached decision when under 12 h old (`freshDecision`, the badge's rule).
pub fn fresh(kind: Kind, symbol: &str) -> Option<Rc<CachedDecision>> {
    cached(kind, symbol).filter(|c| now() - c.at <= RECENT_MS)
}

/// `cacheDecision`: kept in the cache and compared with the previous configuration.
pub fn cache_decision(d: &DecisionDoc, personal: bool) {
    let t = now();
    record_configuration(d, personal, t);
    let raw = local_get(CACHE_KEY);
    let text = cache::cache_text(raw.as_deref(), d, personal, t);
    local_set(CACHE_KEY, &text);
    // What is actually stored (a refused write leaves the previous cache).
    CACHE.with(|c| *c.borrow_mut() = None);
    DECISIONS.with(|s| s.set(*s.get() + 1));
}

/// `readConfigState`: parsed again only when the stored text changed.
pub fn config_state() -> Rc<ConfigState> {
    listen();
    let raw = local_get(CONFIG_KEY);
    CONFIG.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((r, s)) = c.as_ref() {
            if *r == raw {
                return s.clone();
            }
        }
        let s = Rc::new(parse_state(raw.as_deref()));
        *c = Some((raw, s.clone()));
        s
    })
}

/// `recordConfiguration`: called with every decision received.
pub fn record_configuration(d: &DecisionDoc, personal: bool, at: f64) -> Option<ConfigTransition> {
    crate::state::local_storage()?;
    let (state, transition) = apply_decision(&config_state(), d, personal, at, &crate::money::display());
    local_set(CONFIG_KEY, &state.to_json());
    CONFIG_SEEN.with(|s| s.set(*s.get() + 1));
    transition
}

pub fn clear_transitions() {
    if crate::state::local_storage().is_none() {
        return;
    }
    let s = ConfigState { transitions: Vec::new(), ..(*config_state()).clone() };
    local_set(CONFIG_KEY, &s.to_json());
    CONFIG_SEEN.with(|s| s.set(*s.get() + 1));
}

/// `useTransitions`: the state whose `transitions` are shown (newest first); re-renders when they change.
#[hook]
pub fn use_transitions() -> Rc<ConfigState> {
    let _seen = use_store(&CONFIG_SEEN);
    config_state()
}

/// Re-renders when a decision is cached; the value is a counter.
#[hook]
pub fn use_decisions_seen() -> u64 {
    *use_store(&DECISIONS)
}
