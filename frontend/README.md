# Front web d'Altim — Rust + Yew (WebAssembly)

Le site de présentation (`/`, `/mentions-legales`, `/confidentialite`, `/risques`) et l'application web (`/app/*`),
en Rust + Yew 0.23, compilés en WebAssembly. Il a remplacé l'ancien front React + Bun (retiré en phase 3 du portage ;
l'historique Git garde `web/src`) : mêmes adresses, mêmes clés et formats localStorage, mêmes classes CSS.

## Construire et servir

```
sh scripts/build-web.sh          # → web/dist (outils : scripts/web-tools.sh, dans $ALTIM_TOOLS)
ALTIM_DEV_OPEN=1 PORT=3000 ALTIM_WEB_ROOT=$PWD/web cargo run --bin altim
```

Le front est fait de parties, un `.wasm` chacune (`altim_web::part`) : le site, chaque groupe d'écrans de l'app
(`altim_core::web::bundle`, la même fonction côté serveur et côté navigateur) et l'app entière. Leurs points d'entrée
sont les exemples Cargo du paquet `altim-bundles` (`frontend/bundles/src/<partie>.rs`, quelques lignes chacun) : la
bibliothèque `altim-web` est compilée une seule fois, puis chaque `.wasm` est lié en parallèle (profil `wasm-release` :
opt-level z, LTO complète avec `-C linker-plugin-lto`, 1 unité de code, panic abort) ; chaque point d'entrée ne nomme
que ses écrans, l'optimisation à l'édition de liens laisse les autres de côté.

| Page | Servie pour | Point d'entrée |
| --- | --- | --- |
| `web/dist/index.html` | `/`, pages légales, toute adresse hors `/app` | `site` |
| `web/dist/app/index.html` | `/app` (Radar), `/app/alertes`, toute autre adresse `/app/…` | `radar` |
| `web/dist/app/actif.html` | `/app/actif/<crypto\|stock>/<symbole>` | `actif` |
| `web/dist/app/avoirs.html` | `/app/avoirs` | `avoirs` |
| `web/dist/app/simulation.html` | `/app/simulation`, `/app/journal` | `simulation` |
| `web/dist/app/selection.html` | `/app/selection`, `/app/opportunites` | `selection` |
| `web/dist/app/actu.html` | `/app/actu` | `actu` |
| `web/dist/app/reglages.html` | `/app/reglages`, `/app/lexique` | `reglages` |
| `web/dist/app/bot.html` | `/app/bot`, `/app/validation` | `bot` |
| (aucune page) | l'app entière, chargée en arrière-plan | `app` |

Chaque partie a ses fichiers `altim-<partie>-<hash>.js` et `altim-<partie>-<hash>_bg.wasm` (le `.wasm` est préchargé
par la page), un chargeur `main-<partie>-<hash>.js` par page, plus les styles partagés (`global-<hash>.css`,
`app-<hash>.css`) ; les icônes et le logo restent dans `web/public`, servis tels quels. Une première visite ne
télécharge que le groupe de sa page. Dans l'app, `use_on_link()` navigue sans recharger : à l'intérieur du groupe, et
vers les autres groupes dès que l'app entière est prête. Le chargeur d'une page de l'app (`frontend/loader-app.js`)
la télécharge et la compile en arrière-plan deux secondes après le chargement (sauf « économie de données ») ; au
premier passage vers un autre groupe, le `.wasm` du groupe s'arrête (`part::stop`) et l'app entière reprend la page à
la nouvelle adresse (`part::hand_over`), sans rechargement. Avant qu'elle soit prête, c'est le `.wasm` du groupe visé
s'il est déjà compilé : le chargeur le prépare dès qu'un lien vers lui va être suivi (survol, doigt posé, focus
clavier) et, pour les groupes voisins (Mes avoirs ⇄ Simulation, `NEIGHBOURS` de build-web.sh), dès l'affichage.
Sinon, ou vers le site, c'est un chargement de page (`route::other_bundle`).

Chaque page de l'app contient déjà le cadre de l'app (`frontend/shell.html` : en-tête et onglets, celui du groupe
allumé, le même balisage que `WebApp`), affiché dès l'arrivée du HTML ; le `.wasm` le remplace par son rendu
(`part::run`). Tant que l'avertissement n'a pas été accepté sur ce navigateur, `frontend/gate.js` (script synchrone,
avant le cadre) le masque : l'app affiche alors l'avertissement. Un changement de l'en-tête ou des onglets dans
`app::WebApp` se reporte dans `shell.html`.

## Animations

