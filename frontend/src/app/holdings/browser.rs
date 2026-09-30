//! Browser helpers of Mes avoirs: file download and upload (through the JavaScript globals, no extra web-sys
//! feature), `confirm` / `alert`, French dates, focus on open.
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use yew::prelude::*;

fn get(o: &JsValue, k: &str) -> Option<JsValue> {
    js_sys::Reflect::get(o, &k.into()).ok().filter(|v| !v.is_undefined() && !v.is_null())
}

/// Saves `text` as a file (a Blob behind a temporary link, as MyHoldings.tsx).
pub fn download(name: &str, mime: &str, text: &str) {
    let global = js_sys::global();
    let (Some(blob_ctor), Some(url)) = (get(&global, "Blob"), get(&global, "URL")) else { return };
    let Ok(blob_ctor) = blob_ctor.dyn_into::<js_sys::Function>() else { return };
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"type".into(), &mime.into());
    let parts = js_sys::Array::of1(&JsValue::from_str(text));
    let Ok(blob) = js_sys::Reflect::construct(&blob_ctor, &js_sys::Array::of2(&parts, &opts)) else { return };
    let Some(create) = get(&url, "createObjectURL").and_then(|f| f.dyn_into::<js_sys::Function>().ok()) else { return };
    let Some(href) = create.call1(&url, &blob).ok().and_then(|h| h.as_string()) else { return };
    if let Ok(a) = gloo::utils::document().create_element("a") {
        let _ = a.set_attribute("href", &href);
        let _ = a.set_attribute("download", name);
        if let Ok(a) = a.dyn_into::<web_sys::HtmlElement>() {
            a.click();
        }
    }
    let _ = web_sys::Url::revoke_object_url(&href);
}

/// Text of the first file chosen in an `<input type="file">`.
pub async fn file_text(input: &web_sys::HtmlInputElement) -> Option<String> {
    let file = get(input, "files").and_then(|f| get(&f, "0"))?;
    let text = get(&file, "text")?.dyn_into::<js_sys::Function>().ok()?;
    let promise = text.call0(&file).ok()?.dyn_into::<js_sys::Promise>().ok()?;
    JsFuture::from(promise).await.ok()?.as_string()
}

pub fn confirm(msg: &str) -> bool {
    web_sys::window().and_then(|w| w.confirm_with_message(msg).ok()).unwrap_or(false)
}

pub fn alert(msg: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.alert_with_message(msg);
    }
}

/// The tab is shown (refreshes are skipped while it is hidden).
pub fn visible() -> bool {
    gloo::utils::document().visibility_state() == web_sys::VisibilityState::Visible
}

fn opts(pairs: &[(&str, &str)]) -> JsValue {
    let o = js_sys::Object::new();
    for (k, v) in pairs {
        let _ = js_sys::Reflect::set(&o, &(*k).into(), &(*v).into());
    }
    o.into()
}

/// "30/09/2026" (`toLocaleDateString("fr-FR")`, the browser's time zone).
pub fn fr_date(ms: f64) -> String {
    js_sys::Date::new(&ms.into()).to_locale_date_string("fr-FR", &opts(&[])).into()
}

/// "5 sept." or "5 sept. 2026" in UTC (the daily closes' calendar days).
pub fn utc_day(ms: i64, year: bool) -> String {
    let mut o = vec![("day", "numeric"), ("month", "short"), ("timeZone", "UTC")];
    if year {
        o.push(("year", "numeric"));
    }
    js_sys::Date::new(&(ms as f64).into()).to_locale_date_string("fr-FR", &opts(&o)).into()
}

/// Focuses the element once it is shown (React's `autoFocus`).
#[hook]
pub fn use_auto_focus(on: bool) -> NodeRef {
    let node = use_node_ref();
    {
        let node = node.clone();
        use_effect_with((), move |_| {
            if on {
                if let Some(el) = node.cast::<web_sys::HtmlElement>() {
                    let _ = el.focus();
                }
            }
        });
    }
    node
}

/// `e.stopPropagation()` of the sheets (a click inside does not close them).
pub fn stop() -> Callback<MouseEvent> {
    Callback::from(|e: MouseEvent| e.stop_propagation())
}

/// The value of the input of an `input` event.
pub fn input_value(e: &InputEvent) -> String {
    e.target_unchecked_into::<web_sys::HtmlInputElement>().value()
}
