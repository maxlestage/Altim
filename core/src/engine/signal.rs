//! Altim signal engine (`web/src/engine/signal.ts`), checked on the same 17 reference scenarios.
use serde::{Deserialize, Serialize};

use crate::js::{now_ms, to_fixed};
pub use crate::types::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    StrongBuy,
    Buy,
    Hold,
    Sell,
    StrongSell,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::StrongBuy => "strongBuy",
            Action::Buy => "buy",
            Action::Hold => "hold",
            Action::Sell => "sell",
            Action::StrongSell => "strongSell",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Action::StrongBuy => "ACHAT FORT",
            Action::Buy => "ACHAT",
            Action::Hold => "ATTENDRE",
            Action::Sell => "VENTE",
            Action::StrongSell => "VENTE FORTE",
        }
    }
}

pub fn is_buy(a: Action) -> bool {
    matches!(a, Action::Buy | Action::StrongBuy)
}
pub fn is_sell(a: Action) -> bool {
    matches!(a, Action::Sell | Action::StrongSell)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Factor {
    pub name: String,
    pub score: f64,
    pub weight: f64,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    pub action: Action,
    pub score: f64,
    pub confidence: f64,
    pub price: f64,
    pub time: i64,
    pub factors: Vec<Factor>,
    pub stop_loss: f64,
    pub take_profit: f64,
    /// false if the ATR is unavailable (no stop/target plan).
    pub has_plan: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AnalyzeOptions<'a> {
    /// Higher-timeframe candles, to confirm the underlying trend.
    pub higher: Option<&'a [Candle]>,
    /// Candle duration (ms) and current time, to detect stale data.
    pub interval_ms: Option<i64>,
    pub now: Option<i64>,
}

pub type Series = Vec<Option<f64>>;

fn is_valid(c: &Candle) -> bool {
    [c.open, c.high, c.low, c.close, c.volume].iter().all(|v| v.is_finite())
        && c.low > 0.0
        && c.high >= c.low
        && c.high >= c.open.max(c.close)
        && c.low <= c.open.min(c.close)
        && c.volume >= 0.0
}

/// Valid candles, sorted, without duplicates.
pub fn sanitize(candles: &[Candle]) -> Vec<Candle> {
    let mut v: Vec<Candle> = candles.iter().filter(|c| is_valid(c)).copied().collect();
    v.sort_by_key(|c| c.time);
    v.dedup_by_key(|c| c.time);
    v
}

pub fn sign(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

pub fn clamp(x: f64, lo: f64, hi: f64) -> f64 {
    hi.min(lo.max(x))
}

pub fn sma(values: &[f64], period: usize) -> Series {
    let mut out = vec![None; values.len()];
    if values.len() < period || period == 0 {
        return out;
    }
    let mut sum: f64 = values[..period].iter().sum();
    out[period - 1] = Some(sum / period as f64);
    for i in period..values.len() {
        sum += values[i] - values[i - period];
        out[i] = Some(sum / period as f64);
    }
    out
}

pub fn ema_opt(values: &[Option<f64>], period: usize) -> Series {
    let mut out = vec![None; values.len()];
    let Some(start) = values.iter().position(|v| v.is_some()) else { return out };
    if values.len() - start < period {
        return out;
    }
    let k = 2.0 / (period as f64 + 1.0);
    let mut prev = 0.0;
    for j in 0..period {
        prev += values[start + j].unwrap_or(f64::NAN);
    }
    prev /= period as f64;
    out[start + period - 1] = Some(prev);
    for i in start + period..values.len() {
        let Some(v) = values[i] else { return out };
        prev = v * k + prev * (1.0 - k);
        out[i] = Some(prev);
    }
    out
}

pub fn ema(values: &[f64], period: usize) -> Series {
    let opt: Series = values.iter().map(|v| Some(*v)).collect();
    ema_opt(&opt, period)
}

pub fn rsi(closes: &[f64], period: usize) -> Series {
    let mut out = vec![None; closes.len()];
    if closes.len() <= period {
        return out;
    }
    let (mut gain, mut loss) = (0.0, 0.0);
    for i in 1..=period {
        let c = closes[i] - closes[i - 1];
        if c >= 0.0 {
            gain += c;
        } else {
            loss -= c;
        }
    }
    let p = period as f64;
    let mut ag = gain / p;
    let mut al = loss / p;
    let value = |g: f64, l: f64| if l == 0.0 { if g == 0.0 { 50.0 } else { 100.0 } } else { 100.0 - 100.0 / (1.0 + g / l) };
    out[period] = Some(value(ag, al));
    for i in period + 1..closes.len() {
        let c = closes[i] - closes[i - 1];
        ag = (ag * (p - 1.0) + c.max(0.0)) / p;
        al = (al * (p - 1.0) + (-c).max(0.0)) / p;
        out[i] = Some(value(ag, al));
    }
    out
}

fn true_range(c: &[Candle]) -> Vec<f64> {
    c.iter()
        .enumerate()
        .map(|(i, x)| if i == 0 { x.high - x.low } else { (x.high - x.low).max((x.high - c[i - 1].close).abs()).max((x.low - c[i - 1].close).abs()) })
        .collect()
}

pub fn atr(c: &[Candle], period: usize) -> Series {
    let mut out = vec![None; c.len()];
    if c.len() < period || period == 0 {
        return out;
    }
    let tr = true_range(c);
    let p = period as f64;
    let mut prev = tr[..period].iter().sum::<f64>() / p;
    out[period - 1] = Some(prev);
    for i in period..c.len() {
        prev = (prev * (p - 1.0) + tr[i]) / p;
        out[i] = Some(prev);
    }
    out
}

pub fn adx(c: &[Candle], period: usize) -> Series {
    let n = c.len();
    let mut out = vec![None; n];
    if n <= period * 2 {
        return out;
    }
    let tr = true_range(c);
    let mut pdm = vec![0.0; n];
    let mut mdm = vec![0.0; n];
    for i in 1..n {
        let up = c[i].high - c[i - 1].high;
        let down = c[i - 1].low - c[i].low;
        pdm[i] = if up > down && up > 0.0 { up } else { 0.0 };
        mdm[i] = if down > up && down > 0.0 { down } else { 0.0 };
    }
    let (mut s_tr, mut s_p, mut s_m) = (0.0, 0.0, 0.0);
    for i in 1..=period {
        s_tr += tr[i];
        s_p += pdm[i];
        s_m += mdm[i];
    }
    let mut dx: Series = vec![None; n];
    let update = |dx: &mut Series, i: usize, s_tr: f64, s_p: f64, s_m: f64| {
        let p = if s_tr == 0.0 { 0.0 } else { 100.0 * s_p / s_tr };
        let m = if s_tr == 0.0 { 0.0 } else { 100.0 * s_m / s_tr };
        dx[i] = Some(if p + m == 0.0 { 0.0 } else { 100.0 * (p - m).abs() / (p + m) });
    };
    update(&mut dx, period, s_tr, s_p, s_m);
    let pf = period as f64;
    for i in period + 1..n {
        s_tr = s_tr - s_tr / pf + tr[i];
        s_p = s_p - s_p / pf + pdm[i];
        s_m = s_m - s_m / pf + mdm[i];
        update(&mut dx, i, s_tr, s_p, s_m);
    }
    let first = 2 * period - 1;
    let mut prev = 0.0;
    for d in dx.iter().take(first + 1).skip(period) {
        prev += d.unwrap_or(0.0);
    }
    prev /= pf;
    out[first] = Some(prev);
    for i in first + 1..n {
        prev = (prev * (pf - 1.0) + dx[i].unwrap_or(0.0)) / pf;
        out[i] = Some(prev);
    }
    out
}

fn sma_optional(values: &[Option<f64>], period: usize) -> Series {
    let mut out = vec![None; values.len()];
    if period == 0 {
        return out;
    }
    for i in period.saturating_sub(1)..values.len() {
        let mut sum = 0.0;
        let mut ok = true;
        for v in &values[i + 1 - period..=i] {
            match v {
                Some(v) => sum += v,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            out[i] = Some(sum / period as f64);
        }
    }
    out
}

fn stochastic(c: &[Candle], period: usize, smooth_k: usize, smooth_d: usize) -> (Series, Series) {
    let mut raw: Series = vec![None; c.len()];
    for i in period.saturating_sub(1)..c.len() {
        let mut hh = f64::NEG_INFINITY;
        let mut ll = f64::INFINITY;
        for x in &c[i + 1 - period..=i] {
            hh = hh.max(x.high);
            ll = ll.min(x.low);
        }
        raw[i] = Some(if hh - ll == 0.0 { 50.0 } else { (c[i].close - ll) / (hh - ll) * 100.0 });
    }
    let k = sma_optional(&raw, smooth_k);
    let d = sma_optional(&k, smooth_d);
    (k, d)
}

fn obv(c: &[Candle]) -> Vec<f64> {
    let mut out = vec![0.0; c.len()];
    for i in 1..c.len() {
        let d = c[i].close - c[i - 1].close;
        out[i] = out[i - 1]
            + if d > 0.0 {
                c[i].volume
            } else if d < 0.0 {
                -c[i].volume
            } else {
                0.0
            };
    }
    out
}

fn raw_slope(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let n = values.len() as f64;
    let mx = (n - 1.0) / 2.0;
    let my = values.iter().sum::<f64>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (i, y) in values.iter().enumerate() {
        let i = i as f64;
        num += (i - mx) * (y - my);
        den += (i - mx) * (i - mx);
    }
    if den > 0.0 { Some(num / den) } else { None }
}

const W_TREND: f64 = 2.0;
const W_MACD: f64 = 1.5;
const W_RSI: f64 = 1.5;
const W_STOCH: f64 = 1.0;
const W_BOLL: f64 = 0.75;
const W_VOLUME: f64 = 1.0;
const W_HIGHER: f64 = 1.5;

fn factor(name: &str, score: f64, weight: f64, detail: String) -> Factor {
    Factor { name: name.into(), score, weight, detail }
}

pub fn analyze(raw: &[Candle], options: &AnalyzeOptions) -> Option<Signal> {
    let candles = sanitize(raw);
    if candles.len() < 60 {
        return None;
    }
    let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
    let last = candles.len() - 1;
    let price = closes[last];
    let mut factors: Vec<Factor> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    let adx_now = adx(&candles, 14)[last];
    let atr_now = atr(&candles, 14)[last].unwrap_or(0.0);

    // 1. Trend
    let e20 = ema(&closes, 20)[last];
    let e50 = ema(&closes, 50)[last];
    let e200 = ema(&closes, 200)[last];
    let (fast, slow, label) = if e200.is_some() { (e50, e200, "EMA 50/200") } else { (e20, e50, "EMA 20/50") };
    if e200.is_none() {
        warnings.push("Moins de 200 bougies : tendance de long terme estimée sur EMA 20/50.".into());
    }
    if let (Some(fast), Some(slow)) = (fast, slow) {
        let raw_trend = 0.5 * sign(fast - slow) + 0.5 * sign(price - slow);
        let strength = adx_now.map(|a| clamp((a - 15.0) / 20.0, 0.3, 1.0)).unwrap_or(0.6);
        let dir = if raw_trend > 0.0 {
            "haussière"
        } else if raw_trend < 0.0 {
            "baissière"
        } else {
            "neutre"
        };
        let adx_text = adx_now.map(|a| format!(" · ADX {}", to_fixed(a, 0))).unwrap_or_default();
        factors.push(factor("Tendance", clamp(raw_trend * strength, -1.0, 1.0), W_TREND, format!("{label} {dir}{adx_text}")));
    }
    if adx_now.is_some_and(|a| a < 18.0) {
        warnings.push("Marché sans tendance (ADX < 18) : signaux moins fiables.".into());
    }

    // 2. MACD
    let f12 = ema(&closes, 12);
    let s26 = ema(&closes, 26);
    let line: Series = f12
        .iter()
        .zip(&s26)
        .map(|(f, s)| match (f, s) {
            (Some(f), Some(s)) => Some(f - s),
            _ => None,
        })
        .collect();
    let sig = ema_opt(&line, 9);
    let hist: Series = line
        .iter()
        .zip(&sig)
        .map(|(l, s)| match (l, s) {
            (Some(l), Some(s)) => Some(l - s),
            _ => None,
        })
        .collect();
    if let (Some(h), Some(hp)) = (hist[last], hist[last - 1]) {
        let mut score = 0.5 * sign(h);
        let mut detail = String::from(if h >= 0.0 { "Histogramme positif" } else { "Histogramme négatif" });
        let mut cross: Option<f64> = None;
        for i in last.saturating_sub(3).max(1)..=last {
            let (Some(a), Some(b)) = (hist[i - 1], hist[i]) else { continue };
            if a <= 0.0 && b > 0.0 {
                cross = Some(1.0);
            } else if a >= 0.0 && b < 0.0 {
                cross = Some(-1.0);
            }
        }
        if let Some(c) = cross {
            score = c;
            detail = (if c > 0.0 { "Croisement haussier récent" } else { "Croisement baissier récent" }).into();
        } else if h > hp {
            score += 0.25;
            detail += ", en hausse";
        } else if h < hp {
            score -= 0.25;
            detail += ", en baisse";
        }
        factors.push(factor("MACD", clamp(score, -1.0, 1.0), W_MACD, detail));
    }

    // 3. RSI
    let r = rsi(&closes, 14);
    if let Some(rv) = r[last] {
        let prev = r[last - 1].unwrap_or(rv);
        let score = if rv < 30.0 {
            (0.5 + (30.0 - rv) / 20.0).min(1.0) * if rv > prev { 1.0 } else { 0.7 }
        } else if rv > 70.0 {
            -(0.5 + (rv - 70.0) / 20.0).min(1.0) * if rv < prev { 1.0 } else { 0.7 }
        } else {
            (rv - 50.0) / 40.0
        };
        let zone = if rv < 30.0 {
            "survente"
        } else if rv > 70.0 {
            "surachat"
        } else {
            "zone neutre"
        };
        factors.push(factor("RSI", clamp(score, -1.0, 1.0), W_RSI, format!("RSI {} ({zone})", to_fixed(rv, 1))));
    }

    // 4. Stochastic
    let (sk, sd) = stochastic(&candles, 14, 3, 3);
    if let (Some(k), Some(d), Some(kp), Some(dp)) = (sk[last], sd[last], sk[last - 1], sd[last - 1]) {
        let mut score = 0.0;
        let mut detail = format!("%K {} / %D {}", to_fixed(k, 0), to_fixed(d, 0));
        if kp <= dp && k > d && k < 30.0 {
            score = 1.0;
            detail += " · croisement haussier en survente";
        } else if kp >= dp && k < d && k > 70.0 {
            score = -1.0;
            detail += " · croisement baissier en surachat";
        } else if k < 20.0 {
            score = 0.5;
        } else if k > 80.0 {
            score = -0.5;
        }
        factors.push(factor("Stochastique", score, W_STOCH, detail));
    }

    // 5. Bollinger
    if let Some(mid) = sma(&closes, 20)[last] {
        let mut variance = 0.0;
        for c in &closes[last - 19..=last] {
            variance += (c - mid).powi(2);
        }
        let sd = (variance / 20.0).sqrt();
        let up = mid + 2.0 * sd;
        let lo = mid - 2.0 * sd;
        if up > lo {
            let pb = (price - lo) / (up - lo);
            let score = if pb < 0.0 {
                0.8
            } else if pb > 1.0 {
                -0.8
            } else {
                (0.5 - pb) * 0.6
            };
            factors.push(factor("Bollinger", score, W_BOLL, format!("%B {}", to_fixed(pb, 2))));
        }
    }

    // 6. Volume
    let o = obv(&candles);
    let o = &o[o.len().saturating_sub(20)..];
    let avg_vol = candles[candles.len().saturating_sub(20)..].iter().map(|c| c.volume).sum::<f64>() / 20.0;
    match raw_slope(o) {
        Some(slope) if avg_vol > 0.0 => {
            let per_bar = slope / avg_vol;
            factors.push(factor(
                "Volume (OBV)",
                clamp(per_bar / 0.3, -1.0, 1.0),
                W_VOLUME,
                (if per_bar >= 0.0 { "Accumulation" } else { "Distribution" }).into(),
            ));
        }
        _ => warnings.push("Volume indisponible : facteur volume ignoré.".into()),
    }

    // 7. Higher-timeframe confirmation
    let mut higher_score: Option<f64> = None;
    let higher = options.higher.map(sanitize).unwrap_or_default();
    if higher.len() >= 50 {
        let hc: Vec<f64> = higher.iter().map(|c| c.close).collect();
        let hl = hc.len() - 1;
        if let (Some(hf), Some(hs)) = (ema(&hc, 20)[hl], ema(&hc, 50)[hl]) {
            let sc = 0.5 * sign(hf - hs) + 0.5 * sign(hc[hl] - hs);
            higher_score = Some(sc);
            let detail = if sc > 0.0 {
                "Tendance de fond haussière"
            } else if sc < 0.0 {
                "Tendance de fond baissière"
            } else {
                "Neutre"
            };
            factors.push(factor("UT supérieure", sc, W_HIGHER, detail.into()));
        }
    }

    let tw: f64 = factors.iter().map(|f| f.weight).sum();
    let score = if tw > 0.0 { factors.iter().map(|f| f.score * f.weight).sum::<f64>() / tw * 100.0 } else { 0.0 };
    let directional: Vec<&Factor> = factors.iter().filter(|f| f.score != 0.0).collect();
    let dw: f64 = directional.iter().map(|f| f.weight).sum();
    let aw: f64 = directional.iter().filter(|f| sign(f.score) == sign(score)).map(|f| f.weight).sum();
    let agreement = if dw > 0.0 { aw / dw } else { 0.0 };
    let confidence = clamp(score.abs() * 1.25, 0.0, 100.0) * (0.5 + 0.5 * agreement);

    let mut action = if score >= 50.0 {
        Action::StrongBuy
    } else if score >= 25.0 {
        Action::Buy
    } else if score <= -50.0 {
        Action::StrongSell
    } else if score <= -25.0 {
        Action::Sell
    } else {
        Action::Hold
    };

    // Never buy against the underlying trend: the signal is downgraded.
    if let Some(h) = higher_score {
        if is_buy(action) && h < 0.0 {
            action = if action == Action::StrongBuy { Action::Buy } else { Action::Hold };
            warnings.push("Achat contre la tendance de l'unité supérieure : signal rétrogradé.".into());
        } else if is_sell(action) && h > 0.0 {
            action = if action == Action::StrongSell { Action::Sell } else { Action::Hold };
            warnings.push("Vente contre la tendance de l'unité supérieure : signal rétrogradé.".into());
        }
    }

    if atr_now > 0.0 && atr_now / price > 0.08 {
        warnings.push(format!("Volatilité extrême (ATR {} % du prix) : réduisez la taille.", to_fixed(atr_now / price * 100.0, 1)));
    }
    let last_time = candles[last].time;
    if let Some(iv) = options.interval_ms.filter(|v| *v != 0) {
        if options.now.unwrap_or_else(now_ms) - last_time > iv * 3 {
            warnings.push("Données possiblement périmées (dernière bougie ancienne).".into());
        }
    }

    let dist = 2.0 * atr_now;
    let sell = is_sell(action);
    Some(Signal {
        action,
        score,
        confidence,
        price,
        time: last_time,
        factors,
        has_plan: atr_now > 0.0,
        stop_loss: if sell { price + dist } else { (price - dist).max(price * 0.01) },
        take_profit: if sell { (price - dist * 2.0).max(price * 0.01) } else { price + dist * 2.0 },
        warnings,
    })
}
