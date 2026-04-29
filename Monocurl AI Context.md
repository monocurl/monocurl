# Monocurl AI Context

Monocurl is a desktop application and programming language for mathematical
animations. Scene files use `.mcs`; library files use `.mcl`. Only scene files
are rendered. Prefer the public stdlib wrappers in `assets/std/std/*.mcl` over
native calls.

Important references:

- `assets/std/std/mesh.mcl`: mesh constructors, operators, tags, layout helpers,
  graph helpers, and mesh queries.
- `assets/std/std/anim.mcl`: `Wait`, `Set`, `Lerp`, `Grow`, `Fade`, `Write`,
  `Trans`, `TagTrans`, `Bend`, `TagBend`, `CameraLerp`, rates, and animation
  time operators.
- `assets/std/std/math.mcl`, `color.mcl`, `util.mcl`, `scene.mcl`: constants,
  vector math, colors, collection helpers, camera/background helpers.
- `assets/default_scenes/*.mcs`: small user-facing examples of intended style.

Most scenes start with:

```monocurl
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene

background = WHITE

slide "Intro"
    mesh title = center{[0, 0.8, 0]} color{BLACK} Text("Hello", 0.8)
    play Write(0.8, [&title])
```

## 1. Monocurl Overview

A scene has an implicit init section before the first `slide`, followed by one
or more slides. Use the init section for imports, helper lambdas, custom
operators, top-level `param` declarations, and scene state such as `background`
or `camera`. Do not put `play` statements in init; animation belongs in slides.

```monocurl
background = WHITE

let DotAt = |pos, col|
    center{pos}
    fill{alpha{0.25} col}
    stroke{col, 2}
    Circle(0.25)

slide "First"
    mesh dot = DotAt(pos: 1l, col: BLUE)
    play Grow(0.5, [&dot])

slide "Second"
    dot = DotAt(pos: 1r, col: ORANGE)
    play Lerp(1, [&dot])
```

The desktop UI shows the editor, viewport, and timeline together. Timeline
shortcuts used by the tutorial scenes are:

- `,`: previous slide.
- `.`: next slide.
- `<`: scene start.
- `>`: scene end.
- `;`: small step backward.
- `'`: small step forward.

Presentation mode shows the current scene as slides and exposes top-level
params as controls. Command/Ctrl-T toggles presentation mode. Scenes can also be
exported as still images or videos.

## 2. Language Basics

Monocurl is line-major like Python and brace-major like C. Comments start with
`#`. Use `let` for immutable names and `var` for mutable construction state.
Values include numbers, strings, lists, maps, lambdas, operators, meshes, and
animation blocks.

```monocurl
let scale = 0.6
var points = []
points .= [-1, 0, 0]
points .= [1, 0, 0]

let colors = ["left" -> BLUE, "right" -> ORANGE]
let f = |x, y = 1| x + y
```

Common vector literals are `1l`, `1r`, `1u`, `1d`, `1f`, and `1b` for left,
right, up, down, forward, and backward. These are 3D vectors. In the default
camera, `1f` is negative z and `1b` is positive z.

Control flow is ordinary:

```monocurl
var total = 0
for (x in [1, 2, 3]) {
    if (x > 1) {
        total = total + x
    }
}
```

Use block lambdas when a helper needs multiple statements:

```monocurl
let ZigZag = |count| {
    var points = []
    for (i in range(0, count)) {
        let x = -3 + i * 6 / (count - 1)
        var y = -0.6
        if (i // 2 * 2 == i) { y = -1.1 }
        points .= [x, y, 0]
    }
    return stroke{TEAL, 3} Polyline(points)
}
```

`block { ... }` initializes the implicit accumulator `_` to `[]`; lines
beginning with `.` append to it, and `_` is returned at the end. This is useful
for helpers that build many mesh pieces:

```monocurl
let Dots = |count| block {
    for (i in range(0, count)) {
        . tag{i} center{[i * 0.4, 0, 0]} Circle(0.08)
    }
}
```

Labeled calls are important. Function calls use parentheses:
`Bubble(pos: 1l, radius: 0.35)`. Operator calls use braces before the target:
`shift{delta: 2r} Circle(radius: 0.5)`. Label arguments that an animation may
later mutate or interpolate.

Recursive lambdas take themselves explicitly:

```monocurl
let Fact = |self, n| {
    if (n <= 1) { return 1 }
    return n * self(self, n - 1)
}
let value = Fact(Fact, 5)
```

## 3. Meshes And Operators

Meshes are the visible values. Primitive constructors usually create canonical,
origin-based geometry. Place and style them with operators such as `shift`,
`center`, `scale`, `rotate`, `in_space`, `fill`, `stroke`, `color`, `fade`,
`tag`, `z_index`, `next_to`, `to_side`, and `to_corner`.

