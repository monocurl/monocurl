# Bases du langage

Cette leçon est une référence. Regardez les sections importantes, mais vous n'avez pas besoin de tout mémoriser ici avant de construire quelque chose. Revenez lorsque vous rencontrez une fonctionnalité que vous n’avez jamais vue auparavant.


## Reliures

Il existe trois formes de liaison utilisateur :

- `let` — nom immuable ; ne peut pas être réaffecté
- `var` — valeur locale mutable
- `mesh` — état de scène visible ; crée une paire leader/suiveur (traitée en profondeur dans la leçon d'animation)

```mcl
let radius = 0.45
var count = 0
mesh dot = fill{CYAN} Circle(radius)
```

## Saisie dynamique

Monocurl est typé dynamiquement. Une variable peut contenir n’importe quelle valeur et peut être réaffectée à un type différent.

```mcl transcript
var x = 7
print type_of(x)
x = "hello"
print x
x = [1, 2, 3]
print [type_of(x), len(x)]
```

Utilisez `print` lors de la création pour inspecter les valeurs. La sortie d'impression apparaît en ligne dans l'éditeur et dans la console (alternative à la chronologie).

## Cordes et évasions

Les chaînes Monocurl utilisent `%` comme marqueur d'échappement, et non `\`. C'est pour que LaTeX reste facile à écrire : les barres obliques inverses sont des chaînes de caractères ordinaires.

```mcl 
let newline = "first%nsecond"
let quoted = "say %"hello%""
let percent = "100%%"
```
## Copie approfondie

Devoir effectue toujours une **copie complète**.

```mcl transcript
var a = [[1, 2], [3, 4]]
var b = a
b[0][0] = 99
print [a, b]
```

## Littéraux de direction

Monocurl a des littéraux vectoriels compacts pour les directions communes, qui sont souvent utilisés pour le positionnement.

```mcl transcript
let p = 1.5r + 0.8u       # [1.5, 0.8, 0]
let q = 2l + 1d           # [-2, -1, 0]
let behind = 3b           # [0, 0, 3]
print p
print q
print behind
```

- `l` / `r` — gauche/droite (négatif/positif x)
- `u` / `d` — haut/bas (y positif/négatif)
- `b` / `f` — arrière/avant (z positif/négatif)

Les littéraux de direction ne sont que des listes de trois éléments. Vous pouvez les transmettre à `shift{}` ou `center{}` et les utiliser partout où un vecteur est attendu.

## Listes et cartes

Les listes utilisent une indexation de base zéro. Les cartes utilisent des paires `key -> value`.

>IMPORTANT L'ajout est une opération courante prise en charge par les opérateurs `..` et `.=`.

```mcl transcript
var pts = []
pts .= 1l
pts .= 1u
pts = pts .. 1r      # .. appends; .= is shorthand for x = x .. y

let labels = ["origin" -> ORIGIN, "right" -> 1r, "up" -> 1u]
print [pts, labels["origin"]]
```

## Syntaxe du bloc

`block {}` est une expression multiligne qui accumule une liste. Les lignes commençant par `.` s'ajoutent à une valeur de retour implicite ; le bloc entier est évalué à cette liste.

```mcl
let row = block {
    for (i in range(0, 4)) {
        . center{[i - 1.5, 0, 0]} fill{CYAN} Square(0.4)
    }
    . center{2r} color{ORANGE} Text("end", 0.55)
}
```

## Lambdas et arguments étiquetés

Les fonctions sont des lambdas, qui sont traitées en première classe. Les arguments peuvent avoir des valeurs par défaut et un corps contreventé peut utiliser `return`.

```mcl transcript
let square = |x| x * x
let capture = 3
let weighted = |x, weight = 1| x * weight + capture
let hyp = |a, b| {
    return sqrt(a * a + b * b)
}
print square(5)
print weighted(4, weight: 2)
print hyp(3, 4)
```

>IMPORTANT Les **arguments étiquetés** sont la fonctionnalité qui alimente le système d'animation de Monocurl. Lorsque vous appelez une fonction avec des arguments nommés, le résultat mémorise ces étiquettes et peut être muté champ par champ ultérieurement. L'intégralité de l'appel est ensuite réévaluée avec les nouveaux arguments.

```mcl video
let Ball = |pos, radius, col|
    center{pos} fill{alpha{0.2} col} stroke{col, 2} Circle(radius)

mesh ball = Ball(pos: 1.4l, radius: 0.35, col: BLUE)

slide "move"
    # Mutate individual fields; the Ball(...) call is recomputed from them
    ball.pos = 1.4r
    ball.radius = 0.6
    ball.col = ORANGE
    play Lerp(1.5)
```

Il ne s’agit pas d’une mutation d’objet au sens habituel du terme. `ball.pos = 1.4r` modifie l'argument étiqueté stocké dans l'invocation `Ball(...)` en direct, puis l'appel entier est réexécuté avec le nouvel argument. C'est ainsi que `Lerp` sait interpoler en douceur entre les deux états : il interpole chaque argument indépendamment et reconstruit le maillage à chaque image.

## Opérateurs

>IMPORTANT Les opérateurs sont des fonctions qui écrivent avant leur cible plutôt qu'autour d'elle. Ils forment un **pipeline** : chaque opérateur reçoit et transforme le maillage à sa droite. Ils interagissent bien avec le système d’attributs.

```mcl
mesh shape =
    center{pos: 1r}       # positions the result
    fill{alpha{0.18} BLUE} stroke{BLUE, 2}
    Circle(radius: 0.5)      # base constructor

# you can access inner attributes
shape.radius = 1.0
# or attributes of the operators
shape.pos = 1l
```

Lire de droite à gauche : `Circle(0.5)` crée la géométrie ; `stroke`, `fill` et `center` le transforment en séquence. Vous pouvez définir vos propres opérateurs avec `operator` en termes d'opérateurs existants :

```mcl
let soft_style = operator |target, col|
    fill{alpha{0.2} col}
    stroke{col, 2}
    target

mesh shapes = [
    center{1.3l} soft_style{BLUE} Circle(0.45),
    center{1.3r} soft_style{ORANGE} Square(0.75)
]
```

Les opérateurs intégrés gèrent le style (`fill`, `stroke`, `color`, `alpha`), le positionnement (`center`, `shift`, `rotate`, `scale`), la mise en page (`next_to`, `to_side`) et l'identité (`tag`).

Les opérateurs portent également une sémantique d'animation, qui sera discutée plus tard.

## Flux de contrôle

Flux de contrôle standard : `if`/`else if`/`else`, `for`, `while`, `break`, `continue`. Utilisez-les pour créer des données avant de les transformer en maillages.

```mcl transcript
var above = []
let values = [0.75, 1.35, 0.92, 1.52, 1.05]
for (v in values) {
    if (v > 1.1) {
        above .= v
    }
}
print above

# range(start, stop) or range(start, stop, step)
for (i in range(0, 5)) {
    print i
}

# destructuring
for ([i, v] in enumerate(["a", 1, 3.14])) {
    print ["index" -> i, "value" -> v]
}
```

Les lambdas récursifs se prennent comme un argument `self` explicite. C'est la norme en calcul lambda, mais cela peut paraître étrange si vous ne l'avez jamais vu auparavant.

```mcl transcript
let fib = |self, n| {
    if (n <= 1) { return n }
    return self(self, n - 1) + self(self, n - 2)
}
print fib(fib, 8)
```

## L'assembler

Une fonction d'assistance typique crée un maillage étiqueté et utilise des opérateurs pour le style. Les arguments étiquetés deviennent des champs d'images clés d'animation.

```mcl image
background = LIGHT_GRAY

let Bar = |height, col, x|
    center{[x, height / 2, 0]}
    fill{soft{} col}
    stroke{col, 2}
    Rect([0.55, height])

mesh chart = block {
    let data = [0.6, 1.2, 0.9, 1.5, 1.1]
    for (i in range(0, len(data))) {
        let x = (i - 2) * 0.72
        let col = keyframe_lerp([0 -> BLUE, 0.5 -> CYAN, 1 -> GREEN], i / 4)
        . Bar(height: data[i], col: col, x: x)
    }
}
```
