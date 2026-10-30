# Architecture

## Les choix

| | **Strata** |
| --- | --- |
| Création | Timeline et keyframes dans le navigateur, marqueurs pause et out, canvas de données et de logique |
| Rendu | **Un seul modèle de scène, deux moteurs** : Rust (baseband) et DOM (HTML5) |
| Sorties | HTML5, NDI, Stream (SRT, RTMP, HLS), SDI (DeckLink), **ST 2110-20 fill + key** |
| Exploitation | Conducteur, zones et couches de playout, API REST et WebSocket, MOS, surfaces de contrôle |

## Vue d'ensemble

```
             ┌──────────── Web app (React/TS) ─────────────┐
             │ Projets · Designer · Opérateur · /output/<ch> │
             └──────┬──────────────┬───────────────▲───────┘
                REST│ (templates)  │WS (commandes) │WS (état 50 Hz)
             ┌──────▼──────────────▼───────────────┴───────┐
             │                 cg-server (axum)            │
             │ projets · utilisateurs · N chaînes (horloge)│
             └───────────────────────┬─────────────────────┘
                                     │ (Project, ChannelState) par frame et par chaîne
                             ┌───────▼────────┐
                             │   cg-render    │  tiny-skia, texte vectoriel
                             └───────┬────────┘
                                     │ Frame RGBA prémultipliée
                   ┌─────────────────┼──────────────────┐
             ┌─────▼─────┐   ┌───────▼───────┐   ┌──────▼──────┐
             │ ST 2110-20 │   │ ST 2110-20    │   │ DeckLink SDI│
             │   fill     │   │   key         │   │ fill + key  │
             └────────────┘   └───────────────┘   └─────────────┘
```

