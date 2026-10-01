# Front web d'Altim — Rust + Yew (WebAssembly)

Le site de présentation (`/`, `/mentions-legales`, `/confidentialite`, `/risques`) et l'application web (`/app/*`),
en Rust + Yew 0.23, compilés en WebAssembly. Il a remplacé l'ancien front React + Bun (retiré en phase 3 du portage ;
l'historique Git garde `web/src`) : mêmes adresses, mêmes clés et formats localStorage, mêmes classes CSS.

## Construire et servir

```
sh scripts/build-web.sh          # → web/dist (outils : scripts/web-tools.sh, dans $ALTIM_TOOLS)
ALTIM_DEV_OPEN=1 PORT=3000 ALTIM_WEB_ROOT=$PWD/web cargo run --bin altim
```

Le script compile le crate une fois pour le site et une fois par groupe d'écrans de l'app (profil `wasm-release` :
opt-level z, LTO complète, 1 unité de code, panic abort), une par fonctionnalité Cargo, pour qu'une première visite ne
télécharge que les écrans de sa page (`altim_core::web::bundle`, la même fonction côté serveur et côté navigateur) :

| Page | Servie pour | Fonctionnalité |
| --- | --- | --- |
| `web/dist/index.html` | `/`, pages légales, toute adresse hors `/app` | `site` |
| `web/dist/app/index.html` | `/app` (Radar), `/app/alertes`, toute autre adresse `/app/…` | `app-radar` |
| `web/dist/app/actif.html` | `/app/actif/<crypto\|stock>/<symbole>` | `app-actif` |
| `web/dist/app/avoirs.html` | `/app/avoirs`, `/app/simulation`, `/app/journal` | `app-avoirs` |
| `web/dist/app/selection.html` | `/app/selection`, `/app/opportunites` | `app-selection` |
| `web/dist/app/actu.html` | `/app/actu` | `app-actu` |
| `web/dist/app/reglages.html` | `/app/reglages`, `/app/lexique` | `app-reglages` |
| `web/dist/app/bot.html` | `/app/bot`, `/app/validation` | `app-bot` |

Chaque partie a ses fichiers `main-<partie>-<hash>.js`, `altim-<partie>-<hash>.js` et `altim-<partie>-<hash>_bg.wasm`
(le `.wasm` est préchargé par la page), plus les styles partagés (`global-<hash>.css`, `app-<hash>.css`) ; les icônes
et le logo restent dans `web/public`, servis tels quels. Les liens entre parties sont des chargements de page complets
(`route::serves`, `route::other_bundle`) ; à l'intérieur d'un groupe, `use_on_link()` navigue sans recharger. Cinq
secondes après l'affichage, chaque page de l'app fait précharger en arrière-plan les fichiers du Radar et de l'écran
d'actif (`<meta name="altim-prefetch">`). La fonctionnalité `app` réunit tous les groupes ; sans fonctionnalité choisie
(vérifications, tests), un seul `.wasm` sert tout.

Les `.wasm`, `.js` et `.css` ont une copie brotli (qualité 11, `.br`) et gzip (`.gz`) écrite par `scripts/precompress`,
que le serveur envoie telle quelle (`Content-Encoding`, `Vary: Accept-Encoding`) au lieu de compresser à la volée.
`wasm-opt -O2` sans sa passe d'inlining (et non `-Oz`) : l'inlining réduit le fichier brut mais il se compresse moins
bien (+12 % en brotli), et le réseau transporte le fichier compressé. Seuls les fuseaux Paris, New York et UTC entrent dans le `.wasm`
(`CHRONO_TZ_TIMEZONE_FILTER`), et les traces internes de Yew sont compilées hors du binaire.

## Organisation

