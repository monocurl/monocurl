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

The code before the first `slide` is the init section. Use init for imports,
helpers, custom operators, top-level `param` declarations, initial `mesh`
leaders, `background`, and `camera`. Do not put `play` statements in init.
Animation belongs in slide bodies.

Indent every slide body, including the first slide. Keep the `slide` line at
top level, then indent all statements in that slide by four spaces. Do not write
unindented `mesh`, assignment, or `play` statements after a `slide`.

Scenes with zero slides are valid for still-image export. They should build
their final frame in init and should not rely on playback.

The default scene camera is positioned at `6b` with a `16 / 9` aspect ratio.
For ordinary 2D scenes on the default `z = 0` plane, this means the comfortable
visible coordinate range is approximately `x = -4..4` and `y = -2.25..2.25`.

Later snippets in this document are often fragments. In complete scenes, keep
`play` statements inside indented slide bodies.

The desktop UI shows the editor, viewport, and timeline together. Timeline
shortcuts used by the tutorial scenes are `,` for previous slide, `.` for next
slide, `<` for scene start, `>` for scene end, `;` for a small step backward,
and `'` for a small step forward. Presentation mode shows the current scene as
slides and exposes top-level params as controls. Command/Ctrl-T toggles
presentation mode. Scenes can also be exported as still images or videos.

## Language Basics

### Values And Assignment

Monocurl is dynamically typed. Assignments are deep copies, so ordinary value
assignment cannot create reference cycles. Values include numbers, strings,
lists, maps, lambdas, operators, meshes, animation blocks, and `nil`.

Use `let` for immutable names and `var` for mutable construction state:

```monocurl
let scale = 0.6
var points = []
points .= [-1, 0, 0]
points .= [1, 0, 0]
points = [points] # deep copy: now [[-1, 0, 0], [1, 0, 0]]

let colors = ["left" -> BLUE, "right" -> ORANGE]
let f = |x, y = 1| x + y
```

`x .. y` appends `y` to `x`. `x .= y` is shorthand for `x = x .. y`.
Deep-copy assignment is intentional: it keeps ordinary values simple and avoids
reference cycles. If you need mutation to affect a visible value over time, use
`mesh` / `param` leaders and `play`.

Common vector literals are `1l`, `1r`, `1u`, `1d`, `1f`, and `1b` for left,
right, up, down, forward, and backward. These are 3D vectors. In the default
camera, `1f` is negative z and `1b` is positive z.

### Control Flow

Use ordinary `if`, `for`, and `while` statements:

```monocurl
var total = 0
for (x in [1, 2, 3]) {
    if (x > 1) {
        total = total + x
        break
    }
}
```

Prefer building data first, then turning it into mesh values. This keeps
algorithm scenes readable.

### Lambdas

Use multiline lambdas when a helper needs multiple statements:

```monocurl
let capture = 6
let Path = |count| {
    var points = []
    for (i in range(0, count)) {
        let x = -2.6 + i * capture / (count - 1)
        var y = -0.7
        if (i // 2 * 2 == i) { y = -1.2 }
        points .= [x, y, 0]
    }
    return stroke{TEAL, 3} Polyline(points)
}
```

Lambdas may capture immutable `let` values. Mutable construction state and scene
leaders (`var`, `mesh`, `param`, `camera`, etc.) should not be captured. Pass
them explicitly, or use `&` reference parameters when the helper lambda needs to mutate
a mesh/param leader.

Recursive lambdas take themselves explicitly:

```monocurl
let fact = |self, n| {
    if (n <= 1) { return 1 }
    return n * self(self, n - 1)
}
let value = fact(fact, 5)
```

### Block Accumulation

`block { ... }` initializes the implicit accumulator `_` to `[]`; lines
beginning with `.` append to it, and `_` is returned at the end. Use this for
helpers that assemble many pieces:

```monocurl
let DotRow = |count, y = 0, col = BLUE| block {
    for (i in range(0, count)) {
        let x = i - (count - 1) / 2
        . tag{i}
            center{[x * 0.55, y, 0]}
            fill{alpha{0.22} col}
            stroke{col, 2}
            Circle(0.16)
    }
}
```

### Calls And Operators

Function calls use parentheses:

