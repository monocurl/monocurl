# Bases de l'animation

Les animations Monocurl sont construites à partir de changements d'état. Vous faites muter les dirigeants ; Les instructions `play` synchronisent les abonnés avec les dirigeants avec une stratégie choisie.

## Leader et suiveur

`mesh x = value` crée deux choses :

- Un **leader** — la variable que votre code lit et écrit, initialisée à `value`
- Un **suiveur** — ce que la fenêtre dessine réellement, initialisé à `[]` (vide)

Changer le leader du code est instantané et invisible. Seule une instruction `play` permet au suiveur de rattraper le leader, et l'animation choisie pour `play` contrôle *comment* il y arrive.

La fenêtre de l'éditeur affiche l'état du suiveur à tout moment. Lorsque vous parcourez la chronologie, vous demandez : "à quoi ressemblaient les suiveurs à ce moment de l'exécution ?"

```mcl video

mesh ball =
    center{1.6l}
    fill{soft{} BLUE}
    stroke{BLUE, 2}
    Circle(0.45)

slide "Leader And Follower"
    play Wait(0.5)

    # This assignment changes the leader immediately.
    # At this point viewport still shows the follower which hasn't caught up yet.
    ball =
        center{1.6r}
        fill{soft{} ORANGE}
        stroke{ORANGE, 2}
        Circle(0.45)

    play Wait(0.5)

    # Now Set() syncs the follower to the leader — the ball jumps.
    play Set()
    play Wait(0.8)
```

>Exception spéciale IMPORTANTE : à la fin de **init**, tous les leaders de maillage sont automatiquement synchronisés avec leurs suiveurs. C'est pourquoi les maillages définis dans init sont déjà visibles lors de la première ouverture d'une scène.


## Ensemble

`Set` copie instantanément chaque leader sale vers son suiveur. Utilisez-le pour des sauts ou pour révéler un nouvel état sans transition.

```mcl video

mesh ball =
    center{1.4l + 0.15d}
    fill{soft{} GREEN}
    stroke{GREEN, 2}
    Circle(0.45)

slide "Set"
    play Wait(1.0)

    ball = center{1.4r} fill{soft{} MAGENTA} stroke{MAGENTA, 2} Circle(0.55)
    play Set()
    play Wait(1.0)
```

En général, les animations peuvent déduire les maillages que vous souhaitez animer en voyant lequel a changé. Dans des scénarios plus complexes, vous pouvez énoncer explicitement les variables souhaitées avec la syntaxe `play Set([&ball])`.

## Lerp

`Lerp` interpole les valeurs compatibles dans le temps. "Compatible" signifie généralement que le leader et le suiveur sont le même appel de fonction avec la même structure, les arguments peuvent donc être interpolés individuellement. La définition exacte peut être trouvée dans la documentation.

```mcl video

let Ball = |pos, radius, col|
    center{pos} fill{soft{} col} stroke{col, 2} Circle(radius)

mesh ball = Ball(pos: 1.4l, radius: 0.35, col: BLUE)

slide "Lerp"
    play Wait(0.6)

    ball.pos = 1.4r
    ball.radius = 0.6
    ball.col = ORANGE
    play Lerp(1.5)
    play Wait(0.8)
```

Étant donné que `ball` est défini comme un appel étiqueté à `Ball`, vous pouvez muter des champs individuels (`ball.pos`, `ball.radius`, `ball.col`), puis `Lerp` interpole chaque argument indépendamment de son ancienne valeur vers sa nouvelle valeur.

>IMPORTANT Il s'agit du modèle d'animation d'images clés de base dans Monocurl : concevoir des fonctions de constructeur avec des arguments étiquetés, puis muter des champs spécifiques pour définir chaque état d'image clé.

`Lerp` et d'autres animations acceptent un modificateur `rate` facultatif pour faciliter :

```mcl
play Lerp(1.5, smooth)     # default: smooth S-curve
play Lerp(1.5, ease_out)   # decelerates
play Lerp(1.5, linear)     # constant speed
```

## Animations d'introduction et de sortie

Certaines animations sont destinées aux maillages qui apparaissent ou disparaissent. Ils comparent l’État suiveur actuel avec le nouvel État leader et animent la différence.

**Écrire** : trace de nouveaux contours comme s'ils étaient dessinés à la main. Idéal pour les courbes et le texte.

```mcl video
slide "Write"
    mesh curve = stroke{BLUE, 3} ExplicitFunc(|x| sin(x * TAU), [-1.5, 1.5, 100])
mesh label = center{1.2d} color{BLUE} Text("sin(2πx)", 0.55)
    play Write(1.2)
    play Wait(0.8)
```

**Fade** — fait apparaître ou disparaître les maillages.
**Grow** : étend la géométrie de son centre vers l'extérieur. Bon pour l'apparition de formes.

