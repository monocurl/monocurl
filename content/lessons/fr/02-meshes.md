# Mailles et opérateurs

Un maillage est n'importe quelle valeur visible dans une scène : un cercle, une ligne de texte, une formule ou une liste de ceux-ci. La plupart des diagrammes sont construits en construisant des formes primitives et en appliquant des opérateurs pour les styliser et les positionner.

>IMPORTANT Une liste de maillages (ou des listes de listes de maillages imbriquées) est appelée un arbre à maillages. La plupart des fonctions opèrent sur des arbres de maillage en opérant sur chaque maillage constitutif.

## Constructeurs

Les constructeurs créent une géométrie à l'origine ou à proximité. Les opérateurs décident où va la géométrie et à quoi elle ressemble. La séparation maintient les constructeurs simples et composables.


```mcl image

mesh demo = [
    center{1.6l} fill{soft{} BLUE} stroke{BLUE, 2} Capsule(0.5l, 0.5r, 0.22),
    center{ORIGIN} color{ORANGE} Vector(0.9r + 0.25u, 0.45l + 0.12d),
    center{1.6r} fill{soft{} GREEN} stroke{GREEN, 2} RegularPolygon(5, 0.44),
    center{1.6l + 0.75d} color{BLUE} Text("Capsule", 0.55),
    center{ORIGIN + 0.75d} color{ORANGE} Text("Vector", 0.55),
    center{1.6r + 0.75d} color{GREEN} Text("RegularPolygon", 0.55)
]
```

Une liste de nombreux constructeurs pour référence. Consultez la documentation pour plus de détails et d'options.

- `Circle(radius)` — cercle rempli dans le plan XY
- `Annulus(inner, outer)` — anneau rempli avec un trou
- `Square(width)` — carré centré
- `Rect([width, height])` — rectangle aligné sur l'axe
- `Arrow(start, end)` — flèche dirigée avec pointe de flèche
- `Vector(delta, tail)` — vecteur en forme de flèche partant d'une queue par un delta
- `Line(start, end)` — segment de ligne simple
- `Polyline(vertices)` — chemin ouvert à travers une liste de points
- `Capsule(start, end, radius)` — forme de capsule arrondie
- `RegularPolygon(n, circumradius)` — polygone à n côtés
- `Arc(radius, [start_angle, end_angle])` — arc de cercle
- `Triangle(p, q, r)` — triangle à trois points
- `LineGrid(x_bounds, y_bounds)` — grille de lignes rectangulaires
- `ColorGrid(color_at, x_bounds, y_bounds)` — grille colorée échantillonnée
- `Axis1d(...)`, `Axis2d(...)`, `Axis3d(...)` — axes de coordonnées
- `ExplicitFunc(func, [x_min, x_max, samples])` — courbe d'une fonction
- `Field(glyph_func, x_bounds, y_bounds)` — glyphe répétitif à chaque point de la grille
- `Label(target, string, direction)` — Étiquette TeX placée à côté d'un maillage
- `Text(string, size)` — texte rendu
- `Tex(string_or_list, size)` — formule LaTeX rendue
- `Dot(point)` — point unique face à l'écran

## Opérateurs de style

Les opérateurs transforment le maillage à leur droite. Les opérateurs de style définissent les propriétés visuelles.

```mcl image

mesh demo = [
    center{1.4l} fill{soft{} BLUE} stroke{BLUE, 3} Annulus(0.22, 0.46),
    center{0r} fill{alpha{0.4} soft{} ORANGE} stroke{ORANGE, 3} Capsule(0.42l, 0.42r, 0.24),
    center{1.4r} color{GREEN} ExplicitFunc(|x| sin(x * TAU), [-0.5, 0.5, 32])
]
```

- `fill{color}` — remplit l'intérieur d'un maillage fermé
- `stroke{color, width}` — décrit le maillage avec un trait coloré
- `color{color}` — définit le contour et le remplissage
- `alpha{opacity}` — met à l'échelle la transparence du maillage ; s'applique à tout ce qui est à son droit
- `scale{factor}` — échelle uniforme ; ou `scale{[sx, sy, sz]}` par axe

## Opérateurs de positionnement

Les opérateurs de positionnement placent des maillages dans l'espace.