```monocurl
mesh disk =
    center{[0, 0.3, 0]}
    fill{alpha{0.25} BLUE}
    stroke{BLUE, 2}
    Circle(0.6)
```

Lists of meshes are also mesh values:

```monocurl
let Axes = || [
    stroke{DARK_GRAY, 1.5} Line([-4, 0, 0], [4, 0, 0]),
    stroke{DARK_GRAY, 1.5} Line([0, -2, 0], [0, 2, 0])
]
```

Operators transform a target mesh/value and compose right-to-left around the
target. Custom operators are good for repeated style chains:

```monocurl
let token = operator |target, col, id|
    tag{id}
    fill{alpha{0.22} col}
    stroke{col, 2}
    target

mesh row = [
    token{BLUE, 1} center{[-1, 0, 0]} Circle(0.35),
    token{ORANGE, 2} center{[1, 0, 0]} Square(0.65)
]
```

Tags are stable identities attached to mesh leaves. Use `tag{...}` when later
animations, filters, or styles need to know which subpart is which. Tags should
be simple numbers or lists of numbers. Filter lambdas receive a tag list, so
patterns like `color{BLUE, |tags| 1 in tags}` style only tagged pieces.

```monocurl
let style_terms = operator |target|
    color{ORANGE, |tags| 2 in tags}
    color{BLUE, |tags| 1 in tags}
    target
```

`text_tag{...}` is the text/Tex/Latex version of this pattern. It wraps a
string or fragment so generated glyph meshes carry that tag. Use it for equation
transforms and selective formula styling:

```monocurl
mesh eq = Tex([
    text_tag{1} "x^2",
    " + ",
    text_tag{2} "2x",
    " = ",
    text_tag{3} "(x+1)^2 - 1"
], 0.9)

eq.tex = [
    text_tag{3} "(x+1)^2 - 1",
    " = ",
    text_tag{1} "x^2",
    " + ",
    text_tag{2} "2x"
]
play TagTrans(1, [&eq])
```

Use `Text` for plain text, `Tex` for math fragments, and `Latex` for a full
LaTeX fragment. Text is still mesh geometry, so it can be styled, tagged, and
animated.

## 4. Animations

A `mesh` declaration creates a leader/follower pair. This is the core animation
model:

- the leader is the script value. Assignments and attribute edits change it
  immediately.
- the follower is the visible viewport value. It stays where it was until a
  `play` statement synchronizes it.

Think of the leader as the next destination keyframe, and the follower as the
current rendered frame. Assignments do not draw by themselves; they only prepare
the destination. `play Set` snaps followers to leaders, while `play Lerp`,
`play Trans`, `play Write`, `play Grow`, and related animations decide how the
follower catches up.

```monocurl
mesh ball = center{1l} Circle(0.3)
play Set([&ball])

ball = center{1r} ball
play Lerp(1, [&ball])
```

In the example above, `ball = center{1r} ball` changes only the leader. The
screen still shows the left ball until `play Lerp(1, [&ball])` animates the
follower from the old visible state to the new leader. `&ball` means "the mesh
leader named `ball`"; use explicit references in any nontrivial animation.

`Wait` reserves time without changing leaders or followers.
`Lerp` uses general interpolation rules:

- equal values stay fixed.
- numbers interpolate linearly.
- same-length lists interpolate element by element, including vectors/colors.
- two live calls to the same function interpolate labeled arguments and rerun
  the function.
- operators interpolate between their identity state and acted state.

This live-call pattern is preferred:

```monocurl
let Bubble = |pos, radius, col|
    center{pos}
    fill{alpha{0.25} col}
    stroke{col, 2}
    Circle(radius)

mesh bubble = Bubble(pos: 1l, radius: 0.25, col: BLUE)
play Set([&bubble])

bubble.pos = 1r
bubble.radius = 0.55
bubble.col = ORANGE
play Lerp(1, [&bubble])
```

Operators can also animate by their own live arguments:

```monocurl
mesh moved = shift{delta: 0r} Circle(radius: 0.35)
play Set([&moved])

moved.delta = 2r
moved.radius = 0.6
play Lerp(1, [&moved])
```

Use `Trans` when topology or shape changes enough that argument-level
interpolation is not the right model. Use `TagTrans` when subparts should be
paired by tags instead of raw order.

```monocurl
mesh pair = [tag{1} center{1l} Circle(0.25), tag{2} center{1r} Circle(0.25)]
play Set([&pair])

pair = [tag{2} center{1l} Square(0.45), tag{1} center{1r} Circle(0.35)]
play TagTrans(1, [&pair])
```

Animation blocks package sequences and can run in parallel. In nontrivial
parallel animations, pass explicit leader references so branches do not try to
animate the same dirty leader.

```monocurl
let move_left = anim {
    left = shift{delta: 1l} left
    play Lerp(1, [&left])
}

let move_right = anim {
    right = shift{delta: 1r} right
    play Lerp(1, [&right])
}

play [move_left, move_right]
```

