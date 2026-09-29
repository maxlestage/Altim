//! Practical guidance built on a decision (`/api/decision`), all derived from what the decision engine already
//! measured (vetoes, plan, structure, families, events); none of it changes the verdict:
//! - `noTrade`: "quand ne PAS trader", the reasons that make now a bad moment whatever the verdict (extreme
//!   volatility, thin liquidity, abnormal spread, earnings just before or after, imminent announcement, trendless
//!   market, weak signal, degraded signal, market closed);
//! - `actionZones`: the plan as a price ladder (invalidation, exit, buy, wait, profit-taking) and where the price is;
//! - scenario conditions and `unfolding`: each scenario's conditions checked on the current data, and the one whose
//!   conditions are the most met;
//! - `counterArgument`: favourable vs unfavourable reasons and what could invalidate the scenario;
//! - `snapshot`: compact numbers kept by the clients to explain a later change of the signal.
use serde::{Deserialize, Serialize};

use super::decision_types::{Degraded, EarningsDate, Family, Plan, Scenario, ScenarioKind, Status, Veto};
use super::structure::{SrLevel, SwingTrend};
use crate::calendar::{CalendarEvent, EventKind, Importance};
use crate::js::{fr, iso_date};
use crate::types::{DAY_MS, Kind};

// ---------- Thresholds ----------

/// Earnings within this many days ahead: not the moment (same limit as the earnings veto).
pub const EARNINGS_BEFORE_DAYS: f64 = 5.0;
/// Sessions after earnings during which the market is still digesting them.
pub const EARNINGS_AFTER_SESSIONS: usize = 2;
/// ADX under it with a swing structure without direction: trendless market.
pub const TRENDLESS_ADX: f64 = 20.0;
/// Composite score within ± this and confidence under `WEAK_CONFIDENCE`: signal too weak (the "Neutre" band).
pub const WEAK_SCORE: f64 = 15.0;
pub const WEAK_CONFIDENCE: f64 = 50.0;
/// Half-width of the exit zone around the stop, in daily ATR.
pub const EXIT_ATR: f64 = 0.5;
/// Volume from which buyers are back (non-bullish counter-argument), times the 20-day average.
pub const BREAKOUT_VOLUME: f64 = 1.5;

fn usd(v: f64) -> String {
    crate::fx::money(v)
}
/// "29/10/2026".
fn date_fr(ms: i64) -> String {
    let d = iso_date(ms);
    format!("{}/{}/{}", &d[8..10], &d[5..7], &d[0..4])
}
fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n > 1 { many } else { one })
}
/// Volumes: "12,3 M", "850 k".
fn compact(v: f64) -> String {
    if v >= 1e9 {
        format!("{} Md", fr(v / 1e9, 0, 1))
    } else if v >= 1e6 {
        format!("{} M", fr(v / 1e6, 0, 1))
    } else if v >= 1e4 {
        format!("{} k", fr(v / 1e3, 0, 0))
    } else {
        fr(v, 0, 2)
    }
}

// ---------- Quand ne PAS trader ----------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoTradeReason {
    /// "volatility" | "liquidity" | "spread" | "earnings" | "announcement" | "event" | "trendless" | "weakSignal" |
    /// "degraded" | "marketClosed"
    pub code: String,
    pub label: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoTrade {
    /// At least one reason.
    pub active: bool,
    /// "🕰️ Pas le moment de trader : …" when active, empty otherwise.
    pub headline: String,
    pub reasons: Vec<NoTradeReason>,
    /// Checks that could not be made for lack of data ("Écart achat/vente trop grand : non vérifiable").
    pub unchecked: Vec<String>,
}

pub struct NoTradeInput<'a> {
    pub kind: Kind,
    pub now: i64,
    pub vetoes: &'a [Veto],
    /// Next (or last, when already past) earnings date given by the fundamentals.
    pub earnings: Option<&'a EarningsDate>,
    /// Earnings dates of this stock in the calendar (ms): used when the fundamentals give none.
    pub calendar_earnings: Vec<i64>,
    /// Daily ADX 14.
    pub adx: Option<f64>,
    /// Swing structure of the daily candles; None when not measured.
    pub swing: Option<SwingTrend>,
    /// The guard's regime sees no trend (used when the swing structure is not measured).
    pub range: bool,
    pub score: Option<f64>,
    pub confidence: f64,
    pub degraded: &'a Degraded,
    /// Stocks: US regular session open now; None for a crypto (always open).
    pub market_open: Option<bool>,
}

/// Monday = 0 … Sunday = 6 of a UTC day number (1970-01-01 was a Thursday).
fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

/// Weekdays strictly after the day of `from` up to the day of `to` (UTC days): the sessions since an earnings day.
pub fn sessions_between(from: i64, to: i64) -> usize {
    let (a, b) = (from.div_euclid(DAY_MS), to.div_euclid(DAY_MS));
    (a + 1..=b).filter(|d| weekday(*d) < 5).count()
}

