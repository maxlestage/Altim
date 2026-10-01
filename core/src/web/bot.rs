//! « Bot Altim » JSON contract (`web/src/webapp/model-bot.ts`), the part the presentation site reads: the report's
//! v3 headline (/api/bot) and today's views (/api/bot/views). Fields are optional or defaulted where the TypeScript
//! allows an older server; the Bot screen's texts are in `screen`.
use serde::{Deserialize, Serialize};

// The server's own enums (same JSON: "buy", "edge", "peers"…), shared by the site, the Bot screen and the decision.
pub use crate::engine::bot::BotAction;
pub use crate::engine::bot_v3::Family;
pub use crate::engine::validation::Verdict;
use crate::js::fr;
use crate::types::Kind;

/// Narrow no-break space (fr-FR before "%").
pub const NNBSP: &str = "\u{202f}";

/// (label, tone) of an action (`ACTION_UI`).
pub fn action_ui(a: BotAction) -> (&'static str, &'static str) {
    match a {
        BotAction::Buy => ("ACHETER", "buy"),
        BotAction::Wait => ("ATTENDRE", "wait"),
        BotAction::Sell => ("VENDRE", "sell"),
    }
}

/// `FAMILY_LABEL`.
pub fn family_label(f: Family) -> &'static str {
    f.label()
}

/// One view of /api/bot/views (the fields the site reads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BotView {
    pub symbol: String,
    pub kind: Kind,
    pub available: bool,
    #[serde(default)]
    pub action: Option<BotAction>,
    #[serde(default)]
    pub counts: bool,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BotViews {
    #[serde(default)]
    pub as_of: Option<f64>,
    #[serde(default)]
    pub views: Vec<BotView>,
}

pub fn bot_views_url(items: &[(String, Kind)]) -> String {
    let list: Vec<String> = items.iter().take(20).map(|(s, k)| format!("{s}:{}", k.as_str())).collect();
    format!("/api/bot/views?symbols={}", encode_uri_component(&list.join(",")))
}

/// `encodeURIComponent`.
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// One side of a configuration (`SideStats`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SideStats {
    #[serde(default)]
    pub signals: u32,
    #[serde(default)]
    pub excess: Option<f64>,
    #[serde(default)]
    pub t: Option<f64>,
    #[serde(default)]
    pub verdict: Option<Verdict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConfigStats {
    pub buy: SideStats,
    pub sell: SideStats,
    #[serde(default)]
    pub buy_vs_mean: Option<SideStats>,
    #[serde(default)]
    pub sell_vs_mean: Option<SideStats>,
}

