# Animations parallèles

Jusqu'à présent, chaque instruction `play` exécute une animation jusqu'à son terme avant le début de la suivante. Cette leçon explique comment exécuter des animations simultanément, comment créer des assistants d'animation réutilisables et quand transmettre des références explicites.

## Jouer en parallèle

Passer une liste à `play` exécute tous les éléments simultanément. La scène attend que chaque branche se termine.

```mcl video

mesh circle = center{1.4l} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.42)
mesh label = center{0.9d} color{BLUE} Text("parallel", 0.55)

slide "Parallel"
    play [
        Write(1.2, [&label]),
        Grow(1.0, [&circle])
    ]
    play Wait(0.5)

    circle = center{1.4r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.55)
    label = center{0.9d} color{ORANGE} Text("done", 0.55)
    play [
        Trans(1.2, [&circle]),
        Write(1.0, [&label])
    ]
```

Chaque branche doit posséder son propre maillage. Le fait que plusieurs animations simultanées fonctionnent sur le même maillage entraînera une erreur d'exécution.

## Blocs d'animation

`anim {}` crée un bloc d'animation - une séquence différée de code et d'instructions `play`. Le bloc ne fait rien lorsqu'il est défini ; il ne s'exécute que lorsqu'il est transmis à `play`.

```mcl transcript
mesh ball = Square(1)

slide "Animation Block"
  let DoSomething = anim {
      print "inside block: start"
      ball = shift{2r} ball
      play Lerp(1.2)
      play Wait(0.4)
      ball = shift{2l} ball
      play Lerp(1.2)
      print "inside block: done"
  }
  
  print "block has been defined"
  play DoSomething
  print "slide code resumed"
```

Les blocs d'animation se comportent comme des coroutines (également appelées promesses dans d'autres langages) : lorsqu'ils sont joués, ils s'exécutent étape par étape, cédant à chaque instruction `play`. C'est pourquoi `print` à l'intérieur d'un bloc `anim {}` n'apparaît dans la transcription qu'une fois que la tête de lecture a passé cette ligne.

## Progresseurs

Un **progressor** est un idiome dans lequel un lambda accepte un leader par références et le mute dans un bloc d'animation. La syntaxe `&` transmet une référence plutôt qu'une copie.

```mcl video

let MoveBy = |&m, delta| anim {
    m = shift{delta} m
    play Lerp(1.2, [&m])
}

mesh ball = center{1.5l} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.42)
mesh pulse = []

slide "Progressors"
    play Set()
    play Wait(0.5)

    let pulse_ring = anim {
        pulse = fill{CLEAR} stroke{CYAN, 3} Circle(0.35)
        play Grow(1.3)
        pulse = fill{CLEAR} stroke{CLEAR, 3} Circle(0.9)
        play Trans(1.5)
    }

    play [
        MoveBy(&ball, 3r),
        pulse_ring
    ]
```

`MoveBy` prend `&ball` (une référence au leader) et `delta`. À l'intérieur du bloc, `m = shift{delta} m` mute le leader via la référence. Lorsque `MoveBy` est joué dans le cadre de la liste parallèle, il s'exécute en même temps que `pulse_ring`.

Les progresseurs vous permettent de modifier l'état de la scène en parallèle des animations.

## Références explicites

Le moteur d'animation déduit généralement quels leaders sont sales et doivent être animés. Mais il est agressif et anime tous les sales dirigeants ensemble. Dans deux cas, vous devez être explicite :

**1. Vous avez modifié un leader mais vous ne le souhaitez pas dans cette animation.** Passez `[&specific_leader]` pour limiter l'animation à ce leader uniquement.

**2. Les branches parallèles modifient toutes deux la géométrie associée.** Être explicite sur quelle branche possède quel leader évite toute interférence accidentelle.

```mcl
# Without explicit refs, Lerp would animate both ball and label together
ball.pos = 1.4r
label = next_to{ball, 1d, 0.2} Text("moved", 0.55)

play [
    Lerp(1.2, [&ball]),    # ball moves smoothly
    Set([&label])           # label snaps to new position instantly
]
```

## Retard

Le modificateur `delay{}` se compense lorsqu'une animation démarre dans un bloc parallèle.

```mcl video

slide "Stagger"
    mesh a = center{1.4l} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.38)
    mesh b = center{0r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.38)
    mesh c = center{1.4r} fill{soft{} GREEN} stroke{GREEN, 2} Circle(0.38)
  
    play [
        Fade(0.8, [&a]),
        delay{0.2} Fade(0.8, [&b]),
        delay{0.4} Fade(0.8, [&c])
    ]
    play Wait(0.6)
```