```mcl image

mesh demo = [
    center{1.5l + 0.5u} fill{soft{} BLUE} stroke{BLUE, 2} Triangle(0.45l + 0.35d, 0.45r + 0.35d, 0.45u),
    center{0r} rotate{PI / 4} fill{soft{} ORANGE} stroke{ORANGE, 2} Square(0.6),
    shift{1.5r} scale{1.2} fill{soft{} GREEN} stroke{GREEN, 2} RegularPolygon(3, 0.42)
]
```

- `center{position}` — déplace le maillage de sorte que son centre de boîte englobante soit à `position`
- `shift{delta}` — traduit le maillage par un décalage vectoriel
- `rotate{angle}` — tourne autour de l'axe Z (ou passe un vecteur d'axe complet)
- `in_space{origin, x_basis, y_basis}` — place le maillage dans un cadre de coordonnées local

## Opérateurs de mise en page

Les opérateurs de disposition positionnent un maillage par rapport à un autre, les coordonnées n'ont donc pas besoin d'être codées en dur.

```mcl image

let box = center{0.6l} fill{soft{} BLUE} stroke{BLUE, 2} Rect([1.2, 0.65])

mesh demo = [
    box,
    next_to{box, 1r, 0.25} stroke{ORANGE, 2} Capsule(0.45l, 0.45r, 0.24),
    to_side{1d, 0.2} color{GRAY} Text("caption", 0.55)
]
```

- `next_to{base, direction, spacing}` — positionne un maillage à côté de `base` le long de `direction`
- `to_side{direction, spacing}` — positionne un maillage près du bord du cadre visible

## Opérateurs personnalisés

Lorsque plusieurs maillages partagent une recette visuelle, extrayez-la dans un opérateur.

```mcl image

let soft_style = operator |target, col|
    fill{alpha{0.2} col}
    stroke{col, 2}
    target

mesh demo = [
    center{1.3l} soft_style{BLUE} Annulus(0.2, 0.45),
    center{0r} soft_style{ORANGE} Capsule(0.45l, 0.45r, 0.25),
    center{1.3r} soft_style{GREEN} RegularPolygon(5, 0.48)
]

slide "custom"
    play Grow(1)
```

## Arbres maillés
Rappelons qu’une liste d’arbres maillés est elle-même un arbre maillé. C'est ainsi que la plupart des diagrammes sont assemblés.

```mcl image

let Row = |items| block {
    for (i in range(0, len(items))) {
        let x = (i - (len(items) - 1) / 2) * 1.0
        let kind = mod(i, 3)
        var shape = []
        if (kind == 0) {
            shape = Capsule(0.3l, 0.3r, 0.18)
        } else if (kind == 1) {
            shape = RegularPolygon(5, 0.34)
        } else {
            shape = Annulus(0.16, 0.34)
        }
        . center{[x, 0, 0]} fill{soft{} items[i]} stroke{items[i], 2} shape
    }
}

mesh demo = Row([RED, ORANGE, YELLOW, GREEN, CYAN, BLUE])
```

Construire des maillages en boucles avec `block {}` et `.=` est le modèle standard pour les diagrammes basés sur les données.

## Balises et filtres

