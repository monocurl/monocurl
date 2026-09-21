# Aperçu

Cette leçon présente l'éditeur, la structure d'une scène et comment passer de l'écriture de code à l'exportation ou à la présentation de votre travail.

## L'éditeur

![The Monocurl editor](/img/home/monocurl-editor.png)

L'éditeur dispose de trois surfaces de travail :

- **Éditeur source** (à gauche) — où vous écrivez les fichiers de scène `.mcs`
- **Viewport** (en haut à droite) : affiche l'image rendue à la position actuelle de la timeline
- **Chronologie** (en bas à droite) : parcourt les diapositives et les étapes `play` individuelles dans chaque diapositive

Lorsque vous modifiez le code, Monocurl réévalue et l'aperçu de la fenêtre est mis à jour en fonction de la position actuelle dans la chronologie.

## Structure de la scène

Chaque scène comporte trois parties : les importations, une section d'initialisation et des diapositives. Nous omettons les importations dans la plupart des exemples par souci de concision.

```mcl 
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene

# --- init section ---
# runs first; sets up helpers and the initial visible state

mesh dot = center{ORIGIN} fill{soft{} CYAN} stroke{CYAN, 2} Circle(0.4)

slide "intro"
    # slide containing some animations
    mesh title = center{0.8u} Text("Hello", 0.7)
    play Write(0.9)

slide
    dot = center{1.2r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.5)
    play Lerp(1.2)
```

Le code avant le premier `slide` est **init**. C'est là que résident les importations, aux côtés des constantes, des fonctions d'assistance et de l'état visible de départ. Après le premier mot-clé `slide`, le code appartient à cette diapositive et peut contenir des animations `play`.

## Une première scène

Voici une petite scène complète. Le modèle est le suivant : configurer certains assistants dans init, puis dans chaque diapositive, muter l'état de la scène de manière continue via des animations de lecture.

```mcl video
slide "intro"
    mesh circle = 
      center{pos: 1.4l} 
      color{col: BLUE}
      Circle(0.4)
    mesh label = 
      center{pos:1.4l + 0.75d} 
      Text(text: "hello", 0.65)
    # introduce the newly created meshes in an animated fashion
    play [Write(0.9, [&label]), Fade(0.9, [&circle])]

slide "transform"
    circle.pos = 1.4r
    circle.col = ORANGE
    label.pos = 1.4r + 0.75d
    label.text = "world"
    # transform both meshes into new state
    play Trans(1.2)
```

## Navigation dans la chronologie

Pour un contrôle plus précis, vous pouvez cliquer pour rechercher. Mais de manière générale, utilisez le clavier pour vous déplacer dans votre scène :

- `,` / `.` — diapositive précédente/suivante
- `<` / `>` — passer au début/fin de la scène
- `;` / `'` — petit pas en arrière/en avant


Une habitude utile lors de la création consiste à parcourir la chronologie après avoir ajouté une nouvelle diapositive pour vérifier que chaque étape `play` fait ce que vous vouliez.

## Présentation et exportation

Le même fichier source `.mcs` peut être utilisé de trois manières :

- **Exportation vidéo** — Menu Fichier → Exporter la vidéo. Rend la scène complète sous la forme d'un `.mp4`.
- **Exportation d'image** — Menu Fichier → Exporter l'image. Restitue une seule image sous la forme de `.png`.
- **Mode Présentation** — `Cmd/Ctrl-P`. Transforme les diapositives en points de contrôle de navigation, en s'arrêtant à chaque limite `slide`.

## Flux de travail interactif

!vid[](/video/interactive-development.mp4)


En mode présentation, `Cmd/Ctrl-T` ouvre le **panneau de paramètres** où vous pouvez modifier une partie de l'état de la scène avec des curseurs. C’est plus avancé/niche mais peut être puissant.

En mode aperçu et présentation, vous pouvez faire glisser le curseur pour déplacer la caméra. Si vous appuyez sur Maj et faites glisser le curseur, vous pouvez effectuer un panoramique de la caméra. Ceux-ci sont particulièrement utiles pour créer des scènes 3D.

## Monocurl sur le Web

[Monocurl Essays](https://www.monocurl.com/monocurl-essays/) montre comment les scènes Monocurl peuvent être exécutées directement sur le Web. Le runtime sous-jacent est également disponible sous forme de [package NPM](https://www.npmjs.com/package/monocurl).
