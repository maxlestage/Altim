# Porter l'app web vers Rust + Yew — guide de la phase 2

Phase 1 (fait) : espace de travail Cargo, crate partagé `altim-core`, crate `altim-web` (Yew 0.23), site de
présentation complet, coquille de l'app, stores, client API, affichage des montants, primitives d'UI, flux en direct,
chaîne de build et de déploiement. Phase 2 : quatre lots (A, B, C, D) portent les écrans **en parallèle**, chacun dans
ses propres fichiers. Phase 3 : branchements entre lots, retrait du front React (`web/src`, Bun).

## Transition : qui sert quoi

`sh scripts/build-web.sh` construit **les deux** fronts dans `web/dist` :

| Chemin | Servi par | Fichiers |
| --- | --- | --- |
| `/`, `/mentions-legales`, `/confidentialite`, `/risques` | **Yew** | `web/dist/index.html`, `altim-<hash>.js`, `altim-<hash>_bg.wasm`, `main-<hash>.js`, `global-/app-<hash>.css` |
| `/app`, `/app/*` | **React** tant que `web/dist/app/index.html` existe | `web/dist/app/*` (bundle Bun, `--public-path=/app/`) |

Le serveur (`backend/src/app/web.rs`) sert `dist/app/index.html` pour `/app/*` s'il existe, sinon `dist/index.html`.
Pour voir **vos écrans Yew** sous `/app`, construisez sans React : `ALTIM_REACT_APP=0 sh scripts/build-web.sh`.
Les liens du site vers `/app` sont des chargements de page complets (jamais interceptés), donc les deux fronts
cohabitent sur la même origine, avec les mêmes clés localStorage. La phase 3 retire l'étape React du script.

## Organisation

```
Cargo.toml                 espace de travail (backend, core, frontend) ; profil wasm-release (opt-level z, lto fat)
core/  (altim-core)        logique pure, compilée pour le serveur ET pour wasm32 (aucune E/S, horloge passée en paramètre)
  src/engine/*             moteurs (ex-backend/src/engine) : signal, fibonacci, backtest, guard, decision, bot…
  src/js.rs, jstime.rs     sémantique JS : fr-FR, toFixed, Date.UTC, séance de New York
  src/calendar.rs, fx.rs   contrat des événements d'agenda ; taux des textes des moteurs (fourni par l'hôte)
  src/web/                 logique pure du front : money, store, fx, market, bot, danger
  src/web/{insights,decision,portfolio,trading}/   dossiers réservés aux lots A, B, C, D
frontend/ (altim-web)
  index.html               gabarit (build-web.sh y met les noms hachés) ; aucun script inline (CSP)
  styles/global.css, app.css   copies de web/src/styles : MÊMES classes ; modifiez ICI, plus dans web/src
  src/lib.rs               point d'entrée wasm (#[wasm_bindgen(start)])
  src/route.rs             enum Route (mêmes URL qu'avant), Root, use_on_link()
  src/state/               stores localStorage : app (altim.webapp.v1), holdings (altim.holdings.v1), fx (altim.fx.v1)
  src/money.rs             devise d'affichage : money(), price(), compact(), use_money(), cur_param(), fx_line()
  src/api.rs               get / get_pending / batched / get_public, 401 → /login?next=, 202 { pending }
  src/live.rs              use_live (SSE /api/live), LiveBadge, LivePrice
  src/ui.rs                ActionBadge, ReliabilityBadge, Change, Price, Gauge, Sparkline, PriceChart, Segmented,
                           NotPorted, technical_text, action_kind, dates fr (fr_time, fr_day_month, fr_date_time_short)
  src/hooks.rs             use_reveal, use_interval, use_ticks, set_title
  src/site/                site de présentation (fini)
  src/app/mod.rs           coquille : en-tête, onglets, avertissement, aiguillage des routes (ne pas modifier, sauf A : 1 ligne)
  src/app/common.rs        FxNote, PortfolioTabs (partagés, finis)
  src/app/<écran>/mod.rs   un dossier par écran, déjà branché sur sa route (squelette « en cours de portage »)
```

