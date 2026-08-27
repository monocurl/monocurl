# Standard library additions — `feat/stdlib-primitives`

Goal: round out the mesh standard library into a more complete, composable
"primitive set" for explanatory diagrams — favouring small, broadly-useful
building blocks over niche chart types.

Scope of this branch: **pure-`.mcl` additions to `assets/std/std/mesh.mcl`
only.** No native (`crates/stdlib`) changes. No signature or behaviour changes
to any existing constructor or operator, so every existing scene keeps working
byte-for-byte.

Everything added is built by composing constructors/operators that already
exist natively (`mk_polyline`, `mk_arc`, `mk_line`, `op_dashed`, `mk_label`,
`mk_axis1d`), so there is no new native surface to maintain.

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

---

## Tests

`crates/integration_tests/tests/basic_executor_tests/stdlib_primitives.rs`
(new, registered in `basic_executor_tests.rs`) — 10 cases covering rank,
edge/vertex counts, bounding boxes, the reflex sweep, dashed splitting,
`Brace` label pairing, and `NumberLine` label density vs `major_tick_rate`.

---

## Deliberately skipped (and why)

These were on the suggested list but need native changes that are risky to do
well without a broad test pass; left for a follow-up branch:

- **Hide axis arrowheads** — `push_axis_arrows` is unconditionally called in
  `graphs.rs`; needs an `Option<f32>` / `draw_arrows` flag threaded through all
  three axis constructors + `axis_title_anchor`.
- **Explicit tick-position list / ticks-both-sides / tick number formatting on
  the axis itself** — all live in `read_axis_style` list parsing in `graphs.rs`.
- **`Field` length-normalisation modes / colour-by-magnitude** — native `mk_field`.
- **`Arrow`/`Vector` tip-ratio knobs** — `VectorLikeStyle` is native-only today.
- **`ExplicitFunc` fill-to-axis** — already expressible as
  `ExplicitFuncDiff(f, |x| 0, ...)`; a dedicated flag is native work on
  `mk_explicit`.
- **`outline{}` / `round_corners{}` operators** — no clean pure-`.mcl`
  composition; need native geometry ops.
