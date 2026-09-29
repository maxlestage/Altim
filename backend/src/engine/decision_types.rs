//! Contract of `GET /api/decision` (read by the web app, iPhone and Android): one decision per asset, with its
//! reasons, what would change it and what could make it wrong. Informational by default (data and scenarios
//! observed); "personal" only when the user's own position or portfolio weights are passed (never stored).
//!
//! Every figure comes from a named source; what no free and verifiable source gives is marked unavailable,
//! never estimated.
//!
//! Units: prices and amounts in USD; fields named `*_pct`, `*_margin`, `*_growth`, `*_yield`, `*_change`,
//! `win_rate`, `confidence`, `weight`, `share`, `circulating_pct`, `btc_dominance`, `roe`, `roic`, `roic_tax_rate`,
//! `percentile` are percentages (46.8 = 46,8 %); `mc_fdv`, `per`, `ps`, `pb` and correlations are ratios;
//! `funding_rate` is the fraction per 8-hour period as published by the exchange (6.88e-05 = 0,0069 %); `hash_rate`
//! in hashes per second; times in ms since the epoch.
use serde::{Deserialize, Serialize};

pub use super::guidance::{
    ActionZone, ActionZones, CheckState, CounterArgument, DecisionSnapshot, Invalidator, NoTrade, NoTradeReason, ScenarioCheck, SnapshotFamily,
    SnapshotLevel, SnapshotNews, Unfolding,
};
pub use super::model_evidence::ModelEvidence;
pub use super::structure::Structure;
pub use super::synthesis::{CompositeScore, Degraded, HorizonClass, MarketRegime, Rating};
use crate::types::Kind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Setup complete (zone reached + confirmation), no veto, risk/reward acceptable.
    Buy,
    /// Price in the buy zone, confirmation still missing: a limit order or patience.
    BuyZone,
    /// Nothing to do now (no setup, or a veto, or a risk/reward too low).
    Wait,
    /// Stay out: main trend down or a blocking veto, not held.
    NoPosition,
    /// Held: take part of the profits or reduce the risk.
    Trim,
    /// Held: scenario invalidated (support broken, trend reversed, macro shock).
    Sell,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Buy => "ACHETER",
            Verdict::BuyZone => "ZONE D'ACHAT",
            Verdict::Wait => "ATTENDRE",
            Verdict::NoPosition => "AUCUNE POSITION",
            Verdict::Trim => "ALLÉGER",
            Verdict::Sell => "VENDRE",
        }
    }
}

/// 🟢 strong · 🟡 moderate · ⚪ waiting · 🟠 highRisk · 🔴 exit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    Strong,
    Moderate,
    Waiting,
    HighRisk,
    Exit,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Strong => "Signal fort",
            Level::Moderate => "Signal modéré",
            Level::Waiting => "Attente",
            Level::HighRisk => "Risque élevé",
            Level::Exit => "Sortie / risque d'invalidation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Positive,
    Neutral,
    Negative,
    /// No free and verifiable source, or not enough history: shown as such, never guessed.
    Unavailable,
}

/// One independent family of evidence (trend, momentum, volume, volatility, valuation, fundamentals, macro,
/// sentiment, news, liquidity, onchain).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Family {
    /// "trend" | "momentum" | "volume" | "volatility" | "valuation" | "fundamentals" | "macro" | "sentiment" |
    /// "news" | "liquidity" | "onchain"
    pub key: String,
    pub label: String,
    /// −100 (very unfavourable) … +100 (very favourable); None when unavailable.
    pub score: Option<f64>,
    pub status: Status,
    /// One line ("Moyennes 50 et 200 jours alignées, ADX 31").
    pub summary: String,
    pub points: Vec<String>,
    pub source: String,
}

/// A reason that forbids buying now. All checks are listed (active or not), so the user sees what was verified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Veto {
    /// "spread" | "liquidity" | "volatility" | "event" | "earnings" | "crash" | "pump" | "volumeAnomaly" |
    /// "regulation" | "unlock" | "divergence" | "downtrend" | "riskReward" | "macro" | "reliability"
    pub code: String,
    pub label: String,
    pub active: bool,
    /// false: no source to check it (e.g. token unlocks), shown as "non vérifiable".
    pub verifiable: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StepState {
    Ok,
    No,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub label: String,
    pub state: StepState,
    pub detail: String,
}

