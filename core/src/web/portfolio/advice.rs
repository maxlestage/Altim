//! Altim's advice on an asset, in plain language (`web/src/engine/advice.ts`). Altim never places orders: it
//! advises. Asset already held → recommendation from the holdings analysis (`holdings`); asset not held → consider
//! buying / wait / avoid, with plan and prudent amount.
use serde::{Deserialize, Serialize};

use super::holdings::{LineAnalysis, Recommendation, positive, reason_text};
use super::risk::{Plan, RiskSettings, position_size, risk_reward};
use crate::engine::backtest::TrackRecord;
use crate::engine::guard::{Direction, ShockLevel};
use crate::engine::reliability::ReliabilityLevel;
use crate::engine::signal::{Action, Signal};
use crate::js::{fr, fr_sig, number_to_string, round, to_fixed};
use crate::types::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdviceTone {
    Buy,
    Hold,
    Sell,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvicePlan {
    pub entry: f64,
    pub stop: f64,
    pub target: f64,
    pub risk_reward: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Advice {
    pub tone: AdviceTone,
    pub title: String,
    pub points: Vec<String>,
    pub plan: Option<AdvicePlan>,
    /// Prudent amount (USD) to allocate, if the user's wealth is known.
    pub amount: Option<f64>,
    /// Corresponding quantity (whole shares for a stock).
    pub quantity: Option<f64>,
}

/// Below this track record on the asset itself, a buy signal is not advised.
pub const MIN_TRACK_TRADES: usize = 5;
/// Same threshold as GUARD.reversalHigh (guard.ts).
pub const REVERSAL_HIGH: f64 = 50.0;
pub const MIN_WIN_RATE: f64 = 40.0;

/// `moneyFmt(v, 0 decimals from 100, else 2, " ")`.
fn usd(v: f64) -> String {
    crate::fx::money_with(v, |x| fr(x, 0, if x >= 100.0 { 0 } else { 2 }))
}

/// Price to the cent above 1 $, 4 significant digits below (0,000009312 $ for PEPE, not 0,00001 $).
pub fn px(v: f64) -> String {
    crate::fx::money_with(v, |x| if x >= 1.0 { fr(x, 2, 2) } else { fr_sig(x, 4) })
}

fn pct(v: f64) -> String {
    format!("{}{} %", if v >= 0.0 { "+" } else { "−" }, fr(v.abs(), 0, 1))
}

/// Quantity to buy or sell: whole shares for a stock, 6 significant digits (rounded down) for a crypto.
pub fn advice_quantity(amount: f64, price: f64, kind: Kind) -> f64 {
    if !positive(amount) || !positive(price) {
        return 0.0;
    }
    let raw = amount / price;
    if kind == Kind::Stock {
        return (raw + 1e-9).floor();
    }
    let step = 10f64.powf(raw.log10().floor() - 5.0);
    (raw / step + 1e-9).floor() * step
}

pub fn quantity_text(q: f64, kind: Kind, symbol: &str) -> String {
    if kind == Kind::Stock {
        return format!("{} action{} {symbol}", number_to_string(q), if q > 1.0 { "s" } else { "" });
    }
    format!("{} {symbol}", fr_sig(q, 6))
}

/// Buy zone of the user's horizon (fibonacci) and what the macro context changes for it.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneContext {
    pub label: String,
    pub status: String,
    pub text: String,
    pub macro_note: Option<String>,
}

/// Market guard: shock level and counter-trend reversal risk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuardContext {
    pub shock: ShockLevel,
    pub reversal_score: f64,
    pub reversal_direction: Option<Direction>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdviceInput<'a> {
    pub signal: Option<&'a Signal>,
    pub reliability: Option<ReliabilityLevel>,
    pub price: Option<f64>,
    pub line: Option<&'a LineAnalysis>,
    pub capital: Option<f64>,
    pub risk: RiskSettings,
    /// Backtest of the buy signals on this asset (same candles).
    pub track: Option<TrackRecord>,
    pub symbol: String,
    pub kind: Kind,
    pub guard: Option<GuardContext>,
    pub zone: Option<ZoneContext>,
}