fn earnings_reason(inp: &NoTradeInput) -> Result<Option<NoTradeReason>, String> {
    if inp.kind != Kind::Stock {
        return Ok(None);
    }
    let (date, estimated) = match (inp.earnings, inp.calendar_earnings.iter().filter(|d| **d >= inp.now - DAY_MS).min()) {
        (Some(e), _) => (e.date, e.estimated),
        (None, Some(d)) => (*d, false),
        (None, None) => return Err("Résultats : date indisponible".into()),
    };
    let est = if estimated { " (date estimée)" } else { "" };
    let days = ((date - inp.now) as f64 / DAY_MS as f64).ceil();
    if (0.0..=EARNINGS_BEFORE_DAYS).contains(&days) {
        return Ok(Some(NoTradeReason {
            code: "earnings".into(),
            label: "Résultats imminents".into(),
            detail: format!(
                "Résultats le {}{est}, dans {} jour(s) (seuil {}) : le cours peut sauter dans un sens ou dans l'autre à la publication",
                date_fr(date),
                fr(days, 0, 0),
                fr(EARNINGS_BEFORE_DAYS, 0, 0)
            ),
        }));
    }
    if date < inp.now {
        let s = sessions_between(date, inp.now);
        if s <= EARNINGS_AFTER_SESSIONS {
            return Ok(Some(NoTradeReason {
                code: "earnings".into(),
                label: "Lendemain de résultats".into(),
                detail: format!(
                    "Résultats publiés le {}{est}, {} : le marché les digère encore (1 à {} séances), les écarts de prix restent larges",
                    date_fr(date),
                    if s == 0 { "séance en cours".to_string() } else { format!("il y a {}", plural(s, "séance", "séances")) },
                    EARNINGS_AFTER_SESSIONS
                ),
            }));
        }
    }
    Ok(None)
}

/// Every reason comes from a measurement already made (vetoes, ADX, swing structure, composite score, confidence,
/// degraded signal, earnings date, US session); a check without data goes to `unchecked`, never counted.
pub fn no_trade(inp: &NoTradeInput) -> NoTrade {
    let mut reasons = vec![];
    let mut unchecked = vec![];
    let from_veto = |code: &str, label: &str, reasons: &mut Vec<NoTradeReason>, unchecked: &mut Vec<String>| {
        if let Some(v) = inp.vetoes.iter().find(|v| v.code == code) {
            if v.active {
                reasons.push(NoTradeReason { code: code.into(), label: label.into(), detail: v.detail.clone() });
            } else if !v.verifiable {
                unchecked.push(format!("{} : non vérifiable", v.label));
            }
        }
    };
    from_veto("volatility", "Volatilité extrême", &mut reasons, &mut unchecked);
    from_veto("liquidity", "Liquidité insuffisante", &mut reasons, &mut unchecked);
    from_veto("spread", "Écart achat/vente anormal", &mut reasons, &mut unchecked);
    match earnings_reason(inp) {
        Ok(Some(r)) => reasons.push(r),
        Ok(None) => {}
        Err(u) => unchecked.push(u),
    }
    from_veto("announcement", "Annonce économique imminente", &mut reasons, &mut unchecked);
    from_veto("event", "Événement majeur", &mut reasons, &mut unchecked);
    // Trendless: weak ADX and swings without direction (the guard's regime when the swings are not measured).
    match inp.adx {
        Some(a) if a < TRENDLESS_ADX => {
            let flat = match inp.swing {
                Some(s) => s == SwingTrend::Mixed,
                None => inp.range,
            };
            if flat {
                reasons.push(NoTradeReason {
                    code: "trendless".into(),
                    label: "Marché sans direction".into(),
                    detail: format!(
                        "ADX {} (sous {}) et {} : les signaux de tendance y sont moins fiables",
                        fr(a, 0, 0),
                        fr(TRENDLESS_ADX, 0, 0),
                        if inp.swing.is_some() { "sommets et creux sans direction" } else { "pas de tendance de fond nette" }
                    ),
                });
            }
        }
        Some(_) => {}
        None => unchecked.push("Direction du marché : ADX non mesuré (historique trop court)".into()),
    }
    match inp.score {
        Some(s) if s.abs() < WEAK_SCORE && inp.confidence < WEAK_CONFIDENCE => reasons.push(NoTradeReason {
            code: "weakSignal".into(),
            label: "Signal trop faible".into(),
            detail: format!(
                "Score composite {}{} (entre −{w} et +{w}) et confiance {}/100 (sous {})",
                if s > 0.0 {
                    "+"
                } else if s < 0.0 {
                    "−"
                } else {
                    ""
                },
                fr(s.abs(), 0, 0),
                fr(inp.confidence, 0, 0),
                fr(WEAK_CONFIDENCE, 0, 0),
                w = fr(WEAK_SCORE, 0, 0)
            ),
        }),
        Some(_) => {}
        None => unchecked.push("Force du signal : score composite non calculé".into()),
    }
    if inp.degraded.active {
        reasons.push(NoTradeReason { code: "degraded".into(), label: "Signal dégradé".into(), detail: inp.degraded.reasons.join(" ; ") });
    }
    if inp.market_open == Some(false) {
        reasons.push(NoTradeReason {
            code: "marketClosed".into(),
            label: "Marché fermé".into(),
            detail: "Hors séance régulière de Wall Street (9 h 30 – 16 h, heure de New York, du lundi au vendredi ; jours fériés non connus) : un ordre attend l'ouverture, qui peut se faire loin du dernier cours".into(),
        });
    }
    let headline = if reasons.is_empty() {
        String::new()
    } else {
        let names: Vec<String> = reasons.iter().take(3).map(|r| r.label.to_lowercase()).collect();
        let more = if reasons.len() > 3 { format!(" (+{} autre(s))", reasons.len() - 3) } else { String::new() };
        format!("🕰️ Pas le moment de trader : {}{more}.", names.join(", "))
    };
    NoTrade { active: !reasons.is_empty(), headline, reasons, unchecked }
}