/// The entry pattern being looked for (e.g. "Achat sur repli en tendance haussière", 9 steps).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    pub name: String,
    pub steps: Vec<Step>,
    pub met: usize,
    pub total: usize,
}

/// Entry plan: buy zone, stop / invalidation, three targets, risk/reward (gain to target 1 ÷ risk to the stop).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub zone_from: f64,
    pub zone_to: f64,
    /// Price used for the ratio: the current price when inside the zone, else the highest price of the zone that
    /// still gives the minimum risk/reward (the top of the zone when all of it does, its bottom when none does).
    pub entry: f64,
    pub stop: f64,
    pub target1: f64,
    pub target2: Option<f64>,
    pub risk_pct: f64,
    pub reward1_pct: f64,
    pub reward2_pct: Option<f64>,
    pub risk_reward: f64,
    /// Below it, no entry (2.0).
    pub min_risk_reward: f64,
    pub acceptable: bool,
    /// Horizon of the zone ("moyen terme · quelques semaines").
    pub horizon: String,
    /// Target 3: the nearest support / resistance level above target 2, capped by the projection target 2 +
    /// (target 2 − target 1); None without target 2.
    #[serde(default)]
    pub target3: Option<f64>,
    #[serde(default)]
    pub reward3_pct: Option<f64>,
    /// Where target 3 comes from ("résistance touchée 3 fois", "projection …").
    #[serde(default)]
    pub target3_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub text: String,
    /// Price level to watch, when there is one.
    pub level: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScenarioKind {
    Bull,
    Neutral,
    Bear,
}

/// "Si X se produit → …": never a prediction, what to watch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub kind: ScenarioKind,
    pub title: String,
    pub condition: String,
    pub consequence: String,
    pub level: Option<f64>,
    /// Conditions checked on the current data (3 for the bullish and bearish ones, 2 for the neutral one).
    #[serde(default)]
    pub conditions: Vec<ScenarioCheck>,
    /// Conditions met.
    #[serde(default)]
    pub met: usize,
    /// The scenario being realised (see `Decision::unfolding`).
    #[serde(default)]
    pub unfolding: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Uncertainty {
    Low,
    Medium,
    High,
}

/// "Pourquoi pas ?": what could make this decision wrong, searched on purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhyNot {
    pub risks: Vec<String>,
    pub uncertainty: Uncertainty,
    pub invalidation: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EarningsDate {
    /// ms, New York date.
    pub date: i64,
    /// true when the date is estimated by the data vendor (not announced by the company).
    pub estimated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EarningsSurprise {
    /// "Q2 2026"
    pub quarter: String,
    pub eps: f64,
    pub consensus: f64,
    pub surprise_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revisions {
    /// Consensus EPS of the current fiscal year, one month ago and now.
    pub month_ago: f64,
    pub now: f64,
    pub change_pct: f64,
}

/// Sector of a company: its SIC code as filed at the SEC (submissions API), grouped by SIC division.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sector {
    /// Short French label of the SIC division ("Industrie manufacturière").
    pub label: String,
    /// "3571"
    pub sic: String,
    /// As filed: "Electronic Computers".
    pub sic_description: String,
    pub source: String,
}

/// A valuation ratio against its own daily history (split-inconsistent days left out, never adjusted).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RatioHistory {
    /// Today's ratio (current price, last twelve months filed).
    pub current: f64,
    pub median: f64,
    pub min: f64,
    pub max: f64,
    /// Share of the days of the window with a ratio at or below today's, % (90 = more expensive than 90 % of days).
    pub percentile: f64,
    /// Number of daily values in the window.
    pub days: usize,
    /// First and last day of the window (ms).
    pub from: i64,
    pub to: i64,
}

/// P/E and P/S against their own history (up to 5 years of daily closes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValuationHistory {
    pub per: Option<RatioHistory>,
    pub ps: Option<RatioHistory>,
    /// How the daily values are computed (French).
    pub method: String,
    pub source: String,
}

