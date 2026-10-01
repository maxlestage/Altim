//! « Alertes » of the web app (`web/src/webapp/alerts-store.ts` and the pure part of `notify.ts`; same rules as the
//! iPhone and Android Alerts tab, AltimKit PriceAlerts / AlertTracker / NewsAlertTracker): price alerts chosen by
//! the user, which buy alerts and news were already notified, the journal of the alerts received with what each one
//! gave since, and the texts of the browser notifications. Kept in this browser only ("altim.alerts.v1", same JSON as
//! the TypeScript); the clock and the money display are passed in.
use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::engine::news::NewsItem;
use crate::js::fr;
use crate::types::Kind;
use crate::web::money::{Currency, MoneyDisplay, symbol};

pub const ALERTS_KEY: &str = "altim.alerts.v1";

/// "crypto:BTC".
fn id(symbol: &str, kind: Kind) -> String {
    crate::web::store::asset_key(symbol, kind)
}

/// A price alert. `price` is in `currency` (the display currency when it was created): the current dollar price is
/// converted at the current rate before comparing, so a threshold in euros is reached by the price in euros.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceTarget {
    pub id: String,
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    /// true: reached when the price rises to or above `price`; false: when it falls to or below.
    pub above: bool,
    pub currency: Currency,
    pub created: f64,
    pub triggered: Option<f64>,
    pub price: f64,
    /// Move alert: reached when the price moves by at least this many % (up or down) from `price`.
    #[serde(rename = "move")]
    pub move_pct: Option<f64>,
}

