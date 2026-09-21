# Conceptos básicos de animación

Las animaciones monocurl se crean a partir de cambios de estado. Mutas a los líderes; Las declaraciones `play` sincronizan a los seguidores con aquellos líderes con una estrategia elegida.

## Líder y seguidor

`mesh x = value` crea dos cosas:

- Un **líder**: la variable que su código lee y escribe, inicializada en `value`
- Un **seguidor**: lo que realmente dibuja la ventana gráfica, inicializado en `[]` (vacío)

Cambiar el líder en el código es instantáneo e invisible. Sólo una declaración `play` hace que el seguidor alcance al líder, y la animación elegida para `play` controla *cómo* llega allí.

La ventana gráfica en el editor muestra el estado del seguidor en cualquier momento. Cuando borras la línea de tiempo, te preguntas: "¿cómo se veían los seguidores en este momento de la ejecución?"

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

>IMPORTANTE Excepción especial: al final de **init**, todos los líderes de la malla se sincronizan automáticamente con sus seguidores. Es por eso que las mallas definidas en init ya son visibles cuando se abre una escena por primera vez.


## Colocar

`Set` copia instantáneamente cada líder sucio a su seguidor. Úselo para cortes de salto o para revelar un nuevo estado sin transición.

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

En general, las animaciones pueden inferir las mallas que desea animar al ver cuál ha cambiado. En escenarios más complejos, puede decir explícitamente las variables que desee con la sintaxis `play Set([&ball])`.

## lerp

`Lerp` interpola valores compatibles a lo largo del tiempo. "Compatible" generalmente significa que el líder y el seguidor son la misma llamada de función con la misma estructura, por lo que los argumentos se pueden interpolar individualmente. La definición exacta se puede encontrar en los documentos.

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

Debido a que `ball` se define como una llamada etiquetada a `Ball`, puede mutar campos individuales (`ball.pos`, `ball.radius`, `ball.col`) y luego `Lerp` interpola cada argumento independientemente de su valor anterior a su nuevo valor.

>IMPORTANTE Este es el patrón principal de animación de fotogramas clave en Monocurl: diseñar funciones constructoras con argumentos etiquetados y luego mutar campos específicos para definir cada estado de fotograma clave.

`Lerp` y otras animaciones aceptan un modificador `rate` opcional para facilitar:

```mcl
play Lerp(1.5, smooth)     # default: smooth S-curve
play Lerp(1.5, ease_out)   # decelerates
play Lerp(1.5, linear)     # constant speed
```

## Animaciones de introducción y cierre

Algunas animaciones están destinadas a mallas que aparecen o desaparecen. Comparan el estado seguidor actual con el nuevo estado líder y animan la diferencia.

**Escribir**: traza nuevos contornos como si los hubiera dibujado a mano. Lo mejor para curvas y texto.

```mcl video
slide "Write"
    mesh curve = stroke{BLUE, 3} ExplicitFunc(|x| sin(x * TAU), [-1.5, 1.5, 100])
mesh label = center{1.2d} color{BLUE} Text("sin(2πx)", 0.55)
    play Write(1.2)
    play Wait(0.8)
```

**Fundido**: desvanece las mallas hacia adentro o hacia afuera.
**Crecer**: expande la geometría desde su centro hacia afuera. Bueno para que aparezcan formas.

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

## Trans y etiquetaTrans

**Trans** es la transformación de malla de uso general. Transforma al seguidor actual en el líder actual haciendo coincidir contornos utilizando una función de costo. Se puede utilizar cuando `Lerp` no es posible (debido a diferentes estructuras de líder y seguidor).

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

**TagTrans** es una especialización de `Trans` que restringe la coincidencia con fragmentos con la misma etiqueta. Esto le brinda un control detallado sobre el proceso de coincidencia de contornos.

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

Cuando las etiquetas cambian de nombre entre estados, pasa `tag_map` a las listas de etiquetas de origen del grupo
con listas de etiquetas de destino. Afecta únicamente a la coincidencia; no reescribe la malla.

```mcl transcript
# full tag list [1, 2] matches full tag list [3, 4]
let one_to_one = [[1, 2] -> [3, 4]]
print one_to_one[[1, 2]]

# full tag lists [1] and [2] both match full tag list [3]
let many_to_one = [[[1], [2]] -> [[3]]]
print many_to_one
print many_to_one[[[1], [2]]]
```

## Lerp con operadores

`Lerp` no es sólo para argumentos de funciones etiquetadas. Las expresiones basadas en operadores también se pueden interpolar, porque los operadores conocen su propio estado de identidad.

Por ejemplo, interpolar de `x` a `rotate{angle} x` hace que la malla gire suavemente.

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

Lo mismo funciona para `shift`, `scale`, `color` y la mayoría de los demás operadores. El patrón general es: `mesh = operator{args} mesh` establece el líder en la versión operada y `Lerp` interpola desde el seguidor (no operado) al líder (operado).

También puedes encadenar esto para animar una secuencia de aplicaciones de operador:

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

## Elegir una animación

- **Establecer**: ajuste instantáneo; Úselo para cortes de salto y revelaciones iniciales.
- **Lerp** — interpolación suave; Usar cuando líder y seguidor tienen la misma estructura.
- **Escribir** — rastreo; Úselo para nuevas curvas, trazados y texto.
- **Crecer** — expansión; utilizar para nuevas formas rellenas
- **Desvanecimiento** — opacidad; Uso para geometría que aparece o desaparece.
- **Trans** — transformación general; utilizar cuando la estructura cambia significativamente
- **TagTrans** — transformación etiquetada; utilizar cuando varias piezas independientes necesitan cada una su propia contraparte