/// A comparable company: same activity (Nasdaq classification), figures from its own SEC filings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub symbol: String,
    pub name: String,
    pub per: Option<f64>,
    pub ps: Option<f64>,
    pub operating_margin: Option<f64>,
    pub net_margin: Option<f64>,
    pub revenue_growth: Option<f64>,
    /// End of its last twelve months filed (ms).
    pub period_end: i64,
}

/// Comparison with companies of the same activity: medians of the peers whose figures were actually read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerComparison {
    /// Activity shared with the peers (Nasdaq classification, in English as published).
    pub group: String,
    pub peers: Vec<Peer>,
    pub median_per: Option<f64>,
    pub median_ps: Option<f64>,
    pub median_operating_margin: Option<f64>,
    pub median_net_margin: Option<f64>,
    pub median_revenue_growth: Option<f64>,
    /// Date of the peers' prices (ms).
    pub date: i64,
    pub source: String,
}

/// Company figures from its filings (SEC EDGAR), trailing twelve months unless said otherwise. None = not reported.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StockFundamentals {
    /// "12 mois au 28/06/2026 (10-Q)"
    pub period: String,
    pub revenue: Option<f64>,
    pub revenue_growth: Option<f64>,
    pub net_income: Option<f64>,
    pub eps: Option<f64>,
    pub eps_growth: Option<f64>,
    pub gross_margin: Option<f64>,
    pub operating_margin: Option<f64>,
    pub net_margin: Option<f64>,
    pub free_cash_flow: Option<f64>,
    pub fcf_margin: Option<f64>,
    pub debt: Option<f64>,
    pub cash: Option<f64>,
    pub net_debt: Option<f64>,
    pub roe: Option<f64>,
    pub per: Option<f64>,
    pub peg: Option<f64>,
    pub ev_ebitda: Option<f64>,
    pub dividend_yield: Option<f64>,
    /// Change of the share count over a year, % (negative = buybacks).
    pub share_change: Option<f64>,
    pub next_earnings: Option<EarningsDate>,
    pub surprises: Vec<EarningsSurprise>,
    pub revisions: Option<Revisions>,
    /// Sector comparison, or why it is not given.
    pub sector_note: String,
    pub source: String,
    /// Price ÷ sales: market cap ÷ revenue (12 months).
    #[serde(default)]
    pub ps: Option<f64>,
    /// Price ÷ book: market cap ÷ stockholders' equity at `period_end`.
    #[serde(default)]
    pub pb: Option<f64>,
    /// Return on invested capital, %: operating income × (1 − tax rate) ÷ (debt + equity − cash).
    #[serde(default)]
    pub roic: Option<f64>,
    /// Tax rate used for the ROIC, % (effective: income tax ÷ pre-tax income, kept within 0–50 %).
    #[serde(default)]
    pub roic_tax_rate: Option<f64>,
    /// true when the effective rate could not be computed and the 21 % US federal statutory rate is used.
    #[serde(default)]
    pub roic_tax_statutory: bool,
    /// End of the last period filed and date of that filing (ms, midnight UTC).
    #[serde(default)]
    pub period_end: Option<i64>,
    #[serde(default)]
    pub filed_at: Option<i64>,
    #[serde(default)]
    pub sector: Option<Sector>,
    #[serde(default)]
    pub valuation_history: Option<ValuationHistory>,
    #[serde(default)]
    pub peers: Option<PeerComparison>,
    /// Valuation vs growth in one sentence (P/E percentile, PEG, EPS growth).
    #[serde(default)]
    pub valuation_verdict: Option<String>,
    /// Management guidance, or why it is not given.
    #[serde(default)]
    pub guidance: String,
}

/// Developer activity of the project's code (CoinGecko, else its main GitHub repository).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevActivity {
    /// "owner/name" of the repository read (GitHub only).
    pub repo: Option<String>,
    /// Commits over the last 4 weeks.
    pub commits4w: Option<f64>,
    /// Pull requests merged since the repository was created.
    pub pull_requests_merged: Option<f64>,
    pub contributors: Option<f64>,
    pub stars: Option<f64>,
    /// Lines added / deleted over the last 4 weeks.
    pub additions4w: Option<f64>,
    pub deletions4w: Option<f64>,
    /// CoinGecko classifies the asset as a smart-contract platform.
    pub smart_contract_platform: bool,
    pub source: String,
}