## Écrire un écran

1. Remplacez le corps du composant de votre `frontend/src/app/<écran>/mod.rs` (il est déjà routé). Les sous-composants
   vont dans des fichiers du même dossier, déclarés dans ce `mod.rs`. **Aucun autre fichier partagé à modifier.**
2. Nouvelle route (rare) : une variante dans `route.rs` + un bras dans `app::screen` — à signaler, c'est partagé.
3. Composant fonctionnel : `#[component] pub fn Nom(p: &NomProps) -> Html`, props `#[derive(Properties, PartialEq)]`.
   Même balisage et mêmes classes que le `.tsx` : le CSS s'applique tel quel. Attributs SVG en kebab-case
   (`stroke-width`), `class={classes!(...)}`, `if let … { }` dans `html!`.
4. Liens internes : `let on_link = use_on_link();` puis `<a href="/app/bot" onclick={on_link.clone()}>` (comme `onLink`).
   Liens vers le site (`/`, `/risques`) : `<a href>` simple.
5. Texte : identique au TypeScript. Attention aux espaces du JSX (`{" "}`, fins de ligne) : reproduisez les nœuds texte
   exacts, sinon la mise en page bouge.

## État, montants, API

- Réglages : `let s = use_app_state();` / `set_app_state(|s| s.interval = Interval::D1)` (sauvegarde + rendu).
  Avoirs : `use_stored_holdings()` (tels que saisis), `use_holdings()` (en dollars pour moteurs et serveur),
  `set_holdings(|h| …)`, `add_holdings(lignes)`. Formats et validations : `altim_core::web::store` (testés).
- Montants : appelez `let _m = crate::money::use_money();` dans tout composant qui affiche un montant (re-rendu au
  changement de devise ou de taux), puis `money::money(usd)`, `money::price(usd)`, `money::compact(usd)`,
  `money::from_display(saisie)`. Requêtes dont le serveur écrit des textes chiffrés : ajoutez `money::cur_param()`.
- API : `api::get::<T>(url).await`, `api::get_pending::<T>(url)` pour /api/bot, /api/validation, /api/selection,
  /api/opportunities, `api::batched(&items, |list| format!("/api/radar?symbols={list}&interval=4h"))` (lots de 20).
  Types de réponse : **réutilisez ceux du serveur** quand ils existent dans `altim_core::engine` (ils dérivent
  `Deserialize`) : `decision_types::Decision` (/api/decision), `fibonacci::FibZone`, `guard::GuardResult`,
  `screener::*`, `opportunities::*`, `strategies::*`, `news::NewsItem`, `brief::*`, `reliability::*`, `signal::*`,
  `calendar::CalendarEvent`. Sinon, définissez-les dans votre dossier `core/src/web/<lot>/` avec `#[serde(default)]`
  sur ce qui est optionnel côté TypeScript (un serveur plus ancien doit rester lisible). Vérifiez chaque type sur une
  vraie réponse (fichiers `backend/tests/golden`, `backend/tests/samples`, ou `curl` sur votre serveur local).
- Données locales : mêmes clés, même JSON (utilisateurs existants). Lecture tolérante champ par champ, comme le
  TypeScript ; `state::local_get/local_set` (jamais de panique si le stockage est refusé). Clés et propriétaires :

| Clé | Lot | Clé | Lot |
| --- | --- | --- | --- |
| altim.webapp.v1, altim.holdings.v1, altim.fx.v1, altim.dangers.v1 | phase 1 | altim.decision.v1, altim.configChanges.v1, altim.notes.v1, altim.radar.sort | B |
| altim.alerts.v1, altim.news.filter, altim.news.view, altim.agenda.filter, altim.agenda.mine | A | altim.rebalance | C |
| altim.paper.v1 (+ .invalid), altim.journal.v1, altim.selection.budget/horizon/market, altim.opportunities.v1 | D | | |