Rates remap animation time. Common rates include `linear`, `smooth`, `bounce`,
and `elastic`. Use `rate{...}` to override a primitive animation's rate:

```monocurl
play rate{bounce} Lerp(1, [&ball])
```

Params use the same leader/follower idea and are exposed in presentation mode.
`param` declarations must be top-level. A plain read uses the leader value.
`$param` reads the live follower value, which is what lets presentation sliders
drive a mesh while it is on screen. Only put `$param` inside labeled
calls/operators assigned to `mesh` leaders, so the mesh can recompute from the
live value.

```monocurl
param radius = 0.8
mesh live_circle = Circle(radius: $radius)
play Set()

radius = 1.4
play Lerp(1)
```

`camera` is a special scene leader. Animate it with `CameraLerp`. If a mesh must
remain fixed in camera space while the camera moves, use `camera_transfer`.
Do not capture mutable `camera` inside a helper lambda; pass it as an argument.

```monocurl
let ORIGINAL_CAMERA = Camera(4b)
camera = ORIGINAL_CAMERA

let Hud = |text, live_camera|
    camera_transfer{ORIGINAL_CAMERA, live_camera}
    center{[-2.7, 1.6, 0]}
    color{BLACK}
    Text(text, 0.25)

mesh hud = Hud("fixed overlay", camera)
camera = Camera([1.5, 0.8, 4], [0, 0, 0], 1u)
play CameraLerp(&camera, 1)
```

## Debug Prints And Transcript

Use `print <expr>` for debugging and quick inspection. It evaluates an
expression and appends the value's string form to the execution transcript.

```monocurl
let points = [1, 2, 4]
print points
print ["count" -> len(points)]
```

The editor shows transcript lines inline under the statement and in the bottom
console panel. Use `print` intentionally; repeated prints inside frequent
updates or loops can flood the transcript.

## Patterns And Anti Patterns

Prefer these patterns:

- Build scenes from small helper lambdas that return mesh values.
- Keep init for imports, helpers, custom operators, params, background, and
  initial camera. Put `play` only in slides.
- Construct simple shapes at the origin, then place/style them with operators.
- Use labels for semantically important arguments such as `pos:`, `radius:`,
  `delta:`, `tex:`, `col:`, `count:`, and `phase:`.
- Label operator arguments when later code may mutate them, such as
  `shift{delta: 2r}` or `rotate{radians: PI / 4}`.
- Use `block` / dot accumulation or an explicit `var ret = []` for repeated
  mesh assembly.
- Use numeric tags or numeric-list tags on pieces that must persist across
  transforms, especially repeated dots, formula terms, graph regions, and
  objects animated with `TagTrans` or `TagBend`.
- Assign destination states first, then call `play`. Treat `play` as the point
  where followers catch up to leaders.
- Target specific leaders with `&mesh` in nontrivial scenes, especially inside
  `play [a, b]`.
- Use `Write` for strokes/text drawing on, `Grow` for appearing from center,
  `Fade` for opacity entrance/exit, `Lerp` for labeled argument interpolation,
  `Trans` for general mesh morphs, and `TagTrans` for identity-preserving
  morphs.
- Keep top-level params simple and feed them into reactive mesh expressions
  through `$param`.
- Pass mutable scene leaders such as `camera` into helpers instead of capturing
  them in lambdas.

Avoid these mistakes:

- Do not pass centers/normals to constructors that do not expose them; use
  `shift`, `center`, `in_space`, or other operators.
- Do not put `play` statements before the first `slide`.
- Do not store `$param` reactive expressions in `let`, `var`, or `param`; only
  store stateful values in `mesh` leaders.
- Do not mutate the same mesh leader from two parallel animation branches.
- Do not rely on unlabeled positional arguments for complex calls when labels
  make later animation clearer.
- Do not use string tags. Use numbers or lists of numbers.
- Do not use deterministic `random` / `randint` inside helper lambdas; they are
  root-frame scene operations.
- Do not hand-author low-level mesh topology for ordinary scenes; use stdlib
  constructors/operators and mesh queries.

Useful examples to imitate:

- `assets/default_scenes/(Tutorial) Monocurl Overview.mcs`: app structure,
  scene sections, timeline shortcuts, presentation/export.
- `assets/default_scenes/(Tutorial) Language Basics.mcs`: values, lambdas,
  control flow, labels, operators, block accumulation, recursion.
- `assets/default_scenes/(Tutorial) Meshes.mcs`: mesh declarations,
  constructors, mesh trees, tags, custom mesh helpers, custom operators.
- `assets/default_scenes/(Tutorial) Animations.mcs`: leader/follower state,
  Set/Lerp/Trans/TagTrans, parallel blocks, rates, params, camera transfer.
