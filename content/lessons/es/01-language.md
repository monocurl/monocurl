# Conceptos básicos del idioma

Esta lección es una referencia. Mire las secciones importantes, pero no es necesario que memorice todo aquí antes de construir algo. Vuelve cuando encuentres una característica que no hayas visto antes.


## Fijaciones

Hay tres formas de vinculación de usuarios:

- `let` — nombre inmutable; no puede ser reasignado
- `var` — valor local mutable
- `mesh` — estado de escena visible; crea un par líder/seguidor (tratado en profundidad en la lección de Animación)

```mcl
let radius = 0.45
var count = 0
mesh dot = fill{CYAN} Circle(radius)
```

## Escritura dinámica

Monocurl se escribe dinámicamente. Una variable puede contener cualquier valor y puede reasignarse a un tipo diferente.

```mcl transcript
var x = 7
print type_of(x)
x = "hello"
print x
x = [1, 2, 3]
print [type_of(x), len(x)]
```

Utilice `print` durante la creación para inspeccionar valores. La salida de impresión aparece en línea en el editor y en la consola (alternando a la línea de tiempo).

## Cuerdas y escapes

Las cadenas monocurl utilizan `%` como marcador de escape, no `\`. Esto es para que LaTeX siga siendo fácil de escribir: las barras invertidas son caracteres de cadena normales.

```mcl 
let newline = "first%nsecond"
let quoted = "say %"hello%""
let percent = "100%%"
```
## Copia profunda

La tarea siempre realiza una **copia profunda**.

```mcl transcript
var a = [[1, 2], [3, 4]]
var b = a
b[0][0] = 99
print [a, b]
```

## Literales de dirección

Monocurl tiene literales vectoriales compactos para direcciones comunes, que a menudo se usan para posicionamiento.

```mcl transcript
let p = 1.5r + 0.8u       # [1.5, 0.8, 0]
let q = 2l + 1d           # [-2, -1, 0]
let behind = 3b           # [0, 0, 3]
print p
print q
print behind
```

- `l` / `r` — izquierda / derecha (x negativa / positiva)
- `u` / `d` — arriba / abajo (y positivo / negativo)
- `b` / `f` — atrás / adelante (z positiva / negativa)

Los literales de dirección son sólo listas de tres elementos. Puede pasarlos a `shift{}` o `center{}` y usarlos en cualquier lugar donde se espere un vector.

## Listas y mapas

Las listas utilizan indexación de base cero. Los mapas utilizan pares `key -> value`.

>IMPORTANTE Agregar es una operación común admitida por los operadores `..` y `.=`.

```mcl transcript
var pts = []
pts .= 1l
pts .= 1u
pts = pts .. 1r      # .. appends; .= is shorthand for x = x .. y

let labels = ["origin" -> ORIGIN, "right" -> 1r, "up" -> 1u]
print [pts, labels["origin"]]
```

## Sintaxis de bloque

`block {}` es una expresión de varias líneas que acumula una lista. Las líneas que comienzan con `.` se agregan a un valor de retorno implícito; todo el bloque se evalúa según esa lista.

```mcl
let row = block {
    for (i in range(0, 4)) {
        . center{[i - 1.5, 0, 0]} fill{CYAN} Square(0.4)
    }
    . center{2r} color{ORANGE} Text("end", 0.55)
}
```

## Lambdas y argumentos etiquetados

Las funciones son lambdas, que se tratan de primera clase. Los argumentos pueden tener valores predeterminados y un cuerpo arriostrado puede usar `return`.

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

>IMPORTANTE **Los argumentos etiquetados** son la característica que impulsa el sistema de animación de Monocurl. Cuando llamas a una función con argumentos con nombre, el resultado recuerda esas etiquetas y puede modificarse campo por campo más adelante. Luego se reevalúa toda la llamada con los nuevos argumentos.

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

Ésta no es una mutación de objeto en el sentido habitual. `ball.pos = 1.4r` edita el argumento etiquetado almacenado dentro de la invocación en vivo `Ball(...)`, luego toda la llamada se vuelve a ejecutar con el nuevo argumento. Así es como `Lerp` sabe cómo interpolar suavemente entre los dos estados: interpola cada argumento de forma independiente y reconstruye la malla en cada cuadro.

## Operadores

>IMPORTANTE Los operadores son funciones que escriben antes de su objetivo en lugar de alrededor de él. Forman un **pipeline**: cada operador recibe y transforma la malla a su derecha. Interactúan muy bien con el sistema de atributos.

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

Leer de derecha a izquierda: `Circle(0.5)` crea la geometría; `stroke`, `fill` y `center` lo transforman en secuencia. Puedes definir tus propios operadores con `operator` en términos de los existentes:

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

Los operadores integrados manejan el estilo (`fill`, `stroke`, `color`, `alpha`), el posicionamiento (`center`, `shift`, `rotate`, `scale`), el diseño (`next_to`, `to_side`) y la identidad (`tag`).

Los operadores también llevan semántica de animación, que se discutirá más adelante.

## Controlar el flujo

Flujo de control estándar: `if`/`else if`/`else`, `for`, `while`, `break`, `continue`. Úsalos para construir datos antes de convertirlos en mallas.

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

Las lambdas recursivas se toman a sí mismas como un argumento `self` explícito. Esto es estándar en el cálculo lambda, pero puede parecer extraño si no lo ha visto antes.

```mcl transcript
let fib = |self, n| {
    if (n <= 1) { return n }
    return self(self, n - 1) + self(self, n - 2)
}
print fib(fib, 8)
```

## Poniéndolo junto

Una función auxiliar típica crea una malla etiquetada y utiliza operadores para diseñar. Los argumentos etiquetados se convierten en campos de fotogramas clave de animación.

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
