# Animationsgrundlagen

Monocurl-Animationen werden aus Zustandsänderungen erstellt. Ihr mutiert Anführer; `play`-Anweisungen synchronisieren Follower mit den Führungskräften mit einer gewählten Strategie.

## Anführer und Anhänger

`mesh x = value` erstellt zwei Dinge:

- Ein **Leader** – die Variable, die Ihr Code liest und schreibt, initialisiert auf `value`
- Ein **Follower** – was das Ansichtsfenster tatsächlich zeichnet, initialisiert auf `[]` (leer)

Das Ändern des Anführers im Code erfolgt sofort und unsichtbar. Nur eine `play`-Anweisung sorgt dafür, dass der Follower den Anführer einholt, und die für `play` ausgewählte Animation steuert, *wie* er dorthin gelangt.

Das Ansichtsfenster im Editor zeigt den Follower-Status zu jedem Zeitpunkt an. Wenn Sie die Zeitleiste durchsuchen, fragen Sie sich: „Wie sahen die Follower in diesem Moment der Ausführung aus?“

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

>WICHTIGE Sonderausnahme: Am Ende von **init** werden alle Mesh-Leader automatisch mit ihren Followern synchronisiert. Aus diesem Grund sind in init definierte Meshes bereits sichtbar, wenn eine Szene zum ersten Mal geöffnet wird.


## Satz

`Set` kopiert jeden Dirty Leader sofort zu seinem Follower. Verwenden Sie es für Sprungschnitte oder um einen neuen Zustand ohne Übergang anzuzeigen.

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

Im Allgemeinen können Animationen auf die Netze schließen, die Sie animieren möchten, indem sie sehen, welches sich geändert hat. In komplexeren Szenarien können Sie die gewünschten Variablen explizit mit der Syntax `play Set([&ball])` angeben.

## Lerp

`Lerp` interpoliert kompatible Werte im Zeitverlauf. „Kompatibel“ bedeutet normalerweise, dass es sich bei Leader und Follower um denselben Funktionsaufruf mit derselben Struktur handelt, sodass Argumente einzeln interpoliert werden können. Die genaue Definition finden Sie in den Dokumenten.

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

Da `ball` als beschrifteter Aufruf von `Ball` definiert ist, können Sie einzelne Felder (`ball.pos`, `ball.radius`, `ball.col`) mutieren und dann `Lerp` jedes Argument unabhängig von seinem alten Wert auf seinen neuen Wert interpolieren.

>WICHTIG Dies ist das zentrale Keyframe-Animationsmuster in Monocurl: Entwerfen Sie Konstruktorfunktionen mit beschrifteten Argumenten und mutieren Sie dann bestimmte Felder, um jeden Keyframe-Status zu definieren.

`Lerp` und andere Animationen akzeptieren einen optionalen `rate`-Modifikator zur Erleichterung:

```mcl
play Lerp(1.5, smooth)     # default: smooth S-curve
play Lerp(1.5, ease_out)   # decelerates
play Lerp(1.5, linear)     # constant speed
```

## Intro- und Outro-Animationen

Einige Animationen sind für Netze gedacht, die erscheinen oder verschwinden. Sie vergleichen den aktuellen Follower-Status mit dem neuen Leader-Status und animieren den Unterschied.

**Schreiben** – zeichnet neue Konturen nach, als wären sie von Hand gezeichnet. Am besten für Kurven und Text geeignet.

```mcl video
slide "Write"
    mesh curve = stroke{BLUE, 3} ExplicitFunc(|x| sin(x * TAU), [-1.5, 1.5, 100])
mesh label = center{1.2d} color{BLUE} Text("sin(2πx)", 0.55)
    play Write(1.2)
    play Wait(0.8)
```

**Einblenden** – Blendet Meshes ein oder aus.
**Wachstum** – erweitert die Geometrie von der Mitte nach außen. Gut für das Erscheinen von Formen.

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

## Trans und TagTrans

**Trans** ist die allgemeine Netztransformation. Es verwandelt den aktuellen Follower in den aktuellen Leader, indem es Konturen mithilfe einer Kostenfunktion anpasst. Es kann verwendet werden, wenn `Lerp` nicht möglich ist (aufgrund unterschiedlicher Leader- und Follower-Strukturen).

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

**TagTrans** ist eine Spezialisierung von `Trans`, die den Abgleich auf Fragmente mit demselben Tag beschränkt. Dadurch erhalten Sie eine detaillierte Kontrolle über den Konturanpassungsprozess.

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

Wenn Tags ihren Namen zwischen den Zuständen ändern, übergeben Sie `tag_map` an die Gruppenquell-Tag-Listen
mit Ziel-Tag-Listen. Es wirkt sich nur auf den Abgleich aus; Das Netz wird nicht neu geschrieben.

```mcl transcript
# full tag list [1, 2] matches full tag list [3, 4]
let one_to_one = [[1, 2] -> [3, 4]]
print one_to_one[[1, 2]]

# full tag lists [1] and [2] both match full tag list [3]
let many_to_one = [[[1], [2]] -> [[3]]]
print many_to_one
print many_to_one[[[1], [2]]]
```

## Lerp mit Operatoren

`Lerp` gilt nicht nur für beschriftete Funktionsargumente. Operatorbasierte Ausdrücke können auch interpoliert werden, da Operatoren ihren eigenen Identitätsstatus kennen.

Wenn Sie beispielsweise von `x` nach `rotate{angle} x` interpolieren, dreht sich das Netz gleichmäßig.

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

Dasselbe funktioniert für `shift`, `scale`, `color` und die meisten anderen Operatoren. Das allgemeine Muster ist: `mesh = operator{args} mesh` setzt den Leader auf die betriebene Version und `Lerp` interpoliert vom Follower (nicht bedient) zum Leader (betrieben).

Sie können dies auch verketten, um eine Folge von Bedieneranwendungen zu animieren:

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

## Eine Animation auswählen

- **Set** – sofortiger Schnappschuss; Verwendung für Sprungschnitte und erste Laibungen
- **Lerp** – glatte Interpolation; verwenden, wenn Leader und Follower die gleiche Struktur haben
- **Schreiben** – Ablaufverfolgung; Für neue Kurven, Pfade und Texte verwenden
- **Wachstum** – Erweiterung; Für neue gefüllte Formen verwenden
- **Fade** – Deckkraft; Verwendung zum Erscheinen oder Verschwinden von Geometrie
- **Trans** – allgemeines Morphing; Verwendung, wenn sich die Struktur erheblich ändert
- **TagTrans** – getaggtes Morphing; Verwenden Sie es, wenn mehrere unabhängige Teile jeweils ein eigenes Gegenstück benötigen
