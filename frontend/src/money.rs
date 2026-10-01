//! Display currency of every amount (money.ts): the formatters read the current `MoneyDisplay`, recomputed when the
//! currency chosen in Réglages or the rate changes. A component that shows amounts calls `use_money()` so it
//! re-renders on those changes. Without a rate the amounts stay in dollars with "$" (never a made-up rate).
use std::rc::Rc;

use altim_core::web::money::{Currency, FxRate, MoneyDisplay, NBSP};
use yew::prelude::*;

use crate::state::{Store, use_store};

thread_local! {
    pub static DISPLAY: Store<MoneyDisplay> = Store::new(MoneyDisplay::usd());
}

/// Recomputes the display from the settings and the rate (`setMoneyDisplay`).
pub fn refresh() {
    let want = crate::state::app::app_state().currency;
    let fx = crate::state::fx::fx_state().fx.clone();
    let d = MoneyDisplay::new(want, fx);
    if DISPLAY.with(|s| *s.get() != d) {
        DISPLAY.with(|s| s.set(d));
    }
}

/// The engines' French texts (altim-core) use the same rate as the display: euros only when euros are shown.
pub fn install_engine_rate() {
    altim_core::fx::install(|| {
        DISPLAY.with(|s| {
            let d = s.get();
            if d.currency() == Currency::Eur { d.fx().map(|r| r.rate) } else { None }
        })
    });
}

pub fn display() -> Rc<MoneyDisplay> {
    DISPLAY.with(|s| s.get())
}

/// The current display; the component re-renders when it changes.
#[hook]
pub fn use_money() -> Rc<MoneyDisplay> {
    use_store(&DISPLAY)
}

pub fn currency() -> Currency {
    display().currency()
}

pub fn symbol() -> &'static str {
    display().symbol()
}

/// "212,40 €" (2 decimals).
pub fn money(usd: f64) -> String {
    display().money(usd, 2, 2, NBSP)
}

/// `money(usd, min, max, sep)`.
pub fn money_with(usd: f64, min: usize, max: usize, sep: &str) -> String {
    display().money(usd, min, max, sep)
}

/// A price with `formatPrice` digits: "0,4400 €".
pub fn price(usd: f64) -> String {
    display().money_price(usd, NBSP)
}

/// `moneyPrice(usd, sep)` ("64 210,50 €" with a plain space on the site).
pub fn price_sep(usd: f64, sep: &str) -> String {
    display().money_price(usd, sep)
}

/// "3,16 Md€".
pub fn compact(usd: f64) -> String {
    display().money_compact(usd, NBSP)
}

pub fn to_display(usd: f64) -> f64 {
    display().to_display(usd)
}

pub fn from_display(v: f64) -> f64 {
    display().from_display(v)
}

/// The server writes its texts in euros when it has a rate; a client showing dollars asks for dollars ("&cur=USD").
pub fn cur_param() -> &'static str {
    if currency() == Currency::Usd { "&cur=USD" } else { "" }
}

/// "1 $ = 0,881 € · Yahoo Finance, 14:05" (or the date when not today, in the viewer's time zone).
pub fn fx_line(r: &FxRate, now: f64) -> String {
    let d = js_sys::Date::new(&r.time.into());
    let today = js_sys::Date::new(&now.into());
    let time = crate::ui::fr_time(r.time);
    let when = if today.to_date_string() == d.to_date_string() { time } else { format!("{} {time}", crate::ui::fr_day_month(r.time)) };
    altim_core::web::money::fx_line(r, &when)
}
