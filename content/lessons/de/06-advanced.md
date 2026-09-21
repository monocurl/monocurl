# Fortgeschrittene Themen

Diese Lektion behandelt Themen, die für die alltägliche Szenenerstellung nicht notwendig sind, aber nützlich sind, wenn Sie das Verhalten von Monocurl anpassen oder verstehen möchten, was unter der Haube passiert.

## Benutzerdefinierte Bibliotheken

Sie können allgemeine Definitionen in eine Bibliotheksdatei extrahieren. Eine Bibliotheksdatei ist fast wie eine Szenendatei aufgebaut, allerdings wird nur der Bibliotheksteil in andere Dateien importiert. Der Hauptzweck einer Bibliotheksdatei besteht darin, Hilfsfunktionen, benutzerdefinierte Operatoren, Konstanten und wiederverwendbare Netzdefinitionen bereitzustellen, die in mehreren Szenen verwendet werden können.

Sie können Ihre Bibliotheksdatei mit dem Schlüsselwort `import` in eine Szenendatei importieren. Der Importpfad ist relativ zur Importdatei und sollte nicht die Erweiterung `.mcs` enthalten.

Wenn eine Datei importiert wird, importiert Monocurl nur den Code vor dem ersten Schlüsselwort `slide` dieser Datei. Jede Folie nach dem ersten `slide` wird für Importzwecke ignoriert, einschließlich aller Importe, die nach dieser ersten Folie erscheinen.

Dadurch kann eine Hilfsdatei eine kleine Demoszene unterhalb ihrer Bibliotheksdefinitionen behalten und ist nützlich, um die Bibliotheksdatei isoliert zu bearbeiten.

```mcl transcript
let Double = |x| 2 * x

slide "demo"
    print Double(4)
```

Eine andere Datei, die sie importiert, kann `Double` verwenden, aber die Demofolie wird nicht in die Importszene kompiliert.

## Zustandsbehaftete Werte und Parameter

Zustandsbehaftete Werte sind Werte, die vom Szenenstatus abhängen und kontinuierlich aktualisiert werden. Der Hauptort für zustandsbehaftete Werte sind kamerabewusste Overlays. `camera_transfer{camera, $camera}` hält ein Netz relativ zum Rahmen fest, während sich die Kamera bewegt (lesen Sie die Dokumentation für eine vollständige Erklärung), und `orient_to_camera{$camera}` dreht kontinuierlich ebene Netzbäume in Richtung der Kamera. Abgesehen davon sollte jedoch Stateful vermieden werden und es ist idiomatischer, den Meshes Attribute zu geben, die Sie explizit aktualisieren.

```mcl video
mesh fully_fixed =
    camera_transfer{camera, $camera}
    to_side{1u, 0.2}
    Text("fixed in frame")
mesh rotationally_fixed = 
    orient_to_camera{$camera}
    to_side{1d, 0.2}
    Text("rotationally fixed")
mesh not_fixed = Text("not fixed")

slide "Camera"
    camera = Camera([2, 1, 5])
    play CameraLerp(&camera, 1.2)
```

Die Zustandsquelle ist `param`, die ähnlich wie `mesh` funktioniert: Sie hat einen Leader-Wert, der vom Code bearbeitet wird, und einen Follower-Wert, auf den die Animation abgespielt wird. Parameter der obersten Ebene werden im Präsentationsmodus auch als interaktive Steuerelemente angezeigt.

Die gebräuchlichsten Parameter sind `camera` und `background`, die etwas Besonderes sind, da sie sich tatsächlich auf die visuelle Szene auswirken. Andere Parameter werden normalerweise zur Steuerung von Netzen verwendet.

Beim normalen Lesen eines Parameters, z. B. `radius`, wird dessen aktueller Leader-Wert gelesen. Durch das Lesen mit `$`, z. B. `$radius`, wird ein zustandsbehafteter Verweis auf den Live-Wert erstellt. Sie können es sich so vorstellen, als würde der Ausdruck ständig neu ausgewertet, wenn sich der Wert ändert (obwohl dies in der Praxis effizienter ist).

Sie können Meshes nur Zustandswerte zuweisen. Es gibt begrenzte Vorgänge, die Sie mit ihnen durchführen können. Sie dürfen sie als Funktionsargumente, Operatorargumente und in Listen verwenden. In den meisten Fällen können Sie auch auf Attribute zugreifen. Wenn Sie einem Netz einen zustandsbehafteten Wert zuweisen und das Netz über eine beliebige Animation synchronisieren, handelt es sich bei der Synchronisierung nicht um eine „reine“ Synchronisierung. Stattdessen wird das Leader-Netz anhand der Leader-Parameter und das Follower-Netz anhand des Follower-Parameters ausgewertet. Das bedeutet, dass Sie durch die Animation der Parameter die vom Bildschirm abhängigen Netze ändern.