impl<'a> AdviceInput<'a> {
    /// The required fields; the others start empty (symbol "", crypto, like the TypeScript defaults).
    pub fn new(signal: Option<&'a Signal>, reliability: Option<ReliabilityLevel>, price: Option<f64>, risk: RiskSettings) -> Self {
        AdviceInput {
            signal,
            reliability,
            price,
            line: None,
            capital: None,
            risk,
            track: None,
            symbol: String::new(),
            kind: Kind::Crypto,
            guard: None,
            zone: None,
        }
    }
}

/// Advice on an asset. The buy zone of the user's horizon, when known, is added as the last points: it says where
/// to buy (or to wait for), whatever the signal says about when.
pub fn advise_asset(input: &AdviceInput) -> Advice {
    let mut advice = advise_core(input);
    let Some(z) = input.zone.as_ref().filter(|z| z.status != "none") else { return advice };
    advice.points.push(format!("{} (votre horizon) : {}", z.label, z.text));
    if let Some(n) = z.macro_note.as_ref().filter(|n| !n.is_empty()) {
        advice.points.push(n.clone());
    }
    advice
}

fn advice(tone: AdviceTone, title: &str, points: Vec<String>) -> Advice {
    Advice { tone, title: title.into(), points, plan: None, amount: None, quantity: None }
}

