# Altim

Application iOS (Swift / SwiftUI) de **conseil** pour la crypto et les actions : quand acheter, attendre, alléger ou protéger, en tenant compte de **ce que vous possédez déjà**. Altim **ne passe aucun ordre** et ne demande aucun accès à vos comptes : vous suivez ou non ses conseils chez votre courtier habituel. Plus un site de présentation en React + TypeScript + Bun déployé sur Heroku.

> ⚠️ Altim est un outil d'aide à la décision, pas un conseil en investissement. Aucun algorithme ne garantit de gain.

## Contenu

| Dossier | Rôle |
|---|---|
| `ios/AltimCore` | Moteur en Swift pur, testé : indicateurs, moteur de signaux, gestion du risque, backtest, données de marché, analyse des avoirs, conseiller |
| `ios/Altim` | App SwiftUI (style néon/holographique) : Radar, analyse détaillée, **conseil adapté à vos avoirs**, **Mes avoirs (base SQLite)**, réglages |
| `web` | Site vitrine + **application web `/app`** (React + TS), serveur **Express sur Bun**, mobile first, **multi-source** |
| `.github/workflows` | CI iOS (build + tests sur macOS), CI web, déploiement Heroku, envoi TestFlight |

➡️ **Déploiement depuis un iPhone, sans ordinateur : voir [DEPLOIEMENT.md](DEPLOIEMENT.md).**

## Application web (`/app`)

Accessible depuis le bouton **« Ouvrir l'app »** du site, sans installation :