/// Stablecoins in circulation (USD value, DefiLlama): the crypto market's cash, a liquidity indicator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StablecoinFlows {
    /// "Tous réseaux" or the chain's name.
    pub scope: String,
    /// Day of the last value (ms, midnight UTC).
    pub date: i64,
    pub total: f64,
    /// Change over 7 and 30 days, USD and %.
    pub change7d: Option<f64>,
    pub change7d_pct: Option<f64>,
    pub change30d: Option<f64>,
    pub change30d_pct: Option<f64>,
    pub source: String,
}

/// Token and network figures. None = not given by a free verifiable source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CryptoFundamentals {
    pub market_cap: Option<f64>,
    pub fdv: Option<f64>,
    /// market cap ÷ FDV (1 = everything already in circulation).
    pub mc_fdv: Option<f64>,
    pub circulating_supply: Option<f64>,
    pub total_supply: Option<f64>,
    pub max_supply: Option<f64>,
    pub circulating_pct: Option<f64>,
    pub tvl: Option<f64>,
    pub fees30d: Option<f64>,
    pub btc_dominance: Option<f64>,
    pub funding_rate: Option<f64>,
    pub open_interest: Option<f64>,
    /// Bitcoin only (blockchain.com): transactions per day and hash rate.
    pub tx_per_day: Option<f64>,
    pub hash_rate: Option<f64>,
    /// "non vérifiable : aucune source gratuite (DefiLlama le réserve à son offre payante)".
    pub unlocks: String,
    pub source: String,
    #[serde(default)]
    pub dev_activity: Option<DevActivity>,
    /// All chains, and the asset's own chain when it is one.
    #[serde(default)]
    pub stablecoins: Option<StablecoinFlows>,
    #[serde(default)]
    pub chain_stablecoins: Option<StablecoinFlows>,
    /// What is not covered and why (no free verifiable source).
    #[serde(default)]
    pub not_covered: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
// One value per decision: the size difference between the variants does not matter.
#[allow(clippy::large_enum_variant)]
pub enum Fundamentals {
    Stock(StockFundamentals),
    Crypto(CryptoFundamentals),
}

/// Liquidity at the time of the decision (top of the order book, traded value).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Liquidity {
    pub spread_pct: Option<f64>,
    /// Average daily traded value over 20 days, USD.
    pub daily_value: Option<f64>,
    /// Today's volume ÷ 20-day average.
    pub relative_volume: Option<f64>,
    pub source: String,
}

/// Past behaviour of the signal on this asset (daily candles, walk-forward: each decision only sees the candles
/// before it), fees and slippage included.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub period: String,
    pub trades: usize,
    pub win_rate: f64,
    pub avg_win: Option<f64>,
    pub avg_loss: Option<f64>,
    pub profit_factor: Option<f64>,
    pub sharpe: Option<f64>,
    pub sortino: Option<f64>,
    pub max_drawdown: f64,
    pub total_return: f64,
    pub buy_and_hold: f64,
    pub fees_pct: f64,
    pub slippage_pct: f64,
    /// Longest losing streak (trades).
    pub losing_streak: usize,
    pub note: String,
    /// Spread cost, expectancy, R multiples, results by market regime (fields at this level in the JSON).
    #[serde(flatten, default)]
    pub details: super::metrics::TrackDetails,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExitKind {
    Profit,
    Defensive,
    Macro,
}

/// A step of a progressive exit ("vendre 20 % si …").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exit {
    pub kind: ExitKind,
    /// Share of the position, % (20, 30, …; the rest is kept).
    pub share: f64,
    pub trigger: String,
    pub price: Option<f64>,
    /// Condition already met now.
    pub now: bool,
}

/// Only when the user passes their average cost (`cost=`): their position, never stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub cost: f64,
    pub pnl_pct: Option<f64>,
    pub advice: String,
    pub exits: Vec<Exit>,
}