```
core/  (altim-core)        logique pure, compilée pour le serveur ET pour wasm32 (aucune E/S, horloge passée en paramètre)
  src/engine/*             moteurs : signal, fibonacci, backtest, guard, decision, bot…
  src/js.rs, jstime.rs     sémantique JS : fr-FR, toFixed, Date.UTC, séance de New York
  src/web/                 logique pure du front : money, store, fx, market, bot, danger
  src/web/decision/        Radar, écran d'actif, décision (cache altim.decision.v1, changements de configuration…)
  src/web/insights/        bot, validation, alertes, actualités, agenda
  src/web/portfolio/       Mes avoirs et ses outils (analyse, risques, secteurs, DCA, conseil)
  src/web/trading/         sélection, opportunités, simulation, journal
frontend/ (altim-web)
  index.html               gabarit (build-web.sh y met les noms hachés) ; aucun script inline (CSP)
  styles/global.css, app.css
  src/lib.rs               point d'entrée wasm (#[wasm_bindgen(start)]) ; fonctionnalités `site` et `app`
  src/route.rs             enum Route, Root, use_on_link()
  src/state/               stores localStorage : app (altim.webapp.v1), holdings (altim.holdings.v1), fx (altim.fx.v1)
  src/money.rs             devise d'affichage : money(), price(), compact(), use_money(), cur_param(), fx_line()
  src/api.rs               get / get_pending / batched / get_public, 401 → /login?next=, 202 { pending }
  src/live.rs              use_live (SSE /api/live), LiveBadge, LivePrice
  src/ui.rs                ActionBadge, ReliabilityBadge, Change, Price, Gauge, Sparkline, PriceChart, Segmented, dates fr
  src/hooks.rs             use_reveal, use_interval, use_ticks, visible, every_visible, set_title
  src/site/                site de présentation
  src/app/mod.rs           coquille : en-tête, onglets, avertissement, aiguillage des routes
  src/app/<écran>/         un dossier par écran (sous-composants dans le même dossier)
```

## Écrire un écran

1. Composant fonctionnel : `#[component] pub fn Nom(p: &NomProps) -> Html`, props `#[derive(Properties, PartialEq)]`.
   Attributs SVG en kebab-case (`stroke-width`), `class={classes!(...)}`, `if let … { }` dans `html!`.
2. Nouvelle route : une variante dans `route.rs` + un bras dans `app::screen` (avec le `#[cfg(feature = "app-<groupe>")]`
   de son groupe) + son adresse dans `altim_core::web::bundle::bundle_of`.
3. Liens internes : `let on_link = use_on_link();` puis `<a href="/app/bot" onclick={on_link.clone()}>`. Liens vers le
   site (`/`, `/risques`) : `<a href>` simple.
4. Réglages : `use_app_state()` / `set_app_state(|s| …)`. Avoirs : `use_stored_holdings()` (tels que saisis),
   `use_holdings()` (en dollars pour moteurs et serveur), `set_holdings(|h| …)`, `add_holdings(lignes)`.
5. Montants : `let _m = crate::money::use_money();` dans tout composant qui affiche un montant, puis `money::money(usd)`,
   `money::price(usd)`, `money::from_display(saisie)` ; requêtes dont le serveur écrit des textes chiffrés :
   `money::cur_param()`.
6. Réponses du serveur : réutilisez les types d'`altim_core::engine` (ils dérivent `Deserialize`), sinon définissez-les
   dans `core/src/web/…` avec `#[serde(default)]` sur ce qui est optionnel. Données locales : lecture tolérante champ par
   champ, `state::local_get/local_set` (jamais de panique si le stockage est refusé).
7. Prix en direct : `let live = use_live(items);` puis `<LivePrice tick={live.get(&sym, kind).cloned()} …/>`.

## Tests et vérifications

- Logique pure dans `core/src/web/…` ou `core/src/engine/…`, `#[cfg(test)]`, `cargo test -p altim-core` ; fichiers
  d'exemple réels dans `backend/tests/samples`. Les vues restent minces et sont vérifiées dans un navigateur.
- Avant de livrer : `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo clippy -p altim-core -p altim-web --target wasm32-unknown-unknown -- -D warnings` (et avec
  `--no-default-features --features site`, puis `app`, puis chaque `app-<groupe>`), `cargo test --workspace`,
  `sh scripts/build-web.sh`, captures à
  320 px et 1 100 px, aucun défilement horizontal, aucune erreur dans la console.