- Prix en direct : `let live = use_live(items);` puis `<LivePrice tick={live.get(&sym, kind).cloned()} fallback={prix}
  format={Callback::from(crate::money::price)} />` et `<LiveBadge status={live.status} last={live.last} />`.
- Heure : `js_sys::Date::now()` côté vue ; la logique pure reçoit `now` en paramètre (testable).

## Tests

- Logique pure : dans `core/src/web/<lot>/…` ou `core/src/engine/…`, `#[cfg(test)]`, `cargo test -p altim-core`.
  Portez les cas des tests Bun listés ci-dessous (mêmes valeurs attendues). Rien de navigateur dans altim-core.
- Vues : pas de rendu testé ; gardez-les minces. Un test `wasm-bindgen-test` est possible mais facultatif.
- Avant de rendre : `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo clippy -p altim-core -p altim-web --target wasm32-unknown-unknown -- -D warnings`, `cargo test --workspace`,
  `ALTIM_REACT_APP=0 sh scripts/build-web.sh`, serveur local (`ALTIM_DEV_OPEN=1 PORT=47xx ALTIM_WEB_ROOT=$PWD/web
  ./target/debug/altim`), captures à 320 px et 1100 px **comparées à l'écran React** (même page servie par un build
  React : `cd web && bun build ./src/index.html --outdir <dossier>/dist --public-path=/`), aucun défilement horizontal.

## Lots (aucun fichier partagé entre lots)

