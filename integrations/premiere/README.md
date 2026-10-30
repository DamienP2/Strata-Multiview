# Panneau Strata pour Adobe Premiere Pro

Ce panneau permet d'utiliser les habillages Strata en post-production.

Un écran = un geste. Le panneau a le même design que l'application.

## Projets

L'écran d'accueil liste les projets du serveur, synchronisés en continu (badge ✓ up to date / ⟳ syncing…), avec une recherche et l'état de la connexion. En bas : le serveur et l'interrupteur **Auto-sync**. L'engrenage ouvre les réglages.

## Templates

Les templates du projet : image fixe rendue par le serveur (`/api/projects/{id}/templates/{tid}/thumbnail.png`, au point d'antenne), nom, zone, durée du clip. Recherche et filtres par zone. **+** (ou un glisser vers la séquence) pose le template tel quel à la tête de lecture. Un clic ouvre le clip.

## Le clip

- Un **aperçu animé** (le moteur HTML5 du serveur, `/embed/preview`) avec lecture, boucle et une tête de lecture : un clic ou un glisser sur la règle ou la barre s'y place. Il montre le graphique avec les valeurs saisies et la timeline du clip, comme il sera rendu.
- **Total** (durée totale) en tête du bloc Durations. La changer déplace tous les marqueurs proportionnellement (60 → 30 s : pause 20 → 10, in/out 40 → 20) ; chaque poignée reste ensuite libre.
- **↺ Origin** remet la durée et tous les marqueurs aux valeurs du template.
- La timeline du clip, comme dans le Designer : un seul marqueur **IN / OUT** (le point d'antenne : vert avant, rouge après), les **pauses** (losanges orange), la **fin** (trait violet), la règle. Chaque marqueur se glisse n'importe où : il pousse ceux qu'il dépasse (ils reviennent s'il repart en arrière), la fin suit ; contre le bord droit, la timeline s'élargit pour allonger le clip.
- Chaque marqueur est l'instant où le graphique **repart** de cet arrêt. Une animation qui a plus de temps qu'il ne lui en faut joue à sa vitesse puis tient sur l'arrêt ; une animation qui en a moins est accélérée (`timing` de `POST /api/render`).
- Les **paramètres exposés** du template, puis **Insert** (à la tête de lecture, sur la première piste vidéo libre au-dessus de V1). Quand un clip Strata est sélectionné dans la séquence, le panneau l'ouvre et le bouton principal devient **Update clip** (le clip est rendu à nouveau et remplacé, à la même place et sur la même piste).

Le serveur rend le clip au format et à la cadence de la séquence (QuickTime, codec PNG avec couche alpha). Un graphique identique déjà rendu est réutilisé. Si la séquence est verticale (9:16), c'est la mise en page 9:16 du projet qui est utilisée, à condition qu'elle existe.

Chaque clip rendu garde ses valeurs et sa timeline dans un fichier `<clip>.mov.pcg.json` à côté de lui : sélectionner le clip dans la timeline de Premiere le rouvre dans le panneau.

## Réglages

Depuis l'engrenage de l'écran Projets seulement : adresse du serveur, clé d'API (**Paste**), **Test connection** (état et latence), Auto-sync (intervalle) et **⟳ Now**, version du panneau et **Check for updates**.

## Installation

Le panneau fonctionne avec Premiere Pro 2020 (14.0) et les versions suivantes, sur macOS et Windows. Il s'installe pour l'utilisateur courant, sans droits d'administrateur.

**Installateurs** (dans la release) :

- **macOS** : `Strata-Premiere-<version>-macos.pkg` ;
- **Windows** : `Strata-Premiere-<version>-windows.msi`. En déploiement silencieux, le serveur peut être fixé : `msiexec /i Strata-Premiere.msi /qn SERVER=http://strata-server:8080`.

**Depuis le serveur** : Strata → Utilisateurs → API keys → **Premiere panel** télécharge un zip déjà réglé sur ce serveur. Le décompresser, puis lancer `./install-mac.sh` dans le Terminal (macOS) ou clic droit sur `install-windows.ps1` → *Exécuter avec PowerShell* (Windows). Les deux scripts acceptent aussi l'adresse du serveur en argument.

Ensuite :

1. Redémarrer Premiere Pro, puis ouvrir **Fenêtre → Extensions → Strata**.
2. Dans le panneau, ouvrir les réglages (engrenage) et renseigner :
   - l'adresse du serveur, si l'installateur ne l'a pas déjà mise, par exemple `http://strata-server:8080` ;
   - une **clé d'API** (Strata → Utilisateurs → API keys), d'un compte designer ou opérateur.

Les rendus vont dans `Documents/Strata Renders/<projet>/`.

L'installation active `PlayerDebugMode`, qui autorise les extensions non signées par Adobe Exchange. Pour une diffusion à grande échelle, le panneau peut être signé en `.zxp` avec `ZXPSignCmd`.

## Mises à jour

Le panneau suit le serveur, comme l'application : seul un petit lanceur (`loader.js`) est installé, et l'interface du panneau (`ui/`, `host/premiere.jsx`) est chargée depuis le serveur Strata à chaque ouverture. Mettre le serveur à jour met donc à jour tous les panneaux.

- **Hors ligne** : la dernière version chargée est gardée, puis, à défaut, celle installée.
- **Lanceur** : quand le serveur a un lanceur plus récent, un point vert apparaît sur l'engrenage. Dans les réglages, **Update** réécrit l'extension dans le dossier de l'utilisateur et recharge le panneau ; si le manifeste a changé, il faut redémarrer Premiere.
- La version chargée est indiquée dans les réglages ; **Check for updates** la compare à celle du serveur.

Pour les développeurs : toute modification de `loader.js`, `index.html` ou `CSXS/manifest.xml` doit incrémenter `LOADER_VERSION` dans `loader.js`, sinon les panneaux déjà installés ne la recevront pas.

## Remarques

- **Durée** : le rendu est fait par le serveur, avec le moteur Rust (le même que pour le SDI et le ST 2110). Il tient environ ×3 le temps réel en 1080p sur un serveur courant.
- **Fichiers** : un clip PNG/alpha de 10 secondes en 1080p fait environ 15 à 30 Mo. Premiere, After Effects, Resolve et Avid lisent ce format avec la transparence.
- **Débogage** : avec le panneau ouvert, ouvrir `http://localhost:8095` dans Chrome.