```monocurl
let f = |x| x * 2
let res = f(24 + 4)
```

Operator calls use braces before the target:

```monocurl
# shift is an operator acting on Circle
shift{2r} Circle(0.5)
```

Function and operator labels become meaningful fields on live calls. That means
later code can address `ball.pos`, `ball.radius`, `token.delta`, or `eq.tex`
instead of rebuilding an expression by positional argument order. Label
arguments that an animation may later mutate or interpolate.

Operators compose around a target. Custom operators are good for repeated style
chains:

```monocurl
let badge_style = operator |target, col|
    fill{alpha{0.18} col}
    stroke{col, 2.2}
    target
```

### set_default (Advanced)

`set_default{name, value}` is an operator that pre-fills a named argument on a
callable target without invoking it. It returns an identity/acted pair, so it is
lerp-compatible like any other operator. `set_defaults{map}` sets multiple
defaults at once.

This is how `axis_style` is implemented — it wraps an axis constructor and
pre-fills its `x_axis` or `y_axis` labeled argument:

```monocurl
# axis_style{"x", 0, 4, "x"} is roughly:
#   set_default{"x_axis", [0, 4, "x", ...]} Axis2d(...)
axis_style{"x", 0, 4, "x", 1, 4}
axis_style{"y", 0, 5, "f(x)", 1, 4}
Axis2d([1r * 0.5, 1u * 0.5])
```

Use `set_default` to build configuration operators that compose cleanly onto
constructors without calling them:

```monocurl
let time_axis = operator |target|
    set_default{"x_axis", [0, 10, "t", 1, 4, |x| x]}
    set_default{"y_axis", [0, 5, "v", 0.5, 4, |y| y]}
    target

time_axis{} Axis2d([1r * 0.45, 1u * 0.45])
```

Prefer `axis_style` over raw `set_default` for axis configuration. Use raw
`set_default` only when building a reusable operator that needs to pre-configure
a constructor's named arguments in a way that no existing stdlib operator covers.

### References

Reference parameters are prefixed with `&`. Use them when a helper needs to
mutate a mesh/param leader or return an animation that will animate it. Only
mesh and parameter leaders can be passed by reference.

```monocurl
# highlight is a stdlib helper that mutates its input leader, so the mesh must
# be passed by reference.
mesh x = Circle(0.5)
play Highlight(&x, YELLOW)

# transfer is also defined in stdlib; this is the core idea
let Transfer = |&from, &into| anim {
    into = [into, from]
    from = []
    play Set([&from, &into])
}

mesh y = []
play Transfer(&x, &y)

let MoveAndGrow = |&m| {
    # you can do setup immediately before returning the actual animation
    # `anim` is just a special expression; it does not run until played
    let amount = 2r
    return anim {
        m = shift{amount} m
        play Lerp(1)
    }
}

play MoveAndGrow(&y)
```

This pattern is powerful because the lambda can parameterize an infinite number of animations and let the caller decide when to `play` it.
Use this for reusable animation helpers that need to mutate one or more leaders. Animations will be discussed more shortly.

## Meshes And Operators

### Mesh Values

Meshes are the visible values. Primitive constructors usually create canonical,
origin-based geometry. Place and style them with operators.

Lists of meshes, including nested lists, are valid mesh values:

```monocurl
let mesh_tree = [Circle(1), [Square(1), Circle(1)]]
```

Treat mesh lists as "trees" of meshes.

### Tags And Filters

Tags are stable identities attached to mesh leaves. Use `tag{...}` when later
animations, filters, or styles need to know which subpart is which. Tags should
be numbers or lists of numbers, not strings.

Many operators take an optional filter as their last argument. A filter receives
the tag list of the candidate mesh leaf and returns whether that subpart should be affected by the current operator:

```monocurl
# creating a custom operator that can be invoked
let tag_palette = operator |target|
    fill{alpha{0.28} MAGENTA, |tags| 3 in tags}
    fill{alpha{0.28} ORANGE, |tags| 2 in tags}
    fill{alpha{0.28} BLUE, |tags| 1 in tags}
    stroke{BLACK, 2}
    target

let subset = tag_filter{|tag| len(tag) > 0} data
```

