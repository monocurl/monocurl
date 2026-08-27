# Standard library additions — `feat/stdlib-primitives`

Goal: round out the mesh standard library into a more complete, composable
"primitive set" for explanatory diagrams — favouring small, broadly-useful
building blocks over niche chart types.

Scope of this branch: mostly **pure-`.mcl` additions to
`assets/std/std/mesh.mcl`** (new constructors, no changes to existing ones),
plus **one small native change** in `crates/stdlib/src/mesh/graphs.rs` to let
axis arrowheads be turned off. No signature or behaviour changes to any existing
constructor or operator, so every existing scene keeps working byte-for-byte
(the axis change is opt-in via `nil`).

Almost everything added is pure `.mcl` composing constructors/operators that
already exist natively (`mk_polyline`, `mk_line`, `op_dashed`, `mk_label`,
`mk_axis1d`, `mk_arrow`, `mk_field`, `mk_explicit_diff`, `mk_dot`), so there is
no new native surface to maintain. The one native change is the axis
arrowhead toggle in §3.

Quick index of what's new: `Angle`, `RightAngle`, `Brace`, `DashedLine`,
`NumberLine`, `VectorField` (new constructors); `ExplicitFunc` gains
`endpoint_dots` + `fill`; `ParametricFunc` gains `endpoint_dots`; axis style
lists accept `arrow_extrusion = nil` to hide arrowheads.

---

## 1. Annotation / geometry markers

### `Angle` — NEW

Angle arc marker at a vertex, between two rays. This is the single most-requested
missing diagram primitive (test scenes already hand-roll an `AngleLabel`).

```
let Angle = |vertex, a, b, radius = 0.4, samples = 32, reflex = 0| ...
```

| arg | default | meaning |
|-----|---------|---------|
| `vertex` | — | corner point of the angle |
| `a` | — | a point on the first ray (direction is `a - vertex`) |
| `b` | — | a point on the second ray (direction is `b - vertex`) |
| `radius` | `0.4` | arc radius in scene units |
| `samples` | `32` | segments approximating the arc (clamped to ≥ 2) |
| `reflex` | `0` | truthy → mark the outer (`> PI`) angle instead of the inner one |

Behaviour: returns an open stroke polyline (`mesh_rank == 1`) in the XY plane,
sweeping the **shorter** way from the `vertex→a` direction to the `vertex→b`
direction (or the longer way when `reflex` is truthy). Authored in world space
(not at the origin), because it is positioned by its `vertex` argument like
`Line`/`Arrow`. Style with `stroke{...}`; pair with `Tex`/`Label` for the measure.

```
# before: no primitive; users composed mk_arc + arctan2 + shift by hand
# after:
mesh a  = stroke{CYAN} Angle(ORIGIN, 1r, 1u + 1r)
mesh lbl = to_side{...} Tex("\theta", 0.4)
```

### `RightAngle` — NEW

Square "perpendicular" corner marker.

```
let RightAngle = |vertex, a, b, size = 0.2| ...
```

| arg | default | meaning |
|-----|---------|---------|
| `vertex` | — | corner point |
| `a` | — | point on the first leg direction |
| `b` | — | point on the second leg direction |
| `size` | `0.2` | leg length of the marker |

Behaviour: a 3-point open polyline forming an `L` (`size × size`) in the plane of
the two rays' XY projection. The rays need not be exactly perpendicular; the
marker is always a square of the requested size along the two ray directions.

```
# after:
mesh r = stroke{CYAN} RightAngle(ORIGIN, 1r, 1u)
```

### `Brace` — NEW

Curly brace spanning two points, bulging to one side, with an optional label.

```
let Brace = |start, end, depth = 0.25, direction = nil, samples = 101,
             label = nil, label_scale = 1, label_buffer = 0.12| ...
```

| arg | default | meaning |
|-----|---------|---------|
| `start`, `end` | — | endpoints of the span |
| `depth` | `0.25` | bulge height at the midpoint, in scene units |
| `direction` | `nil` | bulge direction; component along the span is removed. `nil` → the `-90°` normal of `start→end` (i.e. bulges "below" a left-to-right span) |
| `samples` | `101` | segments in the brace stroke (clamped to ≥ 8) |
| `label` | `nil` | string → result becomes `[brace, text]` with the label past the tip; `nil` → just the brace mesh |
| `label_scale` | `1` | label text scale |
| `label_buffer` | `0.12` | gap between brace tip and label |

