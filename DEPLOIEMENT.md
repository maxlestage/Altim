# Déployer Altim 100 % depuis un iPhone

Tout se fait dans Safari (ou l'app GitHub) : aucun ordinateur n'est nécessaire.

## 0. Accès privé (obligatoire)

Sans ces variables, le site déployé reste fermé : toutes les pages mènent à la connexion.

1. Générez vos valeurs (dans un terminal, ou demandez-les à Claude) : `cd web && bun run secrets -- --user VOTRE_IDENTIFIANT`.
2. Heroku → votre app → **Settings** → **Config Vars** → ajoutez `ALTIM_USER`, `ALTIM_PASSWORD_HASH`, `ALTIM_TOTP_SECRET`, `ALTIM_SESSION_SECRET`, `ALTIM_API_TOKEN`.
3. Dans votre application d'authentification (Google Authenticator, 1Password, Authy…) : **ajouter un compte** → saisir la clé `ALTIM_TOTP_SECRET` (type « basé sur l'heure »).
4. Rangez le **mot de passe** dans un gestionnaire de mots de passe. Il n'est écrit nulle part ailleurs : Heroku ne garde que son hachage.
5. Ouvrez le site : identifiant, mot de passe, puis le code à 6 chiffres.

En cas de doute (appareil perdu, fuite), relancez `bun run secrets` et remplacez toutes les valeurs : l'ancienne session, l'ancien mot de passe et l'ancien jeton cessent de fonctionner.

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

**Option B : intégration GitHub de Heroku** (dépôt privé : connectez votre compte GitHub à Heroku)

Heroku → **New** → **Create new app** → onglet **Deploy** → **GitHub** → sélectionnez le dépôt → branche `master` → **Enable Automatic Deploys** → **Deploy Branch**.

Deux buildpacks, dans cet ordre (le workflow et le bouton Heroku les configurent) : **Rust** (`emk/rust`) compile le serveur de `backend/` (voir `RustConfig`, version de Rust dans le même fichier) et laisse sa chaîne d'outils aux suivants, puis **Bun** (`https://github.com/jakeg/heroku-buildpack-bun`) lance `scripts/heroku-build.sh`, qui construit le site dans `web/dist` avec `scripts/build-web.sh` : front Rust + Yew (WebAssembly) et, pendant la migration, l'app React servie sous `/app`. Le `Procfile` lance `backend/target/release/altim` ; sa phase `release` refuse le déploiement si le site n'a pas été construit. Première compilation ≈ 5 à 10 min, les suivantes réutilisent le cache.

