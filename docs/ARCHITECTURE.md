# Architecture

## Les choix

| | **Strata Multiview** |
| --- | --- |
| Création | Designer dans le navigateur : un multiview s'édite directement (dispositions, tuiles Source, horloges, logos, textes) et se publie ; ni modèles, ni calques, ni timeline, ni animation |
| Rendu | **Un seul modèle de scène, deux moteurs** : Rust (baseband) et DOM (HTML5) |
| Entrées | SRT, UDP, RTMP, HLS, RTSP, URL (ffmpeg), direct web (yt-dlp : YouTube, TikTok, Twitch…), NDI, SDI (DeckLink) ; image et audio (niveaux par canal) |
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
- **Tuile Source** (`wid/source.rs`) : réglages (entrée, ajustement, cadre de tally, UMD, vu-mètres, format, timecode, loudness, alarmes) et **géométrie** (`layout`, `overlay`) : où vont l'image, la barre d'UMD, ses lampes, son texte, chaque barre de vu-mètre et les incrustations. La perte de signal : les barres SMPTE (`BARS`, EG 1-1990) et la carte F AudioVisuel (`no_signal_card`), dont la position est une fonction de l'heure du serveur et de la place de la tuile. Miroir TypeScript : `web/src/widgets/source.tsx`.
- **Widgets broadcast** (`wid/broadcast.rs`) : vu-mètres, loudness, timecode, état, journal des alarmes. Chacun produit une liste d'éléments (`Item::Rect`, `Item::Text`) que les deux moteurs dessinent tels quels ; miroir TypeScript : `web/src/widgets/broadcast.tsx`.
- **Signaux** (`signal.rs`) : ce qu'une entrée porte en plus de son image : `Tally` (antenne, préparation, aux, texte d'UMD), `Level` (crête, puissance, maintien en dBFS par canal), `Loudness` (EBU R128 : momentary, short-term, intégré, LRA, true peak) et son historique, le timecode, les `Alarm` en cours (`AlarmKind`, `Severity`), réunis dans `SourceView` ; pour la machine, `SystemView` (PTP, NTP, journal des alarmes).
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
- **Entrées** (`inputs.rs`) : un fil par entrée. Les flux passent par ffmpeg : l'image en RGBA sur sa sortie standard, l'audio de la première piste mesuré par `ebur128` puis `astats`, dont les valeurs sont lues dans son journal (`MeterLog`) ; une source sans audio est relancée sans mesure. NDI et SDI mesurent leur audio eux-mêmes (le loudness par la crate `ebur128`) ; NDI donne aussi son timecode.
- **Contenu des entrées** (`probe/`) : ce que chaque entrée transporte, tenu à jour pendant qu'elle tourne (`Facts`) et lu par `GET /api/inputs/{id}/analysis`. Les flux SRT, UDP et TCP sont **relayés** : un premier ffmpeg (`-f data`) reçoit les octets, Strata les lit au passage (`probe/ts.rs` : PAT, PMT et descripteurs, SDT, NIT, EIT en cours / suivant, TDT/TOT, SCTE-35, SPS H.264/HEVC, débit, continuité, PCR et embrouillage de chaque PID, contrôles TR 101 290) puis les passe au ffmpeg qui décode. Ce dernier décrit l'entrée à son ouverture (`probe/banner.rs`) et chaque image par le filtre `showinfo` (`probe/frames.rs` : I/P/B, GOP, balayage, couleurs, données annexes, timecode SEI ou de GOP, qui alimente aussi les tuiles). HLS : listes de lecture et dernier segment (`probe/hls.rs`). NDI : format et métadonnées. SDI : `dl_input_info` du pont DeckLink (format détecté, timecode RP 188/VITC, HDR, paquets ANC).
- **Signaux** (`signals.rs`) : tally, niveaux, loudness et timecode par entrée ; les vu-mètres montent aussitôt, descendent de 24 dB/s, maintiennent la crête 2 s. Chaque image reçue laisse une empreinte de luminance 16 × 9 qui détecte le noir, l'image unie et, avec `is_bars`, les mires. L'image figée réunit plusieurs critères (`Signals::frozen`) : ni mire, ni image unie, ni noir ; et soit une image qui avait le bruit d'une image en direct se répète au pixel près depuis 4 s (un décodeur ou un encodeur qui répète sa dernière image), soit une image qui bougeait dans les 30 s d'avant ne bouge plus depuis 10 s en se répétant au pixel près 4 fois sur 5 (une image figée recodée ne change qu'aux images clés ; un plan calme en direct garde son bruit). Une image fixe dès le départ n'est jamais figée. `evaluate` lève et clôt les alarmes (perte, noir, figée, silence, trop fort, crête) et tient le journal (200 lignes). PTP et NTP viennent de la supervision de la synchro. Le moteur les lit par `Inputs::source` et `Inputs::system` ; les navigateurs les reçoivent en évènements `signals` (`{sources, system}`, toutes les 80 ms au plus, quand ils changent).
- **Tally** (`tally/`) : une tâche par source (TSL en UDP, TCP en écoute ou en client ; ATEM en UDP 9910 ; GPI par l'API), l'état de leurs afficheurs, l'aiguillage vers les entrées et les consignes de l'API. Réglages dans `tally.json`, appliqués à chaud ; le tally vit en dehors du cycle du moteur et survit à ses redémarrages.
- **Exemples** (`examples.rs`) : les dix multiviews de régie de diffusion, fichiers `examples/ex-NN-*.json` embarqués (`build.rs`), copiés en multiviews de l'utilisateur, leurs tuiles sur les entrées du serveur (`{{input:N}}`).
- **Multiviews** : la bibliothèque `data/projects/<id>.json` est écrite de façon atomique ; un nouveau multiview naît avec sa page, déjà publiée. Le multiview chargé sur chaque chaîne est mémorisé dans `config.json`.
- **Utilisateurs** : `users.json` stocke des hachages Argon2. Chaque route vérifie le rôle de l'appelant et son périmètre de multiviews et de chaînes.
- **Redémarrage à chaud** : une modification des réglages reconstruit les chaînes et le listener sans arrêter le processus ; les entrées et le tally continuent.

### Synchronisation image, son et vu-mètres

Une entrée arrive souvent par à-coups : les images sortent du décodeur par rafales, et le flux de transport livre l'audio par paquets de 100 à 400 ms. Dater ce qui arrive à son heure d'arrivée ferait saccader l'image et décalerait le son. Chaque entrée a donc **une horloge** (`inputs.rs`, `MediaClock`) qui place ses propres temps (les **PTS**) sur l'horloge murale, de la même façon pour l'image et le son :

- le PTS de chaque image vient du filtre `showinfo` (associé à l'image par son numéro, ffmpeg en `-fps_mode passthrough` : ni ajout ni saut) ; celui du son, du début de l'audio (premier bloc de mesure) plus le nombre d'échantillons (`aresample=async=1` comble les trames perdues) ; celui de chaque mesure des vu-mètres, de son bloc de 40 ms ;
- l'écart (horloge − PTS) de chaque flux est le plus petit vu (une rafale arrive en retard, jamais en avance), et les deux flux prennent le plus grand des deux : chacun est là à temps ;
- sans PTS (NDI, SDI, ffmpeg sans `showinfo`), les éléments sont datés par leur cadence de la même façon (`RateClock`) ; deux images restent au moins à 0,9 période l'une de l'autre.

**Sur le serveur** (rendu, NDI, SDI, flux) : une image s'affiche un tampon de lissage après son heure, allongé tant que le son de l'entrée arrive en retard (+20 ms, décroissant de 10 ms/s). Les vu-mètres dessinés sont ceux du moment de l'image affichée (`Signals::levels_at`, avec la chute des crêtes entre deux mesures), et le son envoyé avec une sortie NDI est celui qui accompagne l'image qui part (`Inputs::sound_of` : le délai de l'image affichée, l'avance de rendu et une demi-période).

**Dans le navigateur** (`engine/liveInputs.ts`) : images (`X-Time`), son (`GET /api/inputs/{id}/audio`, chaque paquet daté) et mesures des vu-mètres (`levelsSeq`, toutes, chacune datée ; envoyées 25 fois par seconde) sont rejoués sur **une ligne de temps commune** : heure serveur + plus petit délai vu + délai de lecture (80 ms au moins, autant que les retards récents). Les vu-mètres sont dessinés à la cadence de l'écran, la crête tombant de 24 dB/s entre deux mesures.

Mesures (source à flash et bip simultanés toutes les 2 s) : son/image +0,5 ms, vu-mètres/image −10 ms, sortie NDI +3 ms en moyenne (−30 à +37 ms) ; images régulières à 0,3 ms près.

### Outils de contrôle de l'image (scopes)

Le widget **Scope** (`LayerKind::Scope`) montre l'oscilloscope, les parades RVB et Y'CbCr, le vectorscope, l'histogramme, la chromaticité CIE 1931 ou les fausses couleurs d'une entrée.

- **La trace** (ce que font les pixels) est calculée par le serveur sur l'image affichée de l'entrée (`strata-mv-render/src/scopes.rs`), environ 130 000 échantillons répartis sur l'image, en quelques millisecondes ; une fois par image et par demande (taille, outil, intensité, couleur), au plus toutes les 70 ms, partagée par toutes les sorties et toutes les pages (`Live::scope`).
- **Le graticule** (lignes, cibles, locus, gamuts, échelle) est disposé dans `strata-mv-core/src/wid/scope.rs` comme une liste d'éléments (`Item::Path`, rectangles, textes) que les deux moteurs dessinent par-dessus la trace ; `web/src/widgets/scope.tsx` le reproduit à l'identique. Les positions (`level_y`, `vector_point`, `cie_point`, `panels`) servent aux deux : la trace tombe sur le graticule.
- **Le transfert** du signal (`VideoInfo`, lu par `showinfo` et l'ouverture de l'entrée) choisit l'échelle : % en SDR et HLG, nits en PQ (ST 2084) ; les primaires BT.2020 ou BT.709 servent au calcul CIE (RVB linéaire → XYZ).
- **Les moteurs** : le moteur Rust demande la trace à la taille où il la dessine (`LiveInputs::scope`) et la compose en mode *screen* (le noir n'ajoute rien ; fausses couleurs : opaque). Les pages web reçoivent toutes les traces de leurs scopes sur **une seule WebSocket** (`GET /api/scopes` : la page envoie la liste de ses scopes, le serveur chaque nouvelle trace en JPEG, 12 par seconde au plus) — un navigateur n'ouvre que six connexions vers un serveur.
- **Le goniomètre** est tracé du son entendu avec l'image affichée (les 80 dernières ms, `Live::heard`, gardé 1,5 s par entrée) ; la **corrélation de phase** (300 ms) part avec les vu-mètres (`SourceView::correlation`, `SignalView::correlation`).
- **`VideoInfo`** (codec, profil, échantillonnage, profondeur, plage, matrice, primaires, transfert, HDR10 / HLG / Dolby Vision / HDR10+, ST 2086, MaxCLL / MaxFALL, pistes audio dont Dolby et Atmos) est relu une fois par seconde et envoyé aux pages quand il change (et toutes les 5 s pour celles qui viennent de s'ouvrir).

### Tableau de bord (courbes)

Le serveur relève chaque seconde (`perf.rs`, `CurveStore`) les processeurs, la mémoire, les processeurs de Strata Multiview et des ffmpeg, l'écart PTP (pmc interrogé chaque seconde ; NTP toutes les dix), l'écart NTP, le temps de rendu de chaque canal et les images par seconde de chaque entrée ; cinq minutes sont gardées (`signal::Curves`). Le moteur Rust les lit dans `SystemView::curves` ; les pages les reçoivent une fois par seconde avec les signaux (pas avec chaque vu-mètre) et gardent les dernières. Le widget **Courbe** (`wid/chart.rs`, `web/src/widgets/chart.tsx`) en trace une, avec ses seuils.

### Alarmes

`signals::evaluate` (toutes les 240 ms) relève les conditions de chaque entrée (`Inputs::watched` : image, cadence mesurée et nominale, redémarrages, corrélation, sous-titres, erreurs TR 101 290) et de l'installation (`Plant` : PTP, NTP, processeurs, mémoire, images perdues par les sorties), note depuis quand chacune est vraie, et lève l'alarme quand elle a duré le délai de sa règle (`strata_mv_core::alarm` : profils partagés par les entrées, règles de l'installation). Les alarmes levées, terminées ou acquittées vont au journal (mille lignes, sauvé dans `alarms-journal.json`) et à `alarms.rs`, qui appelle les systèmes configurés (GET ou POST, 5 s au plus, dans un fil à part). Les pages reçoivent le journal avec les signaux, seulement quand il change (et toutes les 5 s).

### Générateur de test

Le réglage du générateur vit dans la chaîne (`ChannelConfig::generator`, gardé dans config.json) et, à chaud, dans `ChannelRt::generator`. L'horloge de la chaîne le joint à chaque demande de rendu (`RenderJob`) ; les fils de rendu dessinent alors la mire (`Renderer::render_generator`, `strata-mv-render/src/generator.rs` : le motif fixe une fois par réglage et par taille, puis ce qui bouge) au lieu du multiview. Le son de l'image `n` (`strata_mv_core::generator::sound`) part dans la trame (`Frame::audio`) : la sortie NDI l'envoie une fois par trame ; la sortie SDI l'accumule (deux trames en entrelacé) et le passe au shim DeckLink (`dl_output_push_audio`), qui le programme avec l'image (`ScheduleAudioSamples` à l'heure de l'image). Changer le générateur ne redémarre aucune sortie ; l'enregistrement des réglages système garde le générateur en cours.

## Feuille de route

- [ ] Plusieurs pistes audio par flux (aujourd'hui la première, tous ses canaux).
- [ ] Entrées ST 2110 (image et audio 2110-30).
- [ ] Sources de tally supplémentaires : Ross RossTalk, protocoles propriétaires de routeurs (Probel, Quartz) pour les UMD dynamiques.
- [ ] Tuile pilotée par un routeur (l'entrée d'une tuile suit une destination de grille).
- [ ] Timecode des flux (SEI, LTC dans l'audio) et du SDI (VITC, RP 188).
- [ ] Alarmes envoyées vers l'extérieur (SNMP, webhook, GPO).
