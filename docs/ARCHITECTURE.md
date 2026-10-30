# Architecture

## Les choix

| | **Strata Multiview** |
| --- | --- |
| Création | Designer dans le navigateur : un multiview s'édite directement (dispositions, tuiles Source, horloges, logos, textes) et se publie ; ni modèles, ni calques, ni timeline, ni animation |
| Rendu | **Un seul modèle de scène, deux moteurs** : Rust (baseband) et DOM (HTML5) |
| Entrées | SRT, UDP, RTMP, HLS, RTSP, URL (ffmpeg), NDI, SDI (DeckLink) ; image et audio (niveaux par canal) |
| Signaux | Tally et UMD : TSL UMD 3.1 et 5.0, Blackmagic ATEM, GPI, API |
| Sorties | HTML5, NDI, Stream (SRT, RTMP, HLS), SDI (DeckLink), ST 2110-20 |

## Vue d'ensemble

```
             ┌──────────────── Web app (React/TS) ────────────────┐
             │ Multiviews · Designer · Diffusion · Tally · /output/<ch>│
             └──────┬───────────────────────────────▲─────────────┘
                REST│ (multiviews, réglages)        │WS (état, signaux)
             ┌──────▼───────────────────────────────┴─────────────┐
             │                strata-mv-server (axum)               │
             │ multiviews · N chaînes (horloge) · entrées · tally   │
             └──────┬───────────────┬──────────────────┬──────────┘
        images, niveaux│      tally, UMD│                 │ (Project, ChannelState) par image
             ┌─────────▼──────┐ ┌──────▼──────┐   ┌───────▼────────┐
             │ inputs.rs      │ │ tally/      │──►│ strata-mv-render│ tiny-skia, texte vectoriel
             │ ffmpeg·NDI·SDI │ │ TSL·ATEM·GPI│   └───────┬────────┘
             └─────────┬──────┘ └─────────────┘           │ Frame RGBA
                       └──► signals.rs ──────────────────►│
                   ┌──────────────────┬──────────────────┼──────────────┐
             ┌─────▼─────┐   ┌────────▼──────┐   ┌───────▼─────┐  ┌─────▼─────┐
             │ ST 2110-20 │   │ DeckLink SDI  │   │    NDI      │  │  Stream   │
             └────────────┘   └───────────────┘   └─────────────┘  └───────────┘
```

### `strata-mv-core` : le contrat
- **Modèle** (`model.rs`) : un multiview est un `Project` dont l'unique `Template` est la page, faite de `Layer` (group, rect, ellipse, text, image et les widgets), avec ses `props` statiques et ses `tracks` de keyframes. Le modèle est sérialisé en JSON camelCase ; il a un miroir TypeScript dans `web/src/engine/types.ts`.
- **Tuile Source** (`wid/source.rs`) : réglages (entrée, ajustement, cadre de tally, UMD, vu-mètres, format) et **géométrie** (`layout`) : où vont l'image, la barre d'UMD, ses lampes, son texte et chaque barre de vu-mètre. Les deux moteurs la suivent ; miroir TypeScript : `web/src/widgets/source.tsx`.
- **Signaux** (`signal.rs`) : ce qu'une entrée porte en plus de son image, tel que la tuile le dessine : `Tally` (antenne, préparation, aux, texte d'UMD), `Level` (crête, puissance, maintien en dBFS par canal), `SourceView`.
- **Playout** (`playout.rs`) : un multiview est un modèle à l'antenne sur la couche de sa zone. Le Designer ne crée ni images clés ni animation ; le moteur (`anim.rs`) résout simplement les propriétés de chaque calque.
- **Widgets** (`wid/`) : chaque widget est une fonction pure de ses réglages, de l'heure du serveur et du temps à l'antenne : les rendus Rust et HTML5 sont identiques.

