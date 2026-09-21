# Sprachgrundlagen

Diese Lektion ist eine Referenz. Schauen Sie sich die wichtigen Abschnitte an, aber Sie müssen sich hier nicht alles merken, bevor Sie etwas bauen. Kommen Sie zurück, wenn Sie auf eine Funktion stoßen, die Sie zuvor noch nicht gesehen haben.


## Bindungen

Es gibt drei Formen der Benutzerbindung:

- `let` – unveränderlicher Name; kann nicht neu zugewiesen werden
- `var` – veränderlicher lokaler Wert
- `mesh` – sichtbarer Szenenstatus; erstellt ein Leader/Follower-Paar (ausführlich behandelt in der Animationslektion)

```mcl
let radius = 0.45
var count = 0
mesh dot = fill{CYAN} Circle(radius)
```

## Dynamisches Tippen

Monocurl wird dynamisch getippt. Eine Variable kann einen beliebigen Wert enthalten und einem anderen Typ zugewiesen werden.

```mcl transcript
var x = 7
print type_of(x)
x = "hello"
print x
x = [1, 2, 3]
print [type_of(x), len(x)]
```

Verwenden Sie `print` während der Erstellung, um Werte zu überprüfen. Die Druckausgabe erscheint inline im Editor und in der Konsole (alternativ zur Timeline).

## Strings und Escapes

Monocurl-Strings verwenden `%` als Escape-Marker, nicht `\`. Dadurch bleibt LaTeX einfach zu schreiben: Backslashes sind gewöhnliche Zeichenfolgenzeichen.

```mcl 
let newline = "first%nsecond"
let quoted = "say %"hello%""
let percent = "100%%"
```
## Tiefe Kopie

Die Zuweisung führt immer eine **tiefe Kopie** durch.

```mcl transcript
var a = [[1, 2], [3, 4]]
var b = a
b[0][0] = 99
print [a, b]
```

## Richtungsliterale

Monocurl verfügt über kompakte Vektorliterale für gemeinsame Richtungen, die häufig zur Positionierung verwendet werden.

```mcl transcript
let p = 1.5r + 0.8u       # [1.5, 0.8, 0]
let q = 2l + 1d           # [-2, -1, 0]
let behind = 3b           # [0, 0, 3]
print p
print q
print behind
```

- `l` / `r` – links / rechts (negatives / positives x)
- `u` / `d` – nach oben / nach unten (positives / negatives y)
- `b` / `f` – rückwärts / vorwärts (positives / negatives z)

Richtungsliterale sind lediglich Listen mit drei Elementen. Sie können sie an `shift{}` oder `center{}` übergeben und überall dort verwenden, wo ein Vektor erwartet wird.

## Listen und Karten

Listen verwenden eine nullbasierte Indizierung. Karten verwenden `key -> value`-Paare.

>WICHTIG Das Anhängen ist ein allgemeiner Vorgang, der durch die Operatoren `..` und `.=` unterstützt wird.

```mcl transcript
var pts = []
pts .= 1l
pts .= 1u
pts = pts .. 1r      # .. appends; .= is shorthand for x = x .. y

let labels = ["origin" -> ORIGIN, "right" -> 1r, "up" -> 1u]
print [pts, labels["origin"]]
```

## Blocksyntax

`block {}` ist ein mehrzeiliger Ausdruck, der eine Liste akkumuliert. Zeilen, die mit `.` beginnen, werden an einen impliziten Rückgabewert angehängt; Der gesamte Block wird anhand dieser Liste ausgewertet.

```mcl
let row = block {
    for (i in range(0, 4)) {
        . center{[i - 1.5, 0, 0]} fill{CYAN} Square(0.4)
    }
    . center{2r} color{ORANGE} Text("end", 0.55)
}
```

## Lambdas und beschriftete Argumente

Funktionen sind Lambdas, die erstklassig behandelt werden. Argumente können Standardwerte haben und ein geschweifter Körper kann `return` verwenden.

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

>WICHTIG **Beschriftete Argumente** sind die Funktion, die das Animationssystem von Monocurl antreibt. Wenn Sie eine Funktion mit benannten Argumenten aufrufen, merkt sich das Ergebnis diese Beschriftungen und kann später Feld für Feld geändert werden. Der gesamte Aufruf wird dann mit den neuen Argumenten neu ausgewertet.

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

Dabei handelt es sich nicht um eine Objektmutation im üblichen Sinne. `ball.pos = 1.4r` bearbeitet das beschriftete Argument, das im Live-Aufruf von `Ball(...)` gespeichert ist, dann wird der gesamte Aufruf mit dem neuen Argument erneut ausgeführt. Auf diese Weise kann `Lerp` reibungslos zwischen den beiden Zuständen interpolieren: Es interpoliert jedes Argument unabhängig und rekonstruiert das Netz bei jedem Frame.

## Betreiber

>WICHTIG Operatoren sind Funktionen, die vor ihrem Ziel schreiben und nicht darum herum. Sie bilden eine **Pipeline**: Jeder Operator empfängt und transformiert das Netz auf seiner rechten Seite. Sie interagieren gut mit dem Attributsystem.

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

Von rechts nach links lesen: `Circle(0.5)` erstellt die Geometrie; `stroke`, `fill` und `center` transformieren es nacheinander. Sie können mit `operator` Ihre eigenen Operatoren in Bezug auf vorhandene definieren:

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

Integrierte Operatoren kümmern sich um Stil (`fill`, `stroke`, `color`, `alpha`), Positionierung (`center`, `shift`, `rotate`, `scale`), Layout (`next_to`, `to_side`) und Identität (`tag`).

Operatoren tragen auch Animationssemantik, die später besprochen wird.

## Kontrollfluss

Standardkontrollfluss: `if`/`else if`/`else`, `for`, `while`, `break`, `continue`. Verwenden Sie sie, um Daten zu erstellen, bevor Sie sie in Netze umwandeln.

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

Rekursive Lambdas verstehen sich selbst als explizites `self`-Argument. Dies ist in der Lambda-Rechnung Standard, sieht aber möglicherweise seltsam aus, wenn Sie es noch nie gesehen haben.

```mcl transcript
let fib = |self, n| {
    if (n <= 1) { return n }
    return self(self, n - 1) + self(self, n - 2)
}
print fib(fib, 8)
```

## Zusammensetzen

Eine typische Hilfsfunktion erstellt ein beschriftetes Netz und verwendet Operatoren zum Stylen. Die beschrifteten Argumente werden zu Animations-Keyframe-Feldern.

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