fn advise_core(input: &AdviceInput) -> Advice {
    let AdviceInput { signal, reliability, price, line, capital, risk, track, symbol, kind, guard, .. } = input;
    let (kind, symbol) = (*kind, symbol.as_str());
    let reversal_down = guard.is_some_and(|g| g.reversal_direction == Some(Direction::Down) && g.reversal_score >= REVERSAL_HIGH);
    let shock = guard.map(|g| g.shock);
    let reversal_score = guard.map(|g| number_to_string(g.reversal_score)).unwrap_or_default();

    // Asset already held: the portfolio analysis takes precedence.
    if let Some(line) = line {
        let tone = match line.recommendation {
            Recommendation::Sell | Recommendation::Protect => AdviceTone::Sell,
            Recommendation::Strengthen => AdviceTone::Buy,
            Recommendation::Unknown => AdviceTone::Unknown,
            _ => AdviceTone::Hold,
        };
        let mut points = vec![format!(
            "Vous en détenez {} ({}, {}{} depuis l'achat), soit {} % de votre patrimoine.",
            fr(line.quantity, 0, 8),
            usd(line.value),
            if line.pnl >= 0.0 { "+" } else { "−" },
            usd(line.pnl.abs()),
            fr(line.weight, 0, 1)
        )];
        points.extend(line.reasons.iter().map(|r| reason_text(r)));
        if line.recommendation == Recommendation::Lighten && line.trim_value > 0.0 {
            let q = match line.price.filter(|p| *p != 0.0) {
                Some(p) => advice_quantity(line.trim_value, p, line.kind),
                None => 0.0,
            };
            let qty = if q > 0.0 { format!(", soit {}", quantity_text(q, line.kind, &line.symbol)) } else { String::new() };
            points.push(format!("Montant à alléger conseillé : environ {}{qty}.", usd(line.trim_value)));
        }
        if let Some(stop) = line.stop.filter(|s| *s != 0.0) {
            points.push(format!(
                "Stop de protection conseillé : {} (si le cours passe dessous, sortir limite la perte à ≈ {}).",
                px(stop),
                usd(line.loss_at_stop.unwrap_or(0.0))
            ));
        }
        if shock == Some(ShockLevel::Shock) {
            points.push("Marché en choc (mouvements anormaux) : ne renforcez pas maintenant, vérifiez que votre stop est bien en place.".into());
        }
        if reversal_down {
            points.push(format!(
                "Risque de retournement à la baisse élevé ({reversal_score}/100) : resserrez votre stop ou prenez une partie de vos gains."
            ));
        }
        let tone = if tone == AdviceTone::Buy && (shock == Some(ShockLevel::Shock) || reversal_down) { AdviceTone::Hold } else { tone };
        return advice(tone, line.recommendation.label(), points);
    }

    let (Some(signal), Some(price)) = (signal, price.filter(|p| *p != 0.0)) else {
        return advice(AdviceTone::Unknown, "Pas de conseil pour l'instant", vec!["Historique ou cours insuffisant pour analyser cet actif.".into()]);
    };
    if *reliability == Some(ReliabilityLevel::Low) {
        return advice(
            AdviceTone::Unknown,
            "Pas de conseil pour l'instant",
            vec!["Les sources de données sont absentes ou en désaccord : Altim préfère ne rien conseiller plutôt que de mal conseiller.".into()],
        );
    }

    let caution: Vec<String> = if *reliability == Some(ReliabilityLevel::Medium) {
        vec!["Fiabilité des données moyenne (peu de sources indépendantes) : restez prudent.".into()]
    } else {
        Vec::new()
    };
    let a = signal.action;
    let buying = matches!(a, Action::Buy | Action::StrongBuy);
    // Market guard: no buy in a shock or when a reversal against the rise is likely.
    if buying && shock == Some(ShockLevel::Shock) {
        let mut points = vec!["Les indicateurs sont à l'achat, mais le marché fait des mouvements anormaux (volatilité, sauts de prix) : attendez que la tempête passe.".to_string()];
        points.extend(caution);
        return advice(AdviceTone::Hold, "Attendre : marché en choc", points);
    }
    if buying && reversal_down {
        let mut points = vec![format!(
            "Les indicateurs sont à l'achat, mais un retournement à la baisse est probable ({reversal_score}/100 : excès, foule trop optimiste ou actualités défavorables). Attendez qu'il se produise ou soit écarté."
        )];
        points.extend(caution);
        return advice(AdviceTone::Hold, "Attendre : risque de retournement", points);
    }
    // A buy signal must have worked on this very asset: otherwise, wait.
    if let Some(t) = track.filter(|t| buying && t.trades >= MIN_TRACK_TRADES && (t.win_rate < MIN_WIN_RATE || t.avg_return <= 0.0)) {
        let mut points = vec![
            format!(
                "Les indicateurs sont à l'achat, mais sur l'historique de cet actif ce signal n'a réussi que {} % du temps ({} signaux, {} en moyenne par signal, frais inclus).",
                number_to_string(round(t.win_rate)),
                t.trades,
                pct(t.avg_return)
            ),
            "Altim préfère attendre un meilleur point d'entrée plutôt que de suivre un signal qui a surtout échoué ici.".into(),
        ];
        points.extend(caution);
        return advice(AdviceTone::Hold, "Attendre : signal peu fiable sur cet actif", points);
    }
    if buying {
        let plan = signal.has_plan.then(|| {
            let d = signal.price - signal.stop_loss;
            (price, price - d, price + d * 2.0)
        });
        let mut points: Vec<String> = Vec::new();
        let (mut amount, mut quantity) = (None, None);
        let agitated = shock == Some(ShockLevel::Agitated);
        if let Some((entry, stop, target)) = plan.filter(|p| p.1 > 0.0) {
            let rr = risk_reward(&Plan { entry, stop_loss: stop, take_profit: target });
            points.push(format!(
                "Zone d'entrée : autour de {}. Stop conseillé : {}. Objectif : {} (gain potentiel {} fois le risque).",
                px(entry),
                px(stop),
                px(target),
                fr(rr, 0, 1)
            ));
            let capital = capital.filter(|c| *c != 0.0 && *c > 0.0);
            let size = capital.and_then(|c| position_size(risk, c, &Plan { entry, stop_loss: stop, take_profit: target }));
            match (size, capital) {
                (Some(size), Some(capital)) => {
                    let mut q = advice_quantity(size.notional, entry, kind);
                    // Agitated market: half the amount (same rule as the guard's policy for bots).
                    if agitated {
                        q = advice_quantity(size.notional * 0.5, entry, kind);
                    }
                    // Whole shares: the amount and the loss at the stop are those of the rounded quantity.
                    let scale = if q > 0.0 && (kind == Kind::Stock || agitated) {
                        q * entry / size.notional
                    } else if agitated {
                        0.5
                    } else {
                        1.0
                    };
                    let a = size.notional * scale;
                    let qty = if q > 0.0 && !symbol.is_empty() {
                        format!(", soit {}", quantity_text(q, kind, symbol))
                    } else if kind == Kind::Stock && !symbol.is_empty() {
                        " (moins d'une action entière : il faudrait des fractions d'action)".to_string()
                    } else {
                        String::new()
                    };
                    points.push(format!(
                        "Avec votre patrimoine ({}), n'y consacrez pas plus d'environ {}{qty} : si le stop est touché, la perte resterait limitée à ≈ {} ({} % du patrimoine, frais inclus).",
                        usd(capital),
                        usd(a),
                        usd(size.risk_amount * scale),
                        fr(risk.risk_per_trade_percent * scale, 0, 2)
                    ));
                    amount = Some(a);
                    quantity = Some(q);
                }
                _ => points.push("Renseignez vos avoirs dans « Mes avoirs » pour obtenir un montant adapté à votre patrimoine.".into()),
            }
        }
        if agitated {
            points.push("Marché agité : le montant conseillé est divisé par deux, et un stop plus large évite d'être sorti par le bruit.".into());
        }
        points.push(format!(
            "Confiance du signal : {} %. {}",
            number_to_string(round(signal.confidence)),
            if a == Action::StrongBuy { "Les indicateurs sont largement d'accord." } else { "Signal modéré : entrez progressivement." }
        ));
        match track {
            Some(t) if t.trades >= MIN_TRACK_TRADES => points.push(format!(
                "Sur l'historique de cet actif : {} signaux d'achat, {} % gagnants, {} en moyenne par signal (frais inclus). Les performances passées ne préjugent pas des performances futures.",
                t.trades,
                number_to_string(round(t.win_rate)),
                pct(t.avg_return)
            )),
            Some(t) => points.push(format!(
                "Seulement {} signal{} d'achat dans l'historique de cet actif : pas assez pour juger de sa fiabilité, prudence.",
                t.trades,
                if t.trades > 1 { "s" } else { "" }
            )),
            None => {}
        }
        points.extend(caution);
        return Advice {
            tone: AdviceTone::Buy,
            title: if a == Action::StrongBuy { "Achat envisageable (signal fort)" } else { "Achat envisageable" }.into(),
            points,
            plan: plan.map(|(entry, stop, target)| AdvicePlan { entry, stop, target, risk_reward: 2.0 }),
            amount,
            quantity,
        };
    }
    if matches!(a, Action::Sell | Action::StrongSell) {
        let mut points = vec![
            "Tendance baissière : ce n'est pas le moment d'acheter.".to_string(),
            "Si vous en détenez, ajoutez-le dans « Mes avoirs » pour savoir s'il faut protéger ou alléger votre position.".into(),
        ];
        points.extend(caution);
        return advice(AdviceTone::Sell, "À éviter pour l'instant", points);
    }
    let lean = if signal.score > 10.0 {
        "légère orientation haussière"
    } else if signal.score < -10.0 {
        "légère orientation baissière"
    } else {
        "neutre"
    };
    let mut points = vec![
        format!("Pas de signal clair (score {}{}/100, {lean}).", if signal.score >= 0.0 { "+" } else { "" }, to_fixed(signal.score, 0)),
        "Mieux vaut attendre un signal d'achat confirmé par plusieurs indicateurs.".into(),
    ];
    points.extend(caution);
    advice(AdviceTone::Hold, "Attendre", points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::signal::{AnalyzeOptions, Candle, analyze};
    use crate::web::store::DEFAULT_RISK;

    fn base() -> Signal {
        let v: serde_json::Value = serde_json::from_str(include_str!("../../../../backend/tests/samples/swift-fixture.json")).unwrap();
        let candles: Vec<Candle> = v[0]["candles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let x: Vec<f64> = r.as_array().unwrap().iter().map(|n| n.as_f64().unwrap()).collect();
                Candle { time: x[0] as i64, open: x[1], high: x[2], low: x[3], close: x[4], volume: x[5] }
            })
            .collect();
        analyze(&candles, &AnalyzeOptions::default()).unwrap()
    }

    fn sig(b: &Signal, action: Action) -> Signal {
        Signal { action, confidence: 60.0, ..b.clone() }
    }

    fn joined(a: &Advice) -> String {
        a.points.join(" ")
    }

    fn track(trades: usize, win_rate: f64, avg_return: f64) -> Option<TrackRecord> {
        Some(TrackRecord { trades, win_rate, avg_return })
    }

    fn line(recommendation: Recommendation, reasons: &[&str], pnl: f64, weight: f64, trim_value: f64) -> LineAnalysis {
        LineAnalysis {
            id: "x".into(),
            symbol: "ETH".into(),
            kind: Kind::Crypto,
            name: "ETH".into(),
            quantity: 2.0,
            average_price: 0.0,
            price: Some(2500.0),
            value: 5000.0,
            invested: 0.0,
            pnl,
            pnl_percent: 0.0,
            weight,
            stop: Some(2300.0),
            loss_at_stop: Some(400.0),
            recommendation,
            reasons: reasons.iter().map(|r| r.to_string()).collect(),
            trim_value,
        }
    }

    #[test]
    fn buy_with_plan_and_prudent_amount() {
        let b = base();
        let s = sig(&b, Action::Buy);
        let a = advise_asset(&AdviceInput {
            capital: Some(20_000.0),
            ..AdviceInput::new(Some(&s), Some(ReliabilityLevel::High), Some(b.price), DEFAULT_RISK)
        });
        assert_eq!(a.tone, AdviceTone::Buy);
        let plan = a.plan.unwrap();
        assert!(plan.stop < plan.entry);
        assert!(a.amount.unwrap() > 0.0);
        // Position cap.
        assert!(a.amount.unwrap() <= 20_000.0 * 0.2 + 1e-6);
        assert!(joined(&a).contains("n'y consacrez pas plus"));
    }

    #[test]
    fn without_wealth_invites_to_fill_holdings() {
        let b = base();
        let s = sig(&b, Action::StrongBuy);
        let a = advise_asset(&AdviceInput::new(Some(&s), Some(ReliabilityLevel::High), Some(b.price), DEFAULT_RISK));
        assert!(a.title.contains("signal fort"));
        assert_eq!(a.amount, None);
        assert!(joined(&a).contains("Mes avoirs"));
    }

    #[test]
    fn wait_avoid_and_no_advice_when_unreliable() {
        let b = base();
        let (hold, sell, strong) = (sig(&b, Action::Hold), sig(&b, Action::StrongSell), sig(&b, Action::StrongBuy));
        let high = Some(ReliabilityLevel::High);
        assert_eq!(advise_asset(&AdviceInput::new(Some(&hold), high, Some(1.0), DEFAULT_RISK)).tone, AdviceTone::Hold);
        assert_eq!(advise_asset(&AdviceInput::new(Some(&sell), high, Some(1.0), DEFAULT_RISK)).tone, AdviceTone::Sell);
        let low = advise_asset(&AdviceInput {
            capital: Some(1000.0),
            ..AdviceInput::new(Some(&strong), Some(ReliabilityLevel::Low), Some(1.0), DEFAULT_RISK)
        });
        assert_eq!(low.tone, AdviceTone::Unknown);
        assert_eq!(low.amount, None);
        let buy = sig(&b, Action::Buy);
        assert!(
            joined(&advise_asset(&AdviceInput::new(Some(&buy), Some(ReliabilityLevel::Medium), Some(b.price), DEFAULT_RISK))).contains("prudent")
        );
    }

    #[test]
    fn held_asset_from_the_holdings_analysis() {
        let b = base();
        let s = sig(&b, Action::Buy);
        let l = line(Recommendation::Lighten, &["overweight"], -300.0, 40.0, 1200.0);
        let a = advise_asset(&AdviceInput {
            line: Some(&l),
            capital: Some(12_000.0),
            ..AdviceInput::new(Some(&s), Some(ReliabilityLevel::High), Some(2500.0), DEFAULT_RISK)
        });
        assert_eq!(a.title, "Alléger");
        assert!(joined(&a).contains("Vous en détenez"));
        assert!(joined(&a).contains("Montant à alléger"));
        assert_eq!(a.points[0], "Vous en détenez 2 (5\u{202f}000 $, −300 $ depuis l'achat), soit 40 % de votre patrimoine.");
        assert_eq!(a.points[2], "Montant à alléger conseillé : environ 1\u{202f}200 $, soit 0,48 ETH.");
    }

    #[test]
    fn poor_track_record_becomes_wait() {
        let b = base();
        let (strong, buy) = (sig(&b, Action::StrongBuy), sig(&b, Action::Buy));
        let high = Some(ReliabilityLevel::High);
        let poor = advise_asset(&AdviceInput {
            capital: Some(20_000.0),
            track: track(12, 25.0, -0.8),
            ..AdviceInput::new(Some(&strong), high, Some(b.price), DEFAULT_RISK)
        });
        assert_eq!(poor.tone, AdviceTone::Hold);
        assert!(poor.title.contains("peu fiable"));
        assert_eq!(poor.amount, None);
        assert!(poor.points[0].contains("25 %"));
        // Negative average even with a decent win rate.
        assert_eq!(
            advise_asset(&AdviceInput { track: track(8, 55.0, -0.1), ..AdviceInput::new(Some(&buy), high, Some(b.price), DEFAULT_RISK) }).tone,
            AdviceTone::Hold
        );
        // Good track record: the buy stays, with its figures.
        let good = advise_asset(&AdviceInput { track: track(9, 56.0, 1.4), ..AdviceInput::new(Some(&buy), high, Some(b.price), DEFAULT_RISK) });
        assert_eq!(good.tone, AdviceTone::Buy);
        assert!(joined(&good).contains("9 signaux d'achat, 56 % gagnants, +1,4 %"));
        // Too few trades: the buy stays but prudence is stated.
        let few = advise_asset(&AdviceInput { track: track(2, 0.0, -2.0), ..AdviceInput::new(Some(&buy), high, Some(b.price), DEFAULT_RISK) });
        assert_eq!(few.tone, AdviceTone::Buy);
        assert!(joined(&few).contains("pas assez pour juger"));
    }

    #[test]
    fn precise_amounts() {
        assert!((advice_quantity(1000.0, 84_871.45, Kind::Crypto) - 0.0117825).abs() < 5e-8);
        assert!(advice_quantity(1000.0, 84_871.45, Kind::Crypto) * 84_871.45 <= 1000.0);
        assert_eq!(advice_quantity(1000.0, 341.07, Kind::Stock), 2.0);
        assert_eq!(advice_quantity(100.0, 341.07, Kind::Stock), 0.0);
        assert_eq!(advice_quantity(50.0, 0.000009312, Kind::Crypto), 5_369_410.0);
        assert_eq!(quantity_text(2.0, Kind::Stock, "AAPL"), "2 actions AAPL");
        assert_eq!(quantity_text(0.0117825, Kind::Crypto, "BTC"), "0,0117825 BTC");
        assert_eq!(px(0.000009312), "0,000009312 $");
        assert_eq!(px(84_871.456), "84\u{202f}871,46 $");
        let b = base();
        let s = sig(&b, Action::Buy);
        let a = advise_asset(&AdviceInput {
            capital: Some(20_000.0),
            symbol: "BTC".into(),
            kind: Kind::Crypto,
            ..AdviceInput::new(Some(&s), Some(ReliabilityLevel::High), Some(b.price), DEFAULT_RISK)
        });
        assert!(a.quantity.unwrap() * b.price <= a.amount.unwrap() + 1e-9);
        assert!(joined(&a).contains(" BTC :"));
    }

    #[test]
    fn whole_shares() {
        let b = base();
        let s = sig(&b, Action::Buy);
        let a = advise_asset(&AdviceInput {
            capital: Some(20_000.0),
            symbol: "AAPL".into(),
            kind: Kind::Stock,
            ..AdviceInput::new(Some(&s), Some(ReliabilityLevel::High), Some(341.07), DEFAULT_RISK)
        });
        let q = a.quantity.unwrap();
        assert_eq!(q.fract(), 0.0);
        assert!((a.amount.unwrap() - q * 341.07).abs() < 5e-7);
        assert!(joined(&a).contains(&format!("soit {} actions AAPL", number_to_string(q))));
    }

    #[test]
    fn guard_no_buy_in_shock_or_reversal() {
        let b = base();
        let s = sig(&b, Action::StrongBuy);
        let with = |shock: ShockLevel, score: f64, dir: Option<Direction>| {
            advise_asset(&AdviceInput {
                capital: Some(20_000.0),
                symbol: "BTC".into(),
                kind: Kind::Crypto,
                guard: Some(GuardContext { shock, reversal_score: score, reversal_direction: dir }),
                ..AdviceInput::new(Some(&s), Some(ReliabilityLevel::High), Some(b.price), DEFAULT_RISK)
            })
        };
        let shocked = with(ShockLevel::Shock, 0.0, None);
        assert_eq!(shocked.tone, AdviceTone::Hold);
        assert!(shocked.title.contains("choc"));
        assert!(with(ShockLevel::Calm, 60.0, Some(Direction::Down)).title.contains("retournement"));
        // A reversal UPWARD does not block a buy.
        assert_eq!(with(ShockLevel::Calm, 80.0, Some(Direction::Up)).tone, AdviceTone::Buy);
        let calm = with(ShockLevel::Calm, 0.0, None);
        let agitated = with(ShockLevel::Agitated, 0.0, None);
        assert_eq!(agitated.tone, AdviceTone::Buy);
        assert!((agitated.amount.unwrap() - calm.amount.unwrap() / 2.0).abs() < 0.5);
        assert!(agitated.quantity.unwrap() * b.price <= agitated.amount.unwrap() + 1e-9);
        assert!(joined(&agitated).contains("divisé par deux"));
    }

    #[test]
    fn guard_on_a_held_asset() {
        let b = base();
        let s = sig(&b, Action::Buy);
        let l = line(Recommendation::Strengthen, &["momentum"], 800.0, 20.0, 0.0);
        let a = advise_asset(&AdviceInput {
            line: Some(&l),
            guard: Some(GuardContext { shock: ShockLevel::Calm, reversal_score: 55.0, reversal_direction: Some(Direction::Down) }),
            ..AdviceInput::new(Some(&s), Some(ReliabilityLevel::High), Some(2500.0), DEFAULT_RISK)
        });
        assert_eq!(a.tone, AdviceTone::Hold);
        assert!(joined(&a).contains("resserrez votre stop"));
    }

    // zones.test.ts « le conseil ajoute la zone de votre horizon et la prudence macro »
    #[test]
    fn zone_of_the_horizon_and_macro_caution() {
        let zone = ZoneContext {
            label: "Moyen terme".into(),
            status: "above".into(),
            text: "Prix au-dessus de la zone : attendre un repli.".into(),
            macro_note: Some("Contexte macro tendu : entrer en deux ou trois fois.".into()),
        };
        let a = advise_asset(&AdviceInput { zone: Some(zone), ..AdviceInput::new(None, Some(ReliabilityLevel::High), Some(100.0), DEFAULT_RISK) });
        assert_eq!(a.points[a.points.len() - 2], "Moyen terme (votre horizon) : Prix au-dessus de la zone : attendre un repli.");
        assert!(a.points[a.points.len() - 1].contains("macro"));
        let none = ZoneContext { label: "Court terme".into(), status: "none".into(), text: String::new(), macro_note: None };
        let a = advise_asset(&AdviceInput { zone: Some(none), ..AdviceInput::new(None, Some(ReliabilityLevel::High), Some(100.0), DEFAULT_RISK) });
        assert!(!joined(&a).contains("votre horizon"));
    }
}