### `cg-core` : le contrat
- **Modèle** (`model.rs`) : `Project`, puis `Template`, puis `Layer` (group, rect, ellipse, text, image et les widgets), avec ses `props` statiques, ses `tracks` de keyframes et ses `bindings` vers des champs de données. Le modèle est sérialisé en JSON camelCase ; il a un miroir TypeScript dans `web/src/engine/types.ts`.
- **Animation** (`anim.rs`, `easing.rs`) : chaque keyframe porte l'easing du segment qui en part (sémantique After Effects). Les fonctions d'easing sont implémentées à l'identique en Rust et en TypeScript (`web/src/engine/evaluate.ts`), y compris les bezier façon CSS.
- **Playout** (`playout.rs`) : *Take* joue jusqu'à la première pause, *Continue* jusqu'à la suivante (ou jusqu'au marqueur out), *Take out* saute au marqueur out puis joue jusqu'à la fin, après quoi le graphique est retiré. Les commandes s'appliquent par **couche de playout**, ce qui permet d'avoir plusieurs graphiques simultanément.

- **Logic** (`logic.rs`) : graphe de relations par template (sources → opérations → cibles), évalué à chaque image après l'animation et les bindings, dans l'ordre topologique (les cycles sont ignorés). La mesure des textes est fournie par le moteur de rendu (police réelle). Miroir TypeScript : `web/src/engine/logic.ts`.
- **Import After Effects** (`lottie.rs`) : conversion d'un export Bodymovin / Lottie en `Template` (calques, parentage et précomps en groupes, keyframes et easing, textes en champs de données, marqueurs). `cg-server/src/ae_import.rs` lit les archives (`.zip`, `.lottie`), décode les images et refuse les `.mogrt` avec une explication.
- **Données** (`datapath.rs`) : chemins de liaison (`rows[{row}].name`, `items[*].title`…) et remplissage des champs liés à partir des valeurs des sources. Le serveur (`cg-server/src/datasources.rs`) relit les sources (JSON, CSV / Google Sheets, RSS, XML) ou reçoit leurs valeurs (push), les convertit en JSON, les garde sur disque, et réapplique les liaisons aux graphiques à l'antenne quand une valeur change ; les clients HTML5 reçoivent donc des données déjà résolues.
- **Widgets** (`widgets.rs`) : horloge, compte à rebours, minuteur, chronomètre, horloge analogique, ticker, générique, rotator, machine à écrire, compteur, barre de progression, graphique en barres, diaporama et QR code. Chaque widget est une **fonction pure** de l'heure du serveur et du temps passé à l'antenne (`PlayoutItem.age`, qui continue d'avancer pendant les pauses). Le rendu Rust et le rendu HTML5 (`web/src/engine/widgets.ts`) sont donc identiques. Les clients HTML5 reçoivent l'heure du serveur et extrapolent le temps à l'antenne entre deux évènements. Le QR code est calculé par le serveur (`/api/qr`) pour que les deux moteurs dessinent les mêmes modules.

### Conventions de rendu (communes aux deux moteurs)
- `x` et `y` désignent le coin haut-gauche de la boîte dans le repère du parent ; les enfants d'un groupe sont relatifs au groupe.
- La rotation et l'échelle se font autour de l'ancre (`anchorX` et `anchorY` en fraction de la boîte, au centre par défaut).
- `crop*` est un masque de révélation (wipes) ; un groupe en `clip` découpe ses enfants.
- Texte : `font-size` en em, `line-height` à 1,2, `white-space: pre`.

Le test `cg-render::tests::renders_demo_lower_third` et les captures comparatives garantissent que le rendu Rust et le rendu HTML5 restent alignés.

### `cg-render`
Rasteriseur CPU (tiny-skia) : headless, déterministe, environ 1,5 ms par frame 1080p pour la démo. Le texte est converti en chemins vectoriels (ab_glyph vers tiny-skia) : il reste net sous n'importe quelle transformation. Les polices sont cherchées dans le système et dans `data/assets/fonts`.

### `cg-output`
- **ST 2110-20** : YCbCr 4:2:2 10 bits BT.709 (narrow range), RFC 4175 avec un segment de ligne par paquet de 1 200 octets, marker bit en fin de frame, horloge RTP à 90 kHz dérivée de l'heure système, SDP générés. Le fill et la key sont deux flux, chacun avec son thread et sa socket ; les paquets sont étalés sur la période trame (modèle d'émetteur proche de 2110-21 « wide »). Débit mesuré : 50 i/s en 1080p fill + key sur 4 vCPU.
- **SDI** : interface `DeckLinkOutput` (BGRA prémultiplié vers le keyer externe de la carte). Le pont FFI vers le SDK Blackmagic est à brancher derrière la feature `decklink`, car le SDK n'est pas redistribuable.
- **HTML5** : la page `/output/<chaîne>` reçoit le projet et l'état de sa chaîne par WebSocket et rend avec le moteur DOM. Elle est transparente, s'adapte à la taille de la fenêtre (ou reste en 1:1 avec `?scale=1`) et convient aux browser sources.

### `cg-server`
Le serveur repose sur axum et tokio.

- **Chaînes** : chaque chaîne a sa tâche d'horloge, cadencée au format de la chaîne, son `ChannelState` et son thread de rendu. Ses sorties ne reçoivent que ses frames. Quand l'image n'est pas prête à temps, la frame est abandonnée plutôt que d'accumuler de la latence. Les timelines sont avancées de `fps projet / fps chaîne` par tick, et le projet est mis à l'échelle au format de la chaîne.
- **Projets** : la bibliothèque `data/projects/<id>.json` est écrite de façon atomique. Le projet chargé sur chaque chaîne est mémorisé dans `config.json`.
- **Utilisateurs** : `users.json` stocke des hachages Argon2 et est relu s'il change sur disque (CLI `strata-server user`). Les sessions sont gardées en mémoire et survivent aux redémarrages du moteur. Chaque route vérifie le rôle de l'appelant et son périmètre de projets et de chaînes. Le WebSocket filtre les évènements selon ce que l'utilisateur peut voir.
- **Redémarrage à chaud** : une modification des réglages reconstruit les chaînes et le listener sans arrêter le processus.

## Feuille de route

**Moteur et sorties**
- [ ] Verrouillage PTP natif (lecture de la PHC de la carte réseau) et alignement des timestamps RTP sur l'epoch SMPTE 2059.
- [ ] Émission haute performance : `sendmmsg` / UDP GSO, puis DPDK ou NVIDIA Rivermax pour l'UHD et le 2110-21 « narrow ».
- [ ] ST 2110-40 (ANC), ST 2022-7 (redondance), enregistrement NMOS IS-04 / IS-05.
- [ ] Pont FFI DeckLink (C++), genlock sur la référence de la carte, NDI.
- [ ] Backend GPU (wgpu) derrière la même API `Renderer` : flous, ombres, dégradés, effets.
- [ ] Interlacé (1080i50/59.94) : rendu par trame.

**Designer**
- [ ] Éditeur de courbes de valeur et de vitesse (graph editor), poignées bezier dans la timeline.
- [ ] Ombres, dégradés, flou, modes de fusion, masques par calque (track matte).
- [x] Widgets préfabriqués : horloges, compte à rebours, ticker, générique, compteur, sondage, QR code…
- [ ] Animations de texte par caractère, texte auto-fit.
- [ ] Séquences d'images et vidéo (alpha), compositions imbriquées (pré-compositions).
- [ ] Import de fichiers Lottie / After Effects (bodymovin).

**Exploitation et données**
- [x] Sources de données (Google Sheets, JSON/HTTP, CSV, XML, RSS, push) avec liaison vers les champs.
- [ ] MOS (iNews, ENPS, Octopus), Companion / Stream Deck, raccourcis configurables.
- [x] Multi-chaînes, multi-utilisateurs, droits (admin / designer / opérateur / lecture seule).
- [x] Bibliothèque de projets, import et export.
- [ ] Journal de diffusion, versionnage des templates, SSO (OIDC / LDAP).
