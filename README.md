# Altim

> **Logiciel propriétaire, tous droits réservés** (voir `LICENSE`). Ce dépôt n'est pas open source : aucune copie, réutilisation ni déploiement n'est autorisé.

Application web de **conseil** pour la crypto et les actions : quand acheter, attendre, alléger ou protéger, en tenant compte de **ce que vous possédez déjà**. Altim **ne passe aucun ordre** et ne demande aucun accès à vos comptes : vous suivez ou non ses conseils chez votre courtier habituel. Site de présentation et application web (`/app`) en **Rust + Yew** (WebAssembly, `frontend/`), serveur **Rust + Axum** et moteurs partagés (`core/`) : tout en Rust, déployé sur Heroku (buildpacks Rust puis Node.js, ce dernier sans aucune dépendance JavaScript, ou image Docker), et deux **applications natives**, iPhone (SwiftUI) et Android (Kotlin, Jetpack Compose), qui se connectent à ce même serveur privé.

> ⚠️ Altim est un outil d'aide à la décision, pas un conseil en investissement. Aucun algorithme ne garantit de gain.

## Contenu

| Dossier | Rôle |
|---|---|
| `frontend` | Front web **Rust + Yew** (WebAssembly), mobile first : site vitrine et application web `/app` (`sh scripts/build-web.sh` → `web/dist`, un `.wasm` pour le site, un par groupe d'écrans de l'app, copies brotli/gzip précompressées ; voir `frontend/README.md`) |
| `core` | Logique pure partagée serveur / navigateur : moteurs, formats, contrats JSON (testée par `cargo test`) |
| `web` | Ce que le serveur sert : `web/public` (icônes, logo) et `web/dist` (front construit, non versionné) |
| `backend` | Serveur **Rust + Axum** : API **multi-source**, prix en direct, accès privé, **API garde-fou pour bots**, sert le site |
| `ios` | **Application iPhone native** (SwiftUI, iOS 17+) : client du serveur Heroku, projet généré par XcodeGen |
| `ios/AltimKit` | Noyau Swift testé sur Linux et macOS : modèles de l'API, connexion privée, flux des prix en direct, formats français |
| `android` | **Application Android native** (Kotlin, Jetpack Compose, Android 11+), à parité avec l'iPhone |
| `android/kit` | Noyau Kotlin testé sur la JVM, équivalent d'AltimKit (mêmes réponses réelles du serveur en tests) |
| `.github/workflows` | CI web, CI iOS (build + tests), envoi TestFlight, déploiement Heroku, santé quotidienne des sources |

➡️ **Déploiement depuis un iPhone, sans ordinateur : voir [DEPLOIEMENT.md](DEPLOIEMENT.md).**

## Application web (`/app`)

Accessible depuis le bouton **« Ouvrir l'app »** du site, sans installation :