### Text Tags

`text_tag{...}` is the text/Tex/Latex version of `tag{...}`. It wraps a string
or fragment before LaTeX is converted into mesh geometry. In the resultant mesh, the contours that were generated from that specific text will be assigned the tag.

```monocurl
let algebra_line = |left, middle, right| [
    text_tag{1} left,
    " + ",
    text_tag{2} middle,
    " = ",
    text_tag{3} right
]

mesh eq =
    color{BLUE, |tags| 1 in tags}
    color{ORANGE, |tags| 2 in tags}
    Tex(algebra_line("x^2", "2x", "(x+1)^2 - 1"), 0.9)
```

Internally, `text_tag{2} "2x"` simply aliases to
`\text_tag{2}{2x}`; `\tag2{2x}` is a custom latex shortcut as well. Use tagged fragments for
equation transforms and selective formula styling. They're also used for animations.

Use `Text` for plain text, `Tex` for math fragments, and `Latex` for full
LaTeX fragments. Text is still mesh geometry, so it can be styled, tagged, and
animated.

## Animations

### Leader/Follower Model

A `mesh` declaration creates a leader/follower pair:

- The leader is the value the script edits. Assignments and attribute edits
  change it immediately.
- The follower is the value currently visible in the viewport. It stays at the
  last synced state until a `play` statement tells it how to catch up.

Think of the leader as the next destination keyframe and the follower as the
current rendered frame. Assigning a new leader does not draw by itself.

At the end of init, leaders are implicitly synced to followers so zero-slide
still images and initial frames have visible state, but this is not true for slides. In slides, you must `play`
explicitly whenever the visible frame should update.

This is the most important animation rule: code changes the leader first, then
`play` decides how the follower catches up. Right after an assignment, code
reads the destination value, but the viewport still shows the previous follower
until the next `play`.

### Wait And Set

`Set` snaps followers to leaders immediately. `Wait` reserves time without
changing leaders or followers.

```monocurl
slide "Wait And Set"
    # follower starts as []
    mesh ball = Ball(pos: 1l, radius: 0.32, col: BLUE)
    play Wait(0.35) # reserves time; follower is still []

    # follower snaps to the current leader
    # it's not visible on screen as the same content as the leader
    play Set()
```

### Lerp

`Lerp` interpolates the follower toward the leader. It works best when the
leader and follower have the same shape. lerp(a, b, t) evaluates recursively under the following rules:

- equal values stay fixed.
- numbers interpolate linearly.
- same-length lists (and maps with same keys) interpolate element by element, including coordinate lists/colors.
- live calls to the same labeled function interpolate labeled arguments and
  rerun the function.
- live operators interpolate their labeled operator arguments.

```monocurl
mesh ball = Ball(pos: 1l, radius: 0.32, col: BLUE)
play Set([&ball])

ball.pos = 1r
ball.radius = 0.55
ball.col = ORANGE
play Lerp(0.9, [&ball])
```

`Lerp` is strongest when the expression is a labeled live function/operator. The
runtime interpolates the labels and reruns the call at each frame. This is why
`Ball(pos: ..., radius: ..., col: ...)` is better than an unlabeled positional
call in an animated scene.

Operators interpolate through their identity state. For example,
`shift{delta: 2r} Circle(0.5)` can lerp from the unshifted circle `Circle(0.5)` because the
operator supplies an identity value (`delta = 0r`) and a modified value
(`delta = 2r`). You rarely need to author this low-level pair yourself; build
custom operators from stdlib operators when possible.

Internally, a primitive interpolatable operator is shaped like this:

```monocurl
let shift = operator |target, delta| {
    let go = |d| __monocurl__native__ op_shift(target, d)
    return [go(0r), go(delta)]
}
```

That returned pair means "identity appearance" and "acted appearance" in such a way that you can interpolate from the initial and acted appearence. Most
scene code should avoid this low-level pattern and compose existing stdlib
operators instead.

Anyways, all this means that the following is a clearly defined animation for rotation.

```
mesh init = Square(1)
play Set()
init = rotate{TAU} init
play Lerp()
```
### Shape Morphs

Use `Trans` when source and destination differ in a way that is not interpolatable. Use `TagTrans` when you want greater control on the matching algoithm.