impl PriceTarget {
    pub fn key(&self) -> String {
        id(&self.symbol, self.kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum JournalSource {
    #[default]
    Buy,
    StrongBuy,
    Target,
}

/// One alert received; `price` in dollars (the sources' currency) when it was sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertEntry {
    pub id: String,
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    pub source: JournalSource,
    pub title: String,
    pub price: f64,
    pub date: f64,
}

impl AlertEntry {
    pub fn key(&self) -> String {
        id(&self.symbol, self.kind)
    }
}

/// Notifications while the tab is open (the browser cannot run Altim when it is closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NotifySettings {
    pub buy: bool,
    pub strong_only: bool,
    pub news: bool,
}

/// Asset → reasons already notified ("signal+zone:medium"), and since when it is no longer buyable (insertion order
/// kept, like the TypeScript objects).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Tracker {
    pub notified: IndexMap<String, String>,
    pub lost: IndexMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewsSeen {
    pub id: String,
    pub words: Vec<String>,
    pub time: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AlertsState {
    pub notify: NotifySettings,
    pub targets: Vec<PriceTarget>,
    pub journal: Vec<AlertEntry>,
    pub tracker: Tracker,
    pub news_seen: Vec<NewsSeen>,
    pub last_check: Option<f64>,
}

/// `/api/alerts` row ("Can I buy now?", the rule of the notifications of the apps).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuyAlert {
    pub symbol: String,
    pub kind: Kind,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub buy: bool,
    #[serde(default)]
    pub strong: bool,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub cautions: Vec<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl BuyAlert {
    pub fn asset_key(&self) -> String {
        id(&self.symbol, self.kind)
    }
}

// ---------- Price alerts ----------

/// "Au-dessus de 80 000 €" / "Variation de ±5 % (depuis 212,40 €)".
pub fn target_label(t: &PriceTarget, fmt: impl Fn(f64, Currency) -> String) -> String {
    match t.move_pct {
        Some(m) => format!("Variation de ±{} % (depuis {})", fr(m, 0, 1), fmt(t.price, t.currency)),
        None => format!("{} {}", if t.above { "Au-dessus de" } else { "En dessous de" }, fmt(t.price, t.currency)),
    }
}

/// A threshold shown in its own currency ("80 000,00 €", "0,4400 $").
pub fn threshold_text(v: f64, c: Currency) -> String {
    let (min, max) = if v >= 1.0 { (2, 2) } else { (4, 8) };
    format!("{} {}", fr(v, min, max), symbol(c))
}

/// The dollar price in the alert's currency (NaN when no rate allows it: the alert then waits).
pub fn in_target_currency(t: &PriceTarget, usd: f64, d: &MoneyDisplay) -> f64 {
    d.convert(usd, Currency::Usd, t.currency)
}

pub fn is_reached(t: &PriceTarget, usd: f64, d: &MoneyDisplay) -> bool {
    let p = in_target_currency(t, usd, d);
    if !p.is_finite() {
        return false;
    }
    match t.move_pct {
        Some(m) => t.price > 0.0 && (p / t.price - 1.0).abs() * 100.0 >= m,
        None if t.above => p >= t.price,
        None => p <= t.price,
    }
}

/// Re-armed: a move alert starts again from the current price.
pub fn rearm(t: &PriceTarget, usd: Option<f64>, d: &MoneyDisplay) -> PriceTarget {
    let p = usd.map(|u| in_target_currency(t, u, d)).unwrap_or(f64::NAN);
    let mut out = PriceTarget { triggered: None, ..t.clone() };
    if t.move_pct.is_some() && p.is_finite() && p > 0.0 {
        out.price = p;
    }
    out
}

/// Armed targets reached at the given dollar prices (key "kind:SYMBOL"): updated list and those just reached.
pub fn evaluate_targets(
    targets: &[PriceTarget],
    prices: &HashMap<String, f64>,
    now: f64,
    d: &MoneyDisplay,
) -> (Vec<PriceTarget>, Vec<(PriceTarget, f64)>) {
    let mut fired = Vec::new();
    let updated = targets
        .iter()
        .map(|t| match prices.get(&t.key()) {
            Some(&p) if t.triggered.is_none() && p.is_finite() && is_reached(t, p, d) => {
                let done = PriceTarget { triggered: Some(now), ..t.clone() };
                fired.push((done.clone(), p));
                done
            }
            _ => t.clone(),
        })
        .collect();
    (updated, fired)
}

/// What the Alertes screen says under a target: when it was reached, how far the price is, or that it waits.
pub fn target_status(t: &PriceTarget, shown: Option<f64>, triggered_text: impl Fn(f64) -> String) -> String {
    if let Some(at) = t.triggered {
        return format!("Atteinte le {}", triggered_text(at));
    }
    match (shown, t.move_pct) {
        (Some(p), Some(m)) => format!(
            "Prix actuel {} · variation {} % sur ±{} %",
            threshold_text(p, t.currency),
            fr((p / t.price - 1.0) * 100.0, 0, 1),
            crate::js::number_to_string(m)
        ),
        (Some(p), None) => {
            let gap = (t.price / p - 1.0) * 100.0;
            // `signDisplay: "always"`: "+" for 0 and above, the value's own "-" below.
            let signed = if gap.is_sign_negative() { fr(gap, 0, 1) } else { format!("+{}", fr(gap, 0, 1)) };
            format!("Prix actuel {} · encore {signed} %", threshold_text(p, t.currency))
        }
        _ => "En attente".into(),
    }
}

/// The value typed in the price alert sheet: spaces, "$", "€" and "%" ignored, decimal comma accepted (`Number`).
pub fn parse_typed(text: &str) -> f64 {
    let cleaned: String = text.chars().filter(|c| !(c.is_whitespace() || matches!(c, '\u{a0}' | '\u{202f}' | '$' | '€' | '%'))).collect();
    let s = cleaned.replacen(',', ".", 1);
    if s.is_empty() {
        return 0.0;
    }
    crate::js::parse_number(&s)
}

// ---------- Buy alerts: each reason notified once (AlertTracker) ----------

pub const BUY_COOLDOWN_MS: f64 = 6.0 * 3_600_000.0;

fn parts(key: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for p in key.split('+').filter(|p| !p.is_empty()) {
        if !v.iter().any(|x| x == p) {
            v.push(p.to_string());
        }
    }
    v
}

/// The fresh buy alerts (a new reason, or buyable again after the cool-down), and the tracker updated.
pub fn new_buy_alerts(tracker: &Tracker, items: &[BuyAlert], only_strong: bool, now: f64) -> (Tracker, Vec<BuyAlert>) {
    let mut notified = tracker.notified.clone();
    let mut lost = tracker.lost.clone();
    let mut fresh = Vec::new();
    for it in items {
        let k = it.asset_key();
        if it.buy && (!only_strong || it.strong) {
            lost.shift_remove(&k);
            let before = notified.get(&k).map(|s| parts(s));
            let reasons = parts(it.key.as_deref().unwrap_or(""));
            if before.as_ref().is_none_or(|b| reasons.iter().any(|r| !b.contains(r))) {
                fresh.push(it.clone());
            }
            let mut all = before.unwrap_or_default();
            for r in reasons {
                if !all.contains(&r) {
                    all.push(r);
                }
            }
            all.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            notified.insert(k, all.join("+"));
        } else if !it.buy && notified.contains_key(&k) {
            let since = lost.get(&k).copied().unwrap_or(now);
            lost.insert(k.clone(), since);
            if now - since >= BUY_COOLDOWN_MS {
                notified.shift_remove(&k);
                lost.shift_remove(&k);
            }
        }
    }
    (Tracker { notified, lost }, fresh)
}

// ---------- News worth a notification (NewsAlertTracker) ----------

pub const NEWS_FRESH_MS: f64 = 6.0 * 3_600_000.0;
const NEWS_MEMORY_MS: f64 = 48.0 * 3_600_000.0;

/// Words of 4 UTF-16 units or more of a title, lower case, split on anything but letters and digits.
fn words(title: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in title.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if w.encode_utf16().count() >= 4 && !out.iter().any(|x| x == w) {
            out.push(w.to_string());
        }
    }
    out
}

fn similar(a: &HashSet<&str>, b: &HashSet<&str>) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let inter = a.intersection(b).count();
    inter as f64 / a.union(b).count() as f64 >= 0.5
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewsReason {
    Alert,
    Asset,
}

/// A grave escalation told by at least 2 sources, or a story on one of the user's assets told by at least 3.
pub fn news_reason(item: &NewsItem, owned: &HashSet<String>) -> Option<NewsReason> {
    if item.alert && !item.also_in.is_empty() {
        return Some(NewsReason::Alert);
    }
    if item.also_in.len() >= 2 && item.assets.iter().any(|a| owned.contains(a)) {
        return Some(NewsReason::Asset);
    }
    None
}

pub fn new_news(seen: &[NewsSeen], items: &[NewsItem], owned: &HashSet<String>, now: f64) -> (Vec<NewsSeen>, Vec<NewsItem>) {
    let mut kept: Vec<NewsSeen> = seen.iter().filter(|s| now - s.time < NEWS_MEMORY_MS).cloned().collect();
    let mut fresh = Vec::new();
    for item in items {
        if now - item.time as f64 > NEWS_FRESH_MS || news_reason(item, owned).is_none() {
            continue;
        }
        let mut w = words(&item.title);
        let ws: HashSet<&str> = w.iter().map(String::as_str).collect();
        if kept.iter().any(|s| s.id == item.id || similar(&s.words.iter().map(String::as_str).collect(), &ws)) {
            continue;
        }
        w.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        kept.push(NewsSeen { id: item.id.clone(), words: w, time: now });
        fresh.push(item.clone());
    }
    (kept, fresh)
}

// ---------- Journal ----------

pub const JOURNAL_LIMIT: usize = 200;

pub fn add_to_journal(entries: &[AlertEntry], journal: &[AlertEntry]) -> Vec<AlertEntry> {
    let mut all: Vec<AlertEntry> = entries.iter().chain(journal).cloned().collect();
    all.sort_by(|a, b| b.date.partial_cmp(&a.date).unwrap_or(std::cmp::Ordering::Equal));
    all.truncate(JOURNAL_LIMIT);
    all
}

/// Change since the alert, in % (dollar prices both: the currency does not bias it).
pub fn change_since(e: &AlertEntry, usd: Option<f64>) -> Option<f64> {
    usd.filter(|u| u.is_finite() && e.price > 0.0).map(|u| (u / e.price - 1.0) * 100.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JournalSummary {
    pub count: usize,
    pub up: usize,
    pub up_share: f64,
    pub average: f64,
}

/// Buy alerts only (price targets are the user's own thresholds): how many went up since, average change. Alerts of
/// less than `min_age_ms` say nothing yet. Without fees nor exit rule: an indication, not a backtest.
pub fn journal_summary(journal: &[AlertEntry], prices: &HashMap<String, f64>, now: f64, min_age_ms: f64) -> Option<JournalSummary> {
    let changes: Vec<f64> = journal
        .iter()
        .filter(|e| e.source != JournalSource::Target && now - e.date >= min_age_ms)
        .filter_map(|e| change_since(e, prices.get(&e.key()).copied()))
        .collect();
    if changes.is_empty() {
        return None;
    }
    let up = changes.iter().filter(|c| **c > 0.0).count();
    Some(JournalSummary {
        count: changes.len(),
        up,
        up_share: up as f64 / changes.len() as f64 * 100.0,
        average: changes.iter().sum::<f64>() / changes.len() as f64,
    })
}

// ---------- Storage ----------

fn kind_of(v: Option<&Value>) -> Option<Kind> {
    v.and_then(Value::as_str).and_then(Kind::parse)
}

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_string()
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn target_of(t: &Value) -> Option<PriceTarget> {
    let o = t.as_object()?;
    let price = o.get("price")?.as_f64().filter(|p| p.is_finite() && *p > 0.0)?;
    Some(PriceTarget {
        id: o.get("id")?.as_str()?.to_string(),
        symbol: o.get("symbol")?.as_str()?.to_string(),
        kind: kind_of(o.get("kind"))?,
        name: text(o.get("name")),
        above: truthy(o.get("above")),
        currency: if o.get("currency").and_then(Value::as_str) == Some("EUR") { Currency::Eur } else { Currency::Usd },
        created: o.get("created").and_then(Value::as_f64).unwrap_or(0.0),
        triggered: o.get("triggered").and_then(Value::as_f64),
        price,
        move_pct: o.get("move").and_then(Value::as_f64).filter(|m| *m > 0.0),
    })
}

fn entry_of(e: &Value) -> Option<AlertEntry> {
    let o = e.as_object()?;
    Some(AlertEntry {
        id: text(o.get("id")),
        symbol: o.get("symbol")?.as_str()?.to_string(),
        kind: kind_of(o.get("kind"))?,
        name: text(o.get("name")),
        source: o.get("source").and_then(|s| crate::web::json::from_value(s).ok()).unwrap_or_default(),
        title: text(o.get("title")),
        price: o.get("price")?.as_f64()?,
        date: o.get("date")?.as_f64()?,
    })
}

impl AlertsState {
    /// A saved state, fields checked one by one (localStorage can be edited by hand); anything unreadable is dropped.
    pub fn parse(raw: Option<&str>) -> AlertsState {
        let Some(p) = raw.and_then(|r| serde_json::from_str::<Value>(r).ok()) else { return AlertsState::default() };
        if p.get("version").and_then(Value::as_f64) != Some(1.0) {
            return AlertsState::default();
        }
        let list = |k: &str| p.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
        let n = p.get("notify");
        let mut journal: Vec<AlertEntry> = list("journal").iter().filter_map(entry_of).collect();
        journal.truncate(JOURNAL_LIMIT);
        let tracker = match p.get("tracker").filter(|t| t.is_object()) {
            Some(t) => Tracker {
                notified: t
                    .get("notified")
                    .and_then(Value::as_object)
                    .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
                    .unwrap_or_default(),
                lost: t
                    .get("lost")
                    .and_then(Value::as_object)
                    .map(|m| m.iter().filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x))).collect())
                    .unwrap_or_default(),
            },
            None => Tracker::default(),
        };
        AlertsState {
            notify: NotifySettings {
                buy: truthy(n.and_then(|n| n.get("buy"))),
                strong_only: truthy(n.and_then(|n| n.get("strongOnly"))),
                news: truthy(n.and_then(|n| n.get("news"))),
            },
            targets: list("targets").iter().filter_map(target_of).collect(),
            journal,
            tracker,
            news_seen: list("newsSeen")
                .iter()
                .filter(|s| s.get("id").is_some_and(Value::is_string) && s.get("words").is_some_and(Value::is_array))
                .map(|s| NewsSeen {
                    id: text(s.get("id")),
                    words: s["words"].as_array().map(|w| w.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
                    time: s.get("time").and_then(Value::as_f64).unwrap_or(0.0),
                })
                .collect(),
            last_check: p.get("lastCheck").and_then(Value::as_f64),
        }
    }

    /// `JSON.stringify(state)` with the TypeScript key order.
    pub fn to_json(&self) -> Value {
        crate::js::to_value(&json!({
            "version": 1,
            "notify": self.notify,
            "targets": self.targets,
            "journal": self.journal,
            "tracker": self.tracker,
            "newsSeen": self.news_seen,
            "lastCheck": self.last_check,
        }))
    }

    /// Armed price alerts on this asset (the count of the asset page's button).
    pub fn armed_on(&self, symbol: &str, kind: Kind) -> usize {
        self.targets.iter().filter(|t| t.symbol == symbol && t.kind == kind && t.triggered.is_none()).count()
    }
}

/// Targets changed during a check (added, removed, re-armed by the user) are kept; only the triggers are applied.
pub fn merge_targets(current: &[PriceTarget], checked: &[PriceTarget]) -> Vec<PriceTarget> {
    let done: HashMap<&str, f64> = checked.iter().filter_map(|t| t.triggered.map(|at| (t.id.as_str(), at))).collect();
    current
        .iter()
        .map(|t| match done.get(t.id.as_str()) {
            Some(&at) if t.triggered.is_none() => PriceTarget { triggered: Some(at), ..t.clone() },
            _ => t.clone(),
        })
        .collect()
}

// ---------- Notifications (notify.ts) ----------

/// Checked every 5 minutes while an Altim tab is open, and at start.
pub const CHECK_MS: u32 = 5 * 60_000;
pub const DISCLAIMER: &str = " Conseil indicatif : Altim ne passe aucun ordre.";

/// A browser notification: title, body, tag (a newer one with the same tag replaces it), page opened on a click.
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub tag: String,
    pub open: Option<String>,
}

