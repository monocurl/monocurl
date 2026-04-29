Monocurl is a desktop application for programming mathematical animations. A
scene is written in a `.mcs` file, can be previewed live, exported as video, or
shown as a slideshow. Library files use `.mcl`; only scene files are rendered.

When generating Monocurl code, prefer the user-facing stdlib wrappers in
`assets/std/std/*.mcl` over native calls. The most important references are:

- `assets/std/std/mesh.mcl`: mesh constructors, styling/layout/transform
  operators, tags, graphing helpers, and mesh queries.
- `assets/std/std/anim.mcl`: `Wait`, `Set`, `Lerp`, `Grow`, `Fade`, `Write`,
  `Trans`, `TagTrans`, `Bend`, rates, and animation time operators.
- `assets/std/std/math.mcl`, `color.mcl`, `util.mcl`, `scene.mcl`: constants,
  vector math, colors, collection helpers, camera/background helpers.
- `assets/default_scenes/*.mcs`: good small examples of intended style.

## Basics

Start most scenes with the default imports:

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
mesh disk = shift{[0, -0.3, 0]} fill{alpha{0.25} BLUE} stroke{BLUE, 2} Circle(0.6)
play Write(0.8, [&title])
play Grow(0.8, [&disk])
```

Monocurl is line-major like Python and brace-major like C. Comments start with
`#`. Use `let` for immutable names and `var` for mutable construction state.
Lists are `[a, b, c]`; maps are `[key -> value]`. Functions are lambdas:
`let f = |x, y = 1| x + y`, or block lambdas with `return`.

Common vector literals are `1l`, `1r`, `1u`, `1d`, `1f`, `1b` for left, right,
up, down, forward, backward. These are 3D vectors; `1f` is negative z and `1b`
is positive z. Import `std.math` for `PI`, `TAU`, `sin`, `cos`, `lerp`,
`keyframe_lerp`, `dot`, `cross`, `normalize`, etc.

Meshes are the visible values. Most primitive constructors are canonical,
origin-based geometry. Place and style them with operators such as `shift`,
`center`, `scale`, `rotate`, `in_space`, `fill`, `stroke`, `color`,
`fade`, `tag`, `z_index`, `next_to`, `to_side`, and `to_corner`.

Operators transform a target mesh/value and are usually written before the
target. They compose right-to-left around the target:

```monocurl
mesh c =
    shift{[1, 0, 0]}
    fill{alpha{0.25} ORANGE}
    stroke{ORANGE, 2}
    Circle(0.5)
```

Use slides as timeline/presentation boundaries. The implicit initial section
before the first `slide` is special: use it for imports, helper lambdas,
top-level `param` declarations, and initial scene state such as `background`.
Do not put `play` statements there; animations belong inside real slides.

```monocurl
slide "First"
mesh shape = stroke{BLUE, 2} Circle(0.5)
play Fade(0.5, [&shape])

slide "Second"
shape = shift{2r} shape
play Trans(1, [&shape])
```

Animation blocks package sequences:

```monocurl
let pulse = anim {
    halo = stroke{MAGENTA, 3} Circle(0.3)
    play Set([&halo])
    halo = fade{0} scale{3} halo
    play Trans(1, [&halo])
}
play pulse
```

For compound mesh helpers, return a list of meshes. `block { ... }` initializes
the implicit accumulator `_` to `[]`; lines beginning with `.` append to it,
and `_` is implicitly returned at the end:

```monocurl
let Axes = || block {
    . stroke{DARK_GRAY, 1.5} Line([-4, 0, 0], [4, 0, 0])
    . stroke{DARK_GRAY, 1.5} Line([0, -2, 0], [0, 2, 0])
}
```

Text is geometry. Use `Text` for plain text, `Tex` for math, and `Latex` for a
full LaTeX fragment.

Tags are stable identities attached to mesh leaves. Use `tag{...}` on ordinary
mesh pieces when later animations, filters, or style operators need to know
which subpart is which. A filter lambda receives the mesh tag list, so patterns
like `color{BLUE, |tags| 1 in tags}` can style only the tagged pieces.
`TagTrans` and `TagBend` use tags to pair old and new subparts by identity
instead of by raw order or position.

`text_tag{...}` is the text/Tex/Latex version of this pattern. It wraps a string
or text fragment so the generated glyph meshes carry that tag. This is useful
both for equation transforms and for styling independent pieces of one text
object:

```monocurl
let expression = [
    text_tag{1} "x^2",
    " + ",
    text_tag{2} "2x",
    " = ",
    text_tag{3} "(x+1)^2 - 1",
]

let style_terms = operator |target|
    color{ORANGE, |tags| 2 in tags}
    color{BLUE, |tags| 1 in tags}
    target

mesh eq = style_terms{} Tex(expression, 0.9)
play Write(1, [&eq])

eq.tex = [
    text_tag{3} "(x+1)^2 - 1",
    " = ",
    text_tag{1} "x^2",
    " + ",
    text_tag{2} "2x",
]
play TagTrans(1, [&eq])
```

## Advanced Features

### Labeled Arguments And Live Calls

Labeled arguments are central to Monocurl animation. A call like
`Circle(radius: 0.5)` stores the argument name `radius` with the resulting
value. That stored call can later be edited through attributes, and the result
is recomputed:

Function calls use parentheses with `name: value` labels, such as
`Bubble(center: 0l, radius: 0.35)`. Operator calls use braces before the
target, such as `shift{delta: 2r} Circle(radius: 0.5)`. Label any argument an
animation may later mutate or interpolate.

```monocurl
mesh dot = shift{delta: [1, 0, 0]} Circle(radius: 0.25)
dot.radius = 0.4
dot.delta = [2, 0, 0]
```

This applies to both function calls and operator calls. In
`shift{delta: 2r} Circle(radius: 0.5)`, both `delta` from `shift` and `radius`
from `Circle` remain reachable. This is why generated code should label
semantically important arguments: labels make later mutation and interpolation
depend on names rather than on a fragile positional guess.

Helper lambdas should also use meaningful labels at call sites:

```monocurl
let Bubble = |pos, radius, col|
    center{pos}
    fill{alpha{0.25} col}
    stroke{col, 2}
    Circle(radius)

mesh bubble = Bubble(center: 0l, radius: 0.35, col: BLUE)
play Set([&bubble])

bubble = Bubble(center: 2r, radius: 0.75, col: ORANGE)
play Lerp(1, [&bubble])
```

The two `Bubble(...)` values are live calls to the same helper. During `Lerp`,
Monocurl interpolates their arguments (`center`, `radius`, and `col`) and
re-runs `Bubble` at each intermediate value. This is much better than trying to
manually build every intermediate mesh.

### Interpolation

`Lerp(time, [&mesh])` synchronizes a mesh follower to its leader by using
Monocurl's general `lerp(a, b, t)` rules. The most important rules are:

- equal values stay fixed.
- numbers interpolate linearly.
- same-length lists interpolate element by element, so vectors and RGBA colors
  usually interpolate naturally.
- two live calls to the same function interpolate by interpolating each
  corresponding argument, then recomputing the function call.
- operator values can interpolate between an unmodified value and an operated
  value, such as `x` to `shift{2r} x`.

The live-call rule is what makes this animate smoothly:

```monocurl
let LabelDot = |pos, radius, col|
    center{pos}
    fill{alpha{0.3} col}
    stroke{col, 2}
    Circle(radius)

mesh d = LabelDot(pos: 1l, radius: 0.25, col: BLUE)
play Set([&d])

d = LabelDot(pos: 1r, radius: 0.55, col: ORANGE)
play Lerp(1, [&d])
```

At time `t`, the runtime behaves like:

```monocurl
LabelDot(
    pos: lerp(1l, 1r, t),
    radius: lerp(0.25, 0.55, t),
    col: lerp(BLUE, ORANGE, t),
)
```

The operator rule is more subtle and very important. Operators define both an
identity embedding and the acted value, so Monocurl can interpolate from a plain
value to an operated value even though the outer shapes are not the same kind of
call:

```monocurl
mesh base = fill{alpha{0.25} BLUE} stroke{BLUE, 2} Circle(0.45)
play Set([&base])

base = shift{delta: 2r} base
play Lerp(1, [&base])
```

This is not just "different meshes with matching vertices." The destination is
an operator applied to the previous value. The `shift` operator says what its
identity state is (`delta = [0, 0, 0]`) and what its acted state is
(`delta = 2r`), so `Lerp` can animate from `base` to `shift{2r} base`. The same
idea lets `scale{...} x`, `rotate{...} x`, `fade{...} x`,
`fill{...} x`, `stroke{...} x`, and user-defined operators animate cleanly.

`rotate` is a strong example: it shows why operators are different from plain constructors—both the unrotated and rotated versions share identity through `rotate`’s internal `angle` argument, so the runtime can lerp from plain value to `rotate{...} x` safely.

Operators can also keep their own live arguments:

```monocurl
mesh moved = shift{delta: 0r} Circle(radius: 0.35)
play Set([&moved])

moved.delta = 2r
moved.radius = 0.6
play Lerp(1, [&moved])
```