Behaviour: a smooth stroke polyline in the XY plane. It meets both endpoints
tangent to the span and swells to `depth` at the midpoint, where a sharper tip
points outward. Profile is `depth · (0.7·(½ − ½cos πm) + 0.3·e^{−k²}·0.5)` with
`m = 2·min(u, 1−u)`, `k = (u − 0.5)/0.05`. When `label` is set the return value
is a 2-element mesh list, matching the `[seg, Label(...)]` idiom used by
`Measure`.

```
# after:
mesh b      = stroke{CYAN} Brace(1.5l, 1.5r, 0.3)
mesh titled = stroke{CYAN} Brace(1.5l, 1.5r, 0.3, DOWN, 101, "width")
```

Note for the maintainer: this profile is a single-lobe smooth brace (an
"underbrace bump" with an outward tip), not a true double-ogee `\underbrace`
glyph. It reads well for grouping/annotation. If a native TeX-brace is wanted
later this wrapper can be swapped to call it without changing the signature.

---

## 2. Lines

### `DashedLine` — NEW

Convenience for `dashed{lengths, offset} Line(start, end, normal)` — the docs
list "making `dashed{}` easier" as a goal, and a dashed segment is the single
most common use.

```
let DashedLine = |start = [0, 0, 0], end = [1, 0, 0],
                  lengths = [0.2, 0.1], offset = 0, normal = 1b| ...
```

| arg | default | meaning |
|-----|---------|---------|
| `start`, `end` | origin, `[1,0,0]` | endpoints |
| `lengths` | `[0.2, 0.1]` | dash length, or `[dash, gap]`; a scalar uses the same value for both |
| `offset` | `0` | dash phase offset |
| `normal` | `1b` | preferred stroke normal |

Behaviour: identical to applying the existing `dashed` operator to a `Line`.
Returns `[solid_line, dashed_line]` (the same interpolation pair the `dashed`
operator produces), so animating it dashes in.

```
# before:
mesh guide = dashed{[0.15, 0.08]} stroke{LIGHT_GRAY} Line(1.5l, 1.5r)
# after:
mesh guide = stroke{LIGHT_GRAY} DashedLine(1.5l, 1.5r, [0.15, 0.08])
```

---

## 3. Axes

### Hide axis arrowheads — `arrow_extrusion = nil` (native, `graphs.rs`)

`Axis1d` / `Axis2d` / `Axis3d` always drew an arrowhead at each end of every
axis; there was no way to turn them off. Now the `arrow_extrusion` slot of an
axis style list (position 7, i.e. the last):

- `nil` → no arrowheads; the axis title and bounds sit exactly at `range.max`.
- a number (default `0.2`, `0` allowed) → unchanged: arrowhead of that
  extrusion length (`0` = arrowhead flush with the axis end).

Old signature (unchanged shape): style list
`[min, max, (axis_title,) tick_spacing, major_tick_rate, label_map, arrow_extrusion]`.

Native: `AxisStyle` gains a `draw_arrows: bool` field (default `true`);
`read_axis_style` sets it `false` when the slot is `Value::Nil`;
`push_axis_arrows` takes a `draw_arrows` argument and early-returns.

**Animating arrowheads in/out:** `lerp` errors on `lerp(nil, number)` and on any
*unlabeled* `axis_style` argument that differs between the two endpoints, so
animate a **labeled numeric** `arrow_extrusion` between a tiny value and the
target — the arrows grow out as it goes:
`axis_style{"x", -2, 2, nil, 0.25, 4, nil, arrow_extrusion: 0.01}` →
`... arrow_extrusion: 0.4`.
(`test_axis_style_arrow_extrusion_interpolates`.) Fully hiding→showing (`nil ↔
number`) in one animation still isn't expressible; that needs runtime lerp
support for `nil`.