### Conventions de rendu (communes aux deux moteurs)
- `x` et `y` désignent le coin haut-gauche de la boîte dans le repère du parent ; les enfants d'un groupe sont relatifs au groupe.
- La rotation et l'échelle se font autour de l'ancre (`anchorX` et `anchorY` en fraction de la boîte, au centre par défaut).
- Texte : `font-size` en em, `line-height` à 1,2. Un texte trop long se réduit, se resserre puis se coupe (UMD, ticker : `strata-mv-render/src/wid/fit.rs`, `web/src/widgets/text-fit.tsx`).

### `strata-mv-render`
Rasteriseur CPU (tiny-skia) : headless, déterministe. Le texte est converti en chemins vectoriels. Les images des entrées et leurs signaux viennent du serveur par le trait `LiveInputs` (`picture`, `source`).

### `strata-mv-output`
- **ST 2110-20** : YCbCr 4:2:2 10 bits BT.709, RFC 4175, SDP générés ; SMPTE 2022-7.
- **SDI** : DeckLink en sortie et en entrée, par un pont C++ (`decklink/shim.cpp`) vers le SDK Blackmagic, chargé à l'exécution. En entrée : l'image (UYVY convertie en RGBA) et l'**audio embarqué** (48 kHz, 32 bits, 16, 8 ou 2 canaux selon la carte), dont le pont cumule crête et puissance par canal entre deux lectures (`dl_input_audio`).
- **NDI** : runtime chargé à l'exécution ; en réception, l'image et l'**audio** (trames float par canal).
- **Stream** : ffmpeg (SRT, UDP, RTMP, HLS).
- **HTML5** : la page `/output/<chaîne>` reçoit le multiview, l'état de sa chaîne et les signaux des entrées par WebSocket et rend avec le moteur DOM.

### `strata-mv-server`
Le serveur repose sur axum et tokio.

- **Chaînes** : chaque chaîne a sa tâche d'horloge, son `ChannelState` et son thread de rendu. Charger un multiview (`PUT /api/channels/{id}/project`) met sa page publiée à l'antenne (`show_multiview`) ; une publication met à l'antenne des chaînes qui attendaient.
- **Entrées** (`inputs.rs`) : un fil par entrée. Les flux passent par ffmpeg : l'image en RGBA sur sa sortie standard, l'audio de la première piste mesuré par `astats` dont les valeurs sont lues dans son journal (`MeterLog`) ; une source sans audio est relancée sans mesure. NDI et SDI mesurent leur audio eux-mêmes.
- **Signaux** (`signals.rs`) : tally et niveaux par entrée ; les vu-mètres montent aussitôt, descendent de 24 dB/s, maintiennent la crête 2 s. Le moteur les lit par `Inputs::source` ; les navigateurs les reçoivent en évènements `signals` (toutes les 80 ms au plus, quand ils changent).
- **Tally** (`tally/`) : une tâche par source (TSL en UDP, TCP en écoute ou en client ; ATEM en UDP 9910 ; GPI par l'API), l'état de leurs afficheurs, l'aiguillage vers les entrées et les consignes de l'API. Réglages dans `tally.json`, appliqués à chaud ; le tally vit en dehors du cycle du moteur et survit à ses redémarrages.
- **Multiviews** : la bibliothèque `data/projects/<id>.json` est écrite de façon atomique ; un nouveau multiview naît avec sa page, déjà publiée. Le multiview chargé sur chaque chaîne est mémorisé dans `config.json`.
- **Utilisateurs** : `users.json` stocke des hachages Argon2. Chaque route vérifie le rôle de l'appelant et son périmètre de multiviews et de chaînes.
- **Redémarrage à chaud** : une modification des réglages reconstruit les chaînes et le listener sans arrêter le processus ; les entrées et le tally continuent.

## Feuille de route

- [ ] Plusieurs pistes audio par flux (aujourd'hui la première, tous ses canaux) ; mesure LUFS.
- [ ] Entrées ST 2110 (image et audio 2110-30).
- [ ] Sources de tally supplémentaires : Ross RossTalk, protocoles propriétaires de routeurs (Probel, Quartz) pour les UMD dynamiques.
- [ ] Tuile pilotée par un routeur (l'entrée d'une tuile suit une destination de grille).
- [ ] Alarmes : silence, image figée, noir, perte de signal, sur la tuile et dans Performances.
