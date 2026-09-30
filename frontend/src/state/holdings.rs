//! My holdings ("altim.holdings.v1", store.ts): the lines as typed (their own currency), and their dollar view for
//! the engines and the server. Format in `altim_core::web::store::HoldingsState`.
use std::rc::Rc;

use altim_core::web::store::{HOLDINGS_KEY, HoldingsState, StoredHolding, UsdHoldings, merge_line, to_usd_holdings};
use yew::prelude::*;

use super::{Store, local_get, local_set, now, random_uuid, use_store};

thread_local! {
    pub static HOLDINGS: Store<HoldingsState> = Store::new(HoldingsState::parse(local_get(HOLDINGS_KEY).as_deref()));
}

pub fn holdings_state() -> Rc<HoldingsState> {
    HOLDINGS.with(|s| s.get())
}

/// `setHoldings`: applies the change, stamps `updatedAt`, saves.
pub fn set_holdings(update: impl FnOnce(&mut HoldingsState)) {
    let mut s = (*holdings_state()).clone();
    update(&mut s);
    s.updated_at = now();
    local_set(HOLDINGS_KEY, &s.to_json().to_string());
    HOLDINGS.with(|h| h.set(s));
}

/// Holdings exactly as saved (amounts in their own currency): the editing forms.
#[hook]
pub fn use_stored_holdings() -> Rc<HoldingsState> {
    use_store(&HOLDINGS)
}

/// Holdings in dollars (engines, server, decision), recomputed when the holdings or the rate change.
#[hook]
pub fn use_holdings() -> UsdHoldings {
    let s = use_store(&HOLDINGS);
    let d = crate::money::use_money();
    to_usd_holdings(&s, &d)
}

/// `addHoldings`: several lines at once; an asset already held is merged (weighted average cost).
pub fn add_holdings(items: Vec<StoredHolding>) {
    let d = crate::money::display();
    set_holdings(|s| {
        for mut h in items {
            let i = s.holdings.iter().position(|x| x.symbol == h.symbol && x.kind == h.kind);
            match i.and_then(|i| merge_line(&s.holdings[i], h.quantity, h.average_price, h.cost_currency, &d).map(|m| (i, m))) {
                Some((i, m)) => s.holdings[i] = m,
                None => {
                    h.id = random_uuid();
                    s.holdings.push(h);
                }
            }
        }
    });
}