>IMPORTANT Les balises attachent une identité aux maillages en leur attribuant une liste de numéros (par défaut c'est `[]`). Les filtres et les opérateurs peuvent ensuite styliser ou isoler les sous-ensembles balisés.

```mcl image

let muted = |tags| not (2 in tags)

mesh demo =
    fill{soft{} LIGHT_GRAY, muted} # only applies to edge meshes!
    stroke{LIGHT_GRAY, 2, muted}
    [
        tag{1} center{1.2l} fill{soft{} BLUE} Circle(0.45),
        tag{2} center{0r} fill{soft{} ORANGE} Circle(0.45),
        tag{3} center{1.2r} fill{soft{} GREEN} Circle(0.45)
    ]

# selects a subset of the input
let edges = tag_filter{muted} demo
```

Le filtre `|tags| not (2 in tags)` correspond à tout fragment dont le jeu de balises n'inclut pas `2`. Les opérateurs `fill` et `stroke` utilisent ce filtre pour styliser uniquement les fragments muets, laissant le cercle orange en couleur.

Les balises deviennent importantes dans l’animation lorsque les pièces doivent conserver leur identité tout au long d’une transformation.

## Texte et TeX

`Text` et `Tex` produisent des maillages comme n'importe quel constructeur géométrique. Ils peuvent être stylisés, positionnés, étiquetés et animés avec les mêmes opérateurs et animations.

```mcl image
let palette = operator |target|
    color{BLUE, |tag| 1 in tag}
    color{ORANGE, |tag| 2 in tag}
    color{GREEN, |tag| 3 in tag}
    target

mesh formula = center{0.2u} palette{} Tex([
    text_tag{1} "a^2",
    " + ",
    "\text_tag{2}{b^2}",
    " = ",
    "\tag3{c^2}" # all three are valid ways to tagging tect
], 1.0)

mesh caption = center{0.8d} color{GRAY} Text("Pythagorean theorem", 0.55)

slide "formula"
    play [Write(1, [&formula]), Fade(0.8, [&caption])]
```

`text_tag{}` est le pendant textuel de `tag{}`. Il confère aux éléments individuels d'une formule une identité stable. Il se développe en interne en chaîne `\text_tag{tags}{text}` et est alias par `\tag1{}`, `\tag2{}`, etc.

## Exemples travaillés

### Tracé de fonction

```mcl image
let unit = 2
let f = |x| 0.65 * sin(x * PI)
let g = |x| 0.35 * cos(2 * x)
let graph_space = operator |target|
    in_space{0l, unit * 1r, unit * 1u}
    target

let sine = z_index{2} graph_space{} stroke{BLUE, 3} ExplicitFunc(f, [-1.5, 1.5, 120])
let cosine = z_index{2} graph_space{} stroke{ORANGE, 3} dashed{[0.12, 0.08]} ExplicitFunc(g, [-1.5, 1.5, 120])
let area =
    z_index{1}
    graph_space{}
    ExplicitFuncDiff(
        f,
        g,
        [-1.5, 1.5, 120],
        [alpha{0.22} BLUE, alpha{0.22} ORANGE],
        [[1], [2]]
    )

mesh plot = [
    axis_style{"x", -1.5, 1.5, nil, 1, 1}
    axis_style{"y", -1, 1, nil, 0.5, 2}
    Axis2d([unit * 1r, unit * 1u], BLACK, LIGHT_GRAY),
    area,
    sine,
    cosine,
    color{BLUE} center{1.5u + 1r} Tex("0.65 \sin(\pi x)", 0.55),
    color{ORANGE} center{1u + 0.5l} Tex("0.35 \cos(2x)", 0.55)
]
```

### Champ vectoriel

```mcl image
let palette = [0 -> BLUE, 0.45 -> CYAN, 0.75 -> ORANGE, 1 -> RED]

let Needle = |pos, idx| {
    let vx = sin(pos[1] * PI)
    let vy = -cos(pos[0] * PI)
    let strength = norm([vx, vy, 0])
    let col = keyframe_lerp(palette, strength / 1.42)
    return color{col} Vector(0.28 * [vx, vy, 0], pos)
}

mesh grid = stroke{LIGHT_GRAY, 1} LineGrid([-1.8, 1.8, 9], [-1.1, 1.1, 7])
mesh field = Field(Needle, [-1.8, 1.8, 10], [-1.1, 1.1, 7])
mesh center_dot = fill{BLACK} Dot(ORIGIN)
mesh title = center{1.45u} color{GRAY} Text("sampled vector field", 0.55)
```

### Surface 3D

```mcl image
let h = |x, y| 1.2 * (0.5 - (x - 0.5)^2 - (y - 0.5)^2)
let keys = [0 -> BLUE, 0.3 -> CYAN, 0.6 -> ORANGE, 1 -> RED]
let col_at = |pos, idx| keyframe_lerp(keys, h(pos[0], -pos[1]))

# here in space would just be a negation, so we do it manually
mesh surface =
    stroke{BLACK, 1}
    point_map{|p| [p[0], p[1], h(p[0], -p[1])]}
    ColorGrid(col_at, [0, 1, 18], [-1, 0, 18])

mesh wire =
    stroke{GRAY, 1}
    point_map{|p| [p[0], p[1], h(p[0], -p[1]) + 0.01]}
    LineGrid([0, 1, 7], [-1, 0, 7])

mesh axis =
    axis_style{"x", 0, 1, "x"}
    axis_style{"y", 0, 1, "y"}
    axis_style{"z", 0, 1, "z"}
    Axis3d(basis: [1r, 1d, 1b], color: BLACK, grid_color: LIGHT_GRAY, [1u, 1u, 1b])

mesh peak = color{RED} shift{pos:[0.5, -0.5, h(0.5, 0.5)]} Sphere(0.05)
    
camera = Camera([2.2, -2.1, 1.45], [0.5, -0.5, 0.35], [0, 0, 1])
```
