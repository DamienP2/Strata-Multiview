# Strata Multiview

**Multiviewer broadcast logiciel pour la régie de diffusion.** Strata Multiview compose des mosaïques de sources vidéo (multiviews) dans le navigateur : les images des entrées en direct avec leur **tally**, leur **UMD**, leurs **vu-mètres**, leur **loudness EBU R128** et leur **timecode**, les **alarmes** (perte, noir, image figée, silence, niveau), l'état **PTP / NTP**, le journal des défauts, des horloges et des logos. Il envoie le résultat vers les sorties du studio : HTML5, NDI, flux SRT / RTMP / HLS, SDI (DeckLink) et SMPTE ST 2110.

Un seul exécutable par plateforme, un moteur de rendu en Rust, une interface web.

![Le modèle « Régie finale » à l'antenne : tally, UMD, timecode, loudness, alarmes, 16 canaux audio et mire sur les entrées absentes](docs/screenshots/output.png)

## Sommaire

1. [Vue d'ensemble](#1-vue-densemble)
2. [Installation](#2-installation)
3. [Premiers pas](#3-premiers-pas)
4. [Designer](#4-designer)
5. [La tuile Source](#5-la-tuile-source)
6. [Tally et UMD](#6-tally-et-umd)
7. [Audio, loudness et alarmes](#7-audio-loudness-et-alarmes)
8. [Diffusion](#8-diffusion)
9. [Système](#9-système)
10. [API](#10-api)
11. [Développement](#11-développement)
12. [Documentation complémentaire](#12-documentation-complémentaire)

---

## 1. Vue d'ensemble

### 1.1 La chaîne

```
 Entrées en direct ──► image, audio (niveaux, loudness R128), timecode, alarmes
 (SRT, UDP, RTMP, HLS, RTSP, NDI, SDI)        │
                                               ▼
 Régie (TSL 3.1 / 5.0, ATEM, GPI, API) ──► tally et UMD de chaque entrée
                                               │
                                               ▼
 Designer ──► multiviews publiés ──► chaîne ──► sorties (HTML5 · NDI · Stream · SDI · ST 2110)
```

- Les **entrées en direct** sont décodées en continu, et **analysées** : conteneur, programmes et EPG, codecs, images, timecode, sous-titres codés, tables MPEG-TS et TR 101 290, listes HLS, ANC SDI ; leur audio est mesuré canal par canal et en loudness EBU R128 ; leur image est surveillée (noir, image figée) ; une entrée qui tombe lève une alarme.
- Le **tally** et les **UMD** viennent des mélangeurs et des contrôleurs de tally, aiguillés vers les entrées.
- Le **Designer** compose les multiviews : vide, sur une **disposition** (2 × 2, 3 × 3, 1 + 5…) de **tuiles Source**, ou à partir d'un des **dix exemples** de régie de diffusion, puis les widgets de supervision (vu-mètres 16 canaux, loudness, timecode, état, journal des alarmes), des horloges, des logos et des textes.
- Une **chaîne** montre un multiview publié et l'envoie vers ses destinations.

### 1.2 Les pages

| Menu | Page | Rôle |
| --- | --- | --- |
| **Multiviews** | accueil | les multiviews (un par régie ou par mur d'écrans), leurs dossiers, l'import et l'export |
| **Bibliothèque** | médias partagés | images, vidéos, sons et polices communs à tous les multiviews (logos, fonds) |
| **Diffusion** | Flux vidéo · Entrées vidéo · Tally et UMD | chaînes et destinations ; entrées en direct ; sources de tally et aiguillage |
| **Système** | Matériel · Performances · Santé · Utilisateurs · API · Paramètres | cartes SDI, ST 2110, synchro ; supervision ; comptes et droits ; clés d'API ; réglages du serveur |

Un multiview s'ouvre directement dans le **Designer**, dans son propre onglet.

### 1.3 Les rôles

| Rôle | Peut |
| --- | --- |
| **Admin** | tout : utilisateurs, chaînes, entrées, tally, réglages du serveur |
| **Designer** | créer et modifier les multiviews, en mettre un à l'antenne d'une chaîne |
| **Operator** | choisir le multiview à l'antenne d'une chaîne ; régler le tally par l'API |
| **Viewer** | consulter les multiviews et les sorties |

L'interface est en **français ou en anglais**, au choix de chaque utilisateur. Les textes que le multiview écrit lui-même (« PAS DE SIGNAL », « IMAGE FIGÉE ») suivent la langue du multiview.

---

## 2. Installation

Le serveur est **un exécutable autonome** par plateforme : il embarque l'interface web, les polices et le moteur. Il s'installe en service natif.

| Plateforme | Paquet | Service |
| --- | --- | --- |
| **Windows** | installateur `.msi` avec l'app Strata Multiview (zone de notification) ; déploiement silencieux `msiexec /qn` | service Windows `strata-multiview` |
| **macOS** | installateur `.pkg` universel (Apple Silicon + Intel) avec l'app Strata Multiview (barre des menus), livré en image disque | launchd `com.stratamultiview.server` |
| **Linux** | archive avec binaire et `sudo ./install.sh` | systemd `strata-multiview` |

Les commandes de service sont les mêmes partout :

```bash
strata-mv-server service install | uninstall | start | stop | status
```

**À côté d'autres logiciels** : rien n'est partagé. Le port par défaut est **8090**, le cookie de session `strata_mv_session`, les clés d'API commencent par `smv_`. Les dossiers aussi sont distincts : `/opt/strata-multiview`, `/var/lib/strata-multiview`, « Strata Multiview » sous Windows et macOS. Le service tourne sous son propre compte `strata-mv` et l'installateur Windows a son propre code de mise à niveau.

Les **mises à jour** se font depuis **Système › Paramètres › Mises à jour** : en ligne (releases GitHub de `DamienP2/Strata-Multiview`, paquets `StrataMV-<version>-<plateforme>`) ou en déposant l'archive pour un serveur hors ligne, avec retour automatique à la version précédente si la nouvelle ne démarre pas.

ffmpeg est nécessaire aux entrées en flux (et à leur audio), aux sorties Stream et à la conversion des vidéos : les installateurs l'embarquent. Le runtime NDI est livré avec les installateurs macOS et Windows ; sous Linux, installez-le ou indiquez son chemin.

Guide complet, dossiers de données, pare-feu, proxy : [`docs/INSTALL.md`](docs/INSTALL.md).

---

## 3. Premiers pas

1. Ouvrez `http://<serveur>:8090`. Sur le serveur lui-même, la page propose de **créer le compte administrateur** ; à distance, utilisez `strata-mv-server user add admin`.
2. **Diffusion › Entrées vidéo** : déclarez les sources (caméras en SDI, flux SRT d'un encodeur, sources NDI du réseau…).
3. **Multiviews › Nouveau multiview** : nommez-le, choisissez sa résolution, partez d'une page **vide** ou d'une **disposition** (2 × 2, 1 + 5, 4 × 4…) dont les tuiles reçoivent les entrées dans l'ordre. Le multiview s'ouvre dans le **Designer**. Pour partir d'un multiview de régie complet : **Multiviews › Exemples**.
4. Ajustez les tuiles (entrée, UMD, vu-mètres, timecode, loudness), ajoutez des widgets, un logo, un titre, puis **Publiez**.
5. **Diffusion › Flux vidéo** : choisissez le **multiview à l'antenne** de la chaîne : il passe aussitôt à l'antenne. Ajoutez les destinations (HTML5, NDI, SDI, ST 2110, flux).
6. **Diffusion › Tally et UMD** : ajoutez la régie (TSL, ATEM ou GPI) et reliez ses afficheurs aux entrées.

La sortie HTML5 de la chaîne principale est sur `http://<serveur>:8090/output/main`.

![Les multiviews](docs/screenshots/home.png)

---

## 4. Designer

![Designer : le modèle « Régie finale », la tuile PGM sélectionnée](docs/screenshots/designer.png)

### 4.1 L'espace de travail

- **En haut** : le nom du multiview, annuler / rétablir, l'historique des versions et **Publier**.
- **Au centre** : la **scène** (règles, repères, zones de sécurité, fond) avec la barre d'outils : **Disposition**, **Source** (une tuile), les formes et les textes, **+** (vu-mètres, loudness, timecode, état, journal des alarmes, horloge, temps), les **outils de contrôle** (oscilloscope, parade, vectorscope, histogramme, CIE, fausses couleurs), **Affichage**, zoom. En haut, la **sortie** : le canal sur lequel le multiview s'affiche (ceux qui l'affichent, les libres, ceux qui en affichent un autre), et sa page web. Les tuiles y montrent les entrées en direct, leur tally et leurs vu-mètres. Un clic sélectionne, un glisser déplace ; clic droit : **Entrée** (sur une tuile, des vu-mètres, un loudness ou un timecode : l'entrée à montrer, celles qui reçoivent marquées d'un point vert), copier, couper, coller, dupliquer, ordre, supprimer.

![Clic droit sur une tuile : changer d'entrée](docs/screenshots/input-menu.png)
- **À droite** : l'inspecteur de l'élément sélectionné ; sans sélection, le multiview lui-même (résolution, ses tuiles, ses réglages).

Un multiview s'édite directement : une seule page, ni calques, ni timeline, ni animation. Ce qui bouge vient des sources (images, tally, vu-mètres, loudness, alarmes) et des widgets eux-mêmes (timecode, horloges).

### 4.2 Exemples

![Les exemples](docs/screenshots/examples.png)

La page **Multiviews** propose des **Exemples** : dix multiviews dessinés pour la **régie de diffusion** (pas de production). **Ajouter à mes multiviews** en fait une copie, dont les tuiles prennent les entrées du serveur dans l'ordre, et l'ouvre dans le Designer : tout y reste modifiable. Un nouveau multiview, lui, part d'une page vide ou d'une disposition.

| Exemple | Contenu |
| --- | --- |
| **Régie finale d'une chaîne** | PGM (timecode, loudness, alarmes), PVW, playout A et B, quatre retours, 16 canaux audio, loudness, état, journal |
| **Seize chaînes** | 4 × 4 chaînes avec leur loudness et leurs alarmes, l'état de l'installation et le journal |
| **Main et backup** | quatre chaînes, leur main et leur backup côte à côte, avec leur audio |
| **Retours antenne** | le programme face à ses retours (TNT, satellite, IPTV, OTT), loudness et journal |
| **Audio, 16 canaux** | les 16 canaux et le loudness d'une entrée, quatre autres entrées, avec ou sans image |
| **IP, PTP et synchro** | neuf sources ST 2110, PTP et NTP, le timecode station et le journal |
| **Contributions** | six arrivées avec leur timecode et leur loudness, le journal |
| **Nuit, 25 sources** | 5 × 5 sources où seules les alarmes ressortent, le journal à côté |
| **Habillage et sous-titres** | le programme avant et après habillage, son loudness, son audio et le journal |
| **UHD HDR et SDR** | le programme UHD et sa conversion SDR, le loudness, 16 canaux, l'état de l'installation |

Les exemples sont des fichiers `crates/strata-mv-server/examples/ex-NN-*.json` embarqués dans le serveur (1920 × 1080, 50 i/s) ; leurs tuiles lisent `{{input:N}}`, la N-ième entrée du serveur.

![Le modèle « Régie finale » rendu par le moteur Rust](docs/screenshots/model-final.png)

![Le modèle « Seize chaînes » : les entrées absentes montrent la mire](docs/screenshots/model-sixteen.png)

### 4.3 Dispositions

![Nouveau multiview : vide ou une disposition](docs/screenshots/new-multiview.png)

Une **disposition** place les tuiles Source sur une grille, avec un **espace** entre elles et une **marge** autour :

| Disposition | Tuiles |
| --- | --- |
| **1**, **2**, **2 × 2**, **3 × 3**, **4 × 4**, **5 × 5** | grilles régulières, tuiles en 16:9 |
| **1 + 5**, **1 + 7** | une grande tuile en haut à gauche, les autres autour |
| **2 + 8** | deux grandes en haut, huit petites en dessous |
| **1 + 12** | une grande au centre, douze autour |

Elle se choisit à la création d'un multiview (« Vide » par défaut), ou s'applique ensuite par le bouton **Disposition** : les tuiles existantes sont replacées dans l'ordre de lecture, les manquantes sont ajoutées avec les entrées qu'aucune tuile ne montre encore. Chaque tuile reste ensuite libre (position, taille, réglages).

### 4.4 Les widgets

Tous les widgets sont rendus à l'identique en HTML5 et par le moteur Rust (la géométrie est calculée dans `strata-mv-core`, chaque moteur ne fait que dessiner) :

| Pour | Widgets |
| --- | --- |
| **Audio** | **Vu-mètres** : jusqu'à 16 canaux d'une entrée, avec ou sans image ; noms des canaux (« L, R, C, LFE… »), échelle en dBFS, valeurs, crêtes, ligne de loudness |
| **Loudness** | **Loudness** EBU R128 d'une entrée : intégré en grand, momentary, short-term, LRA, true peak ; courbe short-term des deux dernières minutes ; couleurs selon la cible (−23 LUFS ± 1, true peak −1 dBTP) |
| **Temps** | **Timecode** : celui de la station (heure locale à la cadence de la chaîne) ou d'une entrée (NDI) ; Horloge (heure, date, fuseau, cadran analogique) ; Temps (chronomètre, compte à rebours) |
| **Supervision** | **État** : PTP (verrouillage, domaine, écart), NTP, alarmes en cours ; **Journal des alarmes** : les défauts des entrées, les plus récents d'abord, avec leur début et leur fin |
| **Contrôle de l'image** | **Scope** d'une entrée, au choix : **oscilloscope** (luminance), **parade RVB**, **parade Y'CbCr**, **vectorscope** (cibles 75 % et 100 %, ligne des tons chair), **histogramme** (luminance et R, V, B), **chromaticité CIE 1931** (gamuts BT.709, P3, BT.2020, point blanc D65), **fausses couleurs** (bandes d'exposition) ; graticule selon le transfert du signal : % en SDR et HLG (blanc de référence 75 %), **nits en PQ / HDR10** (1 à 10 000, blanc de référence 203) |
| **Signal** | **Fiche signal** d'une entrée : codec et profil, taille, profondeur et échantillonnage (4:2:0, 4:2:2…), **colorimétrie** (matrice, primaires BT.709 / BT.2020 / P3), **transfert** (SDR, **PQ**, **HLG**), plage, badge **SDR / HDR10 / HLG / HDR10+ / Dolby Vision**, métadonnées **ST 2086** et **MaxCLL / MaxFALL**, pistes son avec marques **DOLBY** (Digital, Digital Plus, TrueHD, AC-4, E) et **ATMOS** |
| **Labels et logos** | Texte, Image (PNG, JPEG, SVG, GIF, vidéo en boucle) |
| **Formes** | rectangle, cercle, trait, dégradés |

La **publication** envoie les multiviews vers les sorties ; tant qu'un multiview n'est pas publié, les chaînes gardent la version précédente.

---

## 5. La tuile Source

Une tuile **Source** montre une entrée en direct et tout ce qu'elle porte :

| Élément | Réglages |
| --- | --- |
| **Image** | l'entrée ; ajustement *Contenir* (bandes noires), *Couvrir* ou *Étirer* ; couleur de fond |
| **Perte de signal** | *Mire* : barres SMPTE avec le logo **F AudioVisuel** et « PAS DE SIGNAL » qui rebondissent dessus (chaque tuile ailleurs) ; ou *Simple* : fond uni et texte modifiable |
| **Cadre de tally** | épaisseur ; couleurs *À l'antenne* (rouge), *Préparation* (vert), *Aux* (jaune), *Sans tally* ; le cadre peut ne pas suivre le tally |
| **UMD** | en bas, en haut ou sans ; sur l'image ou à côté ; hauteur ; texte (vide : celui de la régie, sinon le nom de l'entrée ; `{{clé}}` remplacées) ; police, alignement, couleurs ; **lampes** de tally aux deux bouts (antenne à gauche, préparation à droite) |
| **Vu-mètres** | à droite, à gauche ou sans ; un par canal reçu (ou au plus *n*) ; largeur, espace, marge ; échelle en dBFS, jaune et rouge à partir de seuils ; maintien des crêtes |
| **Format** | le format du signal (« 1080i50 ») en haut à gauche |
| **Timecode** | sans, celui de l'entrée ou celui de la station, en haut à droite |
| **Loudness** | le loudness short-term de l'entrée en bas à droite, coloré selon la cible |
| **Alarmes** | cadre rouge clignotant (critique) ou ambre (alerte) et bandeau du défaut (« IMAGE FIGÉE », « PERTE DU SIGNAL »…), avec le nombre d'autres défauts |

Les réglages et la géométrie sont communs aux deux moteurs (`strata-mv-core/src/wid/source.rs`, `web/src/widgets/source.tsx`) : la sortie HTML5 et les sorties du moteur Rust montrent la même image.

---

## 6. Tally et UMD

![Tally et UMD : l'aiguillage des entrées](docs/screenshots/tally.png)

**Diffusion › Tally et UMD** réunit les sources de tally et l'aiguillage de leurs afficheurs vers les entrées. Les réglages sont dans `tally.json` du dossier de données et s'appliquent aussitôt, sans couper les sorties.

### 6.1 Les sources

| Source | Transport | Ce qui est lu |
| --- | --- | --- |
| **TSL UMD 3.1** | UDP, TCP (ce serveur écoute, ou se connecte au mélangeur) | adresse 0–126, tally 1 à 4 (chacun attribuable à l'antenne, la préparation ou l'aux), texte de 16 caractères |
| **TSL UMD 5.0** | UDP, TCP (trames DLE/STX) | index, tally gauche, droite et texte (rouge = antenne, vert = préparation, ambre = aux), texte ASCII ou UTF-16, filtre d'écran |
| **Blackmagic ATEM** | UDP 9910 vers l'adresse du mélangeur | tally de chaque entrée (numérotée comme sur le mélangeur) et son **nom long** comme texte d'UMD |
| **GPI** | un boîtier GPI qui appelle une URL | chaque contact allume la lampe choisie sur sa liaison |

Les mélangeurs Ross (Carbonite, Acuity), Grass Valley, Sony, Panasonic, vMix et la plupart des contrôleurs de tally parlent TSL : choisissez TSL 3.1 ou 5.0 selon leur réglage.

![Une source TSL et ses afficheurs](docs/screenshots/tally-source.png)

La page d'une source montre son état (« À l'écoute en UDP 8900 », « Reçu de 10.0.0.12 », « Connecté »…) et les **afficheurs reçus** avec leurs lampes et leur texte en direct ; chacun se relie d'un clic à une entrée.

### 6.2 L'aiguillage

Chaque entrée prend les lampes et le texte des afficheurs qui lui sont reliés : une source, un index (adresse TSL, numéro d'entrée ATEM, contact GPI) et, pour le GPI, la lampe à allumer. Plusieurs liaisons se cumulent (par exemple l'antenne d'un mélangeur et l'aux d'un contact GPI d'enregistreur). Le texte d'UMD est celui de la première liaison qui en a un.

### 6.3 GPI et API

Un boîtier GPI appelle, à la fermeture et à l'ouverture d'un contact :

```
GET http://<serveur>:8090/api/tally/gpi/<source>/<contact>/on?key=smv_…
GET http://<serveur>:8090/api/tally/gpi/<source>/<contact>/off?key=smv_…
```

L'automation peut aussi régler directement les lampes et l'UMD d'une entrée (les champs absents suivent l'aiguillage) :

```bash
curl -X PUT http://<serveur>:8090/api/tally/inputs/cam1 -H "Authorization: Bearer smv_…" \
  -H 'content-type: application/json' -d '{"program":true,"umd":"PLATEAU"}'
curl -X DELETE http://<serveur>:8090/api/tally/inputs/cam1 -H "Authorization: Bearer smv_…"
```

La clé d'API (Système › API) agit avec les droits de son utilisateur : un compte *Operator* suffit.

---

## 7. Audio, loudness et alarmes

### 7.1 Niveaux

L'audio de chaque entrée est mesuré à la réception, canal par canal : **crête** et **puissance moyenne** toutes les 40 ms environ. Les vu-mètres montent aussitôt, redescendent de 24 dB par seconde et gardent leur crête deux secondes.

| Entrée | Audio mesuré |
| --- | --- |
| **Flux** (SRT, UDP, RTMP, HLS, RTSP, URL) | la première piste audio, tous ses canaux (filtres `ebur128` et `astats` d'ffmpeg) ; une source sans audio est reconnue et décodée sans mesure |
| **NDI** | l'audio de la source, tous ses canaux |
| **SDI** (DeckLink) | l'audio embarqué : 16, 8 ou 2 canaux selon la carte, 48 kHz |

**Écouter un multiview** : sur la page web d'une sortie, un clic sur une tuile fait entendre le son de son entrée (encore un clic : silence) ; la barre d'écoute (source entendue, volume, liste des sources) n'apparaît que lorsque la souris bouge. Une sortie **NDI** peut aussi emporter le son d'une entrée avec l'image (Diffusion › sortie NDI › Son). Le serveur tient le son de chaque entrée en PCM 16 bits stéréo 48 kHz (`GET /api/inputs/{id}/audio`). Les tuiles Source montrent toujours leurs vu-mètres (au repos sans son).

Les niveaux arrivent au moteur et aux navigateurs (une douzaine de fois par seconde) avec le tally. Une entrée sans audio depuis trois secondes n'a plus de vu-mètres.

### 7.2 Loudness et timecode

Le **loudness EBU R128** de chaque entrée est mesuré en continu : momentary (400 ms), short-term (3 s), intégré, LRA et true peak. Les flux passent par le filtre `ebur128` d'ffmpeg, NDI et SDI par la même mesure en Rust. La cible est **−23 LUFS ± 1** et **−1 dBTP**. L'historique short-term (une valeur par seconde, deux minutes) nourrit la courbe du widget Loudness.

Le **timecode** d'une entrée est celui de NDI ; le **timecode station** est l'heure locale du serveur à la cadence de la chaîne.

### 7.3 Alarmes et journal

Chaque entrée est surveillée en permanence :

| Alarme | Gravité | Quand |
| --- | --- | --- |
| **Perte du signal** | critique | plus d'image reçue |
| **Noir** | critique | image noire depuis 3 s |
| **Image figée** | critique | une image vivante qui ne bouge plus : elle bougeait dans les 30 s d'avant et ses images se répètent au pixel près depuis 10 s, ou elle avait le bruit d'une image en direct et se répète au pixel près depuis 4 s. Jamais sur une mire, une image unie, du noir, une image fixe dès le départ (carton, graphique) ni un plan calme en direct (son bruit continue) |
| **Silence** | alerte | audio sous −60 dBFS depuis 5 s |
| **Trop fort** | alerte | short-term au-dessus de la cible de plus de 3 LU depuis 10 s |
| **Crête** | alerte | true peak au-dessus de −1 dBTP (tenue 3 s) |

Les alarmes s'affichent sur les tuiles (cadre et bandeau), dans le widget **État** et dans le **journal** (200 lignes, début et fin de chaque défaut). L'état **PTP** (ptp4l) et **NTP** de la machine accompagne les signaux.

---

## 8. Diffusion

### 8.1 Flux vidéo : sorties

![Flux vidéo : une chaîne, son multiview et ses destinations](docs/screenshots/outputs.png)

Une **chaîne** est un programme indépendant : son format vidéo, sa propre horloge, son multiview à l'antenne. Les chaînes se rangent par **site**. Chaque chaîne se règle sur un canvas : *Programme* → *Destinations*.

Le panneau de la chaîne choisit le **multiview à l'antenne** : il y passe aussitôt, et chaque publication s'y voit à l'image suivante.

| Destination | Détail |
| --- | --- |
| **HTML5** | lien `/output/<chaîne>` pour un écran, un navigateur, OBS ou vMix ; chaque entrée y arrive à la taille de sa tuile, décodée hors du fil principal et jouée régulièrement (une courte mise en mémoire absorbe l'irrégularité du réseau) |
| **NDI** | nom de source, groupes ; runtime livré avec les installateurs |
| **Stream** | SRT (appelant ou écoute), UDP (unicast, multicast), RTMP / RTMPS, HLS servi par Strata ; H.264 ou HEVC, débit, encodage matériel |
| **SDI** | carte Blackmagic DeckLink, genlock |
| **ST 2110** | flux 2110-20 4:2:2 10 bits ; multicast, TTL, SDP ; SMPTE 2022-7 sur les réseaux rouge et bleu |

Quand aucun multiview n'est chargé, la chaîne envoie une **mire**. Une **avance de rendu** d'une ou deux images garde une cadence régulière.

### 8.2 Entrées vidéo

Les **entrées en direct** : SRT, UDP, RTMP, HLS, RTSP, toute URL lue par ffmpeg, les **directs web** (YouTube, TikTok, Twitch, Facebook, Instagram, Kick, Dailymotion, X, Vimeo, Rumble… : le simple lien du direct suffit), **NDI** (sources découvertes sur le réseau) et **SDI** (DeckLink). Chaque entrée est décodée à la taille et à la cadence de la source, avec un tampon de lissage réglable ; la page montre une vignette, l'état, les images reçues et les pertes. Une entrée qui tombe se reconnecte seule.

### 8.3 Contenu du signal

![Contenu du signal : programmes et EPG](docs/screenshots/analysis-epg.png)

Sous les vignettes, le **contenu du signal** de l'entrée choisie, quelle que soit sa source, rafraîchi toutes les deux secondes :

| Onglet | Ce qu'on y lit |
| --- | --- |
| **Présentation** | conteneur et débit, vidéo, pistes audio, sous-titres et sous-titres codés, données, timecode, GOP, état TR 101 290, programme à l'antenne, SCTE-35, variantes HLS |
| **Programmes et EPG** | chaque programme (service) : nom, fournisseur, type, en clair ou embrouillé (systèmes d'accès conditionnel), **EPG** en cours et suivant (titre, horaire, durée, résumé, genre, âge minimum, avancement), ses flux |
| **Flux** | chaque flux et son PID : codec, profil et niveau (lus dans le SPS H.264 / HEVC), image (taille, rapport, cadence, balayage TFF/BFF/progressif, échantillonnage et profondeur, plage, couleurs, **HDR** PQ/HLG), sous-titres codés A/53, audio (fréquence, canaux, format), langue, débit, descripteurs (pages télétexte, sous-titres DVB, CEA-608/708, AC-3, audiodescription…) |
| **Images** | types I / P / B, **GOP**, balayage, format décodé, **timecode** (SEI SMPTE 12-1 ou de GOP : il alimente aussi le timecode des tuiles), CC A/53, AFD, mastering display, MaxCLL/MaxFALL, et toutes les données annexes des images |
| **Transport (TS)** | débit, TS id, réseau, heure du flux (TDT/TOT), tables vues (PAT, PMT, SDT, NIT, EIT, CAT), **TR 101 290** (priorités 1 et 2, SDT et EIT), chaque **PID** (contenu, débit, erreurs de continuité, PCR, embrouillage), **SCTE-35** (splice_insert, time_signal, segmentation, durée, sens) |
| **HLS** | liste principale et ses **variantes** (débit, résolution, cadence, codecs, plage dynamique, groupes), rendus audio / sous-titres / CC, liste de médias (direct, EVENT ou VOD, segments, fenêtre, format TS ou fMP4, program date time, discontinuités, chiffrement, LL-HLS, marqueurs publicitaires) |
| **NDI** | format, rapport d'image, balayage, timecode, audio, métadonnées XML de la source et des trames |
| **SDI** | format, signal détecté (YCbCr 4:2:2 / RGB 4:4:4, profondeur), balayage, couleurs, HDR, timecode RP 188 / VITC, audio embarqué, **paquets auxiliaires** (ANC : DID/SDID, ligne, nature : CEA-708, AFD, SCTE-104, OP-47, timecode ATC…) |

Les entrées **SRT, UDP et TCP** passent par un relais : un premier ffmpeg reçoit le flux tel quel, Strata lit son flux de transport au passage (tables, EPG, SCTE-35, TR 101 290) et le transmet au décodeur. Une entrée **HLS** est décodée dans la variante de la playlist maître la plus proche de sa taille de décodage (les autres ne sont pas téléchargées) ; une **liste de chaînes IPTV** (M3U) est reconnue et le bouton **Chaînes…** en choisit une. Un **direct web** est trouvé sur sa page par [yt-dlp](https://github.com/yt-dlp/yt-dlp), que le serveur télécharge au premier usage dans `<données>/tools` (avec [Deno](https://deno.com) pour YouTube) et tient à jour ; seule l'image et le son sont gardés, plein cadre, et l'adresse du flux, qui expire, est redemandée à chaque reprise. L'entrée HLS est analysée par ses listes de lecture et le dernier segment de sa première variante (relu à la demande, au plus toutes les 30 s). **RTMP** et **RTSP** n'ont pas de flux de transport : leur contenu vient du décodeur.

![Contenu du signal : transport et TR 101 290](docs/screenshots/analysis-ts.png)

### 8.4 Matériel et synchronisation

- **SDI · DeckLink** : cartes détectées, rôle de chaque connecteur (sortie, entrée, référence).
- **SMPTE ST 2110** : flux déclarés, interfaces, PTP.
- **Rôles réseau** : contrôle, NDI, média rouge, média bleu, PTP.
- **Référence d'horloge** des chaînes : libre, PTP (grille ST 2059) ou DeckLink (genlock). Mise en place : [`docs/SYNC.md`](docs/SYNC.md).

---

## 9. Système

- **Performances** : processeurs, mémoire, part de Strata et de ffmpeg, avertissements (rendu en retard, images perdues, entrée en défaut, horloge) ; par chaîne la cadence réelle et le temps de rendu ; par sortie ou entrée les images par seconde et les pertes ; la capacité restante de la machine.
- **Santé** : journal du serveur, arrêts inattendus, archive pour le support.
- **Utilisateurs** : comptes par rôle, accès limités à certains multiviews et à certaines chaînes. Mots de passe hachés (Argon2), session en cookie HttpOnly, clés d'API pour l'automation.
- **Paramètres** : nom du serveur, fuseau horaire, sorties publiques, réseaux, heure, synchro vidéo, mises à jour. *Appliquer* redémarre les sorties sans redémarrer le service (le tally et les entrées continuent).

```bash
strata-mv-server user add <nom> [--role admin|designer|operator|viewer]
strata-mv-server user list | passwd <nom> | delete <nom>
```

---

## 10. API

La page **Système › API** documente toute l'API (OpenAPI 3.1, aussi sur `/api/openapi.json`) et gère les clés. Authentification : `Authorization: Bearer smv_…`, ou `?key=smv_…` dans l'URL pour les appareils qui ne savent pas envoyer d'en-tête.

| Méthode | Route | Rôle |
| --- | --- | --- |
| `POST` | `/api/auth/login` · `/logout` | session (cookie et jeton) |
| `GET` `POST` `PUT` `DELETE` | `/api/projects[/{id}]` · `/export` · `/import` | multiviews (un projet de l'API est un multiview) |
| `PUT` `DELETE` `POST` | `/api/projects/{id}/templates/{tid}` · `/publish` | multiviews, publication |
| `GET` `PUT` | `/api/channels` · `/{id}/state` · `/{id}/project` · `/{id}/multiview` · `/{id}/preview.png` | chaînes, multiview à l'antenne, rendu du programme |
| `GET` `PUT` `DELETE` | `/api/tally` · `/api/tally/config` · `/api/tally/inputs/{entrée}` · `/api/tally/gpi/{source}/{contact}/{on\|off}` | tally et UMD |
| `GET` | `/api/inputs` · `/api/inputs/{id}/live.mjpeg` · `/api/inputs/{id}/analysis` | entrées, leurs images et ce qu'elles transportent |
| `GET` `POST` | `/api/examples` · `/api/examples/{id}` · `/api/examples/{id}/cover.png` | exemples de multiviews |
| `GET` | `/api/outputs/status` · `/api/system/metrics` · `/api/health` | supervision |
| `WS` | `/ws` · `/ws?channel={id}` | évènements temps réel (multiviews, chaînes ; signaux des entrées : niveaux, loudness, timecode, alarmes ; état PTP / NTP et journal) |

```bash
curl -X PUT http://localhost:8090/api/channels/main/multiview -H "Authorization: Bearer smv_…" \
  -H 'content-type: application/json' -d '{"templateId":"<id du multiview>"}'
```

---

## 11. Développement

Pré-requis : Rust stable et Node 22 (ffmpeg pour les tests des entrées).

```bash
# 1. Interface web (Designer, pages, sortie HTML5), embarquée ensuite dans le binaire
cd web && npm install && npm run build && cd ..

# 2. Serveur (API, WebSocket, entrées, tally, rendu, sorties)
cargo run --release -p strata-mv-server -- --data-dir data
# → http://localhost:8090/
# → http://localhost:8090/output/main
```

En développement de l'interface, `npm run dev` dans `web/` sert l'UI sur le port 5173 et relaie l'API vers le port 8090.

### 11.1 Options du serveur

| Option | Variable | Défaut |
| --- | --- | --- |
| `--bind` | `STRATA_MV_BIND` | `0.0.0.0:8090` |
| `--data-dir` | `STRATA_MV_DATA_DIR` | dossier de données de l'utilisateur ; pour le service : `/var/lib/strata-multiview`, `/Library/Application Support/Strata Multiview` ou `%ProgramData%\Strata Multiview` |
| `--web-dir` | `STRATA_MV_WEB_DIR` | UI embarquée (`web/dist` en développement) |
| | `FFMPEG_PATH` · `NDI_LIB_PATH` | ffmpeg et runtime NDI, si hors des emplacements livrés |

### 11.2 Configuration

`config.json` (dossier de données) : serveur, chaînes et leurs destinations, entrées. `tally.json` : sources de tally et aiguillage.

```json
{
  "sources": [
    { "id": "regie", "name": "Régie", "protocol": "tsl5", "transport": "udp", "port": 8901 },
    { "id": "atem", "name": "ATEM", "protocol": "atem", "host": "192.168.10.240" },
    { "id": "gpi", "name": "Boîtier GPI", "protocol": "gpi" }
  ],
  "routes": [
    { "input": "cam1", "source": "regie", "index": 1 },
    { "input": "cam2", "source": "atem", "index": 2 },
    { "input": "cam2", "source": "gpi", "index": 4, "lamp": "aux" }
  ]
}
```

### 11.3 Arborescence

```
crates/
  strata-mv-core     modèle, animation, widgets (dont la tuile Source), signaux des entrées, playout (sans I/O)
  strata-mv-render   moteur de rendu logiciel (tiny-skia, texte vectoriel)
  strata-mv-output   ST 2110-20, SDI DeckLink (image et audio embarqué), NDI (image et audio), flux
  strata-mv-server   axum : REST, WebSocket, chaînes, multiviews, entrées (audio), signaux, tally (TSL, ATEM, GPI), UI embarquée, services OS
  strata-mv-tray     app de la barre des menus / zone de notification
assets/       polices Inter (OFL) et icônes
packaging/    installateurs Linux, macOS, Windows ; ffmpeg ; NDI
web/          React + TypeScript : pages, Designer, Tally, sortie HTML5 ; i18n/ (FR / EN)
docs/         installation, architecture, synchronisation, captures d'écran
```

### 11.4 Tests et vérifications

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd web && npx tsc --noEmit -p . && npm run build
node web/scripts/i18n-check.mjs          # textes sans traduction française
```

---

## 12. Documentation complémentaire

- [`docs/INSTALL.md`](docs/INSTALL.md) : installation détaillée, dossiers, pare-feu, mises à jour.
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) : architecture du moteur et du serveur.
- [`docs/SYNC.md`](docs/SYNC.md) : synchronisation vidéo (PTP, genlock).
- [`docs/outputs.example.json`](docs/outputs.example.json) : exemple de destinations.
