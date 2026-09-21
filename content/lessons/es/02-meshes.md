# Mallas y Operadores

Una malla es cualquier valor visible en una escena: un círculo, una línea de texto, una fórmula o una lista de cualquiera de ellos. La mayoría de los diagramas se construyen construyendo formas primitivas y aplicando operadores para diseñarlas y posicionarlas.

>IMPORTANTE La lista de mallas (o listas de listas de mallas anidadas) se denomina árbol de mallas. La mayoría de las funciones operan en árboles de malla operando en cada malla constituyente.

## Constructores

Los constructores crean geometría en el origen o cerca de él. Los operadores deciden dónde va la geometría y cómo se ve. La separación mantiene a los constructores simples y componibles.


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

Una lista de muchos constructores como referencia. Consulte la documentación para obtener más detalles y opciones.

- `Circle(radius)` — círculo relleno en el plano XY
- `Annulus(inner, outer)` — anillo lleno con un agujero
- `Square(width)` - cuadrado centrado
- `Rect([width, height])` — rectángulo alineado con el eje
- `Arrow(start, end)` — flecha dirigida con punta de flecha
- `Vector(delta, tail)` — vector en forma de flecha desde una cola por un delta
- `Line(start, end)` — segmento de línea simple
- `Polyline(vertices)` — camino abierto a través de una lista de puntos
- `Capsule(start, end, radius)` — forma de cápsula redondeada
- `RegularPolygon(n, circumradius)` - polígono de n lados
- `Arc(radius, [start_angle, end_angle])` - arco circular
- `Triangle(p, q, r)` - triángulo de tres puntos
- `LineGrid(x_bounds, y_bounds)` - cuadrícula de líneas rectangulares
- `ColorGrid(color_at, x_bounds, y_bounds)` — cuadrícula de colores de muestra
- `Axis1d(...)`, `Axis2d(...)`, `Axis3d(...)` - ejes de coordenadas
- `ExplicitFunc(func, [x_min, x_max, samples])` — curva de una función
- `Field(glyph_func, x_bounds, y_bounds)` — glifo repetido en cada punto de la cuadrícula
- `Label(target, string, direction)` — Etiqueta TeX colocada al lado de una malla
- `Text(string, size)` - texto renderizado
- `Tex(string_or_list, size)` - fórmula LaTeX renderizada
- `Dot(point)` — punto único de orientación de la pantalla

## Operadores de estilo

Los operadores transforman la malla a su derecha. Los operadores de estilo establecen propiedades visuales.

```mcl image

mesh demo = [
    center{1.4l} fill{soft{} BLUE} stroke{BLUE, 3} Annulus(0.22, 0.46),
    center{0r} fill{alpha{0.4} soft{} ORANGE} stroke{ORANGE, 3} Capsule(0.42l, 0.42r, 0.24),
    center{1.4r} color{GREEN} ExplicitFunc(|x| sin(x * TAU), [-0.5, 0.5, 32])
]
```

- `fill{color}` — llena el interior de una malla cerrada
- `stroke{color, width}` — delinea la malla con un trazo de color
- `color{color}` — establece trazo y relleno
- `alpha{opacity}` — escala la transparencia de la malla; se aplica a lo que está a su derecha
- `scale{factor}` — escala uniforme; o `scale{[sx, sy, sz]}` por eje

## Operadores de posicionamiento

Los operadores de posicionamiento colocan mallas en el espacio.

```mcl image

mesh demo = [
    center{1.5l + 0.5u} fill{soft{} BLUE} stroke{BLUE, 2} Triangle(0.45l + 0.35d, 0.45r + 0.35d, 0.45u),
    center{0r} rotate{PI / 4} fill{soft{} ORANGE} stroke{ORANGE, 2} Square(0.6),
    shift{1.5r} scale{1.2} fill{soft{} GREEN} stroke{GREEN, 2} RegularPolygon(3, 0.42)
]
```

- `center{position}` — mueve la malla para que el centro del cuadro delimitador esté en `position`
- `shift{delta}` — traduce la malla mediante un desplazamiento vectorial
- `rotate{angle}`: gira alrededor del eje Z (o pasa un vector de eje completo)
- `in_space{origin, x_basis, y_basis}` — coloca la malla en un marco de coordenadas local

## Operadores de diseño

Los operadores de diseño colocan una malla con respecto a otra, por lo que no es necesario codificar las coordenadas.

```mcl image

let box = center{0.6l} fill{soft{} BLUE} stroke{BLUE, 2} Rect([1.2, 0.65])

mesh demo = [
    box,
    next_to{box, 1r, 0.25} stroke{ORANGE, 2} Capsule(0.45l, 0.45r, 0.24),
    to_side{1d, 0.2} color{GRAY} Text("caption", 0.55)
]
```

- `next_to{base, direction, spacing}` — coloca una malla al lado de `base` a lo largo de `direction`
- `to_side{direction, spacing}`: coloca una malla cerca del borde del marco visible

## Operadores personalizados

Cuando varias mallas comparten una receta visual, extráigala en un operador.

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

## Árboles de malla
Recuerde que una lista de árboles de malla es en sí misma un árbol de malla. Así es como se ensamblan la mayoría de los diagramas.

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

Construir mallas en bucles con `block {}` y `.=` es el patrón estándar para diagramas basados ​​en datos.

## Etiquetas y filtros

>IMPORTANTE Las etiquetas adjuntan identidad a las mallas asignándoles una lista de números (por defecto es `[]`). Luego, los filtros y operadores pueden diseñar o aislar subconjuntos etiquetados.

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

El filtro `|tags| not (2 in tags)` coincide con cualquier fragmento cuyo conjunto de etiquetas no incluya `2`. Los operadores `fill` y `stroke` usan este filtro para aplicar estilo solo a los fragmentos silenciados, dejando el círculo naranja a todo color.

Las etiquetas se vuelven importantes en la animación cuando las piezas necesitan mantener su identidad durante una transformación.

## Texto y TeX

`Text` y `Tex` producen mallas como cualquier constructor geométrico. Se les puede diseñar, posicionar, etiquetar y animar con los mismos operadores y animaciones.

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

`text_tag{}` es la contraparte de texto de `tag{}`. Da a las piezas individuales de una fórmula una identidad estable. Se expande internamente a la cadena `\text_tag{tags}{text}` y tiene el alias `\tag1{}`, `\tag2{}`, etc.

## Ejemplos resueltos

### Gráfico de función

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

### Campo vectorial

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

### Superficie 3D

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
