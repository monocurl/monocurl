# Netze und Operatoren

Ein Netz ist jeder sichtbare Wert in einer Szene – ein Kreis, eine Textzeile, eine Formel oder eine Liste davon. Die meisten Diagramme werden erstellt, indem man primitive Formen konstruiert und Operatoren anwendet, um sie zu formatieren und zu positionieren.

>WICHTIG Eine Liste von Maschen (oder Listen verschachtelter Listen von Maschen) wird als Maschenbaum bezeichnet. Die meisten Funktionen arbeiten mit Maschenbäumen, indem sie jedes konstituierende Netz bearbeiten.

## Konstrukteure

Konstrukteure erstellen Geometrie am oder in der Nähe des Ursprungs. Bediener entscheiden, wohin die Geometrie geht und wie sie aussieht. Durch die Trennung bleiben Konstrukteure einfach und zusammensetzbar.


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

Eine Liste vieler Konstruktoren als Referenz. Weitere Details und Optionen finden Sie in der Dokumentation.

- `Circle(radius)` – gefüllter Kreis in der XY-Ebene
- `Annulus(inner, outer)` – gefüllter Ring mit einem Loch
- `Square(width)` – zentriertes Quadrat
- `Rect([width, height])` – achsenausgerichtetes Rechteck
- `Arrow(start, end)` – gerichteter Pfeil mit Pfeilspitze
- `Vector(delta, tail)` – pfeilartiger Vektor von einem Schwanz durch ein Delta
- `Line(start, end)` – einfaches Liniensegment
- `Polyline(vertices)` – offener Pfad durch eine Liste von Punkten
- `Capsule(start, end, radius)` – abgerundete Kapselform
- `RegularPolygon(n, circumradius)` – n-seitiges Polygon
- `Arc(radius, [start_angle, end_angle])` – Kreisbogen
- `Triangle(p, q, r)` – Dreipunktdreieck
- `LineGrid(x_bounds, y_bounds)` – rechteckiges Liniengitter
- `ColorGrid(color_at, x_bounds, y_bounds)` – abgetastetes farbiges Raster
- `Axis1d(...)`, `Axis2d(...)`, `Axis3d(...)` – Koordinatenachsen
- `ExplicitFunc(func, [x_min, x_max, samples])` – Kurve aus einer Funktion
- `Field(glyph_func, x_bounds, y_bounds)` – sich wiederholendes Glyph an jedem Rasterpunkt
- `Label(target, string, direction)` – TeX-Label neben einem Netz platziert
- `Text(string, size)` – gerenderter Text
- `Tex(string_or_list, size)` – gerenderte LaTeX-Formel
- `Dot(point)` – einzelner, dem Bildschirm zugewandter Punkt

## Styling-Operatoren

Operatoren transformieren das Netz zu ihrer Rechten. Styling-Operatoren legen visuelle Eigenschaften fest.

```mcl image

mesh demo = [
    center{1.4l} fill{soft{} BLUE} stroke{BLUE, 3} Annulus(0.22, 0.46),
    center{0r} fill{alpha{0.4} soft{} ORANGE} stroke{ORANGE, 3} Capsule(0.42l, 0.42r, 0.24),
    center{1.4r} color{GREEN} ExplicitFunc(|x| sin(x * TAU), [-0.5, 0.5, 32])
]
```

- `fill{color}` – füllt das Innere eines geschlossenen Netzes
- `stroke{color, width}` – umreißt das Netz mit einem farbigen Strich
- `color{color}` – legt Strich und Füllung fest
- `alpha{opacity}` – skaliert die Transparenz des Netzes; gilt für alles, was sich rechts davon befindet
- `scale{factor}` – einheitliche Skala; oder `scale{[sx, sy, sz]}` pro Achse

## Positionierungsoperatoren

Positionierungsoperatoren platzieren Netze im Raum.

```mcl image

mesh demo = [
    center{1.5l + 0.5u} fill{soft{} BLUE} stroke{BLUE, 2} Triangle(0.45l + 0.35d, 0.45r + 0.35d, 0.45u),
    center{0r} rotate{PI / 4} fill{soft{} ORANGE} stroke{ORANGE, 2} Square(0.6),
    shift{1.5r} scale{1.2} fill{soft{} GREEN} stroke{GREEN, 2} RegularPolygon(3, 0.42)
]
```

- `center{position}` – verschiebt das Netz so, dass die Mitte des Begrenzungsrahmens bei `position` liegt.
- `shift{delta}` – verschiebt das Netz um einen Vektorversatz
- `rotate{angle}` – dreht sich um die Z-Achse (oder übergibt einen vollständigen Achsenvektor)
- `in_space{origin, x_basis, y_basis}` – platziert das Netz in einem lokalen Koordinatenrahmen

## Layout-Operatoren

Layout-Operatoren positionieren ein Netz relativ zu einem anderen, sodass die Koordinaten nicht fest codiert werden müssen.

```mcl image

let box = center{0.6l} fill{soft{} BLUE} stroke{BLUE, 2} Rect([1.2, 0.65])

mesh demo = [
    box,
    next_to{box, 1r, 0.25} stroke{ORANGE, 2} Capsule(0.45l, 0.45r, 0.24),
    to_side{1d, 0.2} color{GRAY} Text("caption", 0.55)
]
```

- `next_to{base, direction, spacing}` – positioniert ein Netz neben `base` entlang `direction`
- `to_side{direction, spacing}` – positioniert ein Netz nahe der Kante des sichtbaren Rahmens

## Benutzerdefinierte Operatoren

Wenn mehrere Netze ein visuelles Rezept teilen, extrahieren Sie es in einen Operator.

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

## Mesh-Bäume
Denken Sie daran, dass eine Liste von Mesh-Bäumen selbst ein Mesh-Baum ist. Auf diese Weise werden die meisten Diagramme zusammengestellt.

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

Das Erstellen von Netzen in Schleifen mit `block {}` und `.=` ist das Standardmuster für datengesteuerte Diagramme.

## Tags und Filter

>WICHTIG Tags verleihen Netzen Identität, indem sie ihnen eine Liste von Nummern zuweisen (standardmäßig `[]`). Filter und Operatoren können dann mit Tags versehene Teilmengen formatieren oder isolieren.

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

Der Filter `|tags| not (2 in tags)` stimmt mit jedem Fragment überein, dessen Tag-Satz `2` nicht enthält. Die Operatoren `fill` und `stroke` verwenden diesen Filter, um nur die stummgeschalteten Fragmente zu formatieren und den orangefarbenen Kreis in voller Farbe zu belassen.

Tags werden in der Animation wichtig, wenn Teile während einer Transformation ihre Identität behalten müssen.

## Text und TeX

`Text` und `Tex` erzeugen Netze wie jeder geometrische Konstruktor. Sie können mit denselben Operatoren und Animationen gestaltet, positioniert, markiert und animiert werden.

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

`text_tag{}` ist das Textgegenstück zu `tag{}`. Es verleiht einzelnen Teilen einer Formel eine stabile Identität. Es wird intern auf die Zeichenfolge `\text_tag{tags}{text}` erweitert und erhält einen Alias ​​durch `\tag1{}`, `\tag2{}` usw.

## Ausgearbeitete Beispiele

### Funktionsplot

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

### Vektorfeld

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

### 3D-Oberfläche

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