Le site (page d'accueil) :

- **Ouverture** (`frontend/door.js`, balisage `door.html`), une fois par session (`sessionStorage` `altim.door`) : le
  logo tracé trait par trait, un compteur 0 → 100 % qui ne bouge que sur des événements réels (octets du `.wasm` reçus,
  comptés par le chargeur du site sur une copie du flux : 80 % ; polices du héros : 10 % ; première réponse `/api/` :
  10 %), puis la porte s'ouvre. Elle s'ouvre au plus tard 0,7 s après le premier rendu, quoi que dise le compteur ; un
  toucher ou une touche l'ouvre aussitôt ; jamais avec « réduire les animations ». `build-web.sh` place `door.js` en
  tête du chargeur du site (`loader-site.js`, un module : rien ne bloque l'analyse de la page).
- **Film au défilement** (`site::film`, logique `altim_core::web::film`) : une scène collante plein écran, une
  légende (texte réel) par station ; le `.wasm` écrit la position dans le film (`data-s`) et l'opacité des légendes.
  `frontend/motion.js`, chargé après le premier rendu (`requestIdleCallback`), y dessine des milliers de grains
  (WebGL : formes envoyées une fois, morphose dans le vertex shader ; Canvas 2D sinon) : 2 000–2 800 sur mobile,
  5 000–7 000 sur ordinateur, `devicePixelRatio` plafonné à 2, en pause hors écran ou onglet caché, rien avec
  « économie de données » (le logo fixe reste). Défilement natif partout ; seule la scène suit avec un peu d'inertie.
- **Ambiance** : grain de film (bruit SVG en CSS), apparition des sections au défilement ; sur pointeur fin seulement
  (`hover: hover` et `pointer: fine`) un curseur rond qui suit la souris (le curseur du système reste) et des boutons
  magnétiques.
- **Réduire les animations** : ni ouverture ni film ; les légendes en liste, chacune avec son dessin fixe.

L'app (sobre, c'est un outil ; rien ne masque ni ne retarde un chiffre, tout s'arrête avec « réduire les animations ») :
le logo tracé une fois en arrivant de `/login` (`gate.js`), les chiffres d'un prix qui changent roulent et le prix
s'allume brièvement en cyan ou en rouge (`LivePrice`, `altim_core::web::motion::digit_runs`, chiffres tabulaires), le
verdict qui change apparaît en fondu (`DecisionBadge`), les cartes montent légèrement à leur première apparition, le
point « EN DIRECT » pulse, les sparklines se dessinent de gauche à droite.

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
  src/lib.rs, part.rs      bibliothèque de tout le front ; une partie = un .wasm (part::run, hand_over, stop)
  loader-app.js            chargeur des pages de l'app (groupe, puis app entière en arrière-plan)
  loader-site.js, door.js  chargeur du site, précédé de l'ouverture de la page d'accueil (door.html)
  motion.js                film de particules, curseur et boutons magnétiques du site (chargé après le premier rendu)
  shell.html, gate.js      cadre de l'app dans la page avant le .wasm, masqué avant l'avertissement
  bundles/                 paquet altim-bundles : un point d'entrée par partie (exemples Cargo)
  src/route.rs             enum Route, Root, use_on_link()
  src/state/               stores localStorage : app (altim.webapp.v1), holdings (altim.holdings.v1), fx (altim.fx.v1)
  src/money.rs             devise d'affichage : money(), price(), compact(), use_money(), cur_param(), fx_line()
  src/api.rs               get / get_pending / batched / get_public, 401 → /login?next=, 202 { pending }
  src/live.rs              use_live (WebSocket /api/ws, repli SSE /api/live), LiveBadge, LivePrice
  src/ui.rs                ActionBadge, ReliabilityBadge, Change, Price, Gauge, Sparkline, PriceChart, Segmented, dates fr
  src/hooks.rs             use_reveal, use_interval, use_ticks, visible, every_visible, set_title
  src/site/                site de présentation (film.rs : le film au défilement)
  src/app/mod.rs           coquille : en-tête, onglets, avertissement, aiguillage des routes
  src/app/<écran>/         un dossier par écran (sous-composants dans le même dossier)
```

## Écrire un écran

1. Composant fonctionnel : `#[component] pub fn Nom(p: &NomProps) -> Html`, props `#[derive(Properties, PartialEq)]`.
   Attributs SVG en kebab-case (`stroke-width`), `class={classes!(...)}`, `if let … { }` dans `html!`.
2. Nouvelle route : une variante dans `route.rs`, un bras dans les écrans de son groupe (`app::<groupe>_screens`) et
   dans `app::all_screens`, son adresse dans `altim_core::web::bundle::bundle_of`.
3. Liens internes : `let on_link = use_on_link();` puis `<a href="/app/bot" onclick={on_link.clone()}>`. Liens vers le
   site (`/`, `/risques`) : `<a href>` simple.
4. Réglages : `use_app_state()` / `set_app_state(|s| …)`. Avoirs : `use_stored_holdings()` (tels que saisis),
   `use_holdings()` (en dollars pour moteurs et serveur), `set_holdings(|h| …)`, `add_holdings(lignes)`.
5. Montants : `let _m = crate::money::use_money();` dans tout composant qui affiche un montant, puis `money::money(usd)`,
   `money::price(usd)`, `money::from_display(saisie)` ; requêtes dont le serveur écrit des textes chiffrés :
   `money::cur_param()`.
6. Réponses du serveur : réutilisez les types d'`altim_core::engine` (ils dérivent `Deserialize`), sinon définissez-les
   dans `core/src/web/…` avec `#[serde(default)]` sur ce qui est optionnel. Données locales : lecture tolérante champ par
   champ, `state::local_get/local_set` (jamais de panique si le stockage est refusé). Lecture d'un `Value` :
   `altim_core::web::json::from_value` (pas `serde_json::from_value`, qui ajoute au `.wasm` un second lecteur par
   structure) ; pas de `#[serde(flatten)]` ni d'énumération étiquetée dans ce qui se lit (lecture à la main à travers
   un `Value`, voir `GuardReport`). Décision : `doc.d` (`DecisionCore`) partout, `doc.full()` sur l'écran d'actif seul.
7. Prix en direct : `let live = use_live(items);` puis `<LivePrice tick={live.get(&sym, kind).cloned()} …/>`.

## Tests et vérifications

- Logique pure dans `core/src/web/…` ou `core/src/engine/…`, `#[cfg(test)]`, `cargo test -p altim-core` ; fichiers
  d'exemple réels dans `backend/tests/samples`. Les vues restent minces et sont vérifiées dans un navigateur.
- Avant de livrer : `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo clippy -p altim-core -p altim-web -p altim-bundles --target wasm32-unknown-unknown --lib --examples -- -D warnings`,
  `cargo test --workspace`,
  `sh scripts/build-web.sh`, captures à
  320 px et 1 100 px, aucun défilement horizontal, aucune erreur dans la console.