// ---------- Zones d'action ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionZone {
    /// "invalidation" | "exit" | "buy" | "wait" | "profit"
    pub kind: String,
    pub label: String,
    /// USD, from ≤ to (equal for a single level: the invalidation, a lone target).
    pub from: f64,
    pub to: f64,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionZones {
    pub price: f64,
    /// Ascending by price (`from`, then `to`).
    pub zones: Vec<ActionZone>,
    /// Kind of the zone the price is in; None between zones, under the invalidation or above the targets.
    pub here: Option<String>,
    /// "Vous êtes ici : zone d'attente (342,10 $)".
    pub here_text: String,
}

/// The plan as a ladder: invalidation level (the stop), exit zone (± ½ daily ATR around the stop, under the buy
/// zone), buy zone, wait zone (top of the buy zone → target 1: no entry, the risk/reward is too thin), profit-taking
/// zone (target 1 → the last target).
pub fn action_zones(plan: &Plan, price: f64, atr: Option<f64>, resistance: Option<&SrLevel>) -> ActionZones {
    let stop = plan.stop;
    let (exit_from, exit_to) = match atr.filter(|a| *a > 0.0) {
        Some(a) => ((stop - EXIT_ATR * a).max(0.0), (stop + EXIT_ATR * a).min(plan.zone_from).max(stop)),
        None => (stop, plan.zone_from.max(stop)),
    };
    let last = plan.target3.or(plan.target2).unwrap_or(plan.target1).max(plan.target1);
    let targets: Vec<String> = [Some(plan.target1), plan.target2, plan.target3]
        .iter()
        .flatten()
        .enumerate()
        .map(|(i, t)| format!("objectif {} {}", i + 1, usd(*t)))
        .collect();
    let wait_note = match resistance.filter(|r| r.price > plan.zone_to && r.price < plan.target1) {
        Some(r) => format!(
            "Au-dessus de la zone d'achat : pas d'entrée, attendre un repli ; résistance à {} (touchée {} fois) avant l'objectif 1",
            usd(r.price),
            r.touches
        ),
        None => "Au-dessus de la zone d'achat : pas d'entrée, attendre un repli".into(),
    };
    let zones = vec![
        ActionZone {
            kind: "invalidation".into(),
            label: "Niveau d'invalidation".into(),
            from: stop,
            to: stop,
            note: format!("Clôture sous {} (stop du plan) : scénario invalidé", usd(stop)),
        },
        ActionZone {
            kind: "exit".into(),
            label: "Zone de sortie".into(),
            from: exit_from,
            to: exit_to,
            note: if atr.is_some() {
                format!("Autour du stop (± ½ ATR journalier) : sortir à la clôture sous {}, ne pas renforcer", usd(stop))
            } else {
                format!("Sous la zone d'achat : repli plus profond que prévu, sortir à la clôture sous {}", usd(stop))
            },
        },
        ActionZone {
            kind: "buy".into(),
            label: "Zone d'achat".into(),
            from: plan.zone_from,
            to: plan.zone_to,
            note: format!("Repli attendu ici : entrer après une confirmation, stop à {}", usd(stop)),
        },
        ActionZone { kind: "wait".into(), label: "Zone d'attente".into(), from: plan.zone_to, to: plan.target1, note: wait_note },
        ActionZone {
            kind: "profit".into(),
            label: "Zone de prise de bénéfices".into(),
            from: plan.target1,
            to: last,
            note: format!("{} : alléger par étapes", cap(&targets.join(", "))),
        },
    ];
    // The price's zone: the highest one containing it (a shared boundary belongs to the upper zone).
    let here = zones.iter().rev().filter(|z| z.kind != "invalidation").find(|z| price >= z.from && price <= z.to).map(|z| z.kind.clone());
    let at = usd(price);
    let here_text = match &here {
        Some(k) => {
            let z = zones.iter().find(|z| &z.kind == k).expect("zone");
            format!("Vous êtes ici : {} ({at})", z.label.to_lowercase())
        }
        None if price < stop => format!("Vous êtes ici : sous le niveau d'invalidation {} ({at})", usd(stop)),
        None if price > last => format!("Vous êtes ici : au-dessus des objectifs ({at})"),
        None if price < exit_from => format!("Vous êtes ici : sous la zone de sortie ({at})"),
        None => format!("Vous êtes ici : entre la zone de sortie et la zone d'achat ({at})"),
    };
    let mut zones = zones;
    zones.sort_by(|a, b| a.from.total_cmp(&b.from).then(a.to.total_cmp(&b.to)));
    ActionZones { price, zones, here, here_text }
}

// ---------- Scénarios surveillés ----------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckState {
    Met,
    Unmet,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioCheck {
    pub text: String,
    pub state: CheckState,
    /// The value read ("Dernière clôture 342,10 $").
    pub detail: String,
}

