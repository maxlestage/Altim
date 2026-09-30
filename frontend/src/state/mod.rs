//! Stores kept in the browser, the Yew equivalent of the TypeScript `useSyncExternalStore` modules: a value per
//! store (thread-local, the page has one thread), listeners, and `use_store` to re-render a component on change.
//! Every write to localStorage is wrapped (private browsing, quota): a failure only means "not remembered".
pub mod app;
pub mod fx;
pub mod holdings;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread::LocalKey;

use wasm_bindgen::JsCast;
use yew::prelude::*;

pub struct Store<T: 'static> {
    value: RefCell<Rc<T>>,
    subs: RefCell<Vec<(u64, Rc<dyn Fn()>)>>,
    next: Cell<u64>,
}

impl<T: 'static> Store<T> {
    pub fn new(v: T) -> Self {
        Store { value: RefCell::new(Rc::new(v)), subs: RefCell::new(Vec::new()), next: Cell::new(0) }
    }

    pub fn get(&self) -> Rc<T> {
        self.value.borrow().clone()
    }

    /// Replaces the value and re-renders the subscribed components.
    pub fn set(&self, v: T) {
        *self.value.borrow_mut() = Rc::new(v);
        let subs: Vec<Rc<dyn Fn()>> = self.subs.borrow().iter().map(|(_, f)| f.clone()).collect();
        for f in subs {
            f();
        }
    }

    pub fn subscribe(&self, f: Rc<dyn Fn()>) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.subs.borrow_mut().push((id, f));
        id
    }

    pub fn unsubscribe(&self, id: u64) {
        self.subs.borrow_mut().retain(|(i, _)| *i != id);
    }
}

/// The store's current value; the component re-renders whenever it changes.
#[hook]
pub fn use_store<T: 'static>(key: &'static LocalKey<Store<T>>) -> Rc<T> {
    let update = use_force_update();
    use_effect_with((), move |_| {
        let id = key.with(|s| s.subscribe(Rc::new(move || update.force_update())));
        move || key.with(|s| s.unsubscribe(id))
    });
    key.with(|s| s.get())
}

/// localStorage, when the browser gives it.
pub fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

pub fn local_get(key: &str) -> Option<String> {
    local_storage().and_then(|s| s.get_item(key).ok().flatten())
}

/// Writes a key; a failure (quota, private browsing) is silently "not remembered", like the TypeScript.
pub fn local_set(key: &str, value: &str) {
    if let Some(s) = local_storage() {
        let _ = s.set_item(key, value);
    }
}

pub fn local_remove(key: &str) {
    if let Some(s) = local_storage() {
        let _ = s.remove_item(key);
    }
}

/// `Date.now()`.
pub fn now() -> f64 {
    js_sys::Date::now()
}

/// `crypto.randomUUID()` (ids of saved lines).
pub fn random_uuid() -> String {
    let c = js_sys::Reflect::get(&js_sys::global(), &"crypto".into()).ok();
    let f = c.as_ref().and_then(|c| js_sys::Reflect::get(c, &"randomUUID".into()).ok()).and_then(|f| f.dyn_into::<js_sys::Function>().ok());
    match (c, f) {
        (Some(c), Some(f)) => f.call0(&c).ok().and_then(|v| v.as_string()).unwrap_or_default(),
        _ => format!("{:x}-{:x}", now() as u64, (js_sys::Math::random() * 1e16) as u64),
    }
}