```
# before: arrowheads always present
mesh axes = axis_style{"x", 0, 4, "x"} Axis2d()
# after: clean axis with no arrowheads
mesh axes = axis_style{"x", 0, 4, "x", 0.25, 4, |x| x, nil}
            axis_style{"y", 0, 3, "y", 0.25, 4, |x| x, nil} Axis2d()
```

### Explicit axis tick positions — native (`graphs.rs`)

The `tick_spacing` slot of an axis style list may now be **a list of exact tick
positions** instead of a uniform step:

- a number → uniform ticks every `n` units (unchanged).
- a list `[x0, x1, ...]` → a tick at each listed position, each drawn as a
  **major** tick (and labelled, subject to `label_map`). Positions outside the
  axis range are dropped. `major_tick_rate` is ignored in this mode.

Works for `Axis1d` / `Axis2d` / `Axis3d` per-axis, via `axis_style` or a raw
style list, and via `NumberLine`. Native: `AxisStyle` gains
`explicit_ticks: Vec<f32>` plus `ticks()` / `tick_budget()` helpers; the three
`mk_axis*` functions call those instead of `axis_tick_values` / `tick_count`
directly. Zero change when `tick_spacing` is a number
(`test_axis_style_explicit_tick_positions`,
`test_axis_style_explicit_ticks_place_labels_at_requested_values`).

```
# before: only uniform spacing
mesh ax = axis_style{"x", 0, 10, "t", 2} Axis2d()
# after: label exactly the interesting points
mesh ax = axis_style{"x", -1, 5, "t", [-1, 0, 1, 2.5, 5]} Axis2d()
mesh nl = NumberLine(0, 7, [0, 3.5, 7], 1, nil, |x| Number(x, 1))
```

### `NumberLine` — NEW

A friendlier `Axis1d`: a labelled 1-D number line where **every** tick is
labelled by default (`Axis1d`/`Axis2d` default `major_tick_rate` to 4).

```
let NumberLine = |min = -5, max = 5, tick_spacing = 1, major_tick_rate = 1,
                  axis_title = nil, label_map = (|x| x),
                  basis = 1r, color = [0, 0, 0, 1]| ...
```

| arg | default | meaning |
|-----|---------|---------|
| `min`, `max` | `-5`, `5` | ends of the line in axis coordinates |
| `tick_spacing` | `1` | distance between ticks in axis coordinates |
| `major_tick_rate` | `1` | every nth tick is major / labelled (**1**, unlike `Axis1d`'s 4) |
| `axis_title` | `nil` | optional title past the positive end |
| `label_map` | `|x| x` | `nil` → unlabelled ruler; a `|value| ...` callable → custom tick text (e.g. `|x| Number(x, 1)`) |
| `basis` | `1r` | scene-space direction and unit length of one axis step |
| `color` | black | axis color |

Behaviour: thin wrapper — builds the 7-element `x_axis` style list
`[min, max, axis_title, tick_spacing, major_tick_rate, label_map, 0.2]` and calls
`mk_axis1d(basis, 1b, color, style)`. No new native behaviour. All existing
`Axis1d` capabilities (tick styling via the style list, `in_space`, animation
via `axis_style`) still apply if you drop down to `Axis1d`.

```
# before:
mesh line = Axis1d(1r, 1b, [0,0,0,1], [0, 10, nil, 1, 1, |x| x, 0.2])
# after:
mesh line = NumberLine(0, 10, 1)
mesh half = NumberLine(-2, 2, 0.5, 2, "x", |x| Number(x, 1))
mesh bare = NumberLine(0, 10, 1, 1, nil, nil)   # ruler, no numbers
```

### Tick-label number formatting — already supported, now documented

No code change. `axis_style`'s `label_map` (and `NumberLine`'s) already accepts
any `|value| ...` callable, so fixed-decimal / `Number`-style tick text is:

```
mesh axes = axis_style{"x", 0, 1, "p", 0.1, 2, |x| Number(x, 2)} Axis2d()
mesh line = NumberLine(0, 1, 0.1, 2, nil, |x| Number(x, 2))
```

---

## 4. Function plots

### `ExplicitFunc` — `endpoint_dots` + `fill` (NEW optional args)

```
# before:
let ExplicitFunc = |f, x_min_max_samples = [-5, 5, 128]| ...
# after:
let ExplicitFunc = |f, x_min_max_samples = [-5, 5, 128], endpoint_dots = 0, fill = nil| ...
```

