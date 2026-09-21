# Überblick

Diese Lektion führt Sie durch den Editor, die Struktur einer Szene und den Übergang vom Schreiben von Code zum Exportieren oder Präsentieren Ihrer Arbeit.

## Der Herausgeber

![The Monocurl editor](/img/home/monocurl-editor.png)

Der Editor verfügt über drei Arbeitsflächen:

- **Quelleditor** (links) – hier schreiben Sie `.mcs`-Szenendateien
- **Ansichtsfenster** (oben rechts) – zeigt den gerenderten Frame an der aktuellen Timeline-Position
- **Zeitleiste** (unten rechts) – scrollt durch die Folien und die einzelnen `play`-Schritte innerhalb jeder Folie

Wenn Sie Code bearbeiten, führt Monocurl eine Neuauswertung durch und die Vorschau des Ansichtsfensters wird entsprechend der aktuellen Position in der Zeitleiste aktualisiert.

## Szenenstruktur

Jede Szene besteht aus drei Teilen: Importen, einem Init-Abschnitt und Folien. Der Kürze halber lassen wir in den meisten Beispielen Importe weg.

```mcl 
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene

# --- init section ---
# runs first; sets up helpers and the initial visible state

mesh dot = center{ORIGIN} fill{soft{} CYAN} stroke{CYAN, 2} Circle(0.4)

slide "intro"
    # slide containing some animations
    mesh title = center{0.8u} Text("Hello", 0.7)
    play Write(0.9)

slide
    dot = center{1.2r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.5)
    play Lerp(1.2)
```

Code vor dem ersten `slide` ist **init**. Hier leben Importe neben Konstanten, Hilfsfunktionen und dem sichtbaren Startzustand. Nach dem ersten Schlüsselwort `slide` gehört der Code zu dieser Folie und kann `play`-Animationen enthalten.

## Eine erste Szene

Hier ist eine komplette kleine Szene. Das Muster ist: Richten Sie bestimmte Helfer in init ein und ändern Sie dann in jeder Folie den Szenenstatus kontinuierlich über Wiedergabeanimationen.

```mcl video
slide "intro"
    mesh circle = 
      center{pos: 1.4l} 
      color{col: BLUE}
      Circle(0.4)
    mesh label = 
      center{pos:1.4l + 0.75d} 
      Text(text: "hello", 0.65)
    # introduce the newly created meshes in an animated fashion
    play [Write(0.9, [&label]), Fade(0.9, [&circle])]

slide "transform"
    circle.pos = 1.4r
    circle.col = ORANGE
    label.pos = 1.4r + 0.75d
    label.text = "world"
    # transform both meshes into new state
    play Trans(1.2)
```

## Timeline-Navigation

Für eine detaillierte Steuerung können Sie auf „Suchen“ klicken. Im Allgemeinen verwenden Sie jedoch die Tastatur, um sich durch Ihre Szene zu bewegen:

- `,` / `.` – vorherige/nächste Folie
- `<` / `>` – zum Szenenanfang/-ende springen
- `;` / `'` – kleiner Schritt zurück / vorwärts


Eine nützliche Angewohnheit beim Verfassen besteht darin, die Zeitleiste nach dem Hinzufügen einer neuen Folie zu durchsuchen, um zu überprüfen, ob jeder `play`-Schritt das tut, was Sie beabsichtigt haben.

## Präsentieren und exportieren

Die gleiche Quelldatei `.mcs` kann auf drei Arten verwendet werden:

- **Videoexport** – Dateimenü → Video exportieren. Rendert die gesamte Szene als `.mp4`.
- **Bildexport** – Menü Datei → Bild exportieren. Rendert einen einzelnen Frame als `.png`.
- **Präsentationsmodus** – `Cmd/Ctrl-P`. Verwandelt Folien in Navigationskontrollpunkte, die an jeder `slide`-Grenze anhalten.

## Interaktiver Workflow

!vid[](/video/interactive-development.mp4)


Im Präsentationsmodus öffnet `Cmd/Ctrl-T` das **Parameterfenster**, in dem Sie einen Teil des Szenenstatus mit Schiebereglern bearbeiten können. Dies ist fortgeschrittener/Nischenmodell, kann aber leistungsstark sein.

Im Vorschau- und Präsentationsmodus können Sie den Cursor ziehen, um die Kamera zu bewegen. Wenn Sie die Umschalttaste drücken und den Cursor ziehen, können Sie die Kamera schwenken. Diese sind besonders hilfreich beim Erstellen von 3D-Szenen.

## Monocurl im Web

[Monocurl Essays](https://www.monocurl.com/monocurl-essays/) zeigt, wie Monocurl-Szenen direkt im Web ausgeführt werden können. Die zugrunde liegende Laufzeit ist auch als [NPM-Paket](https://www.npmjs.com/package/monocurl) verfügbar.