Chaque lot ne touche que : ses dossiers `frontend/src/app/<écran>/`, son dossier `core/src/web/<lot>/`, et les
fichiers indiqués. Ne modifiez pas `web/src` (React sert encore /app jusqu'à la phase 3).

### A — Bot, validation, alertes, actualités (≈ 3 000 lignes TS)
- Écrans : `app/bot` (Bot.tsx, BotV3.tsx, BotRadarCard.tsx, BriefCard.tsx), `app/validation` (Validation.tsx),
  `app/alerts` (Alerts.tsx + `PriceAlertButton` pour l'écran d'actif), `app/news` (News.tsx, Agenda.tsx : `Agenda`,
  `AgendaEvent`).
- Pur : `core/src/web/bot.rs` (compléter model-bot.ts, **ajouts seulement**), `core/src/web/insights/` (model-validation,
  alerts-store, partie pure de notify, news-summary, calendar).
- Seul fichier partagé : `frontend/src/app/mod.rs`, une ligne dans `WebApp` pour démarrer les vérifications d'alertes
  (`startChecks`), à l'endroit marqué « Phase 2 (batch A) ».
- Tests Bun à porter : bot.test, bot-v3.test (parties pures), validation.test, alerts-store.test, news-summary.test,
  agenda.test, money.test (fxLine, déjà en partie en phase 1).

### B — Radar, écran d'actif, décision (≈ 3 500 lignes TS)
- Écrans : `app/radar` (Radar.tsx), `app/asset` (AssetScreen.tsx, DecisionCard.tsx dont `DecisionBadge`,
  `RATING_RANK`, `ratingTone`, TrackDetails.tsx, GuardCard.tsx, ZonesCard.tsx, WhyCard.tsx, StrategiesCard.tsx,
  AnomaliesCard.tsx, NoteCard.tsx).
- Pur : `core/src/web/decision/` (decision.ts : cache hors ligne `altim.decision.v1`, `portfolioWeights`,
  `averageCost`, formats ; config-changes.ts ; strategies.ts). Le type `Decision` est celui du serveur
  (`altim_core::engine::decision_types::Decision`) : vérifiez l'aller-retour JSON sur une vraie réponse.
- Tests Bun : decision.test, config-changes.test, guidance.test, strategies.test, zones.test (partie advice : lot C).

### C — Mes avoirs et outils (≈ 3 200 lignes TS)
- Écran : `app/holdings` (MyHoldings.tsx, AddHoldings.tsx, AssetPicker.tsx + `KIND_LABEL`, AssetSearch.tsx,
  HistoryCard.tsx, RiskCards.tsx, SectorCard.tsx, ToolCards.tsx dont `CompareCard` et `PositionCard`, WhatIfCard.tsx,
  DcaCard.tsx). Composants réutilisés ailleurs : exportez-les en `pub` depuis `app/holdings`.
- Pur : `core/src/web/portfolio/` : moteurs TS sans équivalent serveur — holdings.ts, portfolio-risk.ts (ses `Danger`
  sont `altim_core::web::danger::Danger`, et `saveDangers` = `danger::dangers_json` sous `DANGERS_KEY`), tools.ts,
  dca.ts, risk.ts (RiskSettings est déjà `web::store::RiskSettings`), advice.ts, sectors.ts.
- Tests Bun : holdings.test, portfolio-risk.test, whatif.test, tools.test, dca.test, advice.test, sectors.test.

### D — Sélection, opportunités, réglages, lexique, simulation, journal (≈ 3 000 lignes TS)
- Écrans : `app/selection` (Selection.tsx), `app/opportunities` (Opportunities.tsx), `app/settings` (Settings.tsx ;
  poids du score : `web::store::SCORE_FACTORS`/`ScoreWeights`, déjà portés), `app/glossary` (Glossary.tsx),
  `app/simulation` (Simulation.tsx, PaperOrder.tsx dont `SimulateBuy`), `app/journal` (Journal.tsx dont
  `JournalToggle`, `JournalNote`).
- Pur : `core/src/web/trading/` : engine/paper.ts, engine/journal.ts, paper-ui.ts, paper-store.ts, journal-store.ts
  (partie pure, dont `recordRealTrade`), `parseBudget` de Selection.tsx.
- Tests Bun : paper.test, paper-ui.test, journal.test, money.test (parseBudget).

### Déjà couverts (moteurs portés côté serveur, désormais dans altim-core)
alerts.test, backtest-stats.test, brief.test, guard.test, guard-scenarios.test, history.test, news.test,
opportunities.test, reliability.test, selection.test, signal.test : tests de parité `backend/tests/parity*.rs`
(fichiers dorés générés depuis le TypeScript). Ajoutez un cas seulement si un test Bun vérifie autre chose.

## Branchements laissés à la phase 3

Quand votre écran a besoin d'un élément d'un autre lot, n'y touchez pas : laissez `// TODO(phase 3): <lot> <chemin>`
et n'affichez rien à sa place. Chemins convenus (les propriétaires les exportent en `pub` sous ces noms) :

| Utilisé par | Élément | Propriétaire, chemin |
| --- | --- | --- |
| B Radar | BriefCard, BotRadarCard | A, `crate::app::bot::{BriefCard, BotRadarCard}` |
| B Radar | CompareCard | C, `crate::app::holdings::CompareCard` |
| B Radar | positions dangereuses | phase 1, `altim_core::web::danger::parse_dangers(local_get(DANGERS_KEY))` |
| B écran d'actif | PriceAlertButton | A, `crate::app::alerts::PriceAlertButton` |
| B écran d'actif | DcaCard, PositionCard, analyse de la ligne détenue | C, `crate::app::holdings::*`, `altim_core::web::portfolio::holdings` |
| B DecisionCard | AgendaEvent, libellé du jour | A, `crate::app::news::AgendaEvent`, `altim_core::web::insights::calendar` |
| B DecisionCard | SimulateBuy | D, `crate::app::simulation::SimulateBuy` |
| C Mes avoirs | JournalToggle, recordRealTrade | D, `crate::app::journal::JournalToggle`, `altim_core::web::trading::journal_store` |
| D Réglages | AssetPicker | C, `crate::app::holdings::AssetPicker` |
| D journal | décision récente en cache | B, `altim_core::web::decision::…::cached_decision` |
| A Actu | `Agenda` | A (même lot) |
