# Importer un graphique After Effects

Strata importe les compositions After Effects exportées au format **Lottie** (Bodymovin). C'est le format ouvert de sortie d'After Effects : il décrit les calques, les transformations et les keyframes.

## Pourquoi pas directement le `.aep` ou le `.mogrt` ?

- Un projet `.aep` est un format binaire fermé, que seul After Effects sait lire.
- Un Motion Graphics template `.mogrt` est une archive qui contient ce même projet After Effects (`.aegraphic`, puis `.aep`), plus la liste des contrôles exposés à Premiere.
- Strata reconnaît un `.mogrt` et explique la marche à suivre ; il ne peut pas en lire l'animation.

## 1. Exporter depuis After Effects

1. Installez l'extension gratuite **Bodymovin** (aescripts / Adobe Exchange) ou **LottieFiles for After Effects**.
2. Ouvrez *Fenêtre → Extensions → Bodymovin*, puis cochez la composition à exporter.
3. Dans les réglages (icône engrenage) :
   - laissez **Glyphs** décoché, pour que les textes restent du texte modifiable ;
   - cochez *Assets → Include in json* pour embarquer les images (sinon, exportez le dossier `images/` et zippez-le avec le `.json`).
4. Lancez *Render* pour obtenir un fichier `data.json`, un `.zip` ou un `.lottie`.

## 2. Importer dans Strata

Dans le **Designer**, panneau *Templates*, cliquez sur l'icône d'import (à gauche du `+`) et choisissez le fichier. Le template est créé dans le projet ouvert, puis il s'ouvre.

Par l'API, la même opération s'écrit :

```bash
curl -H "Authorization: Bearer pcg_…" --data-binary @data.json \
  "http://serveur:8080/api/projects/<projet>/import/after-effects?name=Bandeau"
```

## Ce qui est converti

| After Effects | Strata |
|---|---|
| Calque de forme : rectangle (arrondi), ellipse, fond, contour | Rectangle / ellipse ; plusieurs formes deviennent un groupe |
| Solide | Rectangle |
| Texte (police, graisse, taille, couleur, alignement, approche) | Calque texte **relié à un champ de données** que l'opérateur remplit |
| Image (embarquée ou dans le zip) | Image copiée dans la bibliothèque d'assets |
| Objet nul, parentage | Groupe qui contient ses enfants |
| Précomposition (avec son décalage dans le temps) | Groupe |
| Position, point d'ancrage, échelle, rotation, opacité | Propriétés identiques, avec les keyframes |
| Taille animée d'un rectangle | Largeur / hauteur animées, depuis le centre |
| Vitesse et influence des keyframes, keyframes maintenues | Easing bezier ou *hold* identique |
| Points d'entrée / sortie des calques | Opacité en *hold* |
| Marqueurs de composition « pause », « on air », « out » | Marqueurs de playout *Pause* et *Out* |

Sans marqueur, l'import place le marqueur *On air* au début du plus long moment sans animation, et le marqueur *Out* à sa fin. Le template est converti à la cadence du projet et recadré si la composition n'a pas la taille du projet.

## Ce qui ne l'est pas (signalé à l'import)

Les tracés libres dessinés à la plume, les dégradés, les masques et caches (track mattes), les effets, les animateurs de texte, les modificateurs de formes (trim paths, répéteur…), la 3D et les expressions.

Pour un habillage broadcast, construisez les fonds avec les outils rectangle et ellipse : ils deviennent des calques Strata que l'on peut ensuite relier aux textes avec le bouton **Logic** (par exemple, une plaque qui suit la longueur du nom).