- **Radar** : signaux validés de vos actifs (crypto et actions), fiabilité des données, Fear & Greed, opportunités détectées, et **« Achetables maintenant »** (la règle des notifications des apps, avec ses raisons) ; actualisation automatique toutes les 60 s.
- **Analyse** : graphique (EMA 20/50, stop/objectif, entrées du backtest), jauge et détail des 7 facteurs, fiabilité avec la liste des sources et leurs écarts, backtest, sentiment.
- **Le conseil d'Altim** sur chaque actif : *Achat envisageable* (avec zone d'entrée, stop, objectif et **montant prudent** calculé sur votre patrimoine), *Attendre*, *À éviter*, ou *Pas de conseil* si les sources sont en désaccord. Si vous détenez déjà l'actif, le conseil devient celui de votre ligne : *Conserver*, *Renforcer possible*, *Alléger* (avec le montant), *Protéger* ou *Vendre ou protéger*.
- **Catalogue complet** : toutes les cryptos listées en USD/USDT sur OKX, Coinbase, Kraken, KuCoin et Gate (≈ 2 000, classées par capitalisation CoinGecko) et toutes les actions et ETF cotés aux États-Unis (≈ 11 600, annuaire officiel Nasdaq Trader, classés par capitalisation). Parcours par catégorie ou recherche par symbole ou nom (`/api/universe`).
- **Mes avoirs** : vous renseignez en une fois plusieurs cryptos et plusieurs actions (actif, quantité, prix d'achat moyen), plus vos liquidités. Les données sont **gardées dans le navigateur (localStorage)**, avec export/import JSON. Altim en déduit tout : patrimoine et plus-values au prix de consensus, répartition crypto/actions/liquidités, et pour chaque ligne une recommandation expliquée (*Vendre ou protéger*, *Protéger*, *Alléger*, *Renforcer possible*, *Conserver*, ou *Données insuffisantes*) avec le stop de protection conseillé. S'y ajoutent les risques du portefeuille : volatilité, perte possible sur une mauvaise journée (VaR 95 %), perte si les stops sont touchés, concentration, diversification effective et corrélation. Carte **Évolution de mes lignes** : voir [Historique](#historique-du-portefeuille).
- **Point du jour** en tête du Radar et **Si j'avais investi régulièrement** sur chaque fiche : voir [Point du jour et simulateur](#point-du-jour-et-investissement-programmé).
- **Comparer** (Radar), **Taille de position** (fiche d'un actif) et **Rééquilibrer** (Mes avoirs) : voir [Outils de décision](#outils-de-décision).
- **Actu** : toute l'actualité utile au même endroit (voir [Actualités](#actualités-onglet-actu)).
- **Réglages** : radar, prudence des conseils (risque accepté par idée, taille maximale d'une ligne).

Aucun ordre, aucune clé de courtier : les données de l'app web restent dans le navigateur (localStorage).

Le moteur (signal, confirmation par l'unité supérieure, avertissements, garde-fou de fiabilité, backtest, gestion du risque) est **vérifié sur 17 scénarios de référence**, backtest compris trade par trade.

## Application iPhone native (`ios`)

L'app SwiftUI affiche les mêmes analyses que le site, **calculées par votre serveur Heroku** (40 sources, sélection sur 8 durées, zones de Fibonacci, garde-fou, macro) : rien n'est recalculé ni simplifié sur le téléphone, donc les deux donnent toujours les mêmes chiffres.

- **Connexion** : adresse du serveur, identifiant et mot de passe (le même formulaire que le site, protégé contre le CSRF ; code à 6 chiffres seulement si la 2FA est activée sur le serveur). Le mot de passe et la session (cookie de 7 jours) sont chiffrés dans le **trousseau iOS** (« cet appareil uniquement », jamais dans iCloud) ; à l'expiration, l'app se reconnecte seule. HTTPS obligatoire (HTTP seulement pour un serveur local).
- **Face ID** (ou code de l'iPhone) à l'ouverture et après 2 minutes en arrière-plan, désactivable dans Réglages.
- **Radar** : prix en direct (flux `/api/live`, reconnexion automatique), signal 4 h, fiabilité, mini-graphique, contexte macro ; recherche pour ajouter un actif.
- **Fiche d'un actif** : prix en direct et nombre de sources en accord, graphique 1 h / 4 h / 1 j avec la zone d'achat dessinée, signal, zones court / moyen / long terme avec leur vérification historique, garde-fou marché, macro, actualités.
- **Sélection** : actions ou cryptos, 8 durées (30 min à 6 mois), méthode, résultat rejoué avec ses limites, plan (entrée, stop, objectif) et montant pour votre budget.
- **Mes avoirs** : lignes gardées sur l'iPhone (fichier protégé, exclu des sauvegardes), valeur en direct, plus-values, répartition, concentration et signal 1 jour de chaque ligne, et l'[historique](#historique-du-portefeuille) de ces lignes face au Bitcoin et au S&P 500.
- **Notifications « achat possible »** : vérification en arrière-plan (iOS en décide le rythme, au mieux toutes les 15 min) et à chaque ouverture ; une notification seulement quand un actif devient achetable ou que la raison change ; option « seulement les achats conseillés » (signal + zone).
- **Live Activity et Dynamic Island** : sur la fiche d'un actif, « Suivre » affiche son prix et le verdict d'achat sur l'écran verrouillé et dans la Dynamic Island (en direct quand l'app tourne, à chaque vérification en arrière-plan sinon) ; activable dans Réglages.
- **Onglet Alertes** : les actifs achetables maintenant (même règle que les notifications), vos **alertes de prix** (« préviens-moi si BTC passe sous 80 000 $ », une notification puis réarmable, bouton cloche sur la fiche) et le **journal des alertes** : chaque notification reçue avec son prix et ce qu'elle a donné depuis, et un résumé honnête (part des alertes d'achat en hausse, variation moyenne, sans frais ni règle de sortie).
- **Apple Watch** : les actifs achetables et leurs raisons, envoyés par l'iPhone (la montre ne détient ni mot de passe ni session) ; les notifications de l'iPhone arrivent au poignet.
- **Point du jour** (en tête du Radar) et **simulateur d'investissement programmé** (fiche d'un actif) : voir [plus bas](#point-du-jour-et-investissement-programmé).
- **Alertes actualité** (Réglages) : voir [Actualités](#actualités-onglet-actu). Une notification ouvre l'onglet Actu.
- **Onglet Actu** : la même section Actualités que le site (à la une, ce qui domine, rubriques, articles en français seulement) ; un article s'ouvre chez sa source. Les Réglages passent sous la roue dentée du Radar.
- **Hors ligne** : voir [Solidité](#solidité-réseau-coupé-serveur-en-panne).

Le noyau `AltimKit` est testé sur les vraies réponses du serveur (`swift test`) ; avec `ALTIM_SERVER`, `ALTIM_USER` et `ALTIM_PASSWORD`, le test de bout en bout se connecte à un serveur réel (mauvais mot de passe refusé, API fermée sans session, reconnexion automatique, flux en direct). La CI compile l'app en Debug et en Release sur macOS ; l'envoi sur TestFlight se lance depuis l'onglet Actions (voir [DEPLOIEMENT.md](DEPLOIEMENT.md)).

## Application Android native (`android`)

Même application que sur iPhone, écran par écran : connexion privée, Radar en direct, fiche d'un actif (graphique avec la zone d'achat, zones de Fibonacci, garde-fou, macro, actualités), Sélection sur 8 durées avec budget, Mes avoirs, Réglages. Les chiffres viennent du même serveur, avec les mêmes textes et les mêmes seuils de preuve.

- **Sécurité** : mot de passe et session chiffrés en AES-256-GCM par une clé du **Keystore Android** propre au téléphone (non exportable) ; sauvegardes cloud et transferts d'appareil désactivés ; HTTPS obligatoire (HTTP seulement pour un serveur local ou l'émulateur). Empreinte, visage ou code de l'écran à l'ouverture et après 2 minutes en arrière-plan. La clé du mot de passe ne fonctionne que téléphone déverrouillé ; l'aperçu des apps récentes est masqué.
- **Notifications « achat possible »** : toutes les 15 minutes (WorkManager), même règle que l'iPhone, sans répétition, résumé au-delà de 3 ; touche → fiche de l'actif.
- **Onglet Alertes** : achetables maintenant, alertes de prix (carte « Alerte de prix » sur la fiche d'un actif) et journal des alertes avec la variation depuis chaque notification, comme sur iPhone.
- **Point du jour**, **simulateur d'investissement programmé**, **comparateur**, **taille de position**, **rééquilibrage**, **alertes de variation** (« préviens-moi si BTC bouge de ±5 % »), **onglet Actu**, **alertes actualité**, **historique de mes lignes** et **mode hors ligne** comme sur iPhone ; Réglages sous la roue dentée du Radar.
- **Widget d'écran d'accueil** « Achetables maintenant » : les actifs achetables à la dernière vérification (symbole, prix, conseillé ou possible) et l'heure de cette vérification ; mis à jour toutes les 15 minutes avec les notifications d'achat. Aucune quantité ni montant de vos avoirs n'y apparaît ; il est vidé à la déconnexion.
- **Sans rafale** : un actif n'est oublié qu'après 6 h sans être achetable et une raison déjà notifiée ne revient pas, même quand le prix hésite au bord d'une zone (iPhone et Android).
- **Tests** : `./gradlew :kit:test` (38 tests ; avec `ALTIM_SERVER`, `ALTIM_USER`, `ALTIM_PASSWORD`, connexion de bout en bout à un vrai serveur). `./gradlew :app:testDebugUnitTest` avec les mêmes variables fait tourner **les vrais écrans** (Robolectric) comme un utilisateur : avertissement, connexion, Radar, fiche BTC, Sélection crypto, ajout d'un avoir et son historique, alerte de prix, Actu, Réglages (plus le widget et les alertes actualité sur les vrais flux), avec une capture de chaque écran dans `android/app/build/screens`.
- **CI** : tests du noyau, lint, build Debug (APK de test téléchargeable dans l'onglet Actions) et Release minifié ; le workflow « Android APK signé » produit l'APK à installer (voir [DEPLOIEMENT.md](DEPLOIEMENT.md)).

### Serveur (Rust + Axum)

Le serveur (`backend/`, Rust 2024, Axum 0.8, Tokio, reqwest) remplace l'ancien serveur Express sur Bun, route pour route : mêmes adresses, mêmes réponses JSON, mêmes textes, mêmes en-têtes de sécurité, mêmes sessions (les connexions ouvertes et les apps déjà connectées restent valides). Il consomme environ 16 Mo de mémoire au lieu de 180 Mo.

- **Moteurs** (signal, fiabilité, Fibonacci, garde-fou, macro, sélection, actualités, alertes, point du jour…) portés ligne à ligne depuis les anciens moteurs TypeScript, aujourd'hui dans `core/` et partagés avec le front Yew : **plus de 4 000 cas de parité** calculés par les moteurs TypeScript sur de vraies bougies (BTC, ETH, SOL, DOGE, AAPL, NVDA, SPY, toutes unités de temps), figés dans `backend/tests/golden` et comparés champ par champ par `cargo test`, textes français compris.
- **Vérifié de bout en bout** : réponses comparées au serveur TypeScript route par route, audit des données contre des références indépendantes, vrais écrans Android (Robolectric) et noyau iPhone connectés au serveur Rust.
- **Corrigé au passage** : l'ancien serveur rangeait les titres du garde-fou et la page Actu sous la même clé de cache (`news:crypto:BTC`), d'où une erreur 502 sur `/api/guard` selon l'ordre des visites ; le cache Rust inclut le type dans la clé.


| Route | Rôle |
|---|---|
| `GET /api/radar?symbols=BTC:crypto,AAPL:stock&interval=4h` | Signaux validés du radar (calculés côté serveur avec le même moteur) |
| `GET /api/candles?symbol=BTC&kind=crypto&interval=1h` | Bougies par consensus + qualité + score de fiabilité |
| `GET /api/tickers?symbols=…` | Cours par consensus (8 sources crypto, 3 actions) |
| `GET /api/search?q=…` · `GET /api/sentiment?symbol=…` | Recherche d'actifs · Fear & Greed et StockTwits |
| `GET /api/live?symbols=BTC:crypto,AAPL:stock` | **Prix en direct** (Server-Sent Events) : dernier prix tout de suite, puis chaque changement |
| `GET /api/zones?symbol=BTC&kind=crypto` | **Zones d'achat** court / moyen / long terme (Fibonacci), vérifiées sur l'historique, avec le contexte macro |
| `GET /api/macro` | **Contexte macro et géopolitique** : VIX, S&P 500, pétrole, or, dollar, taux, actualités d'escalade |
| `GET /api/alerts?symbols=…` | **« Puis-je acheter ? »** pour les notifications des apps : achetable si le signal 4 h dit ACHAT ou si le prix est dans une zone d'achat Fibonacci, sauf sources en désaccord, risque de choc ou plus bas cassé ; une clé de situation évite les notifications répétées |
| `GET /api/brief?symbols=…` | **Point du jour** : contexte de marché, actifs achetables (règle des notifications), variations depuis la dernière clôture, sujets à la une |
| `GET /api/history?symbols=…&days=90` | Clôtures journalières des actifs détenus sur 30, 90, 365 ou 730 jours, plus Bitcoin et SPY pour comparer (les quantités restent sur l'appareil) |
| `GET /api/decision?symbol=BTC&kind=crypto[&cost=…&weights=…]` | **Décision** : verdict, familles, interdictions d'achat, configuration, plan, scénarios, pourquoi pas, fondamentaux, historique du signal, sorties progressives |
| `GET /api/news?symbols=…` | **Actualités** : ~20 sources regroupées, histoires en double fusionnées, à la une, thèmes et ton des 24 h, état de chaque source ; `summary` : les 5 événements importants du jour au plus (impact potentiel faible / moyen / important par règle ou mesuré sur les bougies horaires déjà en cache, actifs concernés, consensus des sources, liens, lecture face à la tendance technique si le garde-fou est en cache ; règle dans `backend/src/engine/news_summary.rs`) |
| `GET /api/calendar?days=14[&symbols=AAPL,NVDA][&top=1]` | **Agenda** : publications économiques majeures, décisions des banques centrales, résultats, dividendes, splits et introductions en bourse des 1 à 30 prochains jours, avec la source de chaque événement, l'état de chaque source et ce qui n'est pas couvert ; `top=1` garde aussi les événements des plus grandes sociétés avec ceux de `symbols` (calendrier de risque sur 7 jours, règle dans `web/src/webapp/calendar.ts`) |
| `GET /api/sectors?symbols=AAPL,NVDA` | **Exposition sectorielle** (Mes avoirs, 50 actions au plus, entrées `:crypto` ignorées) : secteur de chaque action, du screener Nasdaq (cache 24 h), sinon de la division du code SIC déclaré à la SEC (cache 7 jours par société) ; les ETF (annuaire Nasdaq Trader, ou déposant SEC sans code SIC qui n'est pas une société opérationnelle) ne reçoivent aucun secteur. Réponse : `asOf` (ms), `items[]` : `symbol`, `sector` (libellé français ou null), `classification` (`nasdaq` \| `sec` \| `etf` \| null), `source`, `reason` (pourquoi non couvert), `etf`, `sec` (`label`, `sic`, `sicDescription`, `source`), `nasdaq` (`sector`, `sectorFr`, `industry`) ; `sources[]` (`name`, `ok`, `error`). Calcul des poids : `web/src/engine/sectors.ts`. Exemples : `backend/tests/samples/sectors/` |
| `GET /api/strategies?symbol=AAPL&kind=stock` | **Comparateur de stratégies** (cache 1 h par actif) : tendance (SMA 50/200), momentum (ROC 90, mensuel), cassure (Donchian 55/20), retour à la moyenne (RSI 2), swing (repli EMA 20, 2 R), DCA mensuel, achat-conservation, valeur (PER sous sa médiane 5 ans, actions seulement). Paramètres fixes, décision à la clôture journalière, exécution à l'ouverture suivante, frais 0,1 %, glissement 0,05 %, demi-écart. Réponse : `period`, `from`/`to` (ms), `bars`, `source`, `feesPct`, `slippagePct`, `spreadPct` (%), `notes[]`, `strategies[]` : `id`, `name`, `rule`, `params`, `available`, `unavailable`, `metrics` (`totalReturn`, `cagr`, `maxDrawdown`, `winRate`, `expectancy`, `exposure` en % ; `sharpe`, `sortino`, `profitFactor`, `avgR`, `trades`), `regimes[]`, `equity` (≤ 200 × `[ms, valeur de 100 investis]`), `lowSample` (< 10 trades), `note`. Exemples : `backend/tests/samples/strategies-*.json` |
| `GET /api/fx` | **Taux EUR/USD** de l'affichage en euros (cache 10 min, dernier taux valide gardé 7 jours) : Yahoo Finance `EURUSD=X`, sinon taux de référence de la BCE, sinon Frankfurter (BCE). Réponse : `base` "USD", `quote` "EUR", `rate` (€ pour 1 $), `usdPerEur`, `time` (ms, heure du cours ; 14 h 10 à Francfort pour la BCE), `source`, `fetchedAt`, `stale` (dernier rafraîchissement > 10 min) ; sans aucun taux : `rate` et le reste à `null`, `stale` true, `error`. Jamais de taux inventé. Les textes du serveur (décision, alertes, point du jour, zones, anomalies, agenda, opportunités) donnent leurs montants en euros à ce taux (en $ sans taux, ou avec `cur=USD` sur `/api/decision`, `/api/alerts`, `/api/brief`, `/api/zones`, `/api/anomalies`) ; les champs numériques restent en dollars et ces réponses ajoutent `fx` : `{ currency, rate, usdPerEur, time, source }` (taux utilisé pour les textes). `cost=` de `/api/decision` est toujours en dollars. |

## Actualités (onglet « Actu »)

Tout ce qui peut faire bouger vos actifs, réuni sur une page (web, iPhone, Android) :

- **Sources** (en français et en anglais) : Le Monde Économie, BFM Économie, La Tribune, MarketWatch, CNBC, Investing.com, CoinDesk, Cointelegraph, Decrypt, The Block, Cryptoast, Journal du Token, des recherches Google Actualités (économie, géopolitique, marchés) et, **pour chacun de vos actifs**, Google Actualités FR/EN et Yahoo Finance pour les actions. Chaque flux est lu avec un délai maximal de 8 s et gardé 10 min ; une source en panne est signalée et n'empêche pas les autres.
- **Une histoire, une ligne** : le même sujet repris par plusieurs médias (titres semblables à 60 % en moins de 36 h) n'apparaît qu'une fois, avec « +N sources ».
- **À la une** : les sujets repris le plus largement, et toujours une escalade grave (guerre déclarée, invasion, panique bancaire…) marquée ALERTE.
- **Ce qui domine (24 h)** : thèmes (géopolitique, banques centrales, droits de douane, crise, régulation, résultats) et ton des titres (négatif, neutre, positif). Le ton est un repérage par mots-clés, indicatif.
- **Rubriques** Tout / Mes actifs / Monde / Marchés / Crypto, et « articles en français seulement ». Un article n'est rangé dans « Mes actifs » que s'il nomme vraiment l'actif (nom ou symbole, sans faux positif comme « Pineapple » pour Apple).
- Titres affichés tels que publiés, non traduits ; liens http(s) seulement, ouverts chez la source. Vérifié sur les vrais flux (septembre 2026) : 18 sources sur 18 en ligne, ≈ 270 articles sur 48 h.

### Alertes actualité (iPhone et Android)

Dans Réglages, « Me prévenir des actualités importantes » : une notification, vérifiée avec les alertes d'achat (toutes les 15 minutes), quand dans les 6 dernières heures :

- une **escalade grave** (guerre déclarée, invasion, panique bancaire…) est reprise par **au moins 2 sources** : une tribune qui dit qu'une crise « pourrait » arriver n'est racontée que par un média, elle ne déclenche rien (cas réel rencontré pendant les tests) ;
- ou un sujet sur **un actif de votre radar ou de vos avoirs** est repris par **au moins 3 sources**.

Un même sujet raconté par plusieurs médias ne prévient qu'une fois (même article, ou titre aux mêmes mots, mémorisé 48 h). Au-delà de 2 sujets à la fois, une seule notification résume.

### Agenda (Actu → Agenda, web et Android)

Les événements à venir, groupés par jour (« Aujourd'hui », « Demain », « mer. 30 sept. »), en heures de Paris, sur 7, 14 ou 30 jours. Filtres Tout / Macro / Banques centrales / Résultats / Dividendes / Splits / IPO, et **Mes actifs** (résultats, dividendes et splits des actions du radar et des avoirs, plus l'économie et les banques centrales, qui concernent aussi les cryptos). Point rouge : importance haute ; point jaune : moyenne.

**Sources** (gratuites, sans clé) :

| Événements | Source |
|---|---|
| Publications économiques (inflation CPI / PCE / PPI / HICP, emploi, PIB, ISM, ventes au détail, confiance) et décisions de taux | Nasdaq, calendrier économique (`api.nasdaq.com/api/calendar/economicevents`), jour par jour. Attention : ce calendrier répond à `?date=J` avec les publications de la veille (vérifié le 28/09/2026 : la décision de la Fed du 28/10 sort sous le 29/10, les créations d'emplois du vendredi sous le samedi) ; Altim demande donc le lendemain, et si les inscriptions hebdomadaires au chômage (toujours un jeudi, un mercredi en semaine fériée) tombent un autre jour, le jour est marqué en échec plutôt que montré à une mauvaise date. États-Unis et zone euro ; Allemagne, France, Royaume-Uni, Japon et Chine pour leurs publications majeures seulement. Réel, consensus et précédent tels que publiés. |
| Jours de décision de la Fed | Calendrier officiel du FOMC (federalreserve.gov), confirmé contre Nasdaq : en cas d'écart, la date officielle est retenue et l'écart est affiché (fin septembre 2026, Nasdaq annonçait la décision d'octobre le 29, la Fed le 28). |
| Jours de décision de la BCE | Calendrier officiel du Conseil des gouverneurs (ecb.europa.eu) : réunions de politique monétaire seulement (jour 2 d'une réunion sur deux jours). |
| Résultats, dividendes (date de détachement, montant, date de versement), splits et regroupements | Nasdaq (`calendar/earnings`, `calendar/dividends`, `calendar/splits`) ; sans « Mes actifs », limités aux 500 plus grandes capitalisations américaines (classement Nasdaq). |
| Introductions en bourse | Nasdaq (`ipo/calendar`), à venir (date prévue, fourchette de prix) et prix fixés. |

Chaque jour et chaque source sont gardés en cache séparément (15 min pour le jour même, 3 h pour les jours suivants, 24 h pour les calendriers officiels) ; une source ou un jour en panne est signalé (« Sources incomplètes ») sans bloquer le reste, et la réponse part au plus tard après 20 s. Les heures de Nasdaq sont celles de New York, converties en heure de Paris.

**Non couvert** (affiché sous l'agenda) : déblocages de jetons (aucune source gratuite vérifiable), régulation crypto (suivie via l'actualité, pas de calendrier officiel), mises à jour des réseaux crypto, autres pays, confirmation officielle pour la Banque d'Angleterre, la Banque du Japon et la Banque populaire de Chine, sociétés hors marchés américains.

## Historique du portefeuille

Carte **« Évolution de mes lignes »** dans Mes avoirs (web, iPhone, Android), sur 30 jours, 90 jours ou 1 an :

- la valeur, chaque jour, **des quantités que vous détenez aujourd'hui** au cours de clôture (week-ends et jours fériés : dernier cours connu), comparée au **Bitcoin** et au **S&P 500 (SPY)** détenus sur les mêmes jours, en % depuis le premier jour (un seul axe) ;
- pire recul depuis un sommet, meilleure et pire journée ; toucher ou survoler la courbe affiche la valeur du jour ;
- **limites affichées** : vos achats et ventes passés ne sont pas connus, ce n'est donc pas la performance de votre compte ; les liquidités ne sont pas comptées ; si un actif a un historique plus court, la courbe commence plus tard au lieu d'inventer un gain.
- Le serveur ne reçoit que les symboles ; les quantités restent dans le navigateur ou le téléphone. Même calcul sur les trois plateformes, vérifié sur les mêmes vraies clôtures (ETH + AAPL sur 90 jours : +45,5 %, Bitcoin +40,3 %, pire recul −6,8 %).

## Décision : acheter, attendre, alléger, vendre

En tête de la fiche de chaque actif (web, iPhone, Android), `GET /api/decision?symbol=BTC&kind=crypto` : **ACHETER**, **ZONE D'ACHAT**, **ATTENDRE**, **AUCUNE POSITION**, **ALLÉGER** ou **VENDRE**, avec le pourquoi, le quand et surtout le quand ne pas agir.

- **10 familles indépendantes** notées de −100 à +100, chacune avec sa source : tendance, momentum, volume, volatilité / risque, valorisation, fondamentaux (réseau pour une crypto), macro, sentiment, actualités, liquidité. Sans source gratuite et vérifiable, la famille est « non disponible », jamais estimée.
- **Interdictions d'achat** (16 contrôles, tous affichés, actifs ou non) : écart achat/vente, liquidité, volatilité extrême, choc confirmé par les marchés, annonce économique dans les 48 h (Fed, BCE, inflation, emploi, PIB ; calendrier de l'Agenda, un moment à attendre et non une interdiction), résultats dans les 5 jours, chute sans stabilisation, hausse anormale, volume artificiel, régulation, déblocage de jetons (non vérifiable : sources payantes), divergence baissière, tendance de fond baissière, rapport gain/risque sous 2, choc macro, données peu fiables.
- **Configuration d'entrée en 9 étapes** (tendance de fond, correction, retour sur la zone d'achat, volume en baisse pendant le repli, signal de retournement, retour des acheteurs, confirmation, stop, objectif) : « ACHETER » seulement quand elle est complète, sans interdiction, avec un gain/risque d'au moins 2.
- **Plan** (zone, entrée, stop / invalidation, objectifs 1 et 2, gain/risque), **scénarios** haussier / neutre / baissier avec leurs niveaux, conditions pour passer à l'achat ou à la vente, **points favorables et défavorables** séparés et **« Pourquoi pas ? »** : ce qui pourrait rendre la décision mauvaise, cherché volontairement.
- **Confiance du modèle** (0–100) : accord des familles, fiabilité des données et **historique du signal sur l'actif** (sans regard vers l'avenir, frais et glissement inclus ; Sharpe, Sortino, profit factor, pire recul). Mesuré honnêtement : sur BTC, ETH, AAPL ou SPY le signal technique a fait moins bien que la simple détention ; la confiance est alors plafonnée et la carte le dit.
- **Preuve du modèle** (`modelEvidence`, à côté de la confiance) : ce que la validation sur 34 actifs (`/api/validation`) dit de la classe de l'actif (actions et ETF, Bitcoin, Ethereum, altcoins) et du régime de marché où il se trouve (même règle que la validation, sur la dernière bougie journalière close). Lue seulement dans le cache (la décision ne lance jamais ce calcul lourd ; sinon « Validation pas encore calculée »). Si la classe perd en moyenne ou si la simple détention a fait mieux sur plus des deux tiers de ses actifs, confiance plafonnée à 60 ; sans avantage démontré, un point défavorable l'écrit et ACHAT FORT / VENTE FORTE devient ACHAT / VENDRE (raison dans `ratingReason`). Le verdict ne change pas et la validation n'augmente jamais la confiance.
- **Fondamentaux** : actions d'après les comptes déposés à la SEC (EDGAR : chiffre d'affaires, marges, flux de trésorerie libre, dette, PER, PEG, VE/EBITDA, rachats) et Nasdaq (prochains résultats, date estimée signalée, surprises, révisions) ; cryptos d'après CoinGecko ou CoinPaprika (capitalisation, FDV, offre en circulation), DefiLlama (TVL, frais), OKX (financement, positions ouvertes) et blockchain.com (réseau bitcoin). La comparaison au secteur et les déblocages de jetons n'ont pas de source gratuite : c'est affiché.
- **Position détenue** : avec votre prix de revient, sorties progressives (vendre 20 %, puis 30 %, garder le reste) et sorties défensives ou macro ; avec vos pondérations, alerte d'exposition au même facteur de risque (Bitcoin ou S&P 500). Ces valeurs sont envoyées pour le calcul et jamais conservées.
- **Mode informationnel par défaut** (données et scénarios observés) ; « personnel » seulement quand l'app envoie votre prix de revient ou vos pondérations. Altim reste un outil privé, réservé à son propriétaire, et ne passe aucun ordre ; ce n'est pas un conseil en investissement réglementé.

## Simulation (paper trading)

Avant l'argent réel : **Mes avoirs → Simulation** (web, iPhone, Android), un portefeuille virtuel qui suit les décisions sans argent réel ni ordre passé.

- **« Simuler cet achat »** depuis la carte Décision : montant, stop et objectif du plan pré-remplis ; simuler contre la décision (par exemple quand elle dit ATTENDRE) reste possible, avec un avertissement, pour comparer.
- **Coûts réels** : 0,1 % de frais et 0,05 % de glissement à chaque achat et chaque vente.
- **Sorties automatiques** au stop ou à l'objectif, vérifiées sur les bougies journalières après le jour d'achat (la bougie du jour d'achat n'est pas utilisée : son plus bas peut être antérieur à l'achat). Si une même bougie touche les deux, c'est le stop (le pire cas) ; un écart d'ouverture sous le stop est vendu à l'ouverture.
- **Signal → exécution → résultat** : les ventes sont regroupées par décision affichée à l'achat (ACHETER, ZONE D'ACHAT, ATTENDRE…), pour voir quels verdicts marchent vraiment ; sous une vingtaine de trades, l'écran rappelle que les chiffres veulent dire peu.
- Tout reste sur l'appareil. Même moteur sur les trois plateformes, vérifié sur le même scénario de référence (`backend/tests/samples/paper-fixture.json`), au centime près.

## Journal automatique, « Et si… ? » et « Pourquoi ça bouge ? » (web)

- **Journal** (Mes avoirs → Journal, `/app/journal`, `core/src/web/trading/journal.rs`) : chaque achat simulé et chaque achat ou vente réel enregistré dans Mes avoirs est noté avec la décision affichée à ce moment-là (note, titre, 3 arguments pour et contre, confiance, score composite, étapes de la configuration, interdictions actives, plan), le prix, le stop, les objectifs, le signal utilisé, le contexte (régime de marché, stress macro, ATR, volume relatif, annonces à 7 jours) et une note facultative « Pourquoi je suis entré ». À l'ouverture, revue automatique à 3, 10 et 30 jours sur les bougies journalières **après** l'entrée : plus haut et plus bas atteints, stop ou objectif touché en premier (le stop si une bougie touche les deux), résultat en R (multiples du risque pris), faits « ce qui a fonctionné / pas fonctionné », et cohérence avec les données du moment (interdictions actives, signal dégradé, gain/risque < 2, hors zone d'achat, contre le régime). Profil par note à l'entrée, plan respecté ou non et régime de marché, en R et en pire recul (prendre plus de risque n'est pas récompensé), « échantillon trop faible » sous 5. Le premier ajout d'avoirs (souvent des achats passés) n'est inscrit que si on le coche. Tout reste dans le navigateur (`altim.journal.v1`).
- **Et si… ?** (Mes avoirs) : perte de chaque ligne et du total si le Nasdaq-100 (QQQ), le S&P 500 (SPY) ou le Bitcoin baisse de 5 à 50 % (ou d'un pourcentage libre), éventuellement sur un montant réparti selon les poids actuels. Bêta mesuré sur les jours où l'actif et le marché ont tous deux une clôture (90 jours au plus, 30 au moins) ; une ligne sans bêta mesurable est « non couverte », jamais estimée. Hypothèse affichée : choc instantané, relation stable — rarement vrai en crise.
- **Pourquoi ça bouge ?** (fiche d'un actif, `GET /api/why?symbol=&kind=`) : variation depuis la dernière clôture journalière, volume de la dernière séance close face aux 20 précédentes, marché de référence, financement et ratio acheteurs/vendeurs des perpétuels, Fear & Greed, titres d'actualité et leur ton, stress macro et régime, annonces du jour, lecture de la dernière décision calculée — uniquement des sources déjà en cache. Chaque facteur a un sens, une ampleur, une source et un niveau d'incertitude (« observé », « corrélation possible », « non vérifiable ») ; résumé français déterministe. Ce sont des observations simultanées, pas des causes prouvées.
- **Question libre (facultative)** : uniquement si la variable d'environnement `ANTHROPIC_API_KEY` est définie sur le serveur, `POST /api/ask {symbol, kind, question}` envoie la question et **seulement** les données de `/api/why` (et la dernière décision calculée, jamais vos avoirs) à l'API Anthropic (modèle `claude-sonnet-5-5`, 600 jetons de sortie au plus, repli automatique si le modèle refuse), qui répond en français, uniquement à partir de ces données, en citant les champs utilisés et sans jamais donner d'ordre. **Coût** : chaque question est facturée par Anthropic sur le compte de la clé (quelques milliers de jetons en entrée, 600 au plus en sortie, au tarif de 2 $ / 10 $ par million de jetons : un à deux centimes de dollar) ; 4 questions par minute et par adresse au plus. Sans clé, la route répond 503 et la zone de question n'apparaît pas (`askEnabled` de `/api/why`). La clé n'est jamais journalisée ni envoyée au navigateur.

## Outils de décision

Des calculs, pas des conseils ajoutés (web, iPhone, Android ; même calcul vérifié sur les mêmes cas dans les trois langages) :

- **Comparer** (bas du Radar) : 2 à 4 actifs de votre radar sur 30 jours, 90 jours ou 1 an, sur les mêmes jours (un actif plus jeune raccourcit la période) : variation, volatilité annualisée (365 jours pour les cryptos, 252 pour les actions), pire recul depuis un sommet et corrélation des variations journalières (sur les jours où les deux cotent, au moins 20). Vérifié sur les vraies clôtures : l'Ether bouge plus que le Bitcoin, qui bouge plus qu'Apple, et les deux cryptos sont fortement corrélées.
- **Taille de position** (fiche d'un actif) : combien acheter pour que toucher le stop coûte le pourcentage de capital choisi (1 % par défaut, réglage du site), jamais plus que le capital. Stop proposé : celui du plan (2 × ATR) sur le site, sinon le plus bas qui invalide la zone d'achat, sinon 5 % sous le prix, toujours modifiable. Montre la part du capital, la perte au stop, le gain à l'objectif et le rapport gain/risque ; prévient sous 1,5 et quand le stop est trop proche. Un écart de prix (gap) peut faire perdre plus.
- **Rééquilibrer** (Mes avoirs) : la cible crypto / actions (/ liquidités sur le site) et les achats ou ventes pour l'atteindre, répartis au prorata de vos lignes ; « rien à faire » sous 1 % du total. Rappel des frais et de l'impôt avant de vendre.
- **Lexique** (Réglages → Comprendre, web, iPhone, Android) : signal, confiance, zone d'achat, stop, ATR, gain/risque, RSI, MACD, EMA, volatilité, pire recul, corrélation, consensus de prix, garde-fou, DCA, flat tax, rééquilibrage, expliqués simplement.
- **Tri du radar** (site) : mon ordre, plus fortes variations d'abord, ou signaux d'achat les plus forts d'abord ; le choix est retenu.
- **Projection** (Mes avoirs) : ce que deviendraient vos avoirs plus un versement mensuel sur 5, 10 ou 20 ans, avec trois hypothèses côte à côte (0 %, 4 % et 8 % par an, capitalisation mensuelle) ; des hypothèses à comparer, pas des prévisions (une année peut perdre 30 % ou plus, l'inflation n'est pas déduite).
- **Export tableur (CSV)** sur le site (Mes avoirs → Sauvegarde) : une ligne par avoir (quantité, prix moyen, cours, valeur, plus-value, poids, conseil), séparateur « ; » et virgule décimale pour Excel en français ; un texte qui commence comme une formule est neutralisé.
- **Mes notes** (fiche d'un actif) : pourquoi j'achète, mon plan, quand je vends ; gardées sur l'appareil seulement (navigateur, stockage protégé de l'iPhone, téléphone Android sans sauvegarde), jamais envoyées au serveur.
- **Si je vendais** (Mes avoirs) : ce que vous garderiez ligne par ligne et au total après frais de vente (0,1 % par défaut) et impôt sur la plus-value (30 % par défaut : prélèvement forfaitaire unique français, 12,8 % + 17,2 %), les pertes de l'année compensant les gains. Pour les cryptos, la France calcule la plus-value sur l'ensemble du portefeuille à chaque cession (et exonère les cessions de moins de 305 € par an) : le calcul ligne par ligne est une estimation, dit comme tel.
- **Alertes de variation** (iPhone et Android, sur la fiche d'un actif) : « Bouge de ±5 % » depuis le prix au moment de la création ; une notification, puis réarmable (elle repart alors du prix du moment). Les alertes de prix déjà créées continuent de fonctionner.

## Point du jour et investissement programmé

**Point du jour** (en tête du Radar, web, iPhone, Android), actualisé toutes les 5 minutes :

- une phrase : contexte de marché (calme, tendu, tension élevée, d'après le contexte macro), nombre d'actifs **achetables** de votre radar et de vos avoirs (signal 4 h à l'achat ou prix dans une zone d'achat Fibonacci, sans blocage : la règle des notifications, donc parfois « achetable » alors que le badge du radar, qui ne montre que le signal, dit ATTENDRE) et les plus fortes variations ;
- les sujets du moment, les plus fortes variations **depuis la dernière clôture journalière** (touchez pour ouvrir l'actif) et les sujets à la une (repris par au moins 2 sources).

**« Si j'avais investi régulièrement »** (fiche de chaque actif) : X $ chaque semaine ou chaque mois depuis 1 an ou 2 ans, rejoué sur les vraies clôtures journalières, comparé à la même somme investie en une fois le premier jour, avec le prix moyen payé. Sans frais ni impôts ; le passé ne dit pas ce qui arrivera. 2 ans au plus : les sources crypto ne gardent qu'environ 1 000 jours d'historique. Exemple réel (BTC, 100 $ par mois depuis septembre 2025) : 1 300 $ → 1 388 $ (+6,8 %), contre −23 % investis en une fois.

**À la une plus prudente** : un titre au conditionnel ou une opinion (« pourrait déclencher une panique bancaire », « could », « selon cet économiste ») n'est plus une escalade, et un sujet n'est à la une que s'il est repris par au moins 2 sources (cas réel rencontré).

## Solidité : réseau coupé, serveur en panne

- **Nouvelles tentatives** : une lecture qui échoue (réseau coupé, serveur 502/503/504, redémarrage Heroku) est retentée deux fois (après 0,5 s puis 1,5 s). La connexion n'est jamais retentée, pour ne pas compter de faux échecs de mot de passe.
- **Mode hors ligne (iPhone et Android)** : les dernières réponses valides (radar, fiches, zones, alertes, actualités…) sont gardées sur le téléphone (fichiers protégés, 300 au plus) ; sans réseau, l'app les affiche avec un bandeau « Hors ligne : données du … », puis revient seule aux données fraîches. Les recherches et la connexion ne sont jamais servies depuis ce cache, qui est effacé à la déconnexion.
- **Serveur** : arrêt propre sur SIGTERM (redémarrage quotidien d'Heroku) : les flux en direct sont fermés pour que les apps se reconnectent aussitôt, puis le processus s'arrête en 3 s au plus.
- Testé : 503 puis réseau coupé puis succès à la 3ᵉ tentative, abandon après 3 échecs, connexion jamais retentée, réponse hors ligne datée, recherche jamais servie du cache (`swift test`, `./gradlew :kit:test`).

Sécurité : helmet (CSP stricte, HSTS…), redirection HTTPS, compression gzip, limitation de débit par IP, validation de tous les paramètres, cache mémoire borné avec déduplication des requêtes et dernières données valides en cas de panne d'une source. Connexion : une vérification à la fois par adresse et 2 au plus sur le serveur (argon2id, 64 Mo chacune : impossible de saturer la mémoire), 5 échecs → 15 min de blocage (IPv6 par /64), plafond global de 30 échecs, session révoquée côté serveur à la déconnexion. API sans CORS ouvert et jamais mise en cache par le navigateur ; 8 flux en direct au plus par adresse ; messages d'erreur sans adresse de source ; échec fermé sur Heroku même sans `NODE_ENV`.

## Le moteur de signaux

Chaque indicateur vote entre −1 (baissier) et +1 (haussier) ; le score pondéré va de −100 à +100.

| Facteur | Poids | Logique |
|---|---|---|
| Tendance EMA 50/200 (ou 20/50) | 2 | Croisement + position du prix, modulé par la force de l'ADX |
| MACD 12/26/9 | 1,5 | Croisement récent, sinon signe et pente de l'histogramme |
| RSI 14 (Wilder) | 1,5 | Survente < 30 / surachat > 70, sinon momentum |
| Stochastique 14/3/3 | 1 | Croisements en zones extrêmes |
| Bollinger 20/2 | 0,75 | Retour à la moyenne (%B) |
| Volume (OBV) | 1 | Accumulation / distribution |
| Unité de temps supérieure | 1,5 | Confirmation de la tendance de fond ; un achat contre la tendance est rétrogradé |

- **ACHAT** ≥ +25, **ACHAT FORT** ≥ +50 (symétrique pour la vente), confiance = force du score × accord entre indicateurs.
- Seules les **bougies clôturées** sont utilisées (pas de signal qui « repeint »), les données invalides sont écartées.
- Plan de trade : stop à 2 × ATR, objectif à 2 × le risque.
- **Backtest** sans biais d'anticipation (signal à la clôture, exécution à l'ouverture suivante, frais 0,1 %, stop touché en premier en cas de doute), comparé à l'achat-conservation.

### Résultats honnêtes sur données réelles (500 bougies, septembre 2026)

La stratégie **limite fortement les pertes en marché baissier** (BTC 1 j : −3,8 % contre −29,6 % en achat-conservation ; ETH 1 j : +12,8 % contre −10,6 %) mais **fait moins bien qu'un simple achat-conservation en marché très haussier** (NVDA 4 h : −23 % contre +27 %). C'est pour cela que l'app affiche le backtest de chaque actif avant tout ordre.

## Mes avoirs (localStorage)

- Les avoirs sont stockés dans le `localStorage` du navigateur, jamais sur le serveur (seuls les symboles sont envoyés pour obtenir les cours).
- L'export / import JSON permet de passer ses avoirs d'un appareil ou d'un navigateur à l'autre.
- Règles de recommandation (par ordre de priorité) :
  1. Données peu fiables → aucun conseil.
  2. Signaux baissiers en journalier **et** en 4 h → vendre ou protéger.
  3. Signal journalier baissier, ou vente forte en 4 h → protéger (stop).
  4. Ligne au-dessus de 35 % du patrimoine → alléger jusqu'à 30 %.
  5. Plus de 50 % de gain sans signal haussier → sécuriser une partie.
  6. Signaux haussiers en journalier et en 4 h, avec un poids inférieur à 20 % → renforcement possible.
  7. Sinon → conserver.
- Stop de protection conseillé : 2 × l'ATR journalier.

## Fiabilité des données : 40 sources de prix recoupées

| Classe | Sources (aucune clé nécessaire) |
|---|---|
| Crypto (jusqu'à 23) | Binance, OKX, Coinbase, Kraken, KuCoin, Gate.io, Bitfinex, Binance.US, Bitstamp, Gemini, Crypto.com, Bitget, MEXC, HTX, Poloniex, HitBTC, WhiteBIT, CoinEx, XT, WOO X, BingX, LBank, CoinGecko (cours) |
| Actions / ETF (17) | Bougies journalières : Yahoo Finance (2 serveurs), Nasdaq, Robinhood, Cboe, StockAnalysis, Webull, WSJ / MarketWatch (Dow Jones), Financial Times, Finviz, AlphaQuery, eToro. Cours en direct : TradingView, Zacks, Fidelity, StockCharts, TipRanks, Public.com, et ceux de Robinhood, Cboe, Webull, Nasdaq, Yahoo |
| Contexte (hors score) | Fear & Greed (alternative.me), StockTwits |

HTX, BingX et LBank ne servent qu'en 1 h / 4 h (leur bougie journalière commence à 16 h UTC). En 1 h / 4 h, les bougies d'actions viennent de Yahoo (seules alignées sur la séance) et sont recoupées avec 6 cours en direct.

1. **Consensus** : toutes les sources sont interrogées en parallèle. Pour chaque bougie, la référence est la médiane ; une source qui s'en écarte de plus de 0,5 % (crypto) / 1 % (actions) est écartée, tout comme une source en retard (paire inactive). Les bougies analysées sont la médiane des sources concordantes.
2. **Contrôle qualité** des bougies : trous, pics aberrants aussitôt annulés (erreur de cotation), données périmées, volume absent, prix figés. Les fuseaux de New York (heure d'été / d'hiver) sont gérés pour aligner les séances.
3. **Score de fiabilité** (0–100) : qualité, plafonnée à 40 avec une seule source indépendante (pas de conseil), 60 avec deux, 75 avec trois, 90 avec quatre (les 2 serveurs Yahoo comptent pour une). Moins de la moitié des sources d'accord → 30.
4. **Garde-fou** : fiabilité faible → **signal suspendu et achats bloqués**. Fiabilité moyenne → un signal « fort » est ramené à « normal ». La confiance est pondérée par la fiabilité.
5. **Réseau résilient** : nouvelles tentatives avec attente exponentielle sur les lectures (jamais sur les ordres), respect des limites de débit, disjoncteur par source (60 s), conservation des dernières données valides.
6. **Surveillance** : le workflow *Santé des sources* interroge chaque jour toutes les sources sur toutes les unités de temps et alerte par e-mail si un format d'API change.

Mesuré en direct le 27/09/2026 : sur BTC, ETH, SOL, BNB et DOGE, OKX, Coinbase, Kraken et CoinGecko concordent à **0,02–0,15 %** près. Sur AAPL, JPM et NVDA, les 11 sources de bougies journalières concordent (11/11) et les 11 cours en direct aussi ; fiabilité 100/100. Le FT ne cote pas BRK-B, TipRanks ne couvre pas les ETF : ils sont simplement absents du consensus pour ces titres.

## Prix en direct : rien de figé

Les prix bougent en temps réel : radar, fiche d'un actif, conseil et « Mes avoirs ».

- **Cryptos** : flux WebSocket temps réel de **7 bourses** (OKX, Coinbase, Kraken, Bitfinex, Bitget, Gate.io, Crypto.com). Il y a une seule connexion par bourse, partagée par tous les visiteurs, et les abonnements suivent ce qui est affiché. Le prix affiché est la **médiane des bourses d'accord** : une bourse à plus de 1 % des autres est écartée. Une crypto qu'aucune de ces bourses ne cote, ou dont les flux se taisent, est relayée par les sources REST.
- **Actions** : il n'existe pas de flux temps réel gratuit. Robinhood, TradingView, Zacks et Webull sont interrogés **toutes les 5 s** tant que l'écran est ouvert. Hors séance (9 h 30 – 16 h à New York), le badge « Bourse fermée » signale que c'est le dernier cours.
- **Affichage** : 4 mises à jour par seconde au plus par actif, et rien n'est envoyé si le prix n'a pas changé. Le prix clignote en vert ou en rouge à chaque mouvement. Le badge « EN DIRECT » donne l'heure du dernier tick. La courbe du radar et le graphique se terminent sur le prix en direct.
- **Montants** : la zone d'entrée, la valeur du patrimoine, les gains et les quantités suggérées suivent le prix en direct. Les signaux, eux, ne changent qu'à la clôture d'une bougie, pour ne jamais être décidés sur une bougie inachevée.
- **Web** : Server-Sent Events non compressés, avec un battement toutes les 15 s pour traverser le routeur Heroku. Le flux se ferme quand l'onglet est masqué et reprend avec les derniers prix quand on y revient.

Mesuré le 27/09/2026 : BTC, ETH, SOL et PEPE à 8/8 sources d'accord, et 6 à 13 prix différents en 20 s. Les messages de chaque bourse sont figés dans `backend/tests/samples/live-samples.json`, et les tests les rejouent.

## Quelles actions acheter (onglet « Sélection »)

Chaque jour, Altim analyse les **150 plus grandes sociétés cotées aux États-Unis** (capitalisation et secteur : Nasdaq ; une seule classe d'actions par société). Il en retient **10 à acheter**, avec pour chacune un plan et le détail de la décision.

**Mesurer d'abord, choisir ensuite.** Cinq critères ont été testés un par un. Sur 136 grandes actions, de 2021 à 2026, on compare les 10 mieux notées par chaque critère à la moyenne, sur la durée suivante :

| Critère | 10 jours | 3 mois | 6 mois | Verdict |
|---|---|---|---|---|
| Force relative (6 mois, hors dernier mois) | +2,0 % vs +1,2 % | +9,4 % vs +5,1 % | +19,2 % vs +10,3 % | **classe les actions** |
| Tendance de fond | ≈ moyenne | +5,4 % vs +5,1 % | +11,2 % vs +10,2 % | alerte seulement |
| Signal technique d'Altim | moins bien | moins bien | ≈ moyenne | information |
| Zone d'achat (acheter les replis) | moins bien | ≈ moyenne | moins bien | fixe le prix d'entrée |
| Faible volatilité | moins bien | moins bien | moins bien | règle le stop et le montant |

La première version pondérait les cinq critères. Rejouée sur le passé, elle faisait **moins bien que la moyenne** (+6,0 % contre +9,5 %), et elle a été abandonnée. La version publiée classe uniquement par **force relative**, avec au plus **3 actions par secteur**. Sur 52 périodes rejouées depuis novembre 2021 :

| Horizon | Durée de détention | Sélection | Moyenne des 150 | Fait mieux |
|---|---|---|---|---|
| Court terme | 10 séances | +0,66 % | +0,51 % | 52 % du temps : **pas d'avance réelle** |
| Moyen terme | 3 mois | **+9,9 %** | +5,3 % | 69 % du temps |
| Long terme | 6 mois | **+20,5 %** | +11,2 % | 73 % du temps |

**Limites honnêtes :**
- La liste est celle des plus grandes sociétés d'*aujourd'hui* (biais du survivant), ce qui gonfle ces chiffres.
- Entre fin 2021 et 2023, la force relative n'a presque rien apporté : l'essentiel de l'avance vient de 2023–2026.
- La page affiche ces résultats, recalculés chaque jour.

**Pour chaque action retenue :**
- **Rang et secteur.**
- **Détail des 5 critères** : note sur 100, rôle et explication en clair (« +275 % sur 6 mois, mieux que 100 % des autres », « au-dessus de la zone : repli de 23 % pour l'atteindre »…).
- **Vérifications** : prix recoupés sur plusieurs sources, garde-fou marché (choc, retournement, contexte macro) et tendance de fond. Un titre qui échoue passe « à surveiller », avec la raison.
- **Plan** :
  - entrée maintenant, ou ordre limite en haut de la zone d'achat si le prix en est à moins de 10 % ;
  - stop à 1,5 / 2 / 3 fois la volatilité quotidienne selon l'horizon ;
  - objectif à 2 fois le risque.
- **Montant** : le budget saisi est réparti pour que chaque ligne risque la même somme au stop (une action volatile reçoit moins), plafonné par ligne (Réglages). Le nombre d'actions et la perte maximale sont affichés.
- **Historique des signaux d'Altim** sur ce titre, et **prix en direct**.

### Et pour les cryptos ?

Même page, onglet **Cryptos**. L'univers : les **120 plus grandes cryptos** (classement CoinGecko), sans stablecoins ni jetons adossés (WBTC, stETH, jetons d'or…), soit environ 110 avec un historique. Les données journalières viennent de Gate, MEXC ou Kraken (≈ 1 000 jours).

Sur les cryptos, les mêmes critères ne donnent **pas** les mêmes résultats. Mesure sur 80 cryptos, d'avril 2024 à septembre 2026 :

| Critère | 10 jours | 1 mois | 3 mois | Verdict |
|---|---|---|---|---|
| **Signal technique d'Altim** | +1,2 % vs −0,3 % | +2,3 % vs −1,9 % | +2,0 % vs −6,4 % | **classe les cryptos** : positif dans les deux moitiés de la période |
| Faible volatilité | ≈ moyenne | +0,6 % vs −1,9 % | +2,3 % vs −6,4 % | règle le stop et le montant |
| Force relative | irrégulière selon la fenêtre | irrégulière | irrégulière | information |
| Les plus échangées (volume) | moins bien | moins bien | −9,7 % vs −6,4 % | écartée |
| Zone d'achat (acheter les replis) | moins bien | moins bien | moins bien | prix d'entrée seulement |

Rejeu de la sélection publiée (10 cryptos classées par le signal) sur 36 à 42 périodes depuis janvier 2025 :

| Horizon | Détention | Sélection | Moyenne des cryptos | Bitcoin | Fait mieux |
|---|---|---|---|---|---|
| Court terme | 10 jours | +1,2 % | +0,5 % | +0,05 % | 60 % |
| Moyen terme | 1 mois | +1,3 % | −2,3 % | −0,5 % | 60 % |
| Long terme | 3 mois | −1,0 % | −6,8 % | −3,9 % | 58 % |

**En clair :** la sélection a perdu moins que les autres cryptos et fait mieux que le Bitcoin, mais sur 3 mois elle a quand même perdu. Sur cette période, presque toutes les cryptos ont baissé.

**Limites :**
- L'historique ne couvre qu'environ 2 ans et demi.
- La liste ne contient que les cryptos qui existent encore aujourd'hui, ce qui embellit les chiffres.
- Les quantités sont fractionnées.

### 8 durées de détention : 30 min, 1 h, 5 h, 7 j, 14 j, 1 mois, 3 mois, 6 mois

Chaque durée a ses propres réglages (`SPECS` dans `src/engine/screener.ts`) :
- sa taille de bougies : 5, 15 ou 30 minutes jusqu'à 5 h, un jour au-delà ;
- son critère de classement, choisi après mesure pour chaque marché ;
- son stop, calé sur la volatilité de la période ;
- ses frais aller-retour : 0,05 % pour les actions, 0,2 % pour les cryptos.

Chaque sélection est **rejouée sur le passé** et classée :
- **avance nette** : mieux que la moyenne de plus que les frais, plus de 55 % du temps ;
- **faible** : mieux en moyenne, mais environ une fois sur deux ;
- **aucune** : pas mieux que la moyenne une fois les frais payés. La page l'affiche en rouge : « ne misez pas dessus ».

Rejeu dans l'application le 27/09/2026 :

| Durée | Actions : classement | Rejeu | Cryptos : classement | Rejeu |
|---|---|---|---|---|
| 30 min | rebond (les plus en baisse) | **aucune** avance | rebond sur 6 h | **aucune** |
| 1 h | rebond sur 1 h | **aucune** | force sur 1 h | **aucune** |
| 5 h | force sur 5 h | **aucune** | rebond sur 5 h | **aucune** |
| 7 j | force relative 6 mois | faible (+0,73 % vs +0,29 %, 54 %) | force relative 3 mois | nette (+0,34 % vs −0,19 %, 57 %) |
| 14 j | force relative 6 mois | faible (+1,27 % vs +0,80 %, 52 %) | force relative 3 mois | faible (−0,62 % vs −1,19 %, 48 %) |
| 1 mois | force relative 6 mois | nette (+2,6 % vs +1,7 %, 59 %) | signal technique | nette (+1,3 % vs −2,3 %, 60 %) |
| 3 mois | force relative 6 mois | nette (+9,9 % vs +5,3 %, 69 %) | signal technique | nette (−1,0 % vs −6,8 %, 58 %) |
| 6 mois | force relative 6 mois | nette (+20,5 % vs +11,2 %, 73 %) | les plus calmes | nette (+11,2 % vs −13,1 %, 97 %) |

**En clair :** en dessous de quelques jours, aucun classement n'a fait mieux que le hasard une fois les frais payés, ni pour les actions ni pour les cryptos. Une première étude trouvait une avance à 5 h, mais elle a disparu sur une autre fenêtre de temps : c'était du bruit. Ces durées restent disponibles, avec cet avertissement. Les classements fiables commencent à 1 mois.

Pour les durées courtes :
- **Données** : bougies de Yahoo (actions, 60 jours de rejeu) et de Gate (cryptos, 3,5 jours à 3 semaines selon la taille des bougies). Seules les bougies clôturées sont utilisées.
- **Rafraîchissement** : toutes les 4 à 12 minutes.
- **Bourse fermée** : les actions sont classées sur la dernière séance, et la page le signale.

**Jetons adossés** : une crypto qui ne bouge presque pas (volatilité ramenée à la journée inférieure à 0,5 %) est traitée comme un stablecoin et écartée.

API : `GET /api/selection?kind=stock|crypto&horizon=30m|1h|5h|7d|14d|1m|3m|6m` (les anciens `short`, `medium` et `long` restent acceptés). Les durées journalières sont calculées au démarrage puis toutes les 25 minutes ; les durées courtes le sont à la demande.

## Zones d'achat par horizon (Fibonacci) et contexte macro

On n'achète pas au même endroit selon qu'on investit pour quelques jours ou pour des années. Chaque fiche d'actif montre trois horizons ; celui choisi dans **Réglages → Mon horizon d'investissement** est mis en avant et repris dans le conseil.

| Horizon | Bougies | Mouvement analysé | Détention |
|---|---|---|---|
| Court terme | 4 h | ≈ 2 dernières semaines | quelques jours à 2 semaines |
| Moyen terme | 1 j | ≈ 4 derniers mois | quelques semaines à quelques mois |
| Long terme | 1 semaine (≈ 3 ans d'historique) | ≈ 2 dernières années | plusieurs mois à plusieurs années |

- **Retracements de Fibonacci** du dernier mouvement haussier (du plus bas au plus haut) :
  - zone d'achat : 38,2 % – 61,8 % ;
  - « zone d'or » : 61,8 % – 65 % ;
  - invalidation : sous le plus bas ;
  - objectifs : le plus haut, puis les extensions 127,2 % et 161,8 %.
- **Rebond** : après une baisse, si le prix est remonté d'au moins 38,2 %, le rebond devient le mouvement en cours.
- **Mouvement baissier** : pas de zone d'achat.
- Le statut suit le **prix en direct** : attendre le repli (à −x %), dans la zone, zone d'or, repli profond ou zone invalidée.
- **Vérifié sur chaque actif** : à chaque entrée passée dans la zone (sans regarder le futur), Altim regarde si le prix est revenu au plus haut avant de casser le plus bas, et compare avec une entrée au hasard aux mêmes distances.

  Mesuré en septembre 2026 sur 12 actifs (BTC, ETH, SOL, XRP, BNB, DOGE, AAPL, NVDA, MSFT, SPY, QQQ, TSLA) :

  | Horizon | Cas | Zone | Hasard | Verdict |
  |---|---|---|---|---|
  | Court terme | 95 | 44 % | 39 % | un peu mieux, surtout sur les indices (SPY 62 % contre 33 %) |
  | Moyen terme | 128 | 29 % | 39 % | **moins bien que le hasard** |
  | Long terme | 33 | 55 % | 45 % | trop peu de cas pour conclure |

  Fibonacci est une convention suivie par beaucoup de traders, pas une loi : la fiche le dit, actif par actif.

**Contexte macro et géopolitique** (`/api/macro`, commun à tous les actifs) : une zone d'achat ne protège pas d'une guerre ou d'une crise. Personne ne peut prévoir ces événements, mais leur effet sur les marchés se mesure dès qu'il commence :

- **Marchés** (Yahoo, 5 ans de données) :
  - VIX au-delà de 25 / 30, ou bond inhabituel ;
  - S&P 500 en recul de 4 % / 7 % sur son plus haut du mois ;
  - mouvement inhabituel sur 5 séances, comparé à l'année écoulée : pétrole (choc d'offre, guerres), or (refuge), dollar (fuite vers la sécurité), taux à 10 ans (banques centrales).
- **Actualités** (Google News) : sujets du jour et événements d'escalade des 12 dernières heures (déclaration de guerre, invasion, menace nucléaire, blocage d'un détroit, panique bancaire…). Les titres en forme de question sont ignorés.
- **Vérifié sur chaque actif** : les jours de stress (score ≥ 25) sont-ils suivis d'une forte baisse dans les 5 jours plus souvent que d'habitude ?
  - SPY : 49 % contre 24 % ; QQQ : 43 % contre 23 % ; NVDA : 34 % contre 20 % ; AAPL : 33 % contre 21 %.
  - Aucun effet sur BTC, ETH ni SOL (16 % contre 16 %).
  - Le facteur « macro » du garde-fou compte donc pleinement sur les actions et n'est pas compté sur ces cryptos.
- **Effet sur les zones** :
  - court terme : éviter d'entrer ;
  - moyen terme : entrer en plusieurs fois ;
  - long terme : achats échelonnés.
  - Un bandeau apparaît sur le radar quand le contexte est tendu.

## Accès privé

Le site, l'application et l'API sont réservés à leur propriétaire (`backend/src/auth.rs`). Les secrets vivent uniquement dans les variables d'environnement Heroku, jamais dans le code :

| Variable | Rôle |
|---|---|
| `ALTIM_USER` | identifiant |
| `ALTIM_PASSWORD_HASH` | hachage **argon2id** du mot de passe (le mot de passe lui-même n'est stocké nulle part) |
| `ALTIM_TOTP_SECRET` | second facteur : code à 6 chiffres d'une application d'authentification (RFC 6238) |
| `ALTIM_SESSION_SECRET` | clé de signature des sessions (la changer déconnecte tous les appareils) |
| `ALTIM_API_TOKEN` | jeton des bots : `Authorization: Bearer …` sur `/api/*` uniquement |
| `ANTHROPIC_API_KEY` | *facultatif* : active la question libre de « Pourquoi ça bouge ? » (`POST /api/ask`), facturée par Anthropic à chaque question |

Les valeurs se génèrent avec `cargo run --release --bin altim-secrets` : mot de passe aléatoire de 24 caractères (≈ 139 bits), secret 2FA de 160 bits, clé de session de 512 bits, jeton API de 256 bits.

Protections en place :
- **Session** : cookie signé HMAC-SHA256, `HttpOnly`, `Secure`, `SameSite=Strict`, valable 7 jours.
- **Comparaisons en temps constant** : aucune information ne fuit par le temps de réponse.
- **Code 2FA** : un code déjà utilisé est refusé (anti-rejeu).
- **Verrouillage** : 15 minutes par adresse IP après 5 échecs, avec un délai aléatoire après chaque échec.
- **Formulaires** : l'origine est vérifiée (CSRF), et les redirections vers un autre site sont refusées.
- **Confidentialité** : ni indexation (`noindex`, `robots.txt`), ni cache partagé.
- **Échec fermé** : en production, sans configuration, rien n'est servi.

## Garde-fou marché (et API pour vos bots)

Un bot court terme voit les petites variations mais pas ce qui l'entoure. Le garde-fou ajoute trois couches, pour chaque actif (application web et API) :

| Couche | Ce qu'elle mesure | À quoi elle sert |
|---|---|---|
| **Régime** | Tendance de fond : moyennes 50 / 200 jours, pente, ADX, confirmation en 4 h | Bot long terme : dans quel sens travailler |
| **Risque de choc** (0–100) | Volatilité des 24 h vs normale, sauts de prix en écarts-types, pic de volume, compression des bandes de Bollinger, rafale d'actualités, VIX (actions) | Bot court terme : continuer, réduire la taille, ou suspendre |
| **Risque de retournement** (0–100) | Mouvement **contre** la tendance : RSI extrême, divergences, surextension, bougie de rejet ; positionnement de la foule (financement des perpétuels, ratio acheteurs/vendeurs et positions ouvertes chez OKX) ; sentiment (Fear & Greed, StockTwits) ; ton des actualités (Google News) | Anticiper les retournements que l'analyse de tendance ne voit pas |

**Auto-validation.** Aucun outil ne prévoit une vraie surprise. Chaque signal calculable sur les bougies est donc vérifié sur l'historique de l'actif lui-même, sans regarder le futur. On mesure la part de ses apparitions suivies de l'événement annoncé (grand mouvement dans les 6 h ou les 24 h, ou mouvement contraire de 3 ATR dans les 3 jours), puis on la compare à la normale. Le poids du signal en découle :

| Statut | Condition | Poids |
|---|---|---|
| Vérifié | S'est avéré utile sur cet actif (≥ 1,1 fois la normale) | 0,2 à 1 (plein poids à 1,5 fois la normale) |
| Peu d'historique | Moins de 20 cas passés | Moitié |
| Rejeté | Jamais prédictif sur cet actif | 0, affiché mais ignoré |
| Non vérifiable | Aucun historique gratuit (financement, sentiment, actualités) | 0,75 |

Mesuré le 27/09/2026 sur 10 actifs (BTC, ETH, SOL, DOGE, LINK, XRP, AAPL, NVDA, SPY, TSLA), sur les bougies 1 h et 4 h disponibles :

| Signal | Événement suivant | Fréquence de l'événement vs normale | Cas |
|---|---|---|---|
| Saut horaire ≥ 3 écarts-types | Nouveau grand mouvement dans les 6 h | ×1,49 | 577 |
| Volatilité 24 h ≥ 1,5 × normale | Nouveau grand mouvement dans les 6 h | ×1,20 | 399 |
| RSI 4 h extrême | Mouvement contraire de 3 ATR en 3 jours | ×1,62 | 57 |
| Divergence RSI 4 h | Mouvement contraire de 3 ATR en 3 jours | ×0,96 | 616 |
| Surextension 4 h | Mouvement contraire de 3 ATR en 3 jours | ×0,89 | 197 |
| Compression des bandes 4 h | Grand mouvement dans les 24 h | ×0,93 | 578 |

C'est pourquoi le poids de chaque signal est recalculé pour chaque actif au lieu d'être fixé une fois pour toutes. En tendance, l'essoufflement apparent annonce souvent… la suite de la tendance.

**Politique pour les bots.**

| Situation | Court terme | Taille | Stop |
|---|---|---|---|
| Choc (≥ 65) | Suspendu | × 0 | × 2 |
| Marché agité (≥ 35) | Taille réduite | × 0,5 | × 1,5 |
| Retournement ≥ 50 | Pas de nouvelle position dans le sens de la tendance, stops resserrés | × 0,5 | — |

Les conseils d'Altim appliquent les mêmes règles : pas d'« Achat envisageable » en plein choc ou sur un retournement probable, et montant divisé par deux en marché agité.

### API

```
GET /api/guard?symbol=BTC&kind=crypto      (kind = crypto | stock)
Authorization: Bearer <ALTIM_API_TOKEN>
```

Réponse JSON (mise en cache 60 s, CORS ouvert) :

```json
{
  "symbol": "BTC", "kind": "crypto", "asOf": 1790520000000, "price": 84871.45,
  "regime":   { "trend": "up", "strength": 100, "text": "…" },
  "shock":    { "score": 8, "level": "calm", "factors": [ { "code": "squeeze", "points": 8, "basePoints": 15, "status": "unproven",
                "evidence": { "samples": 11, "rate": 45.5, "base": 22.4, "lift": 2.03 }, "text": "…" } ] },
  "reversal": { "score": 0, "direction": "down", "factors": [] },
  "policy":   { "scalping": "ok", "sizeMultiplier": 1, "stopMultiplier": 1, "notes": ["…"] },
  "inputs":   { "fundingRate": -0.0000096, "longShortRatio": 1.27, "openInterestUsd": 3151244635, "fearGreed": 70,
                "socialBullish": 47.4, "socialSample": 19, "news24h": 13, "newsTone": { "negative": 0, "positive": 3 },
                "headlines": [ { "title": "…", "time": 1790519100000 } ], "vix": null }
}
```

Exemple côté bot : la boucle rapide du bot (millisecondes, secondes) reste la sienne. Il interroge le garde-fou toutes les 30 à 60 s, car ses données changent à l'échelle de la minute, et il applique la politique :

```python
import os, requests, time
guard = {}
while True:
    guard = requests.get("https://VOTRE-APP.herokuapp.com/api/guard", params={"symbol": "BTC", "kind": "crypto"},
                         headers={"Authorization": f"Bearer {os.environ['ALTIM_API_TOKEN']}"}, timeout=10).json()
    time.sleep(45)
# dans la boucle de trading :
# if guard["policy"]["scalping"] == "pause": ne rien ouvrir
# taille *= guard["policy"]["sizeMultiplier"]; stop_distance *= guard["policy"]["stopMultiplier"]
# if guard["reversal"]["score"] >= 50 and guard["reversal"]["direction"] == "down": pas de nouvel achat, resserrer les stops
```

Limites :
- X / Twitter (API payante) et Reddit (bloque les robots) ne sont pas accessibles gratuitement. StockTwits tient lieu de source sociale.
- Le financement et le ratio acheteurs/vendeurs viennent d'OKX, uniquement pour les cryptos qui y ont un contrat perpétuel.
- Les actualités sont notées par mots-clés (anglais et français), pas par une IA.

## Un conseiller, pas un courtier

Altim ne passe aucun ordre et ne demande aucun accès à vos comptes. Les conseils suivent des règles prudentes :

1. **Pas de conseil plutôt qu'un mauvais conseil** : si les sources sont absentes ou en désaccord, Altim le dit et ne conseille rien.
2. **Montant prudent** : pour une idée d'achat, le montant est calculé pour que la perte au stop reste limitée à 1 % du patrimoine (réglable), avec un plafond par ligne.
3. **Vos avoirs d'abord** : sur un actif déjà détenu, le conseil tient compte de votre plus-value, du poids de la ligne et des signaux 1 j / 4 h.
4. **Confidentialité** : vos avoirs restent dans votre navigateur (localStorage), jamais sur un serveur.

## Bloomberg

Les données Bloomberg (Terminal, B-PIPE, API BLPAPI) exigent une licence professionnelle payante et ne sont pas accessibles depuis une app grand public. Altim recoupe à la place les 40 sources ci-dessus ; toute source s'ajoute à la liste `SOURCES` du serveur : un flux Bloomberg peut être ajouté au consensus si vous avez une licence.

## Tests

```bash
cargo test --workspace                                                   # serveur (parité avec les moteurs TS, routes, sources, accès privé) et logique du front (core)
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p altim-core -p altim-web -p altim-bundles --target wasm32-unknown-unknown --lib --examples -- -D warnings   # le front tel que compilé pour le navigateur
sh scripts/build-web.sh                                                  # front construit dans web/dist (site + groupes d'écrans de l'app)
cd backend && cargo test --test sources_live -- --ignored               # toutes les sources × unités de temps + flux temps réel, en réel
ALTIM_DEV_OPEN=1 cargo run --release --bin altim                         # accès sans connexion, limité à 127.0.0.1 ; puis :
cargo run --release --bin altim-audit                                    # audit des données contre des références indépendantes
```

Les écrans eux-mêmes (rendu Yew) sont vérifiés dans un navigateur à 320 et 1 100 px : aucun défilement horizontal, aucune erreur dans la console.

Références vérifiées : RSI de Wilder (exemple StockCharts), parseurs construits à partir de réponses réelles de chaque source.

## Crédits

Conception, design et développement : **Maxime Nathan Lestage**.
