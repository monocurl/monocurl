# Monocurl AI Context

Monocurl is a desktop application and programming language for mathematical
animations. Scene files use `.mcs`; library files use `.mcl`. Only scene files
are rendered. Prefer the public stdlib wrappers exposed by this MCP server over
native calls.

Useful MCP resources:

- `monocurl://stdlib/mesh`: mesh constructors, operators, tags, layout helpers,
  graph helpers, and mesh queries.
- `monocurl://stdlib/anim`: `Wait`, `Set`, `Lerp`, `Grow`, `Fade`, `Write`,
  `Trans`, `TagTrans`, `Bend`, `TagBend`, `CameraLerp`, rates, and animation
  time operators.
- `monocurl://stdlib/math`, `monocurl://stdlib/color`,
  `monocurl://stdlib/util`, and `monocurl://stdlib/scene`: constants, vector
  math, colors, collection helpers, camera/background helpers.
- `monocurl://examples/riemann-rectangles`: complete scene example showing
  imports, helpers, slides, graph helpers, tags, text tags, and animations.

## Scene Skeleton

Most scenes start with the standard imports, optional scene defaults, helper
definitions, and then one or more slides:

```monocurl
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene

background = WHITE

let Label = |text, at, scale = 0.34|
    center{at}
    color{BLACK}
    Text(text, scale)

slide "Intro"
    mesh title = Label("Hello", [0, 0.4, 0], 0.8)
    play Write(0.8, [&title])
```

String escapes use `%`, not `\`. Backslashes are ordinary characters so LaTeX
commands can be written directly, for example `Tex("\frac{x}{2}")`. Use `%"`,
`%n`, `%%`, and `%\` when a string needs a quote, newline, literal percent, or
explicit escaped backslash.

The code before the first `slide` is the init section. Use init for imports,
helpers, custom operators, initial `mesh` leaders, `background`, and `camera`.
Do not put `play` statements in init.
Animation belongs in slide bodies.

Indent every slide body, including the first slide. Keep the `slide` line at
top level, then indent all statements in that slide by four spaces. Do not write
unindented `mesh`, assignment, or `play` statements after a `slide`.

Scenes with zero slides are valid for still-image export. They should build
their final frame in init and should not rely on playback.

The default scene camera is positioned at `4b` with a `16 / 9` aspect ratio.
For ordinary 2D scenes on the default `z = 0` plane, this means the comfortable
visible coordinate range is approximately `x = -4..4` and `y = -2.25..2.25`.

Later snippets in this document are often fragments. In complete scenes, keep
`play` statements inside indented slide bodies.

The desktop UI shows the editor, viewport, and timeline together. Timeline
shortcuts used by the tutorial scenes are `,` for previous slide, `.` for next
slide, `<` for scene start, `>` for scene end, `;` for a small step backward,
and `'` for a small step forward. Presentation mode shows the current scene as
slides and exposes mesh controls when available. Command/Ctrl-T toggles
presentation mode. Scenes can also be exported as still images or videos.