Das Folgende ist eher ein Spielzeugbeispiel, da es leicht mit Attributen hätte gemacht werden können, zeigt aber, wie Parameter/Stateful verwendet werden können. Der häufigere Anwendungsfall ist, wenn viele Variablen natürlich von einem Szenenzustand abhängen würden oder wenn Sie die Parameterwerte im Präsentationsmodus ändern möchten.
```mcl
param radius = 0.36

let Bubble = |radius, col|
    fill{alpha{0.2} col}
    stroke{col, 2}
    Circle(radius)

slide "Parameter"
    mesh bubble = Bubble(radius: $radius, col: BLUE)
    # referencing bubble nakedly uses the CURRENT value
    # so copy does not statefully depend on bubble!
    # try doing $bubble instead to see what happens
    mesh copy = bubble
    
    # the bubble leader depends on the radius leader
    # the bubble follower depends on the radius follower
    play Fade(0.4)

    # This edits the parameter leader.
    radius = 0.8
    # if you inspect bubble (leader) right now
    # it will be a circle

    # now we'll synchronize the radius follower
    # even though bubble was not changed since the last synchronization
    # its follower depends on the radius follower
    # so the bubble will change 
    # on the other hand, `copy` "elided" the stateful value
    # and no longer depends on radius so it remains the same
    play Lerp(1.0)
```

Abschließend stellen wir fest, dass Stateful zwar nicht viele Operationen ausführen kann (z. B. Additionen), aber das Argument für jede Funktion sein kann. Unter der Haube wird die Funktion bei jeder Neuauswertung mit dem aktuellen Wert des zustandsbehafteten Arguments aufgerufen. Dadurch können Sie diese Einschränkung umgehen, wenn auch ausführlicher.

```mcl image

param radius = 2

mesh circle = fill{CLEAR} Circle($radius)
# won't work, can't do * on a stateful value
# mesh square = Square(2 * $radius)
# ... but you can use it as argument to any function
let double = |x| 2 * x
mesh square = fill{CLEAR} Square(double($radius))
```

## Fortgeschrittenes LaTeX

Standardmäßig verwendet Monocurl ein gebündeltes LaTeX-Backend. Wenn Sie die LaTeX-Installation Ihres Systems für Pakete oder Schriftarten benötigen, können die Desktop-Einstellungen zu einem benutzerdefinierten System-Backend mit `latex` und `dvisvgm` wechseln. Die CLI verfügt über das passende Flag `--system-latex`. Dadurch können Sie Funktionen in Latex nutzen, die nicht im Standardpaket enthalten sind.

Bitte beachten Sie, dass Monocurl im Gegensatz zu vielen anderen Sprachen `%` als Escape-Zeichen für Zeichenfolgen anstelle von `\` verwendet, sodass Sie beim Schreiben von LaTeX keine doppelten Escape-Backslashes verwenden müssen.

`Text` ist für wörtlichen Text. `Tex` ist für gewöhnliche mathematische Fragmente. `Latex` ist für vollständigere LaTeX-Body-Fragmente und akzeptiert ein `additional_preamble`-Argument für Paket- oder Schriftartdeklarationen.

`Tex` und `Latex` geben Netzgeometrie zurück, sodass sie wie andere Netze gestylt, markiert, gefiltert und animiert werden können. Verwenden Sie `text_tag{...}` innerhalb der Texteingabe, wenn nur ein Teil des gerenderten Ausdrucks eine stabile Identität benötigt.

```mcl
mesh eq = Tex([text_tag{1} "x", " + ", text_tag{2} "1"], 0.8)

slide "Equation"
    play Write(0.8, [&eq])

    eq = Tex([text_tag{2} "1", " + ", text_tag{1} "x"], 0.8)
    play TagTrans(1.0, [&eq])
