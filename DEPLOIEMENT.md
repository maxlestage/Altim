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

**Option B : bouton Heroku**

Ouvrez `https://www.heroku.com/deploy?template=https://github.com/maxlestage/altim` (le dépôt doit être public ou votre GitHub connecté à Heroku), choisissez un nom, **Deploy app**.

**Option C : intégration GitHub de Heroku**

Heroku → **New** → **Create new app** → onglet **Deploy** → **GitHub** → sélectionnez le dépôt → branche `master` → **Enable Automatic Deploys** → **Deploy Branch**.

Aucun buildpack à configurer : Heroku détecte une app Node.js et Bun est installé automatiquement comme dépendance npm.

### Source WSJ / MarketWatch (facultatif)

La source Dow Jones (WSJ / MarketWatch) a besoin du jeton public de ses graphiques. Il n'est pas écrit dans le code : ajoutez-le dans Heroku → **Settings** → **Config Vars** → `WSJ_TOKEN`. Sans lui, cette source est simplement ignorée et les 16 autres sources d'actions fonctionnent.

## 2. Utiliser l'app sur iPhone

Ouvrez le site dans Safari → **Ouvrir l'app** → bouton **Partager** → **Sur l'écran d'accueil**. Altim s'ouvre alors comme une app, en plein écran.

## 3. Renseigner vos avoirs

Altim est un conseiller : il ne passe aucun ordre et ne demande aucune clé de courtier.

- **Ouvrir l'app** → **Mes avoirs** : ajoutez chaque actif (quantité, prix d'achat moyen) et vos liquidités. Tout est gardé dans le navigateur.
- **Exporter** / **Importer** transfère vos avoirs d'un appareil à l'autre (fichier JSON).

Chaque fiche d'actif affiche alors **« Le conseil d'Altim »**, adapté à ce que vous possédez.