- **Radar** : signaux validés de vos actifs (crypto et actions), fiabilité des données, Fear & Greed, opportunités détectées ; actualisation automatique toutes les 60 s.
- **Analyse** : graphique (EMA 20/50, stop/objectif, entrées du backtest), jauge et détail des 7 facteurs, fiabilité avec la liste des sources et leurs écarts, backtest, sentiment.
- **Le conseil d'Altim** sur chaque actif : *Achat envisageable* (avec zone d'entrée, stop, objectif et **montant prudent** calculé sur votre patrimoine), *Attendre*, *À éviter*, ou *Pas de conseil* si les sources sont en désaccord. Si vous détenez déjà l'actif, le conseil devient celui de votre ligne : *Conserver*, *Renforcer possible*, *Alléger* (avec le montant), *Protéger* ou *Vendre ou protéger*.
- **Catalogue complet** : toutes les cryptos listées en USD/USDT sur OKX, Coinbase, Kraken, KuCoin et Gate (≈ 2 000, classées par capitalisation CoinGecko) et toutes les actions et ETF cotés aux États-Unis (≈ 11 600, annuaire officiel Nasdaq Trader, classés par capitalisation). Parcours par catégorie ou recherche par symbole ou nom, sur le web (`/api/universe`) comme sur l'iPhone.
- **Mes avoirs** : vous renseignez en une fois plusieurs cryptos et plusieurs actions (actif, quantité, prix d'achat moyen), plus vos liquidités. Les données sont **gardées dans le navigateur (localStorage)**, avec export/import JSON. Altim en déduit tout : patrimoine et plus-values au prix de consensus, répartition crypto/actions/liquidités, et pour chaque ligne une recommandation expliquée (*Vendre ou protéger*, *Protéger*, *Alléger*, *Renforcer possible*, *Conserver*, ou *Données insuffisantes*) avec le stop de protection conseillé. S'y ajoutent les risques du portefeuille : volatilité, perte possible sur une mauvaise journée (VaR 95 %), perte si les stops sont touchés, concentration, diversification effective et corrélation.
- **Réglages** : radar, prudence des conseils (risque accepté par idée, taille maximale d'une ligne).

Aucun ordre, aucune clé de courtier : les données de l'app web restent dans le navigateur (localStorage).

Le moteur TypeScript (signal, confirmation par l'unité supérieure, avertissements, garde-fou de fiabilité, backtest, gestion du risque) est **vérifié contre le moteur Swift** sur 17 scénarios, backtest compris trade par trade.

### Serveur (Express sur Bun)

| Route | Rôle |
|---|---|
| `GET /api/radar?symbols=BTC:crypto,AAPL:stock&interval=4h` | Signaux validés du radar (calculés côté serveur avec le même moteur) |
| `GET /api/candles?symbol=BTC&kind=crypto&interval=1h` | Bougies par consensus + qualité + score de fiabilité |
| `GET /api/tickers?symbols=…` | Cours par consensus (8 sources crypto, 3 actions) |
| `GET /api/search?q=…` · `GET /api/sentiment?symbol=…` | Recherche d'actifs · Fear & Greed et StockTwits |

Sécurité : helmet (CSP stricte, HSTS…), redirection HTTPS, compression gzip, limitation de débit par IP, validation de tous les paramètres, cache mémoire avec déduplication des requêtes et dernières données valides en cas de panne d'une source.

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

## Mes avoirs : web (localStorage) et iOS (SQLite)

- **Web** : les avoirs sont stockés dans le `localStorage` du navigateur, jamais sur le serveur (seuls les symboles sont envoyés pour obtenir les cours).
- **iOS** : base **SQLite** sur l'iPhone (`HoldingsDatabase`) avec :
  - schéma versionné (`PRAGMA user_version`) et migrations ;
  - contraintes d'intégrité (quantité > 0, une ligne par actif) ;
  - journal WAL ;
  - import atomique : tout ou rien.
- Le fichier d'export JSON est **commun** : on peut passer ses avoirs du web à l'iPhone et inversement.
- Le moteur d'analyse (`holdings.ts` et `HoldingsAnalyzer.swift`) est identique sur les deux plateformes, vérifié par un fichier de référence partagé.
- Règles de recommandation (par ordre de priorité) :
  1. Données peu fiables → aucun conseil.
  2. Signaux baissiers en journalier **et** en 4 h → vendre ou protéger.
  3. Signal journalier baissier, ou vente forte en 4 h → protéger (stop).
  4. Ligne au-dessus de 35 % du patrimoine → alléger jusqu'à 30 %.
  5. Plus de 50 % de gain sans signal haussier → sécuriser une partie.
  6. Signaux haussiers en journalier et en 4 h, avec un poids inférieur à 20 % → renforcement possible.
  7. Sinon → conserver.
- Stop de protection conseillé : 2 × l'ATR journalier.

## Fiabilité des données : 16 sources de prix recoupées

| Classe | Sources sans clé | Sources avec clé gratuite |
|---|---|---|
| Crypto | Binance, OKX, Coinbase, Kraken, KuCoin, Gate.io, Bitfinex, Binance.US, CoinGecko (cours), Yahoo Finance | Twelve Data |
| Actions / ETF | Yahoo Finance (2 serveurs), Nasdaq (journalier), Cboe (cours) | Alpaca (IEX), Twelve Data, Polygon, Finnhub (cours) |
| Contexte (hors score) | Fear & Greed (alternative.me), StockTwits | |

1. **Consensus** : au moins 3 sources interrogées en parallèle, par ordre de priorité, en basculant sur les suivantes en cas de panne. Pour chaque bougie, la référence est la médiane ; une source qui s'en écarte de plus de 0,5 % (crypto) / 1 % (actions) est écartée.
2. **Contrôle qualité** des bougies : trous, pics aberrants aussitôt annulés (erreur de cotation), données périmées, volume absent, prix figés. Les fuseaux de New York (heure d'été / d'hiver) sont gérés pour aligner les séances.
3. **Score de fiabilité** (0–100) : qualité, plafonnée à 50 avec une seule source indépendante et à 80 avec deux (les 2 serveurs Yahoo comptent pour une). Sources en désaccord → 30.
4. **Garde-fou** : fiabilité faible → **signal suspendu et achats bloqués**. Fiabilité moyenne → un signal « fort » est ramené à « normal ». La confiance est pondérée par la fiabilité.
5. **Réseau résilient** : nouvelles tentatives avec attente exponentielle sur les lectures (jamais sur les ordres), respect des limites de débit, disjoncteur par source (60 s), conservation des dernières données valides.
6. **Surveillance** : le workflow *Santé des sources* interroge chaque jour toutes les sources sur toutes les unités de temps et alerte par e-mail si un format d'API change.

Mesuré en direct le 27/09/2026 : sur BTC, ETH, SOL, BNB et DOGE, OKX, Coinbase, Kraken et CoinGecko concordent à **0,02–0,15 %** près. Sur AAPL, Nasdaq et Yahoo donnent les mêmes clôtures sur 500 séances, date par date.

## Un conseiller, pas un courtier

Altim ne passe aucun ordre et ne demande aucun accès à vos comptes. Les conseils suivent des règles prudentes :

1. **Pas de conseil plutôt qu'un mauvais conseil** : si les sources sont absentes ou en désaccord, Altim le dit et ne conseille rien.
2. **Montant prudent** : pour une idée d'achat, le montant est calculé pour que la perte au stop reste limitée à 1 % du patrimoine (réglable), avec un plafond par ligne.
3. **Vos avoirs d'abord** : sur un actif déjà détenu, le conseil tient compte de votre plus-value, du poids de la ligne et des signaux 1 j / 4 h.
4. **Confidentialité** : vos avoirs restent sur l'appareil (SQLite sur iPhone, localStorage sur le web), jamais sur un serveur.

Le moteur `AltimCore` contient encore des connecteurs Binance / Alpaca testés, qui ne sont plus utilisés par l'app.

## Bloomberg

Les données Bloomberg (Terminal, B-PIPE, API BLPAPI) exigent une licence professionnelle payante et ne sont pas accessibles depuis une app grand public. Altim recoupe à la place les 16 sources ci-dessus ; toute source se branche via le protocole `MarketSource` : un flux Bloomberg peut être ajouté au consensus si vous avez une licence.

## Tests

```bash
cd ios/AltimCore && swift test                         # 83 tests : indicateurs, moteur, risque, backtest, avoirs, conseiller,
                                                       # consensus, qualité, résilience réseau, API publique
ALTIM_LIVE=1 swift test --filter LiveSourcesTests      # toutes les sources × toutes les unités de temps, en réel
cd web && bun test && bun run typecheck                # moteur TS = moteur Swift à 1e-9 près, consensus serveur
```

Références vérifiées : RSI de Wilder (exemple StockCharts), vecteur de signature HMAC officiel de la documentation Binance, corps d'ordres Binance/Alpaca et OCO sur quantité nette de frais, parseurs construits à partir de réponses réelles de chaque source.

## Crédits

Conception, design et développement : **Maxime Nathan Lestage**.