Voyez si vous pouvez imaginer comment un retard pourrait être mis en œuvre.

## Procédure pas à pas : animation par caméra 3D

Pour voir ces modèles fonctionner ensemble, considérez l'exemple d'animation de caméra 3D fourni avec Monocurl. Voici comment vous raisonneriez en le construisant.

Le but est le suivant : afficher une grille plate colorée, puis simultanément la soulever vers une surface et faire tourner la caméra autour d'elle.

**Étape 1 : Créez l'état initial dans init.**

La grille commence à plat, la caméra démarre à la position par défaut en regardant l'origine. Définissez une fonction de couleur et configurez les maillages.

```mcl
let samples = 24
let height = |x, y| 1.15 * ((x - 0.5)^2 + (y - 0.5)^2)
let color_keys = [0 -> BLUE, 0.15 -> YELLOW, 0.3 -> ORANGE, 0.55 -> RED]

let color_at = |pos, idx| keyframe_lerp(color_keys, height(pos[0], -pos[1]))
```

**Étape 2 — Première diapositive : révélez la scène initiale.**

```mcl
slide "Flat Grid"
    mesh grid = stroke{BLACK, 1.5} ColorGrid(
        |pos, idx| BLACK,
        [0, 1, samples],
        [-1, 0, samples]
    )
    mesh axis = Axis3d(basis: [1r, 1d, 1b], color: BLACK, [1u, 1u, 1b])

    play [Fade(0.8, [&axis, &grid]), Write(0.8, [&title])]
```

**Étape 3 — Deuxième diapositive : soulevez la grille et déplacez la caméra en parallèle.**

Chaque action est un bloc `anim {}` avec ses propres étapes internes. Ils courent ensemble dans un `play [...]`.

```mcl
slide "Surface And Camera"
    let lift_grid = anim {
        # First, recolor the flat grid
        grid = stroke{BLACK, 1.5} ColorGrid(color_at, [0, 1, samples], [-1, 0, samples])
        play Trans(0.8, [&grid])

        # Then, lift vertices to match the height function
        grid = point_map{|p| [p[0], p[1], height(p[0], p[1] + 1)]} grid
        play Trans(1.8, [&grid])
    }

    let move_camera = anim {
        camera = Camera([2.2, -2.1, 1.45], [0.5, -0.5, 0.35], [0, 0, 1])
        play CameraLerp(&camera, 2.6)
    }

    play [lift_grid, move_camera]
```

L'idée clé : `lift_grid` et `move_camera` sont complètement indépendants. `lift_grid` possède `grid` ; `move_camera` possède `camera`. Parce qu’ils n’ont pas de dirigeants communs, ils peuvent fonctionner en parallèle sans interférence.

`CameraLerp` est une animation spécialisée pour le mouvement de la caméra qui produit des arcs plus agréables visuellement que le simple `Lerp` pour les positions de la caméra.