```


Wenn ein `Tex(...)`-Aufruf nicht gerendert werden kann, zeigt das Transkript die LaTeX-Compiler-Ausgabe an. Die häufigsten Ursachen sind fehlende Pakete und ungültige LaTeX-Syntax im String-Argument.

## Benutzerdefinierte Operatoren und wie sie unter der Haube arbeiten

Denken Sie daran, dass Operatoren Funktionen sind, die ein Ziel empfangen und ein transformiertes Ziel zurückgeben. Sie werden vor dem Objekt geschrieben, auf das sie einwirken, wodurch sie sich gut für wiederverwendbare Stil- und Platzierungspipelines eignen.

```mcl
let soft_badge = operator |target, col|
    fill{alpha{0.18} col}
    stroke{col, 2}
    target

mesh markers = [
    center{1.2l} soft_badge{BLUE} Circle(0.35),
    center{1.2r} soft_badge{ORANGE} Square(0.6)
]
```

Eine wichtige Eigenschaft von Operatoren besteht darin, dass Sie für viele Operatoren zwischen `x` und `op{} x` wechseln können. Beispielsweise gilt Folgendes
```mcl
mesh org = Triangle(0l, 1u, 1r)
play Set()
org = rotate{180dg} org
play Lerp()
```

Während Sie in den meisten Fällen Ihre eigenen Operatoren in Form von stdlib-Operatoren definieren können, können Sie auch benutzerdefinierte Operatoren erstellen, die über ein eigenes Interpolationsverhalten verfügen. Ein primitiver Operator gibt die Werte „Identität“ und „Operiert“ zurück. Der Identitätswert sollte wie der unveränderte Operand „aussehen“, aber zusätzliche Attribute enthalten, die eine direkte Interpolation mit dem „operierten“ Wert ermöglichen.

So wird beispielsweise `rotate` implementiert:
```mcl
let rotate = operator |target, radians, axis = 1b, pivot = nil, filter = nil| {
    let go = |angle| __monocurl__native__ op_rotate(target, angle, axis, pivot, filter)
    return [go(angle: 0), go(angle: radians)]
}
```
Die eigentliche Drehung erfolgt aus Effizienzgründen durch eine native Rust-Funktion, aber der Hauptpunkt ist, dass wir zwei Werte zurückgeben. Der erste rotiert um Null, was den Identitätszustand darstellt. Der zweite dreht sich um den gewünschten Betrag. In den meisten Berechnungen wird der Operator einfach als zweiter Rückgabewert behandelt. Beim Lerpen zwischen `x` und `rotate{180dg} x` erkennt Monocurl jedoch, dass es sich um einen Operator-Lerp handeln sollte, und prüft den Identitätswert und „eigentlich“ den Lerp zwischen `go(angle: 0)` und `go(angle: 180dg)`, was über herkömmliche Interpolation möglich ist.

## Primitive Animationen

Das Animationsmodell basiert auf der Leader/Follower-Synchronisation. Code bearbeitet Führungskräfte sofort; `play` sagt den Followern, wie sie aufholen können.

Der öffentliche Wrapper der niedrigsten Ebene ist `PrimitiveAnim(time, &vars, embed, lerp, rate)`. Animationen höherer Ebenen wie `Lerp` und `Trans` reduzieren sich schließlich auf primitive Animationen.

`PrimitiveAnim` gibt an, wie der Follower mit dem Leader synchronisiert werden soll. Die Idee ist, dass Sie eine benutzerdefinierte Interpolationsfunktion bereitstellen können, die sich von Lerp unterscheiden kann. So ist beispielsweise CameraLerp in der stdlib definiert.

```mcl
let CameraLerp = |&camera, time = 1, rate = smooth| {
    let embed = |start, dst| __monocurl__native__ camera_lerp_embed(start, dst)
    let value_lerp = |start, end, state, t| __monocurl__native__ camera_lerp_value(start, end, t)
    return PrimitiveAnim(time, &camera, embed, value_lerp, rate)
}
```

Der größte Teil der schweren Arbeit wird in Rust erledigt, aber wir können trotzdem durch den allgemeinen Fluss gehen. Die Einbettungsfunktion verarbeitet den Anfang und das Ende vor und gibt `[mod_start, mod_end, embed_state]` zurück. Dies ist nützlich für Animationen wie Trans, die teure Matching-Algorithmen erfordern, daher würden wir es vorziehen, dies nur einmal zu Beginn tun zu müssen. Die Interpolationsfunktion empfängt die Argumente von „embed“ sowie den normalisierten t-Wert und wird aufgefordert, entsprechend dem gewünschten Verhalten zu interpolieren. Im Fall von `CameraLerp` läuft dies auf die Durchführung einer sphärischen Interpolation hinaus, um die Anzeige natürlicher zu gestalten.