`Trans` uses contour matching based on geometry heuristics
`TagTrans` only allows matches between sources and destination meshes with the same tags. If you tag properly, this lets you

```monocurl
mesh pair = [
    tag{1} center{1l} Circle(0.25),
    tag{2} center{1r} Circle(0.25)
]
play Set([&pair])

pair = [
    tag{2} center{1l} Square(0.45),
    tag{1} center{1r} Circle(0.35)
]
play TagTrans(1, [&pair])
```

Use `Write` for drawing strokes/text on, `Grow` for appearing from a center,
and `Fade` for opacity entrances/exits. To fade out, assign the leader to `[]`
and play `Fade`.

`Write`, `Grow`, and `Fade` compare the current follower and destination leader.
They keep shared contours still, show newly added contours, and hide deleted
contours. This is why `note = []` followed by `play Fade(...)` fades the old
visible note out instead of snapping it away.

```monocurl
mesh note = Text("hello", 0.6)
play Write(0.8, [&note])

note = []
play Fade(0.4, [&note])
```

### Animation Blocks And Parallelism

Animation blocks package sequences. They do not execute until `play` is called.
When given a list of animations, `play` runs them in parallel and waits for all
branches to finish.

Target explicit leaders with `&leader` in nontrivial scenes, especially inside
parallel animation branches. Two parallel branches must not animate the same
leader at the same time.

Think of `anim { ... }` as a coroutine. It can contain assignments, prints,
and nested `play` statements, but none of that runs when the block is created.
The work starts only when the block itself is played.

```monocurl
let move_pair = anim {
    pair = shift{delta: 0.6u} pair
    play Lerp(0.9, [&pair])
}

let pulse = anim {
    ring = Circle(0.4)
    play Grow(0.4, [&ring])
    ring = []
    play Fade(0.4, [&ring])
}

play [move_pair, pulse]
```

For reusable animations, a lambda can return an `anim` block. Notice that these types of animations can actually cause leaders to change, instead of animations like Write. Such animations that cause changes to leaders are called progressors.

```monocurl
let MoveAndGrow = |&m| {
    let amount = 2r
    return anim {
        m = shift{amount} m
        play Lerp(1)
    }
}

play MoveAndGrow(&token)
```

### Rates

Rates remap animation time. Most primitive animations default to `smooth`.
Use `rate{...}` to override timing:

```monocurl
play [
    rate{linear} Lerp(1.2, [&left]),
    rate{bounce} Lerp(1.2, [&right])
]
```

## Params, Camera, And Background

### Params

`param` works like `mesh`: it has a leader value, which code edits, and a
follower value, which is the live value synchronized by `play`. Params are also
exposed as interactive controls in presentation mode, so the viewer can adjust
them while the scene is paused or presenting. `param` declarations must be
top-level.

A plain read such as `radius` reads the current concrete leader value. The `$`
sigil creates a stateful reference to the live follower value, as in
`$radius`. Params are the only leader kind that can be referenced with `$`.

A stateful value continuously re-evaluates using the current follower values of
all params it depends on. This is what lets a visible mesh react while a
presentation slider changes. Stateful values may only be stored in `mesh`
leaders. Assigning one to `let`, `var`, or another `param` is a runtime error.

`$param` must appear inside a labeled function or operator call assigned to a
mesh leader, so the mesh can recompute from the live value. Arithmetic on
`$param` is valid as a sub-expression inside that labeled call, but the
outermost reactive expression must still be a labeled call.

```monocurl
param radius = 0.36
param delta = [-1, 0, 0]
# invalid patterns:
# let illegal = $radius (assignment to let)
# var also_illegal = Circle(radius: $radius) (binary op on stateful)
# param still_illegal = $radius

let Ball = |pos, radius, col|
    center{pos}
    fill{alpha{0.24} col}
    stroke{col, 2}
    Circle(radius)

let double = |x| 2 * x
slide "Parameters"
    # the ball leader depends on radius's live leader value
    mesh ball = Ball(pos: [0, 0, 0], radius: $radius, col: GREEN)
    # this materializes ball into its current evaluation based on the current evaluation of radius
    let ball_eval = ball
    mesh doubled = shift{$delta} Circle(radius: double($radius))
    play Fade(0.7)
    # now, the ball follower depends on the radius follower

    # this edits the parameter leader; Lerp moves the parameter follower
    # and ball recomputes from the live follower during the animation
    radius = 0.8
    play Lerp(1.0)

    # a plain read evaluates the leader concretely
    let current = ball
```