fn notice(title: String, body: String, tag: String, open: &str) -> Notice {
    Notice { title, body, tag, open: Some(open.to_string()) }
}

pub fn asset_path(symbol: &str, kind: Kind) -> String {
    format!("/app/actif/{}/{symbol}", kind.as_str())
}

/// Assets once each, in first-seen order (`new Map(list.map(a => [key, a])).values()`: the last duplicate's value).
pub fn uniq<T: Clone>(list: &[T], key: impl Fn(&T) -> String) -> Vec<T> {
    let mut m: IndexMap<String, T> = IndexMap::new();
    for a in list {
        m.insert(key(a), a.clone());
    }
    m.into_values().collect()
}

/// A price alert just reached: its journal line and its notification (`price_text`: the dollar price as displayed).
pub fn target_fired(t: &PriceTarget, price: f64, now: f64, price_text: &str) -> (AlertEntry, Notice) {
    let label = target_label(t, threshold_text);
    let entry = AlertEntry {
        id: format!("{}:{}", t.id, crate::js::number_to_string(now)),
        symbol: t.symbol.clone(),
        kind: t.kind,
        name: t.name.clone(),
        source: JournalSource::Target,
        title: format!("{} : {}", t.symbol, label.to_lowercase()),
        price,
        date: now,
    };
    let n = notice(
        format!("{} : alerte de prix atteinte", t.symbol),
        format!("{label}. Prix actuel {price_text}. Réarmez-la dans Alertes si besoin."),
        format!("altim.target.{}", t.id),
        &asset_path(&t.symbol, t.kind),
    );
    (entry, n)
}

