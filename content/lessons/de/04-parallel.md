# Parallele Animationen

Bisher führt jede `play`-Anweisung eine Animation vollständig aus, bevor die nächste beginnt. In dieser Lektion erfahren Sie, wie Sie Animationen gleichzeitig ausführen, wiederverwendbare Animationshelfer erstellen und wann explizite Referenzen übergeben werden.

## Parallel spielen

Durch die Übergabe einer Liste an `play` werden alle Elemente gleichzeitig ausgeführt. Die Szene wartet, bis jeder Zweig abgeschlossen ist.

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

Jeder Zweig sollte sein eigenes Mesh besitzen. Wenn mehrere Animationen gleichzeitig auf demselben Netz ausgeführt werden, führt dies zu einem Laufzeitfehler.

## Animationsblöcke

`anim {}` erstellt einen Animationsblock – eine verzögerte Folge von Code und `play`-Anweisungen. Wenn der Block definiert ist, führt er keine Aktion aus; Es wird nur ausgeführt, wenn es an `play` übergeben wird.

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

Animationsblöcke verhalten sich wie Coroutinen (in anderen Sprachen auch Versprechen genannt): Beim Abspielen werden sie Schritt für Schritt ausgeführt und geben bei jeder `play`-Anweisung nach. Aus diesem Grund erscheint `print` innerhalb eines `anim {}`-Blocks erst im Transkript, nachdem der Abspielkopf diese Zeile passiert hat.

## Fortschritter

Ein **Progressor** ist eine Redewendung, bei der ein Lambda einen Anführer durch Referenzen akzeptiert und ihn in einem Animationsblock mutiert. Die `&`-Syntax übergibt eine Referenz und keine Kopie.

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

`MoveBy` nimmt `&ball` (eine Referenz auf den Anführer) und `delta` an. Innerhalb des Blocks mutiert `m = shift{delta} m` den Anführer durch die Referenz. Wenn `MoveBy` als Teil der Parallelliste abgespielt wird, wird es gleichzeitig mit `pulse_ring` ausgeführt.

Mit Progressoren können Sie den Szenenstatus parallel zu Animationen ändern.

## Explizite Referenzen

Die Animations-Engine ermittelt normalerweise, welche Anführer schmutzig sind und animiert werden sollten. Aber es ist aggressiv und animiert alle schmutzigen Anführer zusammen. In zwei Fällen sollten Sie explizit sein:

**1. Sie haben einen Anführer geändert, möchten ihn aber nicht in dieser Animation haben.** Übergeben Sie `[&specific_leader]`, um die Animation nur auf diesen Anführer zu beschränken.

**2. Parallele Zweige ändern beide die zugehörige Geometrie.** Durch die eindeutige Angabe, welcher Zweig welche Führungslinie besitzt, wird eine versehentliche Beeinträchtigung verhindert.

```mcl
# Without explicit refs, Lerp would animate both ball and label together
ball.pos = 1.4r
label = next_to{ball, 1d, 0.2} Text("moved", 0.55)

play [
    Lerp(1.2, [&ball]),    # ball moves smoothly
    Set([&label])           # label snaps to new position instantly
]
```

## Verzögerung

Der Modifikator `delay{}` führt einen Offset aus, wenn eine Animation innerhalb eines Parallelblocks beginnt.

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

Versuchen Sie, sich vorzustellen, wie eine Verzögerung umgesetzt werden könnte.

## Komplettlösung: 3D-Kameraanimation

Um zu sehen, wie diese Muster zusammenarbeiten, betrachten Sie das Beispiel einer 3D-Kameraanimation, das im Lieferumfang von Monocurl enthalten ist. Hier erfahren Sie, wie Sie über den Bau nachdenken würden.

Das Ziel ist: ein flaches farbiges Gitter zu zeigen, es dann gleichzeitig auf eine Oberfläche zu heben und die Kamera darum zu kreisen.

**Schritt 1 – Erstellen Sie den Anfangszustand in init.**

Das Raster beginnt flach, die Kamera startet an der Standardposition und blickt auf den Ursprung. Definieren Sie eine Farbfunktion und richten Sie die Netze ein.

```mcl
let samples = 24
let height = |x, y| 1.15 * ((x - 0.5)^2 + (y - 0.5)^2)
let color_keys = [0 -> BLUE, 0.15 -> YELLOW, 0.3 -> ORANGE, 0.55 -> RED]

let color_at = |pos, idx| keyframe_lerp(color_keys, height(pos[0], -pos[1]))
```

**Schritt 2 – Erste Folie: Enthüllen Sie die erste Szene.**

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

**Schritt 3 – Zweite Folie: Heben Sie das Gitter an und bewegen Sie die Kamera parallel.**

Jede Aktion ist ein `anim {}`-Block mit eigenen internen Schritten. Sie laufen zusammen in einem `play [...]`.

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

Die wichtigste Erkenntnis: `lift_grid` und `move_camera` sind völlig unabhängig. `lift_grid` besitzt `grid`; `move_camera` besitzt `camera`. Da sie keine gemeinsamen Leiter haben, können sie ohne Beeinträchtigung parallel laufen.

`CameraLerp` ist eine spezielle Animation für Kamerabewegungen, die optisch ansprechendere Bögen erzeugt als einfaches `Lerp` für Kamerapositionen.

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