/// What the scenario conditions are checked on (daily candles, closed).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Observed {
    pub price: f64,
    pub last_close: Option<f64>,
    pub rsi: Option<f64>,
    pub adx: Option<f64>,
    /// Last daily volume ÷ average of the 20 before it.
    pub volume_ratio: Option<f64>,
    /// Last daily candle closed above the previous close.
    pub up_day: Option<bool>,
}

fn check(text: String, state: Option<bool>, detail: String) -> ScenarioCheck {
    let state = match state {
        Some(true) => CheckState::Met,
        Some(false) => CheckState::Unmet,
        None => CheckState::Unknown,
    };
    ScenarioCheck { text, state, detail }
}

fn volume_check(o: &Observed, up: bool) -> ScenarioCheck {
    let text = format!("Séance en {} avec un volume au-dessus de la moyenne 20 j", if up { "hausse" } else { "baisse" });
    match (o.volume_ratio, o.up_day) {
        (Some(r), Some(u)) => check(
            text,
            Some(r > 1.0 && u == up),
            format!("Volume {} × la moyenne, dernière séance en {}", fr(r, 1, 1), if u { "hausse" } else { "baisse" }),
        ),
        _ => check(text, None, "Volume indisponible sur les bougies".into()),
    }
}

fn rsi_check(o: &Observed, above: bool) -> ScenarioCheck {
    let text = format!("RSI {} 50", if above { "au-dessus de" } else { "sous" });
    match o.rsi {
        Some(r) => check(text, Some(if above { r > 50.0 } else { r < 50.0 }), format!("RSI 14 j : {}", fr(r, 0, 0))),
        None => check(text, None, "RSI non mesuré (historique trop court)".into()),
    }
}

fn close_detail(o: &Observed) -> String {
    o.last_close.map(|c| format!("Dernière clôture journalière {}", usd(c))).unwrap_or_else(|| "Aucune clôture journalière".into())
}