/// The journal lines of fresh buy alerts (those with a price).
pub fn buy_entries(fresh: &[BuyAlert], now: f64) -> Vec<AlertEntry> {
    fresh
        .iter()
        .filter_map(|a| {
            Some(AlertEntry {
                id: format!("{}:{}:{}", a.kind.as_str(), a.symbol, crate::js::number_to_string(now)),
                symbol: a.symbol.clone(),
                kind: a.kind,
                name: a.name.clone(),
                source: if a.strong { JournalSource::StrongBuy } else { JournalSource::Buy },
                title: a.title.clone().unwrap_or_else(|| a.symbol.clone()),
                price: a.price?,
                date: now,
            })
        })
        .collect()
}

/// Up to 3 buy alerts: one notification each; beyond, a single summary (the first check can find many at once).
pub fn buy_notices(alerts: &[BuyAlert]) -> Vec<Notice> {
    if alerts.len() > 3 {
        let strong: Vec<&str> = alerts.iter().filter(|a| a.strong).map(|a| a.symbol.as_str()).collect();
        let others: Vec<&str> = alerts.iter().filter(|a| !a.strong).map(|a| a.symbol.as_str()).collect();
        let mut body = String::new();
        if !strong.is_empty() {
            body.push_str(&format!("Achat conseillé : {}. ", strong.join(", ")));
        }
        if !others.is_empty() {
            body.push_str(&format!("Achat possible : {}. ", others.join(", ")));
        }
        body.push_str("Ouvrez Altim pour le détail de chacun.");
        return vec![notice(format!("{} actifs achetables", alerts.len()), body + DISCLAIMER, "altim.summary".into(), "/app/alertes")];
    }
    alerts
        .iter()
        .map(|a| {
            notice(
                a.title.clone().unwrap_or_else(|| format!("{} : achetable", a.symbol)),
                format!("{}{DISCLAIMER}", a.body.as_deref().unwrap_or("")),
                format!("altim.{}:{}", a.kind.as_str(), a.symbol),
                &asset_path(&a.symbol, a.kind),
            )
        })
        .collect()
}