| new arg | default | meaning |
|---------|---------|---------|
| `endpoint_dots` | `0` | truthy → append two visible `Dot`s at `(min, f(min))` and `(max, f(max))` |
| `fill` | `nil` | RGBA colour → also shade the area between the curve and the x axis (delegates to `mk_explicit_diff(f, |x| 0, ...)`) |

Behaviour: **unchanged** when both are falsy/`nil` — returns the bare polyline.
When either is set the result is a flat mesh list:
`[ <fill pos region>, <fill neg region>, <fill outline>, curve, <dot>, <dot> ]`
(fill parts only if `fill`, dots only if `endpoint_dots`). `f` still must return
a number at every sample — no discontinuity/pole handling (see skipped).

```
# after:
mesh shaded = stroke{CYAN} ExplicitFunc(|x| sin(x), [0, PI, 160], 0, [0.2, 0.6, 0.9, 0.4])
mesh dotted = stroke{CYAN} ExplicitFunc(|x| x*x, [-2, 2, 120], 1)
```

### `ParametricFunc` — `endpoint_dots` (NEW optional arg)

```
# before: let ParametricFunc = |f, t_min_max_samples = [0, 1, 64]| ...
# after:  let ParametricFunc = |f, t_min_max_samples = [0, 1, 64], endpoint_dots = 0| ...
```

`endpoint_dots` truthy → returns `[curve, Dot(f(t_min)), Dot(f(t_max))]`;
otherwise unchanged (bare polyline).

### Discontinuity / domain-gap handling — native (`graphs.rs`)

`mk_explicit` and `mk_parametric` previously **errored** if the callback returned
a non-number (or built a polyline with a huge vertical jump across a pole). Now:

- callback returns `nil` → that sample is a **gap**
- callback returns a non-finite number (`NaN`, `±inf`) — e.g. `sqrt(x*x-1)` where
  the radicand is negative, `ln(-1)`, `1/(x*x)` blowing up — → that sample is a gap
- a genuinely wrong type (string, list of the wrong length, mesh) → still an error

At each gap the polyline is **split into a separate contour** (via
`push_open_polyline` per contiguous run of ≥2 valid samples) instead of drawing a
vertical line across the discontinuity. The result is still a single mesh; style
operators, `stroke`, `Write`, etc. treat all contours together.

Native detail: new helpers `explicit_sample_y` / `parametric_sample_point`
(return `Option`), and `segmented_open_polyline(&[Option<Float3>], normal)`.
`mk_explicit`/`mk_parametric` build `Vec<Option<Float3>>` and call it.

Zero behaviour change for a callback that returns a finite number at every
sample (test `test_explicit_func_continuous_unchanged`).

```
# before: runtime error "expected float, got nil"  (or a spurious vertical line)
# after:  two clean branches
mesh hyperbola = stroke{CYAN} ExplicitFunc(
    |x| { if (abs(x) < 0.02) { return nil }; return 1 / x }, [-3, 3, 300])
mesh sqrt_gap  = stroke{CYAN} ExplicitFunc(|x| sqrt(x * x - 1), [-3, 3, 300])
mesh piecewise = stroke{CYAN} ParametricFunc(
    |t| { if (t > 0.4 and t < 0.6) { return nil }; return [cos(t*TAU), sin(t*TAU), 0] },
    [0, 1, 200])
```

`ExplicitFunc`'s `fill` option still assumes a continuous curve
(`mk_explicit_diff` is unchanged — see skipped).

---

## 5. Vector fields

### `VectorField` — NEW

The standard "arrow at every grid point" helper. `Field` stays fully generic;
this wraps it with the vector-field concerns the brief called out (length
normalisation mode, colour-by-magnitude).

```
let VectorField = |f, x_min_max_samples = [-1, 1, 11], y_min_max_samples = [-1, 1, 11],
                   mode = "normalized", length = 0.15, color_at = nil, mask = |pos| 1| ...
```

