# Altim

Application iOS (Swift / SwiftUI) de **signaux d'achat et de vente** pour la crypto et les actions, avec exécution des ordres sur Binance (crypto) et Alpaca (actions US), plus un site de présentation en React + TypeScript + Bun déployé sur Heroku.

> ⚠️ Altim est un outil d'aide à la décision, pas un conseil en investissement. Aucun algorithme ne garantit de gain. Commencez en mode démo / testnet.

## Contenu

| Dossier | Rôle |
|---|---|
| `ios/AltimCore` | Moteur en Swift pur, testé : indicateurs, moteur de signaux, gestion du risque, backtest, données de marché, courtiers |
| `ios/Altim` | App SwiftUI (style néon/holographique) : Radar, analyse détaillée, passage d'ordre, portefeuille, réglages |
| `web` | Site vitrine React + TS + Bun, mobile first, **multi-source** : panneau des marchés (consensus de 8 sources crypto et 3 sources actions, `/api/tickers`) et démo live du moteur (consensus de 6 sources, `/api/candles`) |
| `.github/workflows` | CI iOS (build + tests sur macOS), CI web, déploiement Heroku, envoi TestFlight |

➡️ **Déploiement depuis un iPhone, sans ordinateur : voir [DEPLOIEMENT.md](DEPLOIEMENT.md).**

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

## Sécurité des ordres

1. Taille calculée pour risquer 1 % du capital (réglable), plafond par position, coupe-circuit de perte journalière.
2. Vérification locale des règles du marché (pas de quantité, prix, montant minimum) avec des `Decimal` (pas d'erreurs d'arrondi).
3. **Prix du courtier comparé au consensus** des sources indépendantes : au-delà de 1 % d'écart (1,5 % pour les actions), l'ordre est bloqué.
4. Ordre **test** envoyé au courtier (`/api/v3/order/test` chez Binance), puis **Face ID / code**, puis exécution.
5. **Jamais de double ordre** : chaque ordre porte un identifiant unique. Si la connexion coupe pendant l'envoi, Altim retrouve l'ordre par cet identifiant au lieu de le renvoyer, et vous dit clairement s'il est passé ou non.
6. Après un achat Binance : ordre **OCO** stop + objectif posé automatiquement sur la quantité nette de frais. Chez Alpaca : ordre *bracket*.
7. Clés API dans le trousseau iOS (`WhenUnlockedThisDeviceOnly`). N'activez **jamais** la permission de retrait.
8. Mode démo par défaut, testnet Binance / paper trading Alpaca, confirmation explicite pour passer en argent réel.

## Bloomberg

Les données Bloomberg (Terminal, B-PIPE, API BLPAPI) exigent une licence professionnelle payante et ne sont pas accessibles depuis une app grand public. Altim recoupe à la place les 16 sources ci-dessus ; toute source se branche via le protocole `MarketSource` : un flux Bloomberg peut être ajouté au consensus si vous avez une licence.

## Tests

```bash
cd ios/AltimCore && swift test                         # 73 tests : indicateurs, moteur, risque, backtest, courtiers,
                                                       # consensus, qualité, résilience réseau, reprise d'ordre, API publique
ALTIM_LIVE=1 swift test --filter LiveSourcesTests      # toutes les sources × toutes les unités de temps, en réel
cd web && bun test && bun run typecheck                # moteur TS = moteur Swift à 1e-9 près, consensus serveur
```

Références vérifiées : RSI de Wilder (exemple StockCharts), vecteur de signature HMAC officiel de la documentation Binance, corps d'ordres Binance/Alpaca et OCO sur quantité nette de frais, parseurs construits à partir de réponses réelles de chaque source.

## Crédits

Conception, design et développement : **Maxime Nathan Lestage**.