```mcl video

slide "Grow And Fade"
    mesh shapes = [
        center{1.3l} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.42),
        center{0r} fill{soft{} ORANGE} stroke{ORANGE, 2} Square(0.7),
        center{1.3r} fill{soft{} GREEN} stroke{GREEN, 2} RegularPolygon(5, 0.44)
    ]
    play Grow(1.0)
    
    mesh label = center{1.1d} color{GRAY} Text("three shapes", 0.55)
    play Fade(0.8)
    play Wait(0.6)

    # can also be used for hiding!
    shapes = []
    label = []
    play Fade(0.9)
```

## Trans et TagTrans

**Trans** est la transformation de maillage à usage général. Il transforme le suiveur actuel en leader actuel en faisant correspondre les contours à l'aide d'une fonction de coût. Il peut être utilisé lorsque `Lerp` n'est pas possible (en raison de structures de leader et de suiveurs différentes).

```mcl video

mesh shape = fill{soft{} CYAN} stroke{CYAN, 2} Triangle(1.2l, 1.2r, 1.0u)

slide "Trans"
    play Wait(0.8)

    shape = fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.75)
    play Trans(1.2)
    play Wait(0.8)

    shape = Rect([2.0, 1.2])
    play Trans(1.2)
    play Wait(0.6)
```

**TagTrans** est une spécialisation de `Trans` qui restreint la correspondance aux fragments avec la même balise. Cela vous donne un contrôle précis sur le processus de correspondance des contours.

```mcl video

mesh pair = [
    tag{1} center{1.1l} fill{soft{} BLUE} stroke{BLUE, 2} Square(0.55),
    tag{2} center{1.1r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.45)
]

slide "TagTrans"
    play Wait(0.8)

    # Without TagTrans, Trans might match the square to the right circle
    # and the circle to the left square. TagTrans forces tag-1 to tag-1
    # and tag-2 to tag-2, so each shape moves and transforms independently.
    pair = [
        tag{2} center{1.1l} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.5),
        tag{1} center{1.1r} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.5)
    ]
    play TagTrans(1.4)
    play Wait(0.8)
```

Lorsque les balises changent de nom entre les états, transmettez `tag_map` aux listes de balises sources du groupe
avec des listes de balises cibles. Cela affecte uniquement la correspondance ; il ne réécrit pas le maillage.

```mcl transcript
# full tag list [1, 2] matches full tag list [3, 4]
let one_to_one = [[1, 2] -> [3, 4]]
print one_to_one[[1, 2]]

# full tag lists [1] and [2] both match full tag list [3]
let many_to_one = [[[1], [2]] -> [[3]]]
print many_to_one
print many_to_one[[[1], [2]]]
```

## Lerp avec opérateurs

`Lerp` n'est pas uniquement destiné aux arguments de fonction étiquetés. Les expressions basées sur les opérateurs peuvent également être interpolées, car les opérateurs connaissent leur propre état d'identité.

Par exemple, l'interpolation de `x` à `rotate{angle} x` permet au maillage de tourner en douceur.

```mcl video

mesh tri =
    center{ORIGIN}
    fill{soft{} BLUE}
    stroke{BLUE, 2}
    Triangle(0.8l, 0.8r, 0.9u)

slide "Rotate With Lerp"
    play Set()
    play Wait(0.5)

    tri = rotate{PI} tri
    play Lerp(1.8)
    play Wait(0.8)
```

La même chose fonctionne pour `shift`, `scale`, `color` et la plupart des autres opérateurs. Le modèle général est le suivant : `mesh = operator{args} mesh` définit le leader sur la version exploitée, et `Lerp` interpole du suiveur (non exploité) vers le leader (opéré).

Vous pouvez également enchaîner cela pour animer une séquence d’applications opérateurs :

```mcl video

mesh box =
    center{1.5l}
    fill{soft{} BLUE}
    stroke{BLUE, 2}
    Square(0.55)

slide "Shift And Rotate"
    play Set()
    play Wait(0.5)

    box = shift{3r} rotate{PI / 3} box
    play Lerp(1.8)
    play Wait(0.6)
```

## Choisir une animation

- **Set** – instantané ; utiliser pour les coupes sautées et les révélations initiales
- **Lerp** — interpolation fluide ; à utiliser lorsque le leader et le suiveur ont la même structure
- **Écrire** — traçage ; utiliser pour de nouvelles courbes, chemins et texte
- **Croissance** — expansion ; utiliser pour de nouvelles formes remplies
- **Fondu** — opacité ; utiliser pour faire apparaître ou disparaître une géométrie
- **Trans** — morphing général ; à utiliser lorsque la structure change de manière significative
- **TagTrans** — morphing balisé ; à utiliser lorsque plusieurs pièces indépendantes ont chacune besoin de leur propre contrepartie
