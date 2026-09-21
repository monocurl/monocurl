# Animaciones paralelas

Hasta ahora, cada instrucción `play` ejecuta una animación hasta su finalización antes de que comience la siguiente. Esta lección cubre cómo ejecutar animaciones simultáneamente, cómo crear ayudas de animación reutilizables y cuándo pasar referencias explícitas.

## Jugando en paralelo

Pasar una lista a `play` ejecuta todos los elementos simultáneamente. La escena espera hasta que termine cada rama.

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

Cada rama debe poseer su propia malla. Tener varias animaciones simultáneas operando en la misma malla provocará un error de tiempo de ejecución.

## Bloques de animación

`anim {}` crea un bloque de animación: una secuencia diferida de código y declaraciones `play`. El bloque no hace nada cuando está definido; se ejecuta solo cuando se pasa a `play`.

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

Los bloques de animación se comportan como corrutinas (también llamadas promesas en otros idiomas): cuando se reproducen, se ejecutan paso a paso, cediendo en cada declaración `play`. Es por eso que `print` dentro de un bloque `anim {}` solo aparece en la transcripción después de que el cabezal de reproducción pasa esa línea.

## Progresores

Un **progresor** es un modismo en el que una lambda acepta un líder por referencias y lo muta en un bloque de animación. La sintaxis `&` pasa una referencia en lugar de una copia.

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

`MoveBy` toma `&ball` (una referencia al líder) y `delta`. Dentro del bloque, `m = shift{delta} m` muta el líder a través de la referencia. Cuando `MoveBy` se reproduce como parte de la lista paralela, se ejecuta simultáneamente con `pulse_ring`.

Los progresores le permiten modificar el estado de la escena en paralelo con las animaciones.

## Referencias explícitas

El motor de animación normalmente infiere qué líderes están sucios y deberían animarse. Pero es agresivo y anima a todos los líderes sucios a unirse. En dos casos debes ser explícito:

**1. Cambiaste un líder pero no lo quieres en esta animación.** Pase `[&specific_leader]` para limitar la animación solo a ese líder.

**2. Las ramas paralelas modifican la geometría relacionada.** Ser explícito sobre qué rama posee qué líder evita interferencias accidentales.

```mcl
# Without explicit refs, Lerp would animate both ball and label together
ball.pos = 1.4r
label = next_to{ball, 1d, 0.2} Text("moved", 0.55)

play [
    Lerp(1.2, [&ball]),    # ball moves smoothly
    Set([&label])           # label snaps to new position instantly
]
```

## Demora

El modificador `delay{}` se compensa cuando comienza una animación dentro de un bloque paralelo.

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

Vea si puede pensar en cómo se podría implementar el retraso.

## Tutorial: Animación de cámara 3D

Para ver estos patrones trabajando juntos, considere el ejemplo de animación de cámara 3D que se incluye con Monocurl. Así es como razonarías al construirlo.

El objetivo es: mostrar una cuadrícula de color plana, luego levantarla simultáneamente hacia una superficie y orbitar la cámara alrededor de ella.

**Paso 1: construye el estado inicial en init.**

La cuadrícula comienza plana, la cámara comienza en la posición predeterminada mirando al origen. Defina una función de color y configure las mallas.

```mcl
let samples = 24
let height = |x, y| 1.15 * ((x - 0.5)^2 + (y - 0.5)^2)
let color_keys = [0 -> BLUE, 0.15 -> YELLOW, 0.3 -> ORANGE, 0.55 -> RED]

let color_at = |pos, idx| keyframe_lerp(color_keys, height(pos[0], -pos[1]))
```

**Paso 2: Primera diapositiva: revela la escena inicial.**

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

**Paso 3: Segunda diapositiva: levante la rejilla y mueva la cámara en paralelo.**

Cada acción es un bloque `anim {}` con sus propios pasos internos. Corren juntos en un `play [...]`.

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

La idea clave: `lift_grid` y `move_camera` son completamente independientes. `lift_grid` posee `grid`; `move_camera` es propietario de `camera`. Como no comparten líderes, pueden funcionar en paralelo sin interferencias.

`CameraLerp` es una animación especializada para el movimiento de la cámara que produce arcos más agradables visualmente que la simple `Lerp` para las posiciones de la cámara.

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
    play Wait(0.4)
```