Stateful operators still expose labeled fields. Mutating a label edits the
non-stateful part of the leader, while the `$param` part continues to react:

```monocurl
param delta = 0l
mesh shifted = shift{delta: $delta} Circle(center: 0l)

shifted.center = 1l
delta = 3l
play Lerp(1)

let snap = shifted # concrete value, not a stateful reference
```

When followers are synced to leaders with `Set` or at the end of `Lerp`,
stateful leader expressions propagate to the follower, keeping the follower
reactive. Use params when a value should be exposed to presentation mode or
shared between scripted animation and live user controls.

### Camera And Background

`camera` and `background` are effectively scene-level params with special
renderer meaning. They can be assigned and synchronized like other leaders, but
`camera` controls the view and `background` controls scene color. Use
`CameraLerp` for camera movement. Use `camera_transfer{original_camera,
live_camera}` for meshes that should stay fixed relative to the frame while the
camera moves.

With the default camera at `4b` and `16 / 9` frame, the `z = 0` authoring plane
shows roughly `x = -4..4` and `y = -2.25..2.25`. Place most tutorial-style 2D
content inside that box unless the scene intentionally animates the camera.
`background` can be assigned and synchronized like any other scene leader, but
camera motion should usually use `CameraLerp` rather than plain `Lerp`.

```monocurl
mesh object = Circle(1)

# by reading the original and current values of camera and cleverly using
# camera_transfer, the mesh appears fixed in frame
mesh fixed_in_frame =
    camera_transfer{camera, $camera}
    shift{2r + 1u}
    Square(1)

camera = Camera([1.5, 0.8, 4])

# camera lerp is a specialized lerp for cameras that tries to make the overall
# motion more smooth
# as it moves, fixed_in_frame appears to stay in place because camera_transfer
# positions it perfectly relative to the new camera
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

## Patterns And Anti-Patterns

### Prefer

- Start from the standard imports unless the scene deliberately uses only a
  subset.
- Keep init for imports, helpers, custom operators, params, background, camera,
  and initial leaders.
- Indent all slide bodies, including the first slide.
- Build scenes from small helper lambdas that return mesh values.
- Construct simple shapes at the origin, then place/style them with operators.
- Label semantically important arguments such as `pos:`, `radius:`, `delta:`,
  `tex:`, `col:`, `count:`, `phase:`, `src:`, and `dst:`.
- Label operator arguments when later code may mutate them, such as
  `shift{delta: 2r}` or `rotate{radians: PI / 4}`.
- Use `block` / dot accumulation for repeated mesh assembly.
- Use numeric tags or numeric-list tags on pieces that must persist across
  transforms, especially repeated dots, formula terms, graph regions, and
  objects animated with `TagTrans` or `TagBend`.
- Assign destination states first, then call `play`.
- Target specific leaders with `&mesh` in nontrivial scenes.
- Use `print` for sanity checks while authoring algorithmic visuals.
- Use `Write` for strokes/text drawing on, `Grow` for appearing from center,
  `Fade` for opacity entrance/exit, `Lerp` for labeled argument interpolation,
  `Trans` for general mesh morphs, and `TagTrans` for identity-preserving
  morphs.
- Pass mutable scene leaders such as `camera` into helpers instead of capturing
  them in lambdas.

### Avoid

- Do not explicitly specify the variables for animations when they can be inferred (i.e. only one animation running on all of the changed variables). Try to structure and intersperse changes to leaders and play calls so that you can avoid specifying the variables as much as possible
- Do not call `__monocurl__native__` directly from scenes; use stdlib wrappers.
- Do not put `play` statements before the first `slide`.
- Do not leave first-slide statements unindented.
- Do not capture mutable values or scene leaders in lambdas; pass them
  explicitly.
- Do not use params as a substitute for ordinary labeled arguments. Use params
  when interactive presentation controls are genuinely part of the scene.
- Do not pass centers/normals to constructors that do not expose them; use
  `shift`, `center`, `in_space`, or other operators.
- Do not store `$param` reactive expressions in `let`, `var`, or `param`; keep
  stateful values in mesh leaders.
- Do not mutate the same mesh leader from two parallel animation branches.
- Do not rely on unlabeled positional arguments for complex calls when labels
  make later animation clearer.
- Do not use string tags. Use numbers or lists of numbers.
- Do not use deterministic `random` / `randint` inside helper lambdas; they are
  only available in the root frame.
- Do not hand-author low-level mesh topology for ordinary scenes; use stdlib
  constructors/operators and mesh queries.

## Examples To Imitate

- `assets/default_scenes/(Tutorial) Monocurl Overview.mcs`: app structure,
  scene sections, timeline shortcuts, presentation/export.
- `assets/default_scenes/(Tutorial) Language Basics.mcs`: values, lambdas,
  control flow, labels, operators, block accumulation, recursion.
- `assets/default_scenes/(Tutorial) Meshes.mcs`: mesh declarations,
  constructors, mesh trees, tags, custom mesh helpers, custom operators.
- `assets/default_scenes/(Tutorial) Animations.mcs`: leader/follower state,
  `Set`/`Lerp`/`Trans`/`TagTrans`, parallel blocks, rates, params, camera
  transfer.
- `assets/default_scenes/(Example) Algorithm.mcs`: algorithm state, loops,
  `print`, and incremental `TagTrans`.
- `assets/default_scenes/(Example) Riemann Sum.mcs`: graph helpers,
  measurement labels, `text_tag`, and subset transfers.
- `assets/default_scenes/(Example) Flow Field.mcs`: params, fields,
  presentation controls, and live mesh recomputation.
- `assets/default_scenes/(Example) Text.mcs`: text/Tex usage and tagged formula
  transforms.

## Formatting Conventions

### Indentation

- Use four spaces for slide bodies, blocks, loops, and conditionals.
- Put one statement per line unless a very short `if` branch is clearer on one
  line.
- When a mesh expression spans multiple lines, put each operator on its own
  indented line and put the constructor or target last.

```monocurl
mesh disk =
    center{[0, 0.3, 0]}
    fill{alpha{0.25} BLUE}
    stroke{BLUE, 2}
    Circle(0.6)