/// Important news: one notification each for 1 or 2, a summary beyond.
pub fn news_notices(fresh: &[NewsItem]) -> Vec<Notice> {
    if fresh.len() > 2 {
        let titles: Vec<&str> = fresh.iter().take(3).map(|n| n.title.as_str()).collect();
        return vec![notice(format!("{} actualités importantes", fresh.len()), titles.join(" · "), "altim.news.summary".into(), "/app/actu")];
    }
    fresh
        .iter()
        .map(|n| {
            let who: Vec<&str> = n.assets.iter().map(|a| a.rsplit(':').next().unwrap_or(a)).collect();
            let more = if n.also_in.is_empty() { String::new() } else { format!(" +{}", n.also_in.len()) };
            notice(
                if n.alert { "Alerte actualité".into() } else { format!("Actualité : {}", who.join(", ")) },
                format!("{} ({}{more})", n.title, n.source),
                format!("altim.news.{}", n.id),
                "/app/actu",
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::news::{Lang, NewsCategory, NewsTone};
    use crate::web::money::FxRate;

    fn fx() -> MoneyDisplay {
        MoneyDisplay::new(
            Currency::Eur,
            Some(FxRate { rate: 0.9, usd_per_eur: 1.0 / 0.9, time: 0.0, source: "BCE".into(), fetched_at: 0.0, stale: false }),
        )
    }
    fn usd() -> MoneyDisplay {
        MoneyDisplay::new(Currency::Usd, None)
    }
    fn target(f: impl FnOnce(&mut PriceTarget)) -> PriceTarget {
        let mut t = PriceTarget {
            id: "t".into(),
            symbol: "BTC".into(),
            kind: Kind::Crypto,
            name: "Bitcoin".into(),
            above: false,
            price: 80_000.0,
            currency: Currency::Usd,
            created: 0.0,
            triggered: None,
            move_pct: None,
        };
        f(&mut t);
        t
    }

    #[test]
    fn a_dollar_threshold_compares_the_dollar_price() {
        let d = usd();
        assert!(is_reached(&target(|_| {}), 79_999.0, &d));
        assert!(!is_reached(&target(|_| {}), 80_001.0, &d));
        assert!(is_reached(&target(|t| t.above = true), 80_000.0, &d));
    }

    #[test]
    fn a_euro_threshold_uses_the_current_rate_and_waits_without_one() {
        let t = target(|t| {
            t.currency = Currency::Eur;
            t.price = 72_000.0
        });
        assert!(!is_reached(&t, 79_000.0, &usd()));
        assert!(is_reached(&t, 79_000.0, &fx())); // 71 100 €
        assert!(!is_reached(&t, 81_000.0, &fx())); // 72 900 €
    }

    #[test]
    fn move_alerts_and_rearming_from_the_current_price() {
        let d = usd();
        let m = target(|t| {
            t.move_pct = Some(5.0);
            t.price = 100.0;
            t.symbol = "AAPL".into();
            t.kind = Kind::Stock
        });
        assert!(!is_reached(&m, 104.9, &d));
        assert!(is_reached(&m, 94.9, &d));
        let (targets, fired) = evaluate_targets(&[m], &HashMap::from([("stock:AAPL".to_string(), 106.0)]), 42.0, &d);
        assert_eq!(fired.len(), 1);
        assert_eq!(targets[0].triggered, Some(42.0));
        // Already triggered: not fired twice.
        assert!(evaluate_targets(&targets, &HashMap::from([("stock:AAPL".to_string(), 110.0)]), 43.0, &d).1.is_empty());
        let again = rearm(&targets[0], Some(106.0), &d);
        assert_eq!((again.triggered, again.price), (None, 106.0));
    }

    #[test]
    fn labels_and_statuses() {
        let t = target(|_| {});
        assert_eq!(target_label(&t, threshold_text), "En dessous de 80\u{202f}000,00 $");
        let m = target(|t| {
            t.move_pct = Some(5.0);
            t.price = 212.4;
            t.currency = Currency::Eur
        });
        assert_eq!(target_label(&m, threshold_text), "Variation de ±5 % (depuis 212,40 €)");
        assert_eq!(threshold_text(0.44, Currency::Usd), "0,4400 $");
        assert_eq!(target_status(&t, Some(100_000.0), |_| String::new()), "Prix actuel 100\u{202f}000,00 $ · encore -20 %");
        assert_eq!(target_status(&t, Some(64_000.0), |_| String::new()), "Prix actuel 64\u{202f}000,00 $ · encore +25 %");
        assert_eq!(target_status(&m, Some(223.02), |_| String::new()), "Prix actuel 223,02 € · variation 5 % sur ±5 %");
        assert_eq!(target_status(&t, None, |_| String::new()), "En attente");
        assert_eq!(target_status(&target(|t| t.triggered = Some(1.0)), None, |_| "1 oct.".into()), "Atteinte le 1 oct.");
        assert_eq!(parse_typed("80 000,5 €"), 80_000.5);
        assert_eq!(parse_typed("5 %"), 5.0);
        assert!(parse_typed("abc").is_nan());
    }

    fn alert(f: impl FnOnce(&mut BuyAlert)) -> BuyAlert {
        let mut a = BuyAlert {
            symbol: "BTC".into(),
            kind: Kind::Crypto,
            name: "Bitcoin".into(),
            price: Some(1.0),
            buy: true,
            strong: false,
            reasons: vec![],
            blockers: vec![],
            cautions: vec![],
            key: Some("zone:medium".into()),
            title: None,
            body: None,
            error: None,
        };
        f(&mut a);
        a
    }

    #[test]
    fn buy_alerts_notified_once_per_reason() {
        let (t, fresh) = new_buy_alerts(&Tracker::default(), &[alert(|_| {})], false, 0.0);
        assert_eq!(fresh.len(), 1);
        let (t, fresh) = new_buy_alerts(&t, &[alert(|_| {})], false, 1.0);
        assert!(fresh.is_empty());
        let (t, fresh) = new_buy_alerts(
            &t,
            &[alert(|a| {
                a.key = Some("signal+zone:medium".into());
                a.strong = true
            })],
            false,
            2.0,
        );
        assert_eq!(fresh.len(), 1);
        assert_eq!(t.notified["crypto:BTC"], "signal+zone:medium");
        let (t, _) = new_buy_alerts(&t, &[alert(|a| a.buy = false)], false, 10.0);
        let (t, _) = new_buy_alerts(&t, &[alert(|a| a.buy = false)], false, 10.0 + 6.0 * 3_600_000.0);
        assert!(t.notified.is_empty());
        assert!(new_buy_alerts(&Tracker::default(), &[alert(|_| {})], true, 0.0).1.is_empty());
    }

    fn news(f: impl FnOnce(&mut NewsItem)) -> NewsItem {
        let mut n = NewsItem {
            id: "n1".into(),
            title: "Invasion confirmed by officials overnight".into(),
            link: String::new(),
            time: 1_000,
            source: "Reuters".into(),
            summary: None,
            lang: Lang::En,
            category: NewsCategory::Monde,
            themes: vec![],
            tone: NewsTone::Negative,
            assets: vec![],
            also_in: vec!["AP".into()],
            alert: true,
        };
        f(&mut n);
        n
    }

    #[test]
    fn grave_escalation_twice_or_owned_asset_three_times_same_story_once() {
        let none = HashSet::new();
        let btc: HashSet<String> = HashSet::from(["crypto:BTC".to_string()]);
        let (_, fresh) = new_news(
            &[],
            &[
                news(|_| {}),
                news(|n| {
                    n.id = "n2".into();
                    n.title = "Invasion confirmed by officials overnight, markets".into()
                }),
            ],
            &none,
            2_000.0,
        );
        assert_eq!(fresh.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(), ["n1"]);
        let owned = |also: &[&str]| {
            news(|n| {
                n.alert = false;
                n.assets = vec!["crypto:BTC".into()];
                n.also_in = also.iter().map(|s| s.to_string()).collect()
            })
        };
        assert_eq!(new_news(&[], &[owned(&["A", "B"])], &btc, 2_000.0).1.len(), 1);
        assert!(new_news(&[], &[owned(&["A"])], &btc, 2_000.0).1.is_empty());
        assert!(new_news(&[], &[news(|n| n.time = 0)], &none, 7.0 * 3_600_000.0).1.is_empty());
        let (seen, _) = new_news(&[], &[news(|_| {})], &none, 2_000.0);
        assert_eq!(seen[0].words, ["confirmed", "invasion", "officials", "overnight"]);
    }

    #[test]
    fn summary_of_the_buy_alerts_older_than_an_hour() {
        let e = |f: &dyn Fn(&mut AlertEntry)| {
            let mut x = AlertEntry {
                id: "e".into(),
                symbol: "BTC".into(),
                kind: Kind::Crypto,
                name: String::new(),
                source: JournalSource::Buy,
                title: String::new(),
                price: 100.0,
                date: 0.0,
            };
            f(&mut x);
            x
        };
        let journal = [
            e(&|_| {}),
            e(&|x| {
                x.symbol = "ETH".into();
                x.price = 50.0
            }),
            e(&|x| x.source = JournalSource::Target),
            e(&|x| x.date = 3_000_000.0),
        ];
        let prices = HashMap::from([("crypto:BTC".to_string(), 110.0), ("crypto:ETH".to_string(), 45.0)]);
        let s = journal_summary(&journal, &prices, 3_600_000.0, 3_600_000.0).unwrap();
        assert_eq!((s.count, s.up, s.up_share), (2, 1, 50.0));
        assert!(s.average.abs() < 1e-9);
        assert_eq!(add_to_journal(&[journal[3].clone()], &journal[..1]).iter().map(|x| x.date).collect::<Vec<_>>(), [3_000_000.0, 0.0]);
    }

    #[test]
    fn unreadable_saved_data_is_dropped_old_targets_are_dollars() {
        assert!(AlertsState::parse(Some("{")).targets.is_empty());
        let p = AlertsState::parse(Some(
            r#"{"version":1,"targets":[{"id":"a","symbol":"BTC","kind":"crypto","price":80000},{"id":"b","symbol":"X","kind":"bond","price":1}]}"#,
        ));
        assert_eq!(p.targets.len(), 1);
        assert_eq!(p.targets[0].currency, Currency::Usd);
        assert_eq!(p.targets[0].move_pct, None);
        // Round trip, the TypeScript's key order and integers.
        let raw = r#"{"version":1,"notify":{"buy":true,"strongOnly":false,"news":true},"targets":[{"id":"a","symbol":"BTC","kind":"crypto","name":"Bitcoin","above":true,"currency":"EUR","created":1790722229334,"triggered":null,"price":72000.5,"move":null}],"journal":[{"id":"j","symbol":"ETH","kind":"crypto","name":"Ethereum","source":"strongBuy","title":"ETH : achat conseillé","price":2500,"date":1790722229334}],"tracker":{"notified":{"crypto:ETH":"signal+zone:medium"},"lost":{"crypto:SOL":1790722229334}},"newsSeen":[{"id":"n","words":["invasion"],"time":1790722229334}],"lastCheck":1790722229334}"#;
        let s = AlertsState::parse(Some(raw));
        assert_eq!(s.to_json().to_string(), raw);
        assert_eq!(s.armed_on("BTC", Kind::Crypto), 1);
        assert_eq!(AlertsState::parse(Some(r#"{"version":2}"#)), AlertsState::default());
    }

    #[test]
    fn checks_merge_targets_and_notices() {
        let t = target(|_| {});
        let fired = PriceTarget { triggered: Some(5.0), ..t.clone() };
        let added = target(|x| x.id = "new".into());
        assert_eq!(merge_targets(&[t.clone(), added.clone()], std::slice::from_ref(&fired)), [fired.clone(), added]);
        // Removed by the user during the check: stays removed.
        assert!(merge_targets(&[], std::slice::from_ref(&fired)).is_empty());
        let (entry, n) = target_fired(&fired, 79_000.0, 5.0, "79\u{a0}000,00\u{a0}$");
        assert_eq!(entry.title, "BTC : en dessous de 80\u{202f}000,00 $");
        assert_eq!((entry.id.as_str(), entry.source), ("t:5", JournalSource::Target));
        assert_eq!(n.title, "BTC : alerte de prix atteinte");
        assert_eq!(n.body, "En dessous de 80\u{202f}000,00 $. Prix actuel 79\u{a0}000,00\u{a0}$. Réarmez-la dans Alertes si besoin.");
        assert_eq!(n.open.as_deref(), Some("/app/actif/crypto/BTC"));
        let many: Vec<BuyAlert> = ["A", "B", "C", "D"]
            .iter()
            .map(|s| {
                alert(|a| {
                    a.symbol = s.to_string();
                    a.strong = *s == "A"
                })
            })
            .collect();
        let sum = buy_notices(&many);
        assert_eq!(sum.len(), 1);
        assert_eq!(sum[0].body, format!("Achat conseillé : A. Achat possible : B, C, D. Ouvrez Altim pour le détail de chacun.{DISCLAIMER}"));
        let one = buy_notices(&many[..1]);
        assert_eq!((one[0].title.as_str(), one[0].tag.as_str()), ("A : achetable", "altim.crypto:A"));
        assert_eq!(buy_entries(&[alert(|a| a.price = None), alert(|a| a.strong = true)], 7.0)[0].source, JournalSource::StrongBuy);
        let n2 = news_notices(&[news(|n| {
            n.alert = false;
            n.assets = vec!["stock:AAPL".into(), "crypto:BTC".into()]
        })]);
        assert_eq!((n2[0].title.as_str(), n2[0].body.as_str()), ("Actualité : AAPL, BTC", "Invasion confirmed by officials overnight (Reuters +1)"));
        assert_eq!(news_notices(&[news(|_| {}), news(|_| {}), news(|_| {})])[0].title, "3 actualités importantes");
        let u = uniq(&[("A", 1), ("B", 2), ("A", 3)], |x| x.0.to_string());
        assert_eq!(u, [("A", 3), ("B", 2)]);
    }
}
