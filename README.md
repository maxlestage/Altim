# Altim

Application web de **conseil** pour la crypto et les actions : quand acheter, attendre, alléger ou protéger, en tenant compte de **ce que vous possédez déjà**. Altim **ne passe aucun ordre** et ne demande aucun accès à vos comptes : vous suivez ou non ses conseils chez votre courtier habituel. Site de présentation et application (`/app`) en React + TypeScript, serveur Express sur Bun, déployés sur Heroku. Sur iPhone, Safari → Partager → « Sur l'écran d'accueil » l'ouvre comme une app.

> ⚠️ Altim est un outil d'aide à la décision, pas un conseil en investissement. Aucun algorithme ne garantit de gain.

## Contenu

| Dossier | Rôle |
|---|---|
| `web` | Site vitrine + **application web `/app`** (React + TS), serveur **Express sur Bun**, mobile first, **multi-source**, **API garde-fou pour bots** |
| `.github/workflows` | CI web, déploiement Heroku, santé quotidienne des sources |

➡️ **Déploiement depuis un iPhone, sans ordinateur : voir [DEPLOIEMENT.md](DEPLOIEMENT.md).**

## Application web (`/app`)

Accessible depuis le bouton **« Ouvrir l'app »** du site, sans installation :

- **Radar** : signaux validés de vos actifs (crypto et actions), fiabilité des données, Fear & Greed, opportunités détectées ; actualisation automatique toutes les 60 s.
- **Analyse** : graphique (EMA 20/50, stop/objectif, entrées du backtest), jauge et détail des 7 facteurs, fiabilité avec la liste des sources et leurs écarts, backtest, sentiment.
- **Le conseil d'Altim** sur chaque actif : *Achat envisageable* (avec zone d'entrée, stop, objectif et **montant prudent** calculé sur votre patrimoine), *Attendre*, *À éviter*, ou *Pas de conseil* si les sources sont en désaccord. Si vous détenez déjà l'actif, le conseil devient celui de votre ligne : *Conserver*, *Renforcer possible*, *Alléger* (avec le montant), *Protéger* ou *Vendre ou protéger*.
- **Catalogue complet** : toutes les cryptos listées en USD/USDT sur OKX, Coinbase, Kraken, KuCoin et Gate (≈ 2 000, classées par capitalisation CoinGecko) et toutes les actions et ETF cotés aux États-Unis (≈ 11 600, annuaire officiel Nasdaq Trader, classés par capitalisation). Parcours par catégorie ou recherche par symbole ou nom (`/api/universe`).
- **Mes avoirs** : vous renseignez en une fois plusieurs cryptos et plusieurs actions (actif, quantité, prix d'achat moyen), plus vos liquidités. Les données sont **gardées dans le navigateur (localStorage)**, avec export/import JSON. Altim en déduit tout : patrimoine et plus-values au prix de consensus, répartition crypto/actions/liquidités, et pour chaque ligne une recommandation expliquée (*Vendre ou protéger*, *Protéger*, *Alléger*, *Renforcer possible*, *Conserver*, ou *Données insuffisantes*) avec le stop de protection conseillé. S'y ajoutent les risques du portefeuille : volatilité, perte possible sur une mauvaise journée (VaR 95 %), perte si les stops sont touchés, concentration, diversification effective et corrélation.
- **Réglages** : radar, prudence des conseils (risque accepté par idée, taille maximale d'une ligne).

Aucun ordre, aucune clé de courtier : les données de l'app web restent dans le navigateur (localStorage).

Le moteur (signal, confirmation par l'unité supérieure, avertissements, garde-fou de fiabilité, backtest, gestion du risque) est **vérifié sur 17 scénarios de référence**, backtest compris trade par trade.

### Serveur (Express sur Bun)

| Route | Rôle |
|---|---|
| `GET /api/radar?symbols=BTC:crypto,AAPL:stock&interval=4h` | Signaux validés du radar (calculés côté serveur avec le même moteur) |
| `GET /api/candles?symbol=BTC&kind=crypto&interval=1h` | Bougies par consensus + qualité + score de fiabilité |
| `GET /api/tickers?symbols=…` | Cours par consensus (8 sources crypto, 3 actions) |
| `GET /api/search?q=…` · `GET /api/sentiment?symbol=…` | Recherche d'actifs · Fear & Greed et StockTwits |
| `GET /api/live?symbols=BTC:crypto,AAPL:stock` | **Prix en direct** (Server-Sent Events) : dernier prix tout de suite, puis chaque changement |

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

## Fiabilité des données : 31 sources de prix recoupées

| Classe | Sources (aucune clé nécessaire) |
|---|---|
| Crypto (jusqu'à 23) | Binance, OKX, Coinbase, Kraken, KuCoin, Gate.io, Bitfinex, Binance.US, Bitstamp, Gemini, Crypto.com, Bitget, MEXC, HTX, Poloniex, HitBTC, WhiteBIT, CoinEx, XT, WOO X, BingX, LBank, CoinGecko (cours) |
| Actions / ETF (8) | Yahoo Finance (2 serveurs), Nasdaq, Robinhood, Cboe, StockAnalysis, Webull (journalier) ; TradingView, Zacks, et les cours en direct de Robinhood, Cboe, Webull |
| Contexte (hors score) | Fear & Greed (alternative.me), StockTwits |

HTX, BingX et LBank ne servent qu'en 1 h / 4 h (leur bougie journalière commence à 16 h UTC). En 1 h / 4 h, les bougies d'actions viennent de Yahoo (seules alignées sur la séance) et sont recoupées avec 6 cours en direct.

1. **Consensus** : toutes les sources sont interrogées en parallèle. Pour chaque bougie, la référence est la médiane ; une source qui s'en écarte de plus de 0,5 % (crypto) / 1 % (actions) est écartée, tout comme une source en retard (paire inactive). Les bougies analysées sont la médiane des sources concordantes.
2. **Contrôle qualité** des bougies : trous, pics aberrants aussitôt annulés (erreur de cotation), données périmées, volume absent, prix figés. Les fuseaux de New York (heure d'été / d'hiver) sont gérés pour aligner les séances.
3. **Score de fiabilité** (0–100) : qualité, plafonnée à 40 avec une seule source indépendante (pas de conseil), 60 avec deux, 75 avec trois, 90 avec quatre (les 2 serveurs Yahoo comptent pour une). Moins de la moitié des sources d'accord → 30.
4. **Garde-fou** : fiabilité faible → **signal suspendu et achats bloqués**. Fiabilité moyenne → un signal « fort » est ramené à « normal ». La confiance est pondérée par la fiabilité.
5. **Réseau résilient** : nouvelles tentatives avec attente exponentielle sur les lectures (jamais sur les ordres), respect des limites de débit, disjoncteur par source (60 s), conservation des dernières données valides.
6. **Surveillance** : le workflow *Santé des sources* interroge chaque jour toutes les sources sur toutes les unités de temps et alerte par e-mail si un format d'API change.

Mesuré en direct le 27/09/2026 : sur BTC, ETH, SOL, BNB et DOGE, OKX, Coinbase, Kraken et CoinGecko concordent à **0,02–0,15 %** près. Sur AAPL, Nasdaq et Yahoo donnent les mêmes clôtures sur 500 séances, date par date.

## Prix en direct : rien de figé

Les prix bougent en temps réel : radar, fiche d'un actif, conseil et « Mes avoirs ».

- **Cryptos** : flux WebSocket temps réel de **7 bourses** (OKX, Coinbase, Kraken, Bitfinex, Bitget, Gate.io, Crypto.com). Il y a une seule connexion par bourse, partagée par tous les visiteurs, et les abonnements suivent ce qui est affiché. Le prix affiché est la **médiane des bourses d'accord** : une bourse à plus de 1 % des autres est écartée. Une crypto qu'aucune de ces bourses ne cote, ou dont les flux se taisent, est relayée par les sources REST.
- **Actions** : il n'existe pas de flux temps réel gratuit. Robinhood, TradingView, Zacks et Webull sont interrogés **toutes les 5 s** tant que l'écran est ouvert. Hors séance (9 h 30 – 16 h à New York), le badge « Bourse fermée » signale que c'est le dernier cours.
- **Affichage** : 4 mises à jour par seconde au plus par actif, et rien n'est envoyé si le prix n'a pas changé. Le prix clignote en vert ou en rouge à chaque mouvement. Le badge « EN DIRECT » donne l'heure du dernier tick. La courbe du radar et le graphique se terminent sur le prix en direct.
- **Montants** : la zone d'entrée, la valeur du patrimoine, les gains et les quantités suggérées suivent le prix en direct. Les signaux, eux, ne changent qu'à la clôture d'une bougie, pour ne jamais être décidés sur une bougie inachevée.
- **Web** : Server-Sent Events non compressés, avec un battement toutes les 15 s pour traverser le routeur Heroku. Le flux se ferme quand l'onglet est masqué et reprend avec les derniers prix quand on y revient.

Mesuré le 27/09/2026 : BTC, ETH, SOL et PEPE à 8/8 sources d'accord, et 6 à 13 prix différents en 20 s. Les messages de chaque bourse sont figés dans `web/test/live-samples.json`, et les tests les rejouent.

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
import requests, time
guard = {}
while True:
    guard = requests.get("https://VOTRE-APP.herokuapp.com/api/guard", params={"symbol": "BTC", "kind": "crypto"}, timeout=10).json()
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

Les données Bloomberg (Terminal, B-PIPE, API BLPAPI) exigent une licence professionnelle payante et ne sont pas accessibles depuis une app grand public. Altim recoupe à la place les 31 sources ci-dessus ; toute source s'ajoute à la liste `SOURCES` du serveur : un flux Bloomberg peut être ajouté au consensus si vous avez une licence.

## Tests

```bash
cd web && bun test && bun run typecheck                        # moteur, conseils, garde-fou, consensus, prix en direct, serveur
cd web && ALTIM_LIVE=1 bun test test/sources-live.test.ts     # toutes les sources × unités de temps + flux temps réel, en réel
```

Références vérifiées : RSI de Wilder (exemple StockCharts), parseurs construits à partir de réponses réelles de chaque source.

## Crédits

Conception, design et développement : **Maxime Nathan Lestage**.
