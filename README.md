# Altim

Application iOS (Swift / SwiftUI) de **signaux d'achat et de vente** pour la crypto et les actions, avec exécution des ordres sur Binance (crypto) et Alpaca (actions US), plus un site de présentation en React + TypeScript + Bun déployé sur Heroku.

> ⚠️ Altim est un outil d'aide à la décision, pas un conseil en investissement. Aucun algorithme ne garantit de gain. Commencez en mode démo / testnet.

## Contenu

| Dossier | Rôle |
|---|---|
| `ios/AltimCore` | Moteur en Swift pur, testé : indicateurs, moteur de signaux, gestion du risque, backtest, données de marché, courtiers |
| `ios/Altim` | App SwiftUI (style néon/holographique) : Radar, analyse détaillée, passage d'ordre, portefeuille, réglages |
| `web` | Site vitrine React + TS + Bun, avec **démo live** du moteur (portage TypeScript vérifié contre le Swift) |
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

## Sécurité des ordres

1. Taille calculée pour risquer 1 % du capital (réglable), plafond par position, coupe-circuit de perte journalière.
2. Vérification locale des règles du marché (pas de quantité, prix, montant minimum) avec des `Decimal` (pas d'erreurs d'arrondi).
3. Ordre **test** envoyé au courtier (`/api/v3/order/test` chez Binance), puis **Face ID / code**, puis exécution.
4. Après un achat Binance : ordre **OCO** stop + objectif posé automatiquement sur la quantité nette de frais. Chez Alpaca : ordre *bracket*.
5. Clés API dans le trousseau iOS (`WhenUnlockedThisDeviceOnly`). N'activez **jamais** la permission de retrait.
6. Mode démo par défaut, testnet Binance / paper trading Alpaca, confirmation explicite pour passer en argent réel.

## Bloomberg

Les données Bloomberg (Terminal, B-PIPE, API BLPAPI) exigent une licence professionnelle payante et ne sont pas accessibles depuis une app grand public. Altim utilise Binance et Yahoo Finance, mais toute source se branche via le protocole `MarketDataProvider` : un fournisseur Bloomberg peut être ajouté si vous avez une licence.

## Tests

```bash
cd ios/AltimCore && swift test                 # 35 tests (indicateurs, moteur, risque, backtest, courtiers simulés)
ALTIM_LIVE=1 swift test --filter LiveTests     # sur données réelles (réseau)
cd web && bun test && bun run typecheck        # le moteur TS reproduit le moteur Swift à 1e-9 près
```

Références vérifiées : RSI de Wilder (exemple StockCharts), vecteur de signature HMAC officiel de la documentation Binance, corps d'ordres Binance/Alpaca et OCO sur quantité nette de frais.