```mcl video
let samples = 24
let height = |x, y| 1.15 * ((x - 0.5) ^ 2 + (y - 0.5) ^ 2)
let color_keys = [0 -> BLUE, 0.15 -> YELLOW, 0.3 -> ORANGE, 0.55 -> RED]

let color_at = |pos, idx| {
    let value = height(pos[0], -pos[1])
    # keyframe_lerp turns a scalar field into a smooth multi-stop surface gradient.
    return keyframe_lerp(color_keys, value)
}

slide "Flat Grid"
    mesh grid = stroke{BLACK, 1.5} ColorGrid(
        |pos, idx| BLACK,
        [0, 1, samples],
        [-1, 0, samples]
    )
    mesh axis =
        shift{[0, 0, -0.01]} # draw below function
        axis_style{"x", 0, 1, "x"}
        axis_style{"y", 0, 1, "y"}
        axis_style{"z", 0, 1, "z"}
        Axis3d(
            basis: [1r, 1d, 1b],
            color: BLACK,
            grid_color: LIGHT_GRAY,
            [1u, 1u, 1b]
        )
    mesh monocurl = center{0.7u} Text("Monocurl", 2)

    play [Fade(0.8, [&axis, &grid]), Write(0.8, &monocurl)]

slide "Surface And Camera"
    # an anim block is analogous to a coroutine in other languages
    # it does nothing until it is played
    # (you can try experiment with the playhead to see when the
    # "Got here" is actually printed)
    let lift_grid = anim {
        grid = stroke{BLACK, 1.5} ColorGrid(
            color_at,
            [0, 1, samples],
            [-1, 0, samples]
        )
        play Trans(0.8, [&grid])

        print "Got here"

        grid =
            point_map{|point| [point[0], point[1], height(point[0], point[1] + 1)]}
            grid
        play Trans(1.8, [&grid])
    }

    # camera lerp is a specialize anim for interpolating 
    # camera positions, lerp "works" as well 
    # but is visually less pelaseing
    let move_camera = anim {
        camera = Camera([2.2, -2.1, 1.45], [0.5, -0.5, 0.35], [0, 0, 1])
        play CameraLerp(&camera, 2.6)
    }


    play [lift_grid, move_camera]
    play Wait(0*** Add F*** Add File: content/lessons/fr/05-ai-development.md
# Développement de l'IA

Les scènes Monocurl sont des fichiers texte brut et les LLM peuvent être très utiles pour éviter les tâches de mise en page passe-partout et fastidieuses.

![An AI agent editing a Monocurl scene while the desktop app previews the result](/img/lessons/ai-mcp-workflow.png)

## Statut actuel

Monocurl dispose d'un serveur MCP (Model Context Protocol) qui permet aux assistants IA compatibles de lire directement la documentation de Monocurl, la référence stdlib, les notes CLI et les exemples de scènes en tant que ressources structurées. C'est la méthode recommandée pour donner un contexte Monocurl à un assistant IA.


Le serveur est publié sur npm sous le nom `@enigmurl/monocurl-mcp`. Il s'agit uniquement d'un serveur de documentation. Il ne modifie pas les scènes, n'exécute pas Monocurl, ne rend pas les images et ne valide pas le code.

Pour un client MCP qui accepte la configuration du serveur stdio, ajoutez :

```json
{
  "mcpServers": {
    "monocurl": {
      "command": "npx",
      "args": ["-y", "@enigmurl/monocurl-mcp"]
    }
  }
}
```

Pour Claude Code, la commande équivalente est :

```sh
claude mcp add --transport stdio monocurl -- npx -y @enigmurl/monocurl-mcp
```

Pour le Codex, ajoutez ceci à `~/.codex/config.toml` :

```toml
[mcp_servers.monocurl]
command = "npx"
args = ["-y", "@enigmurl/monocurl-mcp"]
```

Après l'avoir installé, demandez à l'assistant de lire les ressources Monocurl MCP avant d'écrire une scène.

## Exemple d'invite

Vous pouvez coller quelque chose comme ceci dans un assistant de codage IA :

```text
You are helping me write a Monocurl scene. Monocurl is a programming language and desktop app for creating mathematical animations with live preview, video export, and slideshow-style presentation.

Install and use the Monocurl MCP context server before writing code. If your MCP client supports stdio servers, add a server named monocurl with:

command: npx
args: ["-y", "@enigmurl/monocurl-mcp"]

Then read the Monocurl MCP resources, especially the language overview, stdlib docs, CLI guide, and example scenes.

When writing and testing Monocurl code, keep these points in mind:

1. If the `monocurl` binary is not on PATH, look for the application binary manually. On macOS, check `/Applications`, `~/Applications`, or the app bundle contents. On Windows, check `Program Files` for `Monocurl.exe`. The binary can run scenes and inspect print output.
2. Be mindful when sizing text. Text sizes smaller than `0.5` are typically illegible. Prefer larger text that is easy to read during presentations.
3. In most cases, do not explicitly specify the variables to animate. The default behavior animates all dirty leaders, so the variables list can usually be left as `[]`.
4. Your sandbox may not have access to a GPU device, so CLI image or video rendering may fail. Use print statements and the transcript command heavily to check positioning, values, and the general structure of the scene.
```

## Mode sans tête

Monocurl surveillera les modifications de fichiers et mettra à jour dynamiquement la chronologie et la fenêtre d'affichage en réponse à toute modification externe, y compris celles des agents IA. Par conséquent, vous pouvez utiliser votre éditeur de texte préféré ou un agent IA pour écrire le fichier source de la scène, puis revenir à l'éditeur Monocurl pour voir les résultats et parcourir la chronologie. Si vous préférez utiliser l'espace de gauche pour un terminal au lieu de l'éditeur par défaut, activez le menu sans tête dans le menu Fichier.
