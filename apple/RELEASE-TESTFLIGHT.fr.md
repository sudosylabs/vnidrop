# Publier VniDrop sur TestFlight interne — guide complet

Ce guide décrit **toutes les étapes** pour compiler l'application et l'envoyer sur
**TestFlight interne** (aucune revue Apple n'est nécessaire pour les testeurs internes).

- **Bundle ID** : `com.vnidrop.app`
- **Team ID Apple** : `A8A4JSMV5D`
- **Branche à utiliser** : `feat/release-test-flight`

> ⚠️ **Point crucial** : il faut compiler avec un **Xcode de version finale (release)**,
> par exemple **Xcode 26** — **pas** une version bêta. Un envoi construit avec un Xcode
> bêta est **refusé** par App Store Connect (« Unsupported SDK or Xcode version »).

---

## 1. Prérequis

- Un Mac sous **macOS stable** (pas une bêta) avec **Xcode 26** installé.
- Un **identifiant Apple** (gratuit) — **aucun abonnement développeur payant n'est
  nécessaire de votre côté**. Le propriétaire du compte vous invitera sur le sien.
- Une connexion internet.

---

## 2. Obtenir l'accès au compte développeur

Le propriétaire du compte doit vous inviter (une seule fois) :

1. Sur **App Store Connect** → **Utilisateurs et accès** → **Ajouter un utilisateur**.
2. Il saisit **votre identifiant Apple** et vous attribue le rôle **Admin**
   (nécessaire pour gérer la signature) ou au minimum **App Manager**.
3. Vous recevez un e-mail d'invitation — **acceptez-le**.

Ensuite, dans **Xcode** → menu **Xcode → Settings → Accounts** → **+** →
connectez-vous avec **votre** identifiant Apple. L'équipe **VniDrop (A8A4JSMV5D)**
doit apparaître.

---

## 3. Installer les outils

Dans le Terminal :

```bash
# Homebrew (si absent) : voir https://brew.sh
# Outils de génération de projet et de qualité de code
brew install xcodegen swiftlint

# Rust (pour compiler le cœur natif)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Installez aussi les **Command Line Tools** de Xcode si demandé :
```bash
xcode-select --install
```

---

## 4. Récupérer le projet

```bash
git clone <URL_DU_DEPOT> vnidrop
cd vnidrop
git checkout feat/release-test-flight
```

> Le fichier `.xcodeproj`, le `Local.xcconfig` et le framework compilé ne sont **pas**
> versionnés : ils seront (re)générés localement aux étapes suivantes.

---

## 5. Configurer la signature

Créez le fichier **`apple/Local.xcconfig`** (ignoré par git) avec ce contenu :

```
DEVELOPMENT_TEAM = A8A4JSMV5D
CODE_SIGN_STYLE = Automatic
CODE_SIGNING_ALLOWED = YES
```

Cela active la signature sur cette machine sans modifier la configuration partagée
(qui reste non signée pour l'intégration continue).

---

## 6. Compiler le cœur Rust

Depuis la racine du dépôt :

```bash
apple/scripts/build-core.sh
```

Cela produit `apple/VnidropCore/vnidrop.xcframework` (avec la tranche **arm64 device**
requise pour TestFlight) et les liaisons Swift.

- La compilation **debug** (par défaut) convient parfaitement pour TestFlight.
- Si vous voulez une compilation **release** : `apple/scripts/build-core.sh release`.
  Sur macOS stable, cela devrait fonctionner. En cas d'erreur `can't find crate`
  (dylibs de macros corrompus), nettoyez et repassez en debug :
  ```bash
  cargo clean
  apple/scripts/build-core.sh
  ```

---

## 7. Incrémenter le numéro de build

Chaque envoi doit avoir un **numéro de build unique et supérieur** au précédent.
Dans **`apple/project.yml`**, cherchez `CURRENT_PROJECT_VERSION` et mettez **`4`**
(les numéros 1 à 3 ont déjà été utilisés) :

```yaml
CURRENT_PROJECT_VERSION: "4"
```

> Pour tout envoi ultérieur, augmentez encore ce nombre (5, 6, …).

---

## 8. Générer le projet Xcode

```bash
cd apple
xcodegen generate
```

Cela crée `apple/VniDrop.xcodeproj` à partir de `project.yml` et du `Local.xcconfig`.

---

## 9. Archiver dans Xcode

1. Ouvrez **`apple/VniDrop.xcodeproj`** dans **Xcode 26**.
2. En haut, sélectionnez le schéma **VniDrop** et la destination
   **Any iOS Device (arm64)** (surtout **pas** un simulateur).
3. Menu **Product → Archive**.
4. À la fin, la fenêtre **Organizer** s'ouvre avec votre archive.

---

## 10. Envoyer sur App Store Connect

1. Dans l'**Organizer**, sélectionnez l'archive → **Distribute App**.
2. Choisissez **App Store Connect** → **Upload**.
3. Laissez les options par défaut (**signature automatique**) → **Upload**.
4. La question sur le chiffrement **ne sera pas posée** (déjà réglée dans l'Info.plist).

Patientez quelques minutes : le build apparaît ensuite dans App Store Connect avec le
statut **« En cours de traitement »**, puis devient disponible.

---

## 11. Publier sur TestFlight interne (sans revue)

1. Sur **App Store Connect** → l'app **VniDrop** → onglet **TestFlight**.
2. Attendez que le build passe de **« En cours de traitement »** à disponible.
3. Section **Tests internes** → créez un groupe (ou utilisez celui par défaut) →
   ajoutez les **testeurs internes** (ce sont des utilisateurs de l'équipe App Store
   Connect ; le propriétaire les ajoute via **Utilisateurs et accès** si besoin).
4. Activez le build pour le groupe.
5. Les testeurs reçoivent un e-mail, installent l'app **TestFlight**, acceptent, puis
   installent VniDrop. **Aucune revue Apple** n'est requise pour les tests internes.

---

## 12. Solution de repli pour la signature

Si, à l'étape 9/10, Xcode **refuse de créer un certificat de distribution**
automatiquement (limitation possible des comptes individuels), le **propriétaire du
compte** doit fournir les éléments de signature :

1. Portail développeur → **Certificates** → créer un certificat **Apple Distribution**,
   puis l'**exporter en `.p12`** (avec la clé privée) depuis le Trousseau (Keychain).
2. **Profiles** → créer un profil de provisioning **App Store** pour `com.vnidrop.app`.
3. Vous transmet le `.p12` (+ son mot de passe) et le profil.

De votre côté :
- Importez le `.p12` dans le **Trousseau** (double-clic).
- Dans Xcode, désactivez la signature automatique et sélectionnez la signature
  **manuelle** avec ce profil, puis reprenez l'archivage (étape 9).

> 🔒 Un `.p12` de distribution permet de signer des apps au nom du propriétaire :
> à n'utiliser qu'entre personnes de confiance. Le certificat peut être révoqué ensuite.

---

## 13. Dépannage

- **« Unsupported SDK or Xcode version »** → vous compilez avec un Xcode **bêta**.
  Utilisez **Xcode 26 (release)**.
- **Échec de la phase SwiftLint** → `brew install swiftlint` (obligatoire, la build
  échoue sinon).
- **Numéro de build déjà utilisé** → augmentez `CURRENT_PROJECT_VERSION` puis
  `xcodegen generate` à nouveau.
- **L'app n'apparaît pas dans TestFlight** → attendez la fin du « traitement » ; la
  conformité export est déjà déclarée, aucune action supplémentaire.
- **Le même Xcode pour le cœur Rust et l'archivage n'est pas obligatoire** (le cœur est
  du Rust), mais l'**archivage** doit impérativement se faire avec **Xcode 26 release**.

---

## Récapitulatif express

```bash
# 1. Outils
brew install xcodegen swiftlint

# 2. Projet
git checkout feat/release-test-flight

# 3. Signature : créer apple/Local.xcconfig (voir §5)

# 4. Cœur natif
apple/scripts/build-core.sh

# 5. Numéro de build : CURRENT_PROJECT_VERSION -> 4 dans apple/project.yml

# 6. Projet Xcode
cd apple && xcodegen generate

# 7. Xcode 26 : schéma VniDrop, destination « Any iOS Device (arm64) »,
#    Product → Archive → Distribute App → App Store Connect → Upload

# 8. App Store Connect → TestFlight → Tests internes → ajouter les testeurs
```