```

### Naming

- Use `lower_snake_case` for local values, mesh leaders, params, numeric/data
  helpers, and most custom operators: `graph_origin`, `live_radius`,
  `embed_graph`, `tag_palette`.
- Use `PascalCase` for mesh factory helpers that behave like constructors:
  `Label`, `Panel`, `Ball`, `FlowScene`, `RecursiveTree`.
- Use `ALL_CAPS` for top-level constants that are configuration values:
  `BASE`, `HEIGHT`, `START`, `GOAL`, `ORIGINAL_CAMERA`.
- Use short lowercase names only for tiny math helpers or loop-local values:
  `f`, `pt`, `col`, `idx`.
- Prefer semantic mesh names over visual-only names: `title`, `axes`, `graph`,
  `rects`, `measure`, `field`, `trail`.

### Labels

Label arguments that later code may mutate or interpolate. This is one of the
most important Monocurl idioms.

```monocurl
mesh ball = Ball(pos: 1l, radius: 0.32, col: BLUE)
play Set([&ball])

ball.radius = 0.55
ball.col = ORANGE
play Lerp(0.9)
```

Use labels for operator arguments too:

```monocurl
mesh token = shift{delta: 1.2l} Circle(radius: 0.32)
play Set()

token.delta = 1.2r
token.radius = 0.48
play Lerp(0.9)
```

### Comments

Use comments to explain intent or non-obvious semantics, not every line of
mechanics. Tutorial scenes use comments before important transitions, animation
model details, and debugging prints.

## Cheat Sheet

This is not the full API. For the full list, read the public wrappers in
`assets/std/std/*.mcl`, especially `mesh.mcl`, `anim.mcl`, `util.mcl`,
`math.mcl`, `color.mcl`, and `scene.mcl`.

### Common Imports

```monocurl
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene
```

### Common Mesh Constructors

- Basic 2D: `Dot`, `Circle`, `Annulus`, `Square`, `Rect`, `RegularPolygon`,
  `Polygon`, `Polyline`, `Line`, `Arrow`, `Arc`, `Capsule`, `Triangle`,
  `Bezier`.
- Basic 3D: `Sphere`, `RectangularPrism`, `Cylinder`, `Cone`, `Torus`, `Plane`,
  `Vector`, `HalfVector`.
- Text and labels: `Text`, `Tex`, `Latex`, `Number`, `Label`, `Brace`,
  `Measure`.
- Layout: `Stack`, `XStack`, `YStack`, `ZStack`, `Grid`, `Table`,
  `BoundingBox`.
- Graphs and fields: `Axis1d`, `Axis2d`, `Axis3d`, `PolarAxis`, `ColorGrid`,
  `LineGrid`, `Field`, `ParametricFunc`, `ExplicitFunc`, `ExplicitFunc2d`,
  `ImplicitFunc2d`, `ExplicitFuncDiff`.
- Media: `Image`.

### Common Mesh Operators

- Placement: `shift`, `center`, `scale`, `rotate`, `in_space`, `next_to`,
  `matched_edge`, `to_side`, `to_corner`, `camera_transfer`, `projected`.
- Styling: `fill`, `stroke`, `color`, `fade`, `dotted`, `dashed`, `gloss`,
  `textured`, `z_index`.
- Identity and subsets: `tag`, `text_tag`, `tag_filter`, `tag_split`,
  `tag_map`, `subset_map`, `contour_separate`.
- Geometry transforms: `point_map`, `color_map`, `uv_map`, `uprank`,
  `downrank`, `wireframe`, `subdivide`, `tesselated`, `extrude`, `revolve`.

### Common Animations

- Timeline control: `Wait(time)`, `Set([&mesh])`.
- Interpolation: `Lerp(time, [&mesh], rate)`, `PrimitiveAnim(...)`.
- Entrances/exits: `Grow`, `Fade`, `Write`.
- Shape morphing: `Trans`, `TagTrans`, `Bend`, `TagBend`.
- Camera: `CameraLerp(&camera, time)`.
- Indication and movement helpers: `Highlight`, `Flash`, `Transfer`, `Copy`,
  `TransferSubset`, `CopySubset`, `TransSubsetTo`, `TransSubsetCopy`.
- Composition and timing: `LaggedMap`, `delay{...}`, `rate{...}`, `slow{...}`,
  `fast{...}`.
- Common rates: `linear`, `smooth`, `identity`, `bounce`, `elastic`,
  `ease_in`, `ease_out`, `ease_in_out`.

### Common Utilities

- Lists and loops: `range`, `sample`, `sample_clopen`, `len`, `map`, `filter`,
  `reduce`, `zip`, `enumerate`, `reverse`, `sort`, `take`, `drop`,
  `list_subset`, `flatten`.
- Aggregates: `all`, `any`, `count`, `sum`, `product`, `max_of`, `min_of`.
- Math: `PI`, `TAU`, `ORIGIN`, `LEFT`, `RIGHT`, `UP`, `DOWN`, `FORWARD`,
  `BACKWARD`, `sqrt`, `sin`, `cos`, `min`, `max`, `clamp`, `lerp`,
  `keyframe_lerp`, `map_range`, `deg_to_rad`, `rad_to_deg`.
- Colors: `RED`, `ORANGE`, `YELLOW`, `GREEN`, `TEAL`, `CYAN`, `BLUE`,
  `PURPLE`, `MAGENTA`, `WHITE`, `LIGHT_GRAY`, `GRAY`, `DARK_GRAY`, `BLACK`,
  `CLEAR`, `rgb`, `hsv`, and `alpha`.
- Type/introspection: `type_of`, `is_list`, `is_map`, `is_mesh`,
  `is_callable`, `has_attr`, `get_attr`, `set_attr`, `get_defaults`,
  `set_default`, `set_defaults`, `runtime_error`.
- Scene: `Camera`, `DEFAULT_CAMERA`, `DEFAULT_BACKGROUND`, `FRAME_X_RADIUS`,
  `FRAME_Y_RADIUS`.
