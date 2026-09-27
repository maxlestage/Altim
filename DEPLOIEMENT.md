# Déployer Altim 100 % depuis un iPhone

Tout se fait dans Safari (ou l'app GitHub) : aucun ordinateur n'est nécessaire.

## 1. Site web sur Heroku

> Heroku est payant (dyno *Eco* ≈ 5 $/mois ou *Basic* ≈ 7 $/mois). Ajoutez un moyen de paiement dans votre compte Heroku.

**Option A : GitHub Actions (recommandé, redéploie à chaque modification)**

1. Sur [dashboard.heroku.com](https://dashboard.heroku.com) → avatar → **Account settings** → **API Key** → *Reveal* → copiez la clé.
2. Sur GitHub, ouvrez le dépôt → **Settings** → **Secrets and variables** → **Actions** → **New repository secret** :
   - `HEROKU_API_KEY` = la clé copiée
   - `HEROKU_APP_NAME` = un nom libre, par ex. `altim-web` (l'app est créée automatiquement en Europe)
3. Onglet **Actions** → **Déploiement Heroku** → **Run workflow**.
4. À la fin, le résumé du workflow affiche l'adresse du site (`https://altim-web-xxxx.herokuapp.com`).

Ensuite, chaque fusion sur `master` qui touche au site redéploie automatiquement.

**Option B : bouton Heroku**

Ouvrez `https://www.heroku.com/deploy?template=https://github.com/maxlestage/altim` (le dépôt doit être public ou votre GitHub connecté à Heroku), choisissez un nom, **Deploy app**.

**Option C : intégration GitHub de Heroku**

Heroku → **New** → **Create new app** → onglet **Deploy** → **GitHub** → sélectionnez le dépôt → branche `master` → **Enable Automatic Deploys** → **Deploy Branch**.

Aucun buildpack à configurer : Heroku détecte une app Node.js et Bun est installé automatiquement comme dépendance npm.

## 2. App iOS

### Vérifier que l'app compile
À chaque modification, le workflow **iOS** compile l'app sur un Mac de GitHub et lance les tests. Rien à faire.

### Installer l'app sur votre iPhone via TestFlight
Nécessite un **compte Apple Developer** (99 €/an), que l'on peut ouvrir depuis l'app *Apple Developer* sur iPhone.

1. [appstoreconnect.apple.com](https://appstoreconnect.apple.com) → **Apps** → **+** → **Nouvelle app** : plateforme iOS, nom « Altim », identifiant de lot `com.maxlestage.altim` (s'il n'apparaît pas, lancez une première fois le workflow TestFlight : il enregistre l'identifiant).
2. **Utilisateurs et accès** → **Intégrations** → **Clés App Store Connect** → **+** → rôle **Admin** → téléchargez le fichier `.p8` (ouvrez-le dans l'app Fichiers et copiez son contenu).
3. Notez l'**Issuer ID**, le **Key ID** et votre **Team ID** (developer.apple.com → Account → Membership).
4. Ajoutez sur GitHub les secrets `APPLE_TEAM_ID`, `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8` (contenu complet du .p8, lignes BEGIN/END comprises).
5. Onglet **Actions** → **TestFlight** → **Run workflow**. Environ 15 min plus tard, le build apparaît dans l'app **TestFlight**.

Alternative avec un iPad : l'app *Swift Playgrounds* peut compiler une app SwiftUI directement sur l'iPad.

## 3. Sources de données supplémentaires (facultatif, gratuit)

Dans l'app → Réglages → *Sources de données supplémentaires* :
- **Twelve Data** : [twelvedata.com](https://twelvedata.com) → *Get free API key* (800 requêtes/jour)
- **Polygon.io** : [polygon.io](https://polygon.io) → *Sign up* → clé gratuite (données différées)
- **Finnhub** : [finnhub.io](https://finnhub.io) → *Get free API key*

Puis **Tester toutes les sources** : l'écran affiche chaque source, son prix et son écart au consensus.

## 4. Connecter vos comptes de trading (dans l'app → Réglages)

**Commencez toujours en test :**
- Binance **testnet** : [testnet.binance.vision](https://testnet.binance.vision) → connexion GitHub → *Generate HMAC_SHA256 Key*. Ces clés ne fonctionnent qu'en mode « Test ».
- Alpaca **paper trading** : [app.alpaca.markets](https://app.alpaca.markets) → compte Paper → *API Keys*.

**Passage au réel :**
- Binance → *Gestion des API* → créer une clé avec **uniquement** « Activer le trading Spot et sur marge » (**jamais** « Activer les retraits »), restreinte à une IP si possible.
- Dans l'app : désactivez le mode démo, collez les clés, choisissez « Réel » et confirmez.