Use `Lerp` for argument-level interpolation. Use `Trans`, `TagTrans`, `Bend`,
or `TagBend` when the mesh topology or subpart matching needs a specialized
mesh animation.

### Leader / Follower State

A `mesh` declaration creates two related values:

- the leader is the value your code reads and mutates.
- the follower is the value currently visible on screen.

When you assign a mesh variable, you change only the leader. The follower stays
where it was until a `play` statement synchronizes it.

```monocurl
mesh ball = shift{1l} Circle(0.3)
play Set([&ball])          # follower instantly becomes the leader

ball = shift{1r} ball      # only the leader changed; screen has not moved yet
play Lerp(1, [&ball])      # follower moves from old state to new leader
```

After `Set`, or after a `Lerp`/`Trans`/`Fade`/`Grow`/`Write` finishes, the
follower has caught up to the leader. The next assignment sets the next
destination keyframe.

Animations often infer their target leaders automatically. If you write
`play Fade(0.5)` or `play Lerp(1)` after changing one or more mesh leaders, the
primitive animation targets the currently dirty leaders. This keeps simple
scenes concise.

Pass `&ball` to target a leader explicitly: `play Lerp(1, [&ball])`. Do this in
nontrivial scenes, and especially in parallel animations. In `play [a, b]`, a
broad inferred target set can make both branches try to animate the same dirty
leader, which is ambiguous or invalid. Split the work yourself by passing
explicit references to each primitive animation:

```monocurl
left = shift{1l} left
right = shift{1r} right
play [Lerp(1, [&left]), Lerp(1, [&right])]
```

`param` uses the same leader/follower idea, but params are also exposed as
presentation-mode controls. `param` declarations must be top-level. A plain
read of a param uses the leader value; `$param` creates a stateful reference to
the live follower value and may only be used inside labeled calls/operators
assigned to a `mesh` leader.

```monocurl
param radius = 0.8
mesh live_circle = Circle(radius: $radius)
play Set()

radius = 1.6
play Lerp(1)
```

The mesh above stays reactive: while the `radius` follower moves, or while a
viewer edits the parameter in presentation mode, `live_circle` recomputes from
the current follower value.

## Patterns and Anti Patterns

Prefer these patterns:

- Build scenes from small helper lambdas that return mesh values.
- Construct simple shapes at the origin, then place/style them with operators.
- Use labels for semantically important arguments (`radius:`, `delta:`,
  `pos:`, `tex:`, `col:`) so later assignments and interpolation are clear.
- Tag subparts that must persist across transforms, especially text fragments,
  equation pieces, repeated dots, graph regions, and anything animated with
  `TagTrans` or `TagBend`.
- Use `block` / dot accumulation or an explicit `var ret = []` when assembling
  many meshes in a loop.
- Assign destination states first, then play an animation. Treat `play` as the
  point where followers catch up to leaders.
- Target specific leaders with `&mesh` in nontrivial scenes, especially inside
  parallel `play [a, b]`.
- Use `Write` for strokes/text drawing on, `Grow` for appearing from center,
  `Fade` for opacity entrance/exit, `Trans` for general mesh morphs, and
  `TagTrans` for identity-preserving morphs.
- Keep `param` values simple top-level controls and feed them into reactive
  mesh expressions through `$param`.

Avoid these mistakes:

- Do not pass centers/normals to constructors that no longer expose them; use
  `shift`, `center`, `in_space`, or other mesh operators instead.
- Do not store `$param` reactive expressions in `let`, `var`, or `param`; only
  store stateful values in `mesh` leaders.
- Do not mutate the same mesh leader from two parallel animations.
- Do not rely on unlabeled positional arguments for complex calls when labels
  make the code more robust.
- Do not use deterministic `random` / `randint` inside helper lambdas; they are
  root-frame scene operations.
- Do not hand-author low-level mesh topology for ordinary scenes; use stdlib
  constructors/operators and mesh queries.

Useful examples to imitate:

- `assets/default_scenes/language_basics.mcs`: functions, labels, loops.
- `assets/default_scenes/meshes_and_operators.mcs`: constructors/operators/tags.
- `assets/default_scenes/animations.mcs`: leader/follower animation style.
- `assets/default_scenes/parameters.mcs`: live presentation parameters.
- `assets/default_scenes/example_text_and_equations.mcs`: `Text`, `Tex`, tags.
- `assets/default_scenes/example_graphing_riemann_sums.mcs` and `graph.mcs`:
  graphing and plotted curves.