/// Conditions of a scenario between `low` (the support / stop) and `high` (the level to break): 3 for the bullish
/// one (close above `high`, up day on volume, RSI above 50), 2 for the neutral one (price between the two, ADX under
/// 20), 3 for the bearish one (close under `low`, down day on volume, RSI under 50).
pub fn scenario_checks(kind: ScenarioKind, low: f64, high: f64, o: &Observed) -> Vec<ScenarioCheck> {
    match kind {
        ScenarioKind::Bull => vec![
            check(format!("Clôture au-dessus de {}", usd(high)), o.last_close.map(|c| c > high), close_detail(o)),
            volume_check(o, true),
            rsi_check(o, true),
        ],
        ScenarioKind::Neutral => vec![
            check(
                format!("Prix entre {} et {}", usd(low), usd(high)),
                (o.price > 0.0).then_some(o.price >= low && o.price <= high),
                format!("Prix {}", usd(o.price)),
            ),
            match o.adx {
                Some(a) => check(
                    format!("ADX sous {} (pas de tendance marquée)", fr(TRENDLESS_ADX, 0, 0)),
                    Some(a < TRENDLESS_ADX),
                    format!("ADX 14 j : {}", fr(a, 0, 0)),
                ),
                None => check(format!("ADX sous {} (pas de tendance marquée)", fr(TRENDLESS_ADX, 0, 0)), None, "ADX non mesuré".into()),
            },
        ],
        ScenarioKind::Bear => vec![
            check(format!("Clôture sous {} (cassure du support)", usd(low)), o.last_close.map(|c| c < low), close_detail(o)),
            volume_check(o, false),
            rsi_check(o, false),
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unfolding {
    pub kind: ScenarioKind,
    pub title: String,
    pub met: usize,
    pub total: usize,
    pub text: String,
}

fn met_of(s: &Scenario) -> usize {
    s.conditions.iter().filter(|c| c.state == CheckState::Met).count()
}

/// The scenario being realised: the highest share of its conditions met; a tie at the top, or nothing met, gives
/// the neutral one. Also sets each scenario's `met` and `unfolding`.
pub fn unfolding(scenarios: &mut [Scenario]) -> Option<Unfolding> {
    for s in scenarios.iter_mut() {
        s.met = met_of(s);
    }
    let share = |s: &Scenario| if s.conditions.is_empty() { 0.0 } else { s.met as f64 / s.conditions.len() as f64 };
    let best = scenarios.iter().map(share).fold(0.0, f64::max);
    let top: Vec<ScenarioKind> = scenarios.iter().filter(|s| best > 0.0 && (share(s) - best).abs() < 1e-9).map(|s| s.kind).collect();
    let kind = if top.len() == 1 { top[0] } else { ScenarioKind::Neutral };
    let i = scenarios.iter().position(|s| s.kind == kind)?;
    scenarios[i].unfolding = true;
    let s = &scenarios[i];
    let total = s.conditions.len();
    let why = if best == 0.0 {
        " (aucune condition nettement remplie : neutre par défaut)"
    } else if top.len() > 1 {
        " (égalité entre scénarios : neutre par défaut)"
    } else {
        ""
    };
    let text = format!(
        "{} en cours : {} sur {} remplie{}{why}. Lecture des données actuelles, pas une prévision.",
        s.title,
        plural(s.met, "condition", "conditions"),
        total,
        if s.met > 1 { "s" } else { "" }
    );
    Some(Unfolding { kind, title: s.title.clone(), met: s.met, total, text })
}

// ---------- Contre-argument ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invalidator {
    /// "level" (price, USD) | "volume" (daily volume in units of the asset) | "event" | "condition"
    pub kind: String,
    pub text: String,
    /// The threshold: a price for "level", a daily volume for "volume"; None otherwise.
    pub value: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterArgument {
    /// Reasons listed in `pros` / `cons` (the placeholders "Aucun …" not counted).
    pub favourable: usize,
    pub unfavourable: usize,
    /// "🟢 Raisons favorables : 7 / 🔴 Raisons défavorables : 5"
    pub text: String,
    /// "Familles : 4 favorables, 3 défavorables, 2 neutres, 2 non mesurées"
    pub families_text: String,
    /// "Points qui pourraient invalider le scénario".
    pub invalidators: Vec<Invalidator>,
}

pub struct CounterInput<'a> {
    pub pros: &'a [String],
    pub cons: &'a [String],
    pub families: &'a [Family],
    /// `whyNot.invalidation`.
    pub invalidation: &'a [String],
    /// The decision bets on the asset (buy, buy zone, or a position kept).
    pub bullish: bool,
    pub support: Option<&'a SrLevel>,
    pub resistance: Option<&'a SrLevel>,
    /// Average daily volume of the 20 sessions before the last one, and the last one ÷ that average.
    pub avg_volume: Option<f64>,
    pub volume_ratio: Option<f64>,
    /// "actions", "BTC"…
    pub volume_unit: &'a str,
    /// Events of the decision (next 7 days), already formatted with their text.
    pub events: Vec<String>,
}

pub fn counter_argument(inp: &CounterInput) -> CounterArgument {
    let favourable = inp.pros.iter().filter(|p| !p.starts_with("Aucun ")).count();
    let unfavourable = inp.cons.iter().filter(|c| !c.starts_with("Aucun ")).count();
    let n = |s: Status| inp.families.iter().filter(|f| f.status == s).count();
    let families_text = format!(
        "Familles : {} favorable(s), {} défavorable(s), {} neutre(s), {} non mesurée(s)",
        n(Status::Positive),
        n(Status::Negative),
        n(Status::Neutral),
        n(Status::Unavailable)
    );
    let mut out: Vec<Invalidator> = vec![];
    // Structure first: the level whose break would prove the scenario wrong.
    if inp.bullish {
        if let Some(s) = inp.support {
            out.push(Invalidator {
                kind: "level".into(),
                text: format!("Cassure du support {} (touché {} fois)", usd(s.price), s.touches),
                value: Some(s.price),
            });
        }
    } else if let Some(r) = inp.resistance {
        out.push(Invalidator {
            kind: "level".into(),
            text: format!("Cassure de la résistance {} (touchée {} fois) : l'attente serait dépassée", usd(r.price), r.touches),
            value: Some(r.price),
        });
    }
    for line in inp.invalidation {
        out.push(Invalidator { kind: "condition".into(), text: line.clone(), value: None });
    }
    if let Some(avg) = inp.avg_volume.filter(|v| *v > 0.0) {
        let now = inp.volume_ratio.map(|r| format!(" ; dernière séance : {} × la moyenne", fr(r, 1, 1))).unwrap_or_default();
        out.push(if inp.bullish {
            Invalidator {
                kind: "volume".into(),
                text: format!(
                    "Volume qui retombe sous sa moyenne 20 j ({} {} par jour{now}) : une hausse sans volume peut s'essouffler",
                    compact(avg),
                    inp.volume_unit
                ),
                value: Some(avg),
            }
        } else {
            Invalidator {
                kind: "volume".into(),
                text: format!(
                    "Volume au-dessus de {} × la moyenne 20 j ({} {} par jour{now}) sur une séance en hausse : les acheteurs pourraient revenir",
                    fr(BREAKOUT_VOLUME, 1, 1),
                    compact(avg * BREAKOUT_VOLUME),
                    inp.volume_unit
                ),
                value: Some(avg * BREAKOUT_VOLUME),
            }
        });
    }
    for e in inp.events.iter().take(3) {
        out.push(Invalidator { kind: "event".into(), text: format!("Événement : {e}"), value: None });
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|i| seen.insert(i.text.clone()));
    CounterArgument {
        favourable,
        unfavourable,
        text: format!("🟢 Raisons favorables : {favourable} / 🔴 Raisons défavorables : {unfavourable}"),
        families_text,
        invalidators: out,
    }
}

/// Events worth citing as a counter-argument: high importance, or this stock's own earnings.
pub fn counter_event(e: &CalendarEvent, symbol: &str) -> bool {
    e.importance == Importance::High || (e.kind == EventKind::Earnings && e.symbol.as_deref() == Some(symbol))
}

// ---------- Snapshot ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotLevel {
    pub price: f64,
    pub touches: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFamily {
    pub key: String,
    pub label: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotNews {
    pub title: String,
    /// "positive" | "negative" | "neutral" (keyword tone of the headline, as in the news family).
    pub tone: String,
    pub time: i64,
}

/// Compact numbers of the decision, kept by the clients to explain why a later signal differs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionSnapshot {
    pub price: Option<f64>,
    /// Composite score −100 … +100.
    pub composite: Option<f64>,
    /// Every family's score (−100 … +100, None when unavailable).
    pub families: Vec<SnapshotFamily>,
    /// Momentum family score.
    pub momentum: Option<f64>,
    /// Last daily volume ÷ average of the 20 before it.
    pub relative_volume: Option<f64>,
    pub rsi: Option<f64>,
    pub adx: Option<f64>,
    pub nearest_support: Option<SnapshotLevel>,
    pub nearest_resistance: Option<SnapshotLevel>,
    /// News family score.
    pub news_score: Option<f64>,
    /// Latest headline of 24 h naming the asset (a toned one first); None without any.
    pub top_news: Option<SnapshotNews>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::structure::LevelKind;

    fn veto(code: &str, active: bool, verifiable: bool) -> Veto {
        Veto { code: code.into(), label: format!("Veto {code}"), active, verifiable, detail: format!("détail {code}") }
    }

    fn base<'a>(vetoes: &'a [Veto], degraded: &'a Degraded) -> NoTradeInput<'a> {
        NoTradeInput {
            kind: Kind::Crypto,
            now: 1_790_000_000_000,
            vetoes,
            earnings: None,
            calendar_earnings: vec![],
            adx: Some(30.0),
            swing: Some(SwingTrend::Up),
            range: false,
            score: Some(40.0),
            confidence: 60.0,
            degraded,
            market_open: None,
        }
    }

    #[test]
    fn no_trade_is_quiet_when_nothing_is_wrong() {
        let v = [veto("volatility", false, true), veto("spread", false, false)];
        let d = Degraded::default();
        let n = no_trade(&base(&v, &d));
        assert!(!n.active && n.headline.is_empty() && n.reasons.is_empty());
        assert_eq!(n.unchecked, ["Veto spread : non vérifiable"]);
    }

    #[test]
    fn no_trade_gathers_every_reason() {
        let v = [veto("volatility", true, true), veto("liquidity", true, true), veto("spread", true, true), veto("announcement", true, true)];
        let d = Degraded { active: true, headline: "x".into(), reasons: vec!["Données peu fiables".into()] };
        let mut i = base(&v, &d);
        i.adx = Some(14.0);
        i.swing = Some(SwingTrend::Mixed);
        i.score = Some(-6.0);
        i.confidence = 40.0;
        i.market_open = Some(false);
        i.kind = Kind::Stock;
        let e = EarningsDate { date: i.now + 2 * DAY_MS, estimated: true };
        i.earnings = Some(&e);
        let n = no_trade(&i);
        let codes: Vec<&str> = n.reasons.iter().map(|r| r.code.as_str()).collect();
        assert_eq!(codes, ["volatility", "liquidity", "spread", "earnings", "announcement", "trendless", "weakSignal", "degraded", "marketClosed"]);
        assert!(n.headline.starts_with("🕰️ Pas le moment de trader : volatilité extrême, liquidité insuffisante"), "{}", n.headline);
        assert!(n.headline.ends_with("(+6 autre(s))."), "{}", n.headline);
        let r = |c: &str| n.reasons.iter().find(|r| r.code == c).unwrap();
        assert!(r("earnings").detail.contains("dans 2 jour(s)") && r("earnings").detail.contains("date estimée"), "{:?}", r("earnings"));
        assert!(r("trendless").detail.starts_with("ADX 14 (sous 20)"), "{:?}", r("trendless"));
        assert_eq!(r("weakSignal").detail, "Score composite −6 (entre −15 et +15) et confiance 40/100 (sous 50)");
        assert_eq!(r("degraded").detail, "Données peu fiables");
    }

    #[test]
    fn earnings_before_and_after() {
        let v: [Veto; 0] = [];
        let d = Degraded::default();
        // Monday 2026-09-28 15:00 UTC.
        let now = 1_790_607_600_000;
        assert_eq!(weekday(now / DAY_MS), 0);
        let mut i = base(&v, &d);
        i.kind = Kind::Stock;
        i.now = now;
        let code = |i: &NoTradeInput| no_trade(i).reasons.into_iter().find(|r| r.code == "earnings").map(|r| r.label);
        // Unknown date: not a reason, said as unchecked.
        assert_eq!(code(&i), None);
        assert!(no_trade(&i).unchecked.contains(&"Résultats : date indisponible".to_string()));
        // Friday 2026-09-25 (bare date): 1 session since (Monday) → still digesting.
        let fri = EarningsDate { date: (now / DAY_MS - 3) * DAY_MS, estimated: false };
        i.earnings = Some(&fri);
        assert_eq!(code(&i).as_deref(), Some("Lendemain de résultats"));
        assert_eq!(sessions_between(fri.date, now), 1);
        // Tuesday 2026-09-22: Wed, Thu, Fri, Mon = 4 sessions → over.
        let tue = EarningsDate { date: (now / DAY_MS - 6) * DAY_MS, estimated: false };
        i.earnings = Some(&tue);
        assert_eq!(code(&i), None);
        // In 6 days: beyond the 5-day window.
        let later = EarningsDate { date: now + 6 * DAY_MS, estimated: false };
        i.earnings = Some(&later);
        assert_eq!(code(&i), None);
        // Calendar fallback.
        i.earnings = None;
        i.calendar_earnings = vec![now + DAY_MS];
        assert_eq!(code(&i).as_deref(), Some("Résultats imminents"));
        // Crypto: not applicable.
        i.kind = Kind::Crypto;
        assert!(no_trade(&i).reasons.is_empty() && no_trade(&i).unchecked.is_empty());
    }

    #[test]
    fn trendless_needs_flat_swings() {
        let v: [Veto; 0] = [];
        let d = Degraded::default();
        let mut i = base(&v, &d);
        i.adx = Some(15.0);
        assert!(!no_trade(&i).active, "ADX low but swings still rising");
        i.swing = None;
        i.range = true;
        assert_eq!(no_trade(&i).reasons[0].code, "trendless");
        i.adx = None;
        assert!(no_trade(&i).unchecked[0].starts_with("Direction du marché"));
    }

    fn plan() -> Plan {
        Plan {
            zone_from: 300.0,
            zone_to: 320.0,
            entry: 320.0,
            stop: 280.0,
            target1: 360.0,
            target2: Some(380.0),
            risk_pct: 12.5,
            reward1_pct: 12.5,
            reward2_pct: Some(18.8),
            risk_reward: 1.0,
            min_risk_reward: 2.0,
            acceptable: false,
            horizon: "moyen terme".into(),
            target3: Some(395.0),
            reward3_pct: Some(23.4),
            target3_source: Some("x".into()),
        }
    }

    #[test]
    fn zones_form_an_ascending_ladder() {
        let r = SrLevel { price: 345.0, touches: 3, kind: LevelKind::Resistance, distance_pct: 1.0 };
        let z = action_zones(&plan(), 342.1, Some(10.0), Some(&r));
        let kinds: Vec<&str> = z.zones.iter().map(|z| z.kind.as_str()).collect();
        assert_eq!(kinds, ["exit", "invalidation", "buy", "wait", "profit"]);
        let get = |k: &str| z.zones.iter().find(|z| z.kind == k).unwrap();
        assert_eq!((get("exit").from, get("exit").to), (275.0, 285.0));
        assert_eq!((get("invalidation").from, get("invalidation").to), (280.0, 280.0));
        assert_eq!((get("wait").from, get("wait").to), (320.0, 360.0));
        assert_eq!((get("profit").from, get("profit").to), (360.0, 395.0));
        assert!(get("wait").note.contains("résistance à 345,00 $ (touchée 3 fois)"), "{}", get("wait").note);
        assert!(get("profit").note.starts_with("Objectif 1 360,00 $, objectif 2 380,00 $, objectif 3 395,00 $"), "{}", get("profit").note);
        assert_eq!(z.here.as_deref(), Some("wait"));
        assert_eq!(z.here_text, "Vous êtes ici : zone d'attente (342,10 $)");
        for w in z.zones.windows(2) {
            assert!(w[0].from <= w[1].from && w[0].from <= w[0].to);
        }
        assert_eq!(action_zones(&plan(), 320.0, Some(10.0), None).here.as_deref(), Some("wait"), "a shared boundary goes up");
        assert_eq!(action_zones(&plan(), 310.0, Some(10.0), None).here.as_deref(), Some("buy"));
        assert_eq!(action_zones(&plan(), 283.0, Some(10.0), None).here.as_deref(), Some("exit"));
        let between = action_zones(&plan(), 292.0, Some(10.0), None);
        assert_eq!((between.here, between.here_text.as_str()), (None, "Vous êtes ici : entre la zone de sortie et la zone d'achat (292,00 $)"));
        assert!(action_zones(&plan(), 270.0, Some(10.0), None).here_text.contains("sous le niveau d'invalidation"));
        assert!(action_zones(&plan(), 400.0, None, None).here_text.contains("au-dessus des objectifs"));
        // Without ATR: the exit zone runs from the stop to the buy zone.
        let z = action_zones(&plan(), 310.0, None, None);
        let e = z.zones.iter().find(|z| z.kind == "exit").unwrap();
        assert_eq!((e.from, e.to), (280.0, 300.0));
    }

    fn scenario(kind: ScenarioKind, conditions: Vec<ScenarioCheck>) -> Scenario {
        Scenario {
            kind,
            title: match kind {
                ScenarioKind::Bull => "Scénario haussier",
                ScenarioKind::Neutral => "Scénario neutre",
                ScenarioKind::Bear => "Scénario baissier",
            }
            .into(),
            condition: String::new(),
            consequence: String::new(),
            level: None,
            conditions,
            met: 0,
            unfolding: false,
        }
    }

    #[test]
    fn scenario_conditions_are_measured() {
        let o = Observed { price: 342.0, last_close: Some(346.0), rsi: Some(58.0), adx: Some(24.0), volume_ratio: Some(1.4), up_day: Some(true) };
        let bull = scenario_checks(ScenarioKind::Bull, 335.61, 345.34, &o);
        assert_eq!(bull.len(), 3);
        assert_eq!(bull[0].text, "Clôture au-dessus de 345,34 $");
        assert!(bull.iter().all(|c| c.state == CheckState::Met), "{bull:?}");
        let neutral = scenario_checks(ScenarioKind::Neutral, 335.61, 345.34, &o);
        assert_eq!(neutral.iter().map(|c| c.state).collect::<Vec<_>>(), [CheckState::Met, CheckState::Unmet]);
        let bear = scenario_checks(ScenarioKind::Bear, 335.61, 345.34, &o);
        assert_eq!(bear[0].text, "Clôture sous 335,61 $ (cassure du support)");
        assert!(bear.iter().all(|c| c.state == CheckState::Unmet));
        let blind = Observed { price: 342.0, ..Default::default() };
        assert!(scenario_checks(ScenarioKind::Bear, 1.0, 2.0, &blind).iter().all(|c| c.state == CheckState::Unknown));
        let mut all = vec![scenario(ScenarioKind::Bull, bull), scenario(ScenarioKind::Neutral, neutral), scenario(ScenarioKind::Bear, bear)];
        let u = unfolding(&mut all).unwrap();
        assert_eq!((u.kind, u.met, u.total), (ScenarioKind::Bull, 3, 3));
        assert!(all[0].unfolding && !all[1].unfolding && all[0].met == 3 && all[1].met == 1);
        assert!(u.text.starts_with("Scénario haussier en cours : 3 conditions sur 3 remplies."), "{}", u.text);
    }

    #[test]
    fn ties_and_nothing_met_give_the_neutral_scenario() {
        let c = |s| ScenarioCheck { text: "c".into(), state: s, detail: String::new() };
        use CheckState::*;
        let mut tie = vec![
            scenario(ScenarioKind::Bull, vec![c(Met), c(Unmet), c(Unmet)]),
            scenario(ScenarioKind::Neutral, vec![c(Unmet), c(Unmet)]),
            scenario(ScenarioKind::Bear, vec![c(Met), c(Unmet), c(Unknown)]),
        ];
        let u = unfolding(&mut tie).unwrap();
        assert_eq!(u.kind, ScenarioKind::Neutral);
        assert!(u.text.contains("égalité"), "{}", u.text);
        let mut none = vec![scenario(ScenarioKind::Bull, vec![c(Unmet)]), scenario(ScenarioKind::Neutral, vec![c(Unknown)])];
        let u = unfolding(&mut none).unwrap();
        assert_eq!((u.kind, u.met), (ScenarioKind::Neutral, 0));
        assert!(u.text.contains("neutre par défaut"));
        // Neutral 1 of 2 (50 %) beats bull 1 of 3.
        let mut half = vec![scenario(ScenarioKind::Bull, vec![c(Met), c(Unmet), c(Unmet)]), scenario(ScenarioKind::Neutral, vec![c(Met), c(Unmet)])];
        assert_eq!(unfolding(&mut half).unwrap().kind, ScenarioKind::Neutral);
    }

    fn fam(key: &str, status: Status) -> Family {
        Family {
            key: key.into(),
            label: key.into(),
            score: (status != Status::Unavailable).then_some(0.0),
            status,
            summary: String::new(),
            points: vec![],
            source: String::new(),
        }
    }

    #[test]
    fn counter_argument_counts_and_invalidators() {
        let pros = vec!["Tendance : haussière".to_string(), "Volume : ok".into()];
        let cons = vec!["Aucun point défavorable mesuré, ce qui ne garantit rien pour la suite".to_string()];
        let fams = vec![fam("trend", Status::Positive), fam("volume", Status::Positive), fam("macro", Status::Unavailable)];
        let s = SrLevel { price: 335.61, touches: 3, kind: LevelKind::Support, distance_pct: -2.0 };
        let inv = vec!["Clôture sous 330,00 $ (stop du plan)".to_string()];
        let c = counter_argument(&CounterInput {
            pros: &pros,
            cons: &cons,
            families: &fams,
            invalidation: &inv,
            bullish: true,
            support: Some(&s),
            resistance: None,
            avg_volume: Some(12_300_000.0),
            volume_ratio: Some(0.7),
            volume_unit: "actions",
            events: vec!["Décision de la Fed, États-Unis, le 30/09 à 20:00".into()],
        });
        assert_eq!((c.favourable, c.unfavourable), (2, 0));
        assert_eq!(c.text, "🟢 Raisons favorables : 2 / 🔴 Raisons défavorables : 0");
        assert_eq!(c.families_text, "Familles : 2 favorable(s), 0 défavorable(s), 0 neutre(s), 1 non mesurée(s)");
        let kinds: Vec<&str> = c.invalidators.iter().map(|i| i.kind.as_str()).collect();
        assert_eq!(kinds, ["level", "condition", "volume", "event"]);
        assert_eq!(c.invalidators[0].text, "Cassure du support 335,61 $ (touché 3 fois)");
        assert_eq!(c.invalidators[2].value, Some(12_300_000.0));
        assert!(c.invalidators[2].text.contains("12,3 M actions par jour ; dernière séance : 0,7 × la moyenne"), "{}", c.invalidators[2].text);
        assert_eq!(c.invalidators[3].text, "Événement : Décision de la Fed, États-Unis, le 30/09 à 20:00");
        // Not bullish: the resistance and a volume surge.
        let r = SrLevel { price: 344.95, touches: 2, kind: LevelKind::Resistance, distance_pct: 1.0 };
        let c = counter_argument(&CounterInput {
            pros: &pros,
            cons: &cons,
            families: &fams,
            invalidation: &[],
            bullish: false,
            support: Some(&s),
            resistance: Some(&r),
            avg_volume: Some(1000.0),
            volume_ratio: None,
            volume_unit: "BTC",
            events: vec![],
        });
        assert!(c.invalidators[0].text.starts_with("Cassure de la résistance 344,95 $"));
        assert_eq!(c.invalidators[1].value, Some(1500.0));
    }
}