impl ConfigStats {
    pub fn side(&self, buy: bool) -> &SideStats {
        if buy { &self.buy } else { &self.sell }
    }
    fn vs_mean(&self, buy: bool) -> Option<&SideStats> {
        if buy { self.buy_vs_mean.as_ref() } else { self.sell_vs_mean.as_ref() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V3Config {
    pub id: String,
    pub family: Family,
    #[serde(default)]
    pub headline: bool,
    pub main: ConfigStats,
    pub forward: ConfigStats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V3Horizon {
    pub horizon: u32,
    #[serde(default)]
    pub configs: Vec<V3Config>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct V3Group {
    pub id: String,
    #[serde(default)]
    pub horizons: Vec<V3Horizon>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KCount {
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct V3Report {
    pub prereg_date: String,
    pub k: KCount,
    pub t_required: f64,
    pub headline: String,
    #[serde(default)]
    pub forward_headline: String,
    #[serde(default)]
    pub groups: Vec<V3Group>,
}

/// /api/bot (the fields the site reads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotReport {
    pub as_of: f64,
    #[serde(default)]
    pub headline: String,
    #[serde(default)]
    pub v3: Option<V3Report>,
}

/// "−1,2" (fr-FR, true minus sign), "—" when missing.
pub fn plain(v: Option<f64>, digits: usize) -> String {
    match v {
        Some(v) if v.is_finite() => format!("{}{}", if v < 0.0 { "−" } else { "" }, fr(v.abs(), 0, digits)),
        _ => "—".into(),
    }
}

/// "12/10/2026" from "2026-10-12".
pub fn fr_iso(s: &str) -> String {
    s.split('-').rev().collect::<Vec<_>>().join("/")
}

/// The 4 headline configurations of a group: (horizon, config).
pub fn headline_configs(g: &V3Group) -> Vec<(u32, &V3Config)> {
    g.horizons.iter().flat_map(|h| h.configs.iter().filter(|c| c.headline).map(move |c| (h.horizon, c))).collect()
}

/// The figures that judge a side: family B's control against the group's mean when measured, else the side itself.
pub fn judged(c: &ConfigStats, buy: bool) -> &SideStats {
    c.vs_mean(buy).unwrap_or(c.side(buy))
}

/// A side proven at the corrected threshold (family B: against the median and the mean).
pub fn proven(c: &ConfigStats, buy: bool) -> bool {
    c.side(buy).verdict == Some(Verdict::Edge) && judged(c, buy).verdict == Some(Verdict::Edge)
}

/// A side proven on the past and confirmed by the forward test (≥ 30 signals, excess ≥ 0 against each reference).
pub fn confirmed(c: &V3Config, buy: bool) -> bool {
    let refs = [Some(c.forward.side(buy)), c.forward.vs_mean(buy)];
    proven(&c.main, buy) && refs.iter().flatten().all(|s| s.signals >= 30 && s.excess.is_some_and(|e| e >= 0.0))
}

/// Signals judged so far in the forward test (headline configurations).
pub fn forward_signals(r: &V3Report) -> u32 {
    r.groups.iter().map(|g| headline_configs(g).iter().map(|(_, c)| c.forward.buy.signals + c.forward.sell.signals).sum::<u32>()).sum()
}

/// "non démontré (t = −1,2)" for a side at the corrected threshold (the presentation site's `verdictShort`).
pub fn verdict_short(cfg: &V3Config, buy: bool) -> String {
    let c = &cfg.main;
    let s = judged(c, buy);
    let t = match s.t {
        None => "t non calculable".to_string(),
        Some(t) => format!("t = {}", plain(Some(t), 1)),
    };
    if confirmed(cfg, buy) {
        return format!("avantage démontré et confirmé sur l'avenir ({t})");
    }
    if proven(c, buy) {
        return format!("avantage mesuré sur le passé ({t}) mais fragile : ne comptera qu'après 30 signaux sur l'avenir qui le confirment");
    }
    if c.side(buy).verdict == Some(Verdict::Edge) {
        return format!("non démontré face à la moyenne du groupe ({t})");
    }
    match s.verdict {
        Some(Verdict::Negative) => format!("moins bien que la référence ({t})"),
        Some(Verdict::Insufficient) => "trop peu de signaux".into(),
        _ => format!("non démontré ({t})"),
    }
}

/// The « Bot Altim » screen, its Radar card and the decision's bot line (the rest of model-bot.ts): texts
/// over the server's own contract (`engine::bot::BotReport`, `engine::bot_v3::V3Report`, `engine::bot::BotView`).
/// Returns and probabilities in %, "points" are differences of % (signal − random day), times in ms. The server
/// computes everything; nothing here recomputes a statistic.
pub mod screen {
    use serde::{Deserialize, Serialize};
    use serde_json::Value;

    use super::{NNBSP, fr_iso, plain};
    pub use super::{action_ui, family_label};
    use crate::engine::bot::{
        BlockOut, BotGroupStat, BotReport, BotView, Bucket, BuyStats, Candidate, Clustered, ExitStats, SellStats, Timing, WaitStats,
    };
    use crate::engine::bot_v3::{ConfigStats, Family, SideStats, V3BlockOut, V3Candidate, V3Config, V3Group, V3Report, VolManaged};
    use crate::engine::validation::Verdict;
    use crate::js::fr;
    use crate::types::Kind;
    use crate::web::insights::validation::signed_pct;

    pub const BOT_URL: &str = "/api/bot";

    fn frm(v: f64, max: usize) -> String {
        fr(v, 0, max)
    }

    /// "+0,52 point", "−2,4 points", "—".
    pub fn points(v: Option<f64>) -> String {
        match v.filter(|v| v.is_finite()) {
            None => "—".into(),
            Some(v) => format!(
                "{}{} point{}",
                if v < 0.0 {
                    "−"
                } else if v > 0.0 {
                    "+"
                } else {
                    ""
                },
                frm(v.abs(), 2),
                if v.abs() >= 2.0 { "s" } else { "" }
            ),
        }
    }

    /// "54 %", "—".
    pub fn pct0(v: Option<f64>) -> String {
        match v.filter(|v| v.is_finite()) {
            None => "—".into(),
            Some(v) => format!("{}{NNBSP}%", frm(v, 0)),
        }
    }

    fn t_text(t: Option<f64>) -> String {
        match t {
            None => "t non calculable".into(),
            Some(t) => format!("t = {}", plain(Some(t), 1)),
        }
    }

    /// The clustered t's of a side, None for a v1 answer (absent there: the server's default is empty).
    pub fn clustered_of(c: &Clustered) -> Option<&Clustered> {
        (c.dates > 0 || c.assets > 0 || c.by_date.is_some() || c.by_asset.is_some() || c.per_signal.is_some()).then_some(c)
    }

    /// v2 (clustered present): the t is by date.
    fn side_t(t: Option<f64>, c: &Clustered) -> String {
        match (clustered_of(c), t) {
            (Some(_), Some(t)) => format!("t par jour = {}", plain(Some(t), 1)),
            _ => t_text(t),
        }
    }

    /// `CANDIDATE_SHORT`.
    pub fn candidate_short(c: Candidate) -> &'static str {
        match c {
            Candidate::V1 => "Logistique v1",
            Candidate::Logit => "Logistique 24",
            Candidate::Trees => "Arbres",
            Candidate::Trend => "Tendance",
        }
    }

    /// "t par jour −2,4 (1067 jours) · par actif −4,5 · par signal −2,8" (the verdict reads the first).
    pub fn clustered_text(c: Option<&Clustered>, t: Option<f64>) -> String {
        let f = |v: Option<f64>| if v.is_none() { "—".to_string() } else { plain(v, 1) };
        match c {
            None => format!("t = {}", f(t)),
            Some(c) => format!("t par jour {} ({} jours) · par actif {} · par signal {}", f(c.by_date), c.dates, f(c.by_asset), f(c.per_signal)),
        }
    }

    /// What leaving at each VENDRE did to the holding: time out, worst fall, return (medians over the assets).
    pub fn exit_text(e: Option<&ExitStats>) -> Option<String> {
        let e = e.filter(|e| e.assets > 0)?;
        Some(format!(
            "En sortant 20 jours à chaque VENDRE : hors marché {} du temps ; pire baisse médiane {} contre {} en gardant ; rendement médian {} contre {} (mieux que garder : {} actif{} sur {}).",
            pct0(e.out_share),
            signed_pct(e.median_bot_max_drawdown, 1),
            signed_pct(e.median_hold_max_drawdown, 1),
            signed_pct(e.median_bot_return, 0),
            signed_pct(e.median_hold_return, 0),
            e.beat_hold,
            if e.beat_hold > 1 { "s" } else { "" },
            e.assets
        ))
    }

    /// "22 actifs du panier et 72 de plus à l'entraînement · historique médian 20,1 ans (le plus long 20,1) · marché : S&P 500 (SPY)".
    pub fn data_text(g: &BotGroupStat) -> Option<String> {
        let u = &g.universe;
        // A v1 answer has no universe (the server's default is empty).
        if u.basket == 0 && u.extra == 0 && u.rows == 0 {
            return None;
        }
        let y = |v: Option<f64>| if v.is_none() { "—".to_string() } else { format!("{} ans", plain(v, 1)) };
        let failed = if u.extra_failed > 0 {
            format!(" ({} indisponible{})", u.extra_failed, if u.extra_failed > 1 { "s" } else { "" })
        } else {
            String::new()
        };
        let market = if g.market.is_empty() { String::new() } else { format!(" · marché : {}", g.market) };
        Some(format!(
            "{} actif{} du panier et {} de plus à l'entraînement{failed} · historique médian {} (le plus long {}){market}",
            u.basket,
            if u.basket > 1 { "s" } else { "" },
            u.extra,
            y(u.median_years),
            y(u.max_years)
        ))
    }

    /// Consecutive retrainings with the same choice: `from` / `to` = start of the first / last retraining of the run.
    #[derive(Debug, Clone, PartialEq)]
    pub struct Run<T> {
        pub from: i64,
        pub to: i64,
        pub chosen: Option<T>,
        pub count: usize,
    }

    fn runs<B, T: PartialEq + Copy>(blocks: &[B], start: impl Fn(&B) -> i64, pick: impl Fn(&B) -> Option<T>) -> Vec<Run<T>> {
        let mut out: Vec<Run<T>> = Vec::new();
        for b in blocks {
            let c = pick(b);
            match out.last_mut() {
                Some(last) if last.chosen == c => {
                    last.to = start(b);
                    last.count += 1;
                }
                _ => out.push(Run { from: start(b), to: start(b), chosen: c, count: 1 }),
            }
        }
        out
    }

    /// v2's retrainings grouped by consecutive identical choices (`selectionRuns`).
    pub fn selection_runs(blocks: &[BlockOut]) -> Vec<Run<Candidate>> {
        runs(blocks, |b| b.start, |b| b.chosen)
    }

    /// Runs of identical choices over the retrainings of one v3 family (`choiceRuns` of BotV3.tsx).
    pub fn choice_runs(blocks: &[V3BlockOut], peers: bool) -> Vec<Run<V3Candidate>> {
        runs(blocks, |b| b.start, |b| if peers { b.peers } else { b.absolute })
    }

    /// "108 achats : +1,7 % en moyenne sur 20 jours, contre +1,85 % pour une entrée au hasard … (−0,15 point, t = −0,1)."
    pub fn buy_text(b: &BuyStats) -> String {
        if b.signals == 0 {
            return "Aucun achat pendant les périodes de test.".into();
        }
        format!(
            "{} achat{} : {} en moyenne sur 20 jours, contre {} pour une entrée au hasard sur le même actif et la même période ({}, {}).",
            b.signals,
            if b.signals > 1 { "s" } else { "" },
            signed_pct(b.mean_net, 2),
            signed_pct(b.baseline_net, 2),
            points(b.excess),
            side_t(b.t_stat, &b.clustered)
        )
    }

    /// What followed the VENDRE signals, said without a sign to decode.
    pub fn sell_text(s: &SellStats) -> String {
        if s.signals == 0 {
            return "Aucune vente pendant les périodes de test.".into();
        }
        let diff = match s.avoided {
            None => String::new(),
            Some(a) if a >= 0.0 => format!("cours ensuite inférieur de {}", points(Some(a)).replacen('+', "", 1)),
            Some(a) => format!("cours ensuite supérieur de {}", points(Some(-a)).replacen('+', "", 1)),
        };
        format!(
            "{} vente{} : le cours a fait {} dans les 20 jours suivants, contre {} après un jour au hasard ({diff}, {}).",
            s.signals,
            if s.signals > 1 { "s" } else { "" },
            signed_pct(s.mean_after, 2),
            signed_pct(s.baseline_after, 2),
            side_t(s.t_stat, &s.clustered)
        )
    }

    pub fn wait_text(w: &WaitStats) -> String {
        if w.days == 0 {
            return "Jamais sur ATTENDRE pendant les tests.".into();
        }
        format!(
            "ATTENDRE {} des jours testés, suivis en moyenne de {} (tous les jours : {}).",
            pct0(w.share),
            signed_pct(w.mean_net, 2),
            signed_pct(w.baseline_net, 2)
        )
    }

    /// Calibration buckets that have days in them.
    pub fn calibration_rows(b: &[Bucket]) -> Vec<&Bucket> {
        b.iter().filter(|x| x.rows > 0).collect()
    }

    /// "meilleur que la fréquence de base" / "moins bon…" from a Brier skill (%).
    pub fn skill_text(skill: Option<f64>) -> String {
        match skill {
            None => "non calculable".into(),
            Some(s) if s.abs() < 0.5 => "pas mieux que la fréquence de base".into(),
            Some(s) if s > 0.0 => format!("{}{NNBSP}% mieux que la fréquence de base", frm(s, 1)),
            Some(s) => format!("{}{NNBSP}% moins bien que la fréquence de base", frm(-s, 1)),
        }
    }

    /// Decision card line: "ATTENDRE · hausse 54 %, baisse 44 %".
    pub fn bot_summary(v: &BotView) -> String {
        match v.action {
            Some(a) if v.available => format!("{} · hausse {}, baisse {}", action_ui(a).0, pct0(v.up), pct0(v.down)),
            _ => v.text.clone(),
        }
    }

    /// Whether any group has an edge on either side (then the headline says which one counts).
    pub fn any_edge(r: &BotReport) -> bool {
        r.groups.iter().any(|g| g.stats.buy.verdict == Some(Verdict::Edge) || g.stats.sell.verdict == Some(Verdict::Edge))
    }

    /// Checks the fields the decision card relies on (`isBotView`).
    pub fn is_bot_view(v: &Value) -> bool {
        v.is_object()
            && v.get("available").is_some_and(Value::is_boolean)
            && v.get("text").is_some_and(Value::is_string)
            && v.get("counts").is_some_and(Value::is_boolean)
    }

    /// A view of /api/bot/views: the decision's `bot` with the asset it is for.
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    pub struct AssetView {
        pub symbol: String,
        pub kind: Kind,
        #[serde(flatten)]
        pub view: BotView,
    }

    /// /api/bot/views with the whole views (the Bot screen's « Vos actifs aujourd'hui »).
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    pub struct AssetViews {
        #[serde(default)]
        pub as_of: Option<f64>,
        #[serde(default)]
        pub views: Vec<AssetView>,
    }

    /// The line of a view: "Hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %) · Arbres", else the server's text.
    pub fn view_line(v: &BotView) -> String {
        if !(v.available && v.action.is_some()) {
            return v.text.clone();
        }
        format!(
            "Hausse {} (seuil {}), baisse {} (seuil {}){}{}",
            pct0(v.up),
            pct0(v.threshold_up),
            pct0(v.down),
            pct0(v.threshold_down),
            v.model_label.as_deref().filter(|l| !l.is_empty()).map(|l| format!(" · {l}")).unwrap_or_default(),
            if v.in_basket { "" } else { " · hors du panier testé" }
        )
    }

    // ---------- v3 ----------

    /// `V3_SHORT`.
    pub fn v3_short(c: V3Candidate) -> &'static str {
        match c {
            V3Candidate::Trend => "Tendance",
            V3Candidate::V1 => "Logistique v1",
            V3Candidate::Logit => "Logistique 24",
            V3Candidate::Trees => "Arbres v2",
            V3Candidate::TreesLong => "Arbres longs",
            V3Candidate::XsMomentum => "Momentum entre pairs",
            V3Candidate::XsLogit => "Logistique entre pairs",
            V3Candidate::XsTrees => "Arbres longs entre pairs",
        }
    }

    /// "t = −1,2 (requis 3,52)".
    pub fn t_vs_required(t: Option<f64>, required: f64) -> String {
        format!("t = {} (requis {})", if t.is_none() { "—".into() } else { plain(t, 1) }, plain(Some(required), 2))
    }

    /// "t par jour 3,7 (requis 3,52) · par actif 0,6": the verdict's t, the one it needs, and the t by asset.
    pub fn t_line(s: &SideStats, required: f64) -> String {
        let f = |v: Option<f64>| if v.is_none() { "—".to_string() } else { plain(v, 1) };
        format!("t par jour {} (requis {}) · par actif {}", f(s.t), plain(Some(required), 2), f(s.t_by_asset))
    }

    /// Verdict at the corrected threshold, in French; says when only the raw t ≥ 2 was reached.
    pub fn v3_verdict_label(s: &SideStats, required: f64) -> String {
        match s.verdict {
            Some(Verdict::Edge) => format!("Avantage au seuil corrigé (t ≥ {}), à confirmer", plain(Some(required), 2)),
            Some(Verdict::Negative) => "Pire que la référence au seuil corrigé".into(),
            Some(Verdict::Insufficient) => "Trop peu de signaux pour conclure".into(),
            _ if s.raw_verdict == Some(Verdict::Edge) => "t ≥ 2 atteint, pas le seuil corrigé : non démontré".into(),
            _ => "Non démontré".into(),
        }
    }

    /// A side of a configuration in one sentence: "412 achats : +0,31 point face à la médiane du groupe (t par jour …)."
    pub fn v3_side_text(family: Family, buy: bool, s: &SideStats, required: f64) -> String {
        if s.signals == 0 {
            return if buy { "Aucun achat." } else { "Aucune vente." }.into();
        }
        let peers = family == Family::Peers;
        let plural = if s.signals > 1 { "s" } else { "" };
        if buy {
            let reference = if peers { "la médiane du groupe" } else { "une entrée au hasard" };
            return format!("{} achat{plural} : {} face à {reference} ({}).", s.signals, points(s.excess), t_line(s, required));
        }
        let verb = if peers { "gain à passer sur l'actif médian" } else { "baisse évitée" };
        format!("{} vente{plural} : {verb} {} ({}).", s.signals, points(s.excess), t_line(s, required))
    }

    /// Forward test of a configuration so far.
    pub fn forward_text(c: &ConfigStats, required: f64) -> String {
        let n = c.buy.signals + c.sell.signals;
        if n == 0 {
            return "Aucun signal jugé pour l'instant (il faut 20 à 60 jours de bourse après le signal).".into();
        }
        format!(
            "{} achat{} ({}, {}) · {} vente{} ({}).",
            c.buy.signals,
            if c.buy.signals > 1 { "s" } else { "" },
            points(c.buy.excess),
            t_vs_required(c.buy.t, required),
            c.sell.signals,
            if c.sell.signals > 1 { "s" } else { "" },
            points(c.sell.excess)
        )
    }

    /// Sharpe, max drawdown, yearly return and volatility of the managed trend vs holding: (label, managed, hold).
    pub fn vol_rows(v: &VolManaged) -> [(&'static str, String, String); 4] {
        [
            ("Ratio de Sharpe", plain(v.managed.sharpe, 2), plain(v.hold.sharpe, 2)),
            ("Pire baisse", signed_pct(v.managed.max_drawdown, 1), signed_pct(v.hold.max_drawdown, 1)),
            ("Rendement annuel", signed_pct(v.managed.annual_return, 1), signed_pct(v.hold.annual_return, 1)),
            ("Volatilité annuelle", pct0(v.managed.annual_vol), pct0(v.hold.annual_vol)),
        ]
    }

    /// "7 min 12 s de calcul sur 1 cœur, pic mémoire 243 Mo (téléchargement 15 s)".
    pub fn compute_text(t: Option<&Timing>) -> Option<String> {
        let t = t?;
        let s = crate::js::round(t.compute_ms as f64 / 1000.0) as i64;
        let dur = if s >= 60 { format!("{} min {} s", s / 60, s % 60) } else { format!("{s} s") };
        let mem = t.peak_rss_mb.map(|m| format!(", pic mémoire {} Mo", crate::js::round(m))).unwrap_or_default();
        Some(format!(
            "{dur} de calcul sur {} cœur{}{mem} (téléchargement {} s)",
            t.threads,
            if t.threads > 1 { "s" } else { "" },
            crate::js::round(t.fetch_ms as f64 / 1000.0)
        ))
    }

    /// The 4 headline configurations of a group: (horizon, config).
    pub fn headline_configs(g: &V3Group) -> Vec<(usize, &V3Config)> {
        g.horizons.iter().flat_map(|h| h.configs.iter().filter(|c| c.headline).map(move |c| (h.horizon, c))).collect()
    }

    fn side(c: &ConfigStats, buy: bool) -> &SideStats {
        if buy { &c.buy } else { &c.sell }
    }

    fn vs_mean(c: &ConfigStats, buy: bool) -> Option<&SideStats> {
        if buy { c.buy_vs_mean.as_ref() } else { c.sell_vs_mean.as_ref() }
    }

    /// The figures that judge a side: family B's control against the group's mean when measured, else the side itself.
    pub fn judged(c: &ConfigStats, buy: bool) -> &SideStats {
        vs_mean(c, buy).unwrap_or(side(c, buy))
    }

    /// A side proven at the corrected threshold (family B: against the median and the mean).
    pub fn proven(c: &ConfigStats, buy: bool) -> bool {
        side(c, buy).verdict == Some(Verdict::Edge) && judged(c, buy).verdict == Some(Verdict::Edge)
    }

    /// A side proven on the past and confirmed by the forward test (≥ 30 signals, excess ≥ 0 against each
    /// reference): only then does it count.
    pub fn confirmed(c: &V3Config, buy: bool) -> bool {
        let refs = [Some(side(&c.forward, buy)), vs_mean(&c.forward, buy)];
        proven(&c.main, buy) && refs.iter().flatten().all(|s| s.signals >= 30 && s.excess.is_some_and(|e| e >= 0.0))
    }

    /// « Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : … » for a proven side awaiting its
    /// forward test; None otherwise.
    pub fn pending_text(c: &V3Config, buy: bool, required: f64) -> Option<String> {
        if !proven(&c.main, buy) || confirmed(c, buy) {
            return None;
        }
        let t = judged(&c.main, buy).t;
        Some(format!(
            "Avantage mesuré sur le passé (t = {} contre {} exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment ({} à ce jour).",
            if t.is_none() { "—".into() } else { plain(t, 2) },
            plain(Some(required), 2),
            side(&c.forward, buy).signals
        ))
    }

    /// Whether a v3 headline configuration has an edge on either side (at the corrected threshold, both references).
    pub fn v3_any_edge(r: &V3Report) -> bool {
        r.groups.iter().any(|g| headline_configs(g).iter().any(|(_, c)| proven(&c.main, true) || proven(&c.main, false)))
    }

    /// Signals judged so far in the forward test (headline configurations).
    pub fn forward_signals(r: &V3Report) -> usize {
        r.groups.iter().map(|g| headline_configs(g).iter().map(|(_, c)| c.forward.buy.signals + c.forward.sell.signals).sum::<usize>()).sum()
    }

    /// The headline's first sentence (the verdict; the details are on the Bot screen).
    pub fn first_sentence(s: &str) -> &str {
        match s.find(". ") {
            None => s,
            Some(i) => &s[..=i],
        }
    }

    /// "Bot v3 · pré-enregistré le 30/09/2026".
    pub fn prereg_title(v3: &V3Report) -> String {
        format!("Bot v3 · pré-enregistré le {}", fr_iso(&v3.prereg_date))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::engine::bot::BotAction;

        fn sample(json: &str) -> BotReport {
            serde_json::from_str(json).unwrap()
        }
        fn v3_report() -> BotReport {
            sample(include_str!("../../../backend/tests/samples/bot.json"))
        }
        fn v2_report() -> BotReport {
            sample(include_str!("../../../backend/tests/samples/bot-v2.json"))
        }
        fn v1_report() -> BotReport {
            sample(include_str!("../../../backend/tests/samples/bot-v1.json"))
        }
        /// Narrow no-break spaces (before %) read as plain spaces in the expectations.
        fn n(s: &str) -> String {
            s.replace('\u{202f}', " ")
        }

        #[test]
        fn contract_sample_v2_adds_up() {
            let r = v2_report();
            assert_eq!(BOT_URL, "/api/bot");
            assert_eq!(r.groups.iter().map(|g| g.id.id()).collect::<Vec<_>>(), ["stock", "crypto"]);
            assert_eq!(r.assets.len() + r.failures.len(), 34);
            assert_eq!(r.overall.buy.signals, r.groups.iter().map(|g| g.stats.buy.signals).sum::<usize>());
            assert_eq!(r.overall.sell.signals, r.groups.iter().map(|g| g.stats.sell.signals).sum::<usize>());
            for g in &r.groups {
                assert_eq!(g.stats.calibration_up.iter().map(|b| b.rows).sum::<usize>(), g.stats.labelled);
                assert_eq!(
                    g.candidates.iter().map(|c| c.id).collect::<Vec<_>>(),
                    [Candidate::V1, Candidate::Logit, Candidate::Trees, Candidate::Trend]
                );
                assert_eq!(g.candidates.iter().map(|c| c.chosen_blocks).sum::<usize>(), g.trained_blocks);
                assert_eq!(g.selection.len(), g.blocks);
                assert_eq!(g.stats.buy.t_stat, g.stats.buy.clustered.by_date);
                assert!(g.universe.extra > 0);
            }
            assert_eq!(r.parameters.horizon_days, 20);
            assert_eq!(r.version, 2);
            assert_eq!(r.features.len(), 24);
            // The real run found no out-of-sample edge: the headline says it does not count.
            assert!(!any_edge(&r));
            assert!(r.headline.starts_with("Hors échantillon, le bot n'a pas fait mieux"));
            assert!(r.headline.contains("il ne compte pas dans les décisions"));
            let g = &r.groups[0];
            assert!(
                data_text(g).unwrap().contains(&format!("{} actifs du panier et {} de plus à l'entraînement", g.universe.basket, g.universe.extra))
            );
            assert!(n(&buy_text(&g.stats.buy)).contains("t par jour ="));
            assert!(wait_text(&g.stats.wait).starts_with("ATTENDRE"));
            assert_eq!(buy_text(&BuyStats { signals: 0, ..g.stats.buy.clone() }), "Aucun achat pendant les périodes de test.");
        }

        #[test]
        fn a_v1_answer_still_reads() {
            let v1 = v1_report();
            assert_eq!(data_text(&v1.groups[0]), None);
            assert_eq!(exit_text(Some(&v1.groups[0].stats.sell.exit)), None);
            assert!(n(&buy_text(&v1.groups[0].stats.buy)).contains("(−0,15 point, t = −0,1)"));
            assert!(v1.groups[0].candidates.is_empty());
        }

        #[test]
        fn v2_texts() {
            let c = Clustered { by_date: Some(-2.4), dates: 1067, by_asset: Some(-4.46), assets: 22, per_signal: Some(-2.84) };
            assert_eq!(clustered_text(Some(&c), Some(-2.4)), "t par jour −2,4 (1067 jours) · par actif −4,5 · par signal −2,8");
            assert_eq!(clustered_text(None, Some(1.25)), "t = 1,3");
            let e = ExitStats {
                assets: 22,
                out_share: Some(15.5),
                median_hold_max_drawdown: Some(-46.13),
                median_bot_max_drawdown: Some(-47.93),
                median_drawdown_avoided: Some(-0.15),
                median_hold_return: Some(647.46),
                median_bot_return: Some(289.42),
                beat_hold: 1,
            };
            assert_eq!(
                n(&exit_text(Some(&e)).unwrap()),
                "En sortant 20 jours à chaque VENDRE : hors marché 16 % du temps ; pire baisse médiane −47,9 % contre −46,1 % en gardant ; rendement médian +289 % contre +647 % (mieux que garder : 1 actif sur 22)."
            );
            assert_eq!(exit_text(Some(&ExitStats::default())), None);
            let b = |start: i64, end: Option<i64>, chosen: Option<Candidate>| BlockOut { start, end, train_rows: 1, chosen, scores: vec![] };
            assert_eq!(
                selection_runs(&[
                    b(1, Some(2), Some(Candidate::Trend)),
                    b(2, Some(3), Some(Candidate::Trend)),
                    b(3, None, Some(Candidate::V1)),
                    b(4, None, None)
                ]),
                [
                    Run { from: 1, to: 2, chosen: Some(Candidate::Trend), count: 2 },
                    Run { from: 3, to: 3, chosen: Some(Candidate::V1), count: 1 },
                    Run { from: 4, to: 4, chosen: None, count: 1 },
                ]
            );
            assert_eq!(candidate_short(Candidate::Trees), "Arbres");
        }

        #[test]
        fn texts() {
            assert_eq!(points(Some(0.52)), "+0,52 point");
            assert_eq!(points(Some(-2.44)), "−2,44 points");
            assert_eq!(points(None), "—");
            assert_eq!(n(&skill_text(Some(-1.1))), "1,1 % moins bien que la fréquence de base");
            assert_eq!(skill_text(Some(0.2)), "pas mieux que la fréquence de base");
            assert_eq!(skill_text(None), "non calculable");
            let empty = Bucket { from: 0.0, to: 30.0, label: "x".into(), rows: 0, predicted: None, realised: None };
            assert!(calibration_rows(&[empty]).is_empty());
            let s = SellStats {
                signals: 7,
                mean_after: Some(-1.01),
                baseline_after: Some(-3.45),
                all_days_after: Some(1.6),
                avoided: Some(-2.44),
                t_stat: Some(-0.66),
                fall_rate: Some(71.4),
                baseline_fall_rate: Some(45.7),
                mean_drawdown: Some(-11.0),
                baseline_drawdown: Some(-8.8),
                verdict: Some(Verdict::Insufficient),
                verdict_label: "Trop peu de ventes pour conclure".into(),
                ..SellStats::default()
            };
            assert!(n(&sell_text(&s)).contains("cours ensuite supérieur de 2,44 points"));
            assert!(n(&sell_text(&SellStats { avoided: Some(1.2), ..s.clone() })).contains("cours ensuite inférieur de 1,2 point"));
            assert_eq!(sell_text(&SellStats { signals: 0, ..s }), "Aucune vente pendant les périodes de test.");
        }

        #[test]
        fn decision_line() {
            let view: BotView = serde_json::from_value(serde_json::json!({
                "available": true, "group": "crypto", "groupLabel": "Cryptos", "inBasket": true, "action": "wait", "actionLabel": "ATTENDRE",
                "up": 46.3, "down": 52, "thresholdUp": 49.7, "thresholdDown": 58.4, "baseUp": 44.7, "baseDown": 53.4, "buyVerdict": "unproven",
                "sellVerdict": "insufficient", "counts": false, "time": 1,
                "contributions": [{ "id": "rsi14", "label": "RSI 14", "value": 40, "valueText": "40", "weight": -0.2, "effect": "down", "text": "RSI 14 : 40 (pèse contre la hausse)" }],
                "text": "ATTENDRE : …", "note": "Le bot n'a pas démontré d'avantage hors échantillon : il ne compte pas dans la décision.", "asOf": 1, "link": "/app/bot"
            }))
            .unwrap();
            assert!(is_bot_view(&serde_json::to_value(&view).unwrap()));
            assert!(!is_bot_view(&serde_json::json!({ "available": true })));
            assert_eq!(n(&bot_summary(&view)), "ATTENDRE · hausse 46 %, baisse 52 %");
            assert_eq!(action_ui(BotAction::Sell).0, "VENDRE");
            let na = BotView { available: false, action: None, contributions: vec![], text: "Bot pas encore entraîné".into(), ..view.clone() };
            assert_eq!(bot_summary(&na), "Bot pas encore entraîné");
            assert_eq!(n(&view_line(&view)), "Hausse 46 % (seuil 50 %), baisse 52 % (seuil 58 %)");
            let views: AssetViews = serde_json::from_value(serde_json::json!({
                "asOf": null, "views": [{ "symbol": "BTC", "kind": "crypto", "available": false, "counts": false, "text": "Bot pas encore entraîné" }]
            }))
            .unwrap();
            assert_eq!((views.views[0].symbol.as_str(), views.views[0].view.text.as_str()), ("BTC", "Bot pas encore entraîné"));
        }

        #[test]
        fn v3_contract_sample() {
            let report = v3_report();
            let v3 = report.v3.as_ref().unwrap();
            assert_eq!(report.version, 3);
            assert_eq!(v3.prereg_date, "2026-09-30");
            assert_eq!((v3.k.v1, v3.k.v2, v3.k.v3, v3.k.total), (4, 20, 90, 114));
            assert!((v3.t_required - 3.5157).abs() < 0.001);
            assert_eq!(v3.parameters.horizons, [20, 60]);
            assert_eq!(v3.groups.iter().map(|g| g.id.id()).collect::<Vec<_>>(), ["stock", "crypto"]);
            for g in &v3.groups {
                assert_eq!(g.horizons.iter().map(|h| h.horizon).collect::<Vec<_>>(), [20, 60]);
                for h in &g.horizons {
                    let ids: Vec<&str> = h.configs.iter().map(|c| c.id.as_str()).collect();
                    assert_eq!(ids, ["absolute", "peers", "v2", "trend", "v1", "logit", "trees", "treesLong", "xsMomentum", "xsLogit", "xsTrees"]);
                    assert_eq!(h.selection.len(), h.blocks);
                    for c in &h.configs {
                        // Family B also judged against the group's mean (control listed as added after the pre-registration).
                        assert_eq!(c.main.buy_vs_mean.is_some() && c.main.sell_vs_mean.is_some(), c.family == Family::Peers);
                        if proven(&c.main, true) {
                            assert!(judged(&c.main, true).t.unwrap() >= v3.t_required);
                        }
                        for s in [&c.main.buy, &c.main.sell] {
                            if s.verdict == Some(Verdict::Edge) {
                                assert!(s.t.unwrap() >= v3.t_required);
                            }
                            if s.signals < 30 {
                                assert_eq!(s.verdict, Some(Verdict::Insufficient));
                            }
                        }
                    }
                }
                assert_eq!(headline_configs(g).len(), 4);
            }
            assert_eq!(report.headline, v3.headline);
            assert_eq!(v3.after_prereg.len(), 2);
            assert!(v3.after_prereg[0].contains("moyenne à parts égales"));
            assert!(v3.after_prereg[1].contains("au moins 30 signaux"));
            // The one side proven on the past (stocks, peers, 20 days, buys) waits for the forward test: shown, not counted.
            let peers20 = v3.groups[0].horizons[0].configs.iter().find(|c| c.id == "peers").unwrap();
            assert!(proven(&peers20.main, true));
            assert!(!confirmed(peers20, true));
            assert_eq!(
                n(&pending_text(peers20, true, v3.t_required).unwrap()),
                "Avantage mesuré sur le passé (t = 3,65 contre 3,52 exigé) mais fragile : il ne comptera qu'après 30 signaux sur l'avenir qui le confirment (0 à ce jour)."
            );
            assert!(v3.headline.contains("mais fragile"));
            assert!(v3.headline.contains("le bot ne pèse pas dans les décisions"));
            assert!(v3.headline.starts_with("Bot v3 : "));
            assert_eq!(v3.headline.contains("aucun avantage démontré"), !v3_any_edge(v3));
            assert!(report.assets.iter().all(|a| a.v3.is_some()));
            assert_eq!(prereg_title(v3), "Bot v3 · pré-enregistré le 30/09/2026");
            // Radar card: the first sentence of the live headline, the forward counter.
            assert!(first_sentence(&v3.headline).ends_with("le bot ne pèse pas dans les décisions."));
            assert_eq!(forward_signals(v3), 0);
            assert_eq!(first_sentence("A. B. C"), "A.");
            assert_eq!(first_sentence("A"), "A");
            // A v2 answer has no v3 part.
            assert!(v2_report().v3.is_none());
        }

        fn side_stats(f: impl FnOnce(&mut SideStats)) -> SideStats {
            let mut s = SideStats {
                signals: 120,
                mean: Some(1.0),
                baseline: Some(0.8),
                excess: Some(0.2),
                t: Some(1.24),
                dates: 100,
                t_by_asset: Some(0.9),
                assets: 20,
                t_per_signal: Some(1.5),
                beat_share: Some(52.0),
                verdict: Some(Verdict::Unproven),
                raw_verdict: Some(Verdict::Unproven),
            };
            f(&mut s);
            s
        }

        #[test]
        fn v3_texts() {
            assert_eq!(t_vs_required(Some(1.24), 3.5157), "t = 1,2 (requis 3,52)");
            assert_eq!(t_vs_required(None, 3.5157), "t = — (requis 3,52)");
            assert_eq!(v3_verdict_label(&side_stats(|_| {}), 3.52), "Non démontré");
            assert_eq!(
                v3_verdict_label(
                    &side_stats(|s| {
                        s.t = Some(2.4);
                        s.raw_verdict = Some(Verdict::Edge)
                    }),
                    3.52
                ),
                "t ≥ 2 atteint, pas le seuil corrigé : non démontré"
            );
            assert_eq!(
                v3_verdict_label(
                    &side_stats(|s| {
                        s.verdict = Some(Verdict::Edge);
                        s.t = Some(4.0)
                    }),
                    3.5157
                ),
                "Avantage au seuil corrigé (t ≥ 3,52), à confirmer"
            );
            assert_eq!(
                v3_verdict_label(
                    &side_stats(|s| {
                        s.verdict = Some(Verdict::Insufficient);
                        s.signals = 3
                    }),
                    3.52
                ),
                "Trop peu de signaux pour conclure"
            );
            assert_eq!(
                n(&v3_side_text(Family::Peers, true, &side_stats(|_| {}), 3.5157)),
                "120 achats : +0,2 point face à la médiane du groupe (t par jour 1,2 (requis 3,52) · par actif 0,9)."
            );
            let sell = side_stats(|s| {
                s.excess = Some(-2.5);
                s.t = Some(-4.51);
                s.t_by_asset = None
            });
            assert_eq!(
                n(&v3_side_text(Family::Absolute, false, &sell, 3.5157)),
                "120 ventes : baisse évitée −2,5 points (t par jour −4,5 (requis 3,52) · par actif —)."
            );
            let t = side_stats(|s| {
                s.t = Some(3.65);
                s.t_by_asset = Some(0.62)
            });
            assert_eq!(t_line(&t, 3.5157), "t par jour 3,7 (requis 3,52) · par actif 0,6");
            assert_eq!(v3_side_text(Family::Peers, false, &side_stats(|s| s.signals = 0), 3.5), "Aucune vente.");
        }

        #[test]
        fn forward_managed_trend_compute_dates() {
            let empty = ConfigStats { buy: side_stats(|s| s.signals = 0), sell: side_stats(|s| s.signals = 0), ..ConfigStats::default() };
            assert!(forward_text(&empty, 3.5).starts_with("Aucun signal jugé pour l'instant"));
            assert_eq!(fr_iso("2026-09-30"), "30/09/2026");
            let report = v3_report();
            let g = &report.v3.as_ref().unwrap().groups[0];
            let rows = vol_rows(g.vol_managed.as_ref().unwrap());
            assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), ["Ratio de Sharpe", "Pire baisse", "Rendement annuel", "Volatilité annuelle"]);
            let t = Timing { fetch_ms: 15_400, compute_ms: 432_000, threads: 1, rss_before_mb: None, peak_rss_mb: Some(243.4) };
            assert_eq!(compute_text(Some(&t)).unwrap(), "7 min 12 s de calcul sur 1 cœur, pic mémoire 243 Mo (téléchargement 15 s)");
            assert_eq!(compute_text(None), None);
            let blocks: Vec<V3BlockOut> = [Some(V3Candidate::Trend), Some(V3Candidate::Trend), None]
                .iter()
                .enumerate()
                .map(|(i, c)| V3BlockOut { start: i as i64, absolute: *c, ..V3BlockOut::default() })
                .collect();
            let r = choice_runs(&blocks, false);
            assert_eq!((r.len(), r[0].count, r[0].to, r[1].chosen), (2, 2, 1, None));
            assert!(choice_runs(&blocks, true).iter().all(|x| x.chosen.is_none()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(signals: u32, excess: f64, t: f64, v: Verdict) -> SideStats {
        SideStats { signals, excess: Some(excess), t: Some(t), verdict: Some(v) }
    }

    #[test]
    fn verdicts() {
        let mut c = V3Config {
            id: "x".into(),
            family: Family::Absolute,
            headline: true,
            main: ConfigStats { buy: side(100, 0.5, 3.7, Verdict::Edge), sell: side(10, -0.2, -1.24, Verdict::Unproven), ..Default::default() },
            forward: ConfigStats { buy: side(12, 0.1, 0.4, Verdict::Insufficient), ..Default::default() },
        };
        assert!(verdict_short(&c, true).starts_with("avantage mesuré sur le passé (t = 3,7) mais fragile"));
        assert_eq!(verdict_short(&c, false), "non démontré (t = −1,2)");
        c.forward.buy.signals = 30;
        assert_eq!(verdict_short(&c, true), "avantage démontré et confirmé sur l'avenir (t = 3,7)");
        c.main.buy_vs_mean = Some(side(100, 0.1, 1.0, Verdict::Unproven));
        assert_eq!(verdict_short(&c, true), "non démontré face à la moyenne du groupe (t = 1)");
        assert_eq!(fr_iso("2026-09-30"), "30/09/2026");
        assert_eq!(plain(None, 1), "—");
    }

    #[test]
    fn parses_partial_report_and_views() {
        let r: BotReport = serde_json::from_str(r#"{"asOf":1,"headline":"h","v3":{"preregDate":"2026-09-30","k":{"total":9,"v1":1},"tRequired":3.52,"headline":"H","forwardHeadline":"F","groups":[{"id":"stock","horizons":[{"horizon":20,"configs":[{"id":"a","family":"peers","headline":true,"main":{"buy":{"signals":1},"sell":{"signals":2}},"forward":{"buy":{"signals":3},"sell":{"signals":4}}}]}]}]}}"#).unwrap();
        assert_eq!(forward_signals(r.v3.as_ref().unwrap()), 7);
        let v: BotViews = serde_json::from_str(
            r#"{"asOf":null,"views":[{"symbol":"BTC","kind":"crypto","available":true,"action":"buy","counts":false,"text":""}]}"#,
        )
        .unwrap();
        assert_eq!(v.views[0].action, Some(BotAction::Buy));
        assert_eq!(
            bot_views_url(&[("BTC".into(), Kind::Crypto), ("AAPL".into(), Kind::Stock)]),
            "/api/bot/views?symbols=BTC%3Acrypto%2CAAPL%3Astock"
        );
    }
}