/// Only when the user passes their portfolio weights (`weights=`): exposure to the same risk factor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exposure {
    /// "Bitcoin" | "S&P 500"
    pub factor: String,
    /// Share of the portfolio in assets correlated above 0.7 to that factor (90 days of daily returns), %.
    pub weight: f64,
    pub assets: Vec<String>,
    /// This asset's correlation to the factor.
    pub correlation: Option<f64>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataSource {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub symbol: String,
    pub kind: Kind,
    pub name: String,
    pub as_of: i64,
    pub price: Option<f64>,
    /// "informational" (market data only) | "personal" (the user's own position or weights were used).
    pub mode: String,
    pub verdict: Verdict,
    pub label: String,
    pub level: Level,
    pub level_label: String,
    /// 0–100: agreement of the families, data reliability, the signal's own track record.
    pub confidence: f64,
    pub confidence_text: String,
    /// One sentence: the verdict and its main reasons.
    pub headline: String,
    pub families: Vec<Family>,
    pub vetoes: Vec<Veto>,
    /// At least one active veto.
    pub blocked: bool,
    pub setup: Setup,
    pub plan: Option<Plan>,
    /// Why not buy now (empty when the verdict is Buy).
    pub why_wait: Vec<String>,
    pub to_buy: Vec<Condition>,
    pub to_sell: Vec<Condition>,
    pub scenarios: Vec<Scenario>,
    pub pros: Vec<String>,
    pub cons: Vec<String>,
    pub why_not: WhyNot,
    pub fundamentals: Option<Fundamentals>,
    pub liquidity: Option<Liquidity>,
    pub track: Option<Track>,
    pub position: Option<Position>,
    pub exposure: Option<Exposure>,
    pub sources: Vec<DataSource>,
    pub disclaimer: String,
    /// 6-level rating derived from the verdict, its level, the confidence and the trend (the verdict is unchanged).
    #[serde(default)]
    pub rating: Rating,
    /// "ACHAT FORT" | "ACHAT" | "ATTENDRE" | "ALLÉGER" | "VENDRE" | "VENTE FORTE"
    #[serde(default)]
    pub rating_label: String,
    /// Why the rating was lowered by the model's evidence (ACHAT FORT → ACHAT, VENTE FORTE → VENDRE); None otherwise.
    #[serde(default)]
    pub rating_reason: Option<String>,
    /// Composite multi-factor score (weights tunable with `w=`).
    #[serde(default)]
    pub score: CompositeScore,
    /// "Signal dégradé": contradictory evidence, unreliable data or a signal that lost money on this asset.
    #[serde(default)]
    pub degraded: Degraded,
    /// Risk-on / risk-off / neutre (macro stress and the benchmark's trend).
    #[serde(default)]
    pub market_regime: Option<MarketRegime>,
    /// Scalping … long terme, from the plan's candles and the distance to target 1 in ATR (None without a plan).
    #[serde(default)]
    pub horizon: Option<HorizonClass>,
    /// Technical structure on the daily candles (Ichimoku, Supertrend, levels, breakouts, relative strength…).
    #[serde(default)]
    pub structure: Option<Structure>,
    /// Scheduled events of the next 7 days that matter for this asset (economy, central banks; for a stock also its
    /// earnings, dividends and splits); None when the calendar could not be loaded.
    #[serde(default)]
    pub events: Option<Vec<crate::calendar::CalendarEvent>>,
    /// "Quand ne PAS trader": what makes now a bad moment whatever the verdict (never changes it).
    #[serde(default)]
    pub no_trade: NoTrade,
    /// The plan as a price ladder with the current price's place; None without a plan.
    #[serde(default)]
    pub action_zones: Option<ActionZones>,
    /// The scenario whose conditions are the most met (ties: the neutral one); None without scenarios.
    #[serde(default)]
    pub unfolding: Option<Unfolding>,
    /// Favourable vs unfavourable reasons and what could invalidate the scenario.
    #[serde(default)]
    pub counter_argument: CounterArgument,
    /// Compact numbers kept by the clients to explain a later change of the signal.
    #[serde(default)]
    pub snapshot: DecisionSnapshot,
    /// « Preuve du modèle »: what the cross-asset validation says about this asset's class and current regime.
    #[serde(default)]
    pub model_evidence: ModelEvidence,
    /// « Bot Altim »: the learned model's view (ACHETER / ATTENDRE / VENDRE) and whether it counts.
    #[serde(default)]
    pub bot: super::bot::BotView,
}