| arg | default | meaning |
|-----|---------|---------|
| `f` | — | `pos -> 3-D vector` |
| `x_min_max_samples` / `y_min_max_samples` | `[-1,1,11]` | grid `[min, max, samples]` |
| `mode` | `"normalized"` | `"true"` (raw vector), `"normalized"` (every arrow length `length`), `"clamped"` (true direction, length capped at `length`) |
| `length` | `0.15` | arrow length (`"normalized"`) / cap (`"clamped"`); ignored for `"true"` |
| `color_at` | `nil` | optional `(pos, magnitude) -> RGBA`; recolours each arrow |
| `mask` | all | `pos -> truthy` cell filter |

Behaviour: returns the same shape as `Field` — a list of `Arrow` meshes, one per
unmasked grid point. Zero-magnitude points in `"normalized"` mode produce a
degenerate (dot) arrow.

```
# before: hand-rolled
mesh f = Field(|p, i| color{...} Arrow(p, p + 0.15*normalize(fn(p))), [-2,2,13], [-2,2,13])
# after:
mesh f = VectorField(fn, [-2, 2, 13], [-2, 2, 13], "normalized", 0.15,
                     |p, mag| [mag / 3, 0.4, 0.9, 1])
```

---

## 6. Grids — already supported, documented

No code change. Major/minor grid lines already exist on `Axis2d`/`Axis3d`
(`grid_color` + `major_tick_rate` split thick/thin, thick/faint). For a
standalone `LineGrid`, compose two at different densities, and use `alpha{}` for
opacity:

```
mesh grid = [
    stroke{alpha{0.15} GRAY} LineGrid([-4, 4, 33], [-3, 3, 25]),   # minor
    stroke{alpha{0.40} GRAY} LineGrid([-4, 4,  9], [-3, 3,  7])    # major
]
```

---

## Tests

- `crates/integration_tests/tests/basic_executor_tests/stdlib_primitives.rs`
  (new, registered in `basic_executor_tests.rs`) — 21 cases: `Angle` rank +
  reflex sweep, `RightAngle` square, `Brace` span/bulge/label-pair, `DashedLine`
  splitting (list + scalar `lengths`), `NumberLine` label density,
  `VectorField` arrow count + normalized-vs-true length + `color_at`,
  `ExplicitFunc` `endpoint_dots` + `fill`, `ParametricFunc` `endpoint_dots`,
  `ExplicitFunc`/`ParametricFunc` gap-splitting (`nil` + non-finite) + a
  continuity-unchanged regression check.
- `crates/integration_tests/tests/basic_executor_tests/live_values.rs` —
  `test_axis_style_nil_arrow_extrusion_hides_arrowheads` (native axis change).

Full run: `cargo test -p integration_tests --test basic_executor_tests` →
**358 passed, 0 failed**; `--test anim_tests` → **117 passed, 0 failed**.

---

## Deliberately skipped (and why)

Need native changes with wide blast radius; noted for a follow-up branch:

- **`ExplicitFuncDiff` / `ExplicitFunc(fill:)` gap handling** — `mk_explicit_diff`
  still assumes both `f` and `g` are continuous over the domain; a `nil`/NaN
  column would break its fill-strip run logic. The plain curve from
  `ExplicitFunc` (no `fill`) does split correctly.
- **One-sided ticks** — axis ticks are drawn symmetric about the axis line
  (`axis_tick_lins` builds `p ± side*extend`). A "ticks only below the axis"
  option would add a style slot + a branch there; not done.
- **`Arrow`/`Vector` tip-ratio / double-headed / tip-style knobs** —
  `VectorLikeStyle` and the head geometry are hard-coded in
  `constructors.rs::vector_like_mesh`; exposing ratios means threading a style
  through `mk_arrow`/`mk_vector`/`mk_half_vector` + the mcl signatures + every
  `Field`/`VectorField` call. `VectorField`'s length modes cover the common need.
- **`ExplicitFunc2d` per-cell colour / `Field` native colour pass** — native
  `mk_explicit2d` / `mk_field`.
- **`outline{}` / `round_corners{}` operators** — no clean pure-`.mcl`
  composition; need native geometry ops.
- **Standalone `LineGrid` major/minor as one styled mesh** — `mk_line_grid`
  returns one flat mesh with no tag structure to select a subset; compose two
  `LineGrid`s instead (see §6).