**Le dépôt se déploie quelle que soit la configuration de l'app** :
- buildpacks Rust puis Bun (recommandé, le plus rapide grâce au cache ; l'ancien ordre Bun puis Rust marche aussi, en installant Rust une seconde fois) ;
- aucun buildpack réglé : Heroku détecte Node.js, le script `heroku-postbuild` (`scripts/heroku-build.sh`) installe alors Rust, construit le site (Yew, et React avec Bun obtenu par npm) et compile le serveur (≈ 6 min à chaque déploiement, sans cache) ;
- app restée sur la pile « container » : `heroku.yml` construit le `Dockerfile`.

La phase `release` refuse le déploiement si le site ou le serveur manque, avec un message clair.

Pour des déploiements plus rapides avec l'intégration GitHub de Heroku (option B), réglez les buildpacks une fois : Heroku → votre app → **Settings** → **Buildpacks** → **Add buildpack** (`emk/rust`, puis `https://github.com/jakeg/heroku-buildpack-bun`, dans cet ordre), ou dans un terminal :

```bash
heroku stack:set heroku-24 -a VOTRE-APP
heroku buildpacks:clear -a VOTRE-APP
heroku buildpacks:add emk/rust -a VOTRE-APP
heroku buildpacks:add https://github.com/jakeg/heroku-buildpack-bun -a VOTRE-APP
```

### Source WSJ / MarketWatch (facultatif)

La source Dow Jones (WSJ / MarketWatch) a besoin du jeton public de ses graphiques. Il n'est pas écrit dans le code : ajoutez-le dans Heroku → **Settings** → **Config Vars** → `WSJ_TOKEN`. Sans lui, cette source est simplement ignorée et les 16 autres sources d'actions fonctionnent.

## 2. Application iPhone native

L'app se connecte à **votre serveur Heroku** (étape 1) : déployez-le et réglez l'accès privé (étape 0) avant de l'ouvrir.

### Vérifier que l'app compile
À chaque modification de `ios/`, le workflow **iOS** compile l'app (Debug et Release) sur un Mac de GitHub et lance les tests. Rien à faire.

### Ouvrir le projet dans Xcode
Le projet Xcode est versionné : ouvrez `ios/Altim.xcodeproj` (schéma **Altim** : app iPhone, Live Activity et app Apple Watch). Dans **Signing & Capabilities**, choisissez votre équipe (Team) pour les trois cibles, puis lancez sur votre iPhone. Il est généré par XcodeGen depuis `ios/project.yml` : après avoir ajouté ou supprimé un fichier Swift, relancez `xcodegen generate` dans `ios/` (la CI prévient si le projet versionné n'est plus à jour).

### Installer l'app sur votre iPhone via TestFlight
Nécessite un **compte Apple Developer** (99 €/an), que l'on peut ouvrir depuis l'app *Apple Developer* sur iPhone.

1. [appstoreconnect.apple.com](https://appstoreconnect.apple.com) → **Apps** → **+** → **Nouvelle app** : plateforme iOS, nom « Altim », identifiant de lot `com.maxlestage.altim` (s'il n'apparaît pas, lancez une première fois le workflow TestFlight : il enregistre l'identifiant).
2. **Utilisateurs et accès** → **Intégrations** → **Clés App Store Connect** → **+** → rôle **Admin** → téléchargez le fichier `.p8` (ouvrez-le dans l'app Fichiers et copiez son contenu).
3. Notez l'**Issuer ID**, le **Key ID** et votre **Team ID** (developer.apple.com → Account → Membership).
4. Ajoutez sur GitHub (**Settings** → **Secrets and variables** → **Actions**) les secrets `APPLE_TEAM_ID`, `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8` (contenu complet du .p8, lignes BEGIN/END comprises).
5. Onglet **Actions** → **TestFlight** → **Run workflow**. Environ 15 min plus tard, le build apparaît dans l'app **TestFlight** : installez-le.

### Premier lancement
1. Acceptez l'avertissement, puis saisissez l'adresse du serveur (par exemple `mon-app.herokuapp.com`, sans `https://`) → **Continuer**.
2. Identifiant (`ALTIM_USER`) et mot de passe → **Se connecter**. Aucun code n'est demandé si `ALTIM_TOTP_SECRET` n'est pas défini sur Heroku.
3. C'est tout : le mot de passe est chiffré dans le trousseau de l'iPhone, l'app se reconnecte seule quand la session de 7 jours expire, et Face ID protège l'ouverture (désactivable dans Réglages).
4. **Notifications d'achat** : Réglages → « Me prévenir quand je peux acheter » → autorisez les notifications. Elles s'affichent aussi sur l'Apple Watch quand l'iPhone est verrouillé.
5. **Dynamic Island** : sur la fiche d'un actif, bouton « Suivre » (icône en haut à droite). Réglages → Live Activity pour l'activer ou arrêter le suivi.
6. **Apple Watch** : l'app s'installe avec celle de l'iPhone (app Watch de l'iPhone → Altim → Installer si ce n'est pas automatique). Elle affiche ce que l'iPhone a calculé ; « Mettre à jour » lui demande une nouvelle vérification.

Le build TestFlight contient trois éléments signés automatiquement par le workflow : l'app (`com.maxlestage.altim`), l'extension Live Activity (`com.maxlestage.altim.widgets`) et l'app Watch (`com.maxlestage.altim.watchkitapp`). Aucune clé de notification Apple (APNs) n'est nécessaire : les notifications sont créées par l'iPhone lui-même après chaque vérification.

Sans compte Apple Developer, le site reste utilisable comme une app : Safari → **Ouvrir l'app** → **Partager** → **Sur l'écran d'accueil**.

## 3. Application Android native

Même principe que l'iPhone : l'app se connecte à votre serveur Heroku. Aucun compte Google Play n'est nécessaire, l'APK s'installe directement.

### Créer la clé de signature (une seule fois)
Android n'accepte une mise à jour que si elle est signée par la **même clé** que la version installée : créez-la une fois et gardez-la.

1. Sur un ordinateur avec Java : `keytool -genkeypair -keystore altim.jks -alias altim -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=Altim"` (choisissez un mot de passe robuste), puis `base64 -w0 altim.jks` (sur Mac : `base64 -i altim.jks`).
2. GitHub → **Settings** → **Secrets and variables** → **Actions** : `ANDROID_KEYSTORE_BASE64` (le texte base64), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` (`altim`), `ANDROID_KEY_PASSWORD` (le même mot de passe).
3. Gardez `altim.jks` et son mot de passe en lieu sûr, hors du dépôt : sans eux, plus de mise à jour possible (il faudrait désinstaller puis réinstaller).

### Installer l'APK
1. Onglet **Actions** → **Android APK signé** → **Run workflow**. Environ 5 min plus tard : **Summary** → **Artifacts** → **Altim-apk**.
2. Ouvrez le fichier sur le téléphone (décompressez le .zip), autorisez l'installation depuis cette source quand Android le demande, puis **Installer**.
3. Premier lancement : avertissement, adresse du serveur (`mon-app.herokuapp.com`), identifiant et mot de passe, comme sur iPhone.
4. **Notifications d'achat** : Réglages → « Me prévenir quand je peux acheter » → Autoriser. Android vérifie toutes les 15 minutes, même app fermée (tant que l'optimisation de batterie ne la bloque pas).

Pour un simple essai sans clé, chaque exécution du workflow **Android** fournit aussi un APK de test (`altim-debug-apk`) ; il ne pourra pas être mis à jour par l'APK signé (désinstallez-le avant).

## 4. Renseigner vos avoirs

Altim est un conseiller : il ne passe aucun ordre et ne demande aucune clé de courtier.

- **Ouvrir l'app** → **Mes avoirs** : ajoutez chaque actif (quantité, prix d'achat moyen) et vos liquidités. Tout est gardé dans le navigateur.
- **Exporter** / **Importer** transfère vos avoirs d'un appareil à l'autre (fichier JSON).

Chaque fiche d'actif affiche alors **« Le conseil d'Altim »**, adapté à ce que vous possédez.

## Récapitulatif des secrets GitHub

GitHub → dépôt → **Settings** → **Secrets and variables** → **Actions** → **New repository secret**. Aucun de ces secrets n'est dans le dépôt, et un fork n'y a pas accès.

| Secret | Utilisé par | Où l'obtenir |
|---|---|---|
| `HEROKU_API_KEY` | Déploiement Heroku | Heroku → Account settings → API Key (ou `heroku authorizations:create`) |
| `HEROKU_APP_NAME` | Déploiement Heroku | Le nom de votre app Heroku |
| `APPLE_TEAM_ID` | TestFlight | developer.apple.com → Account → Membership → Team ID |
| `ASC_KEY_ID` | TestFlight | App Store Connect → Utilisateurs et accès → Intégrations → Clés → Key ID |
| `ASC_ISSUER_ID` | TestFlight | Même page, en haut : Issuer ID |
| `ASC_KEY_P8` | TestFlight | Contenu complet du fichier `.p8` téléchargé à la création de la clé (une seule fois) |
| `ANDROID_KEYSTORE_BASE64` | APK Android signé | Clé de signature encodée en base64 (voir « Créer la clé de signature ») |
| `ANDROID_KEYSTORE_PASSWORD` | APK Android signé | Mot de passe de la clé |
| `ANDROID_KEY_ALIAS` | APK Android signé | `altim` |
| `ANDROID_KEY_PASSWORD` | APK Android signé | Même mot de passe |

Les variables d'accès du serveur (`ALTIM_USER`, `ALTIM_PASSWORD_HASH`, `ALTIM_SESSION_SECRET`) se règlent dans **Heroku → Settings → Config Vars**, pas sur GitHub.
