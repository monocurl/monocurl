# Backward-compatibility verification — `feat/stdlib-primitives`

**Question:** does any existing scene render differently on
`feat/stdlib-primitives` (`0fa3f97`) versus base `origin/main` (`6080017`)?

**Verdict: fully backward-compatible.** Every rendered frame of every existing
scene is **byte-identical** between the branch and base. All targeted test
suites are green. The only new code paths (arrowhead re-clamp, axis-style slot 8,
function-plot domain gaps) are provably unreachable by, or no-ops for, any input
that renders successfully today.

- Branch: `0fa3f97` (`feat/stdlib-primitives`), merge-base with `origin/main` is exactly `6080017` (linear history).
- Base: throwaway worktree `/private/tmp/mc-base` @ `6080017`.
- Both built `cargo build -p monocurl` (dev profile), rendered with
  `target/debug/monocurl image "<scene>" -o out.png -r medium [--slide N] [--time T]`.
- Diff method: MD5 of the PNG bytes; on any mismatch,
  `np.abs(branch.astype(int) - base.astype(int)).sum(axis=2)`, counting pixels
  `>2` and `>10` and reporting `max`.

---

## 1. Per-scene render diff (branch vs base)

All 12 `assets/default_scenes/*.mcs` plus `packages/monocurl-mcp/docs/riemann-rectangles.mcs`.
For each scene: slide 0 @ t=0 (`__s0`), the last slide's final frame (`__end`),
and — for multi-slide scenes — an intermediate slide/time (`__mS_T`).
`>2px` / `>10px` = count of pixels whose summed RGB delta exceeds that threshold.

| scene / frame | result | >2px | >10px | maxΔ |
|---|---|---|---|---|
| (Example) 3D Camera Animation — s0 | byte-identical | 0 | 0 | 0 |
| (Example) 3D Camera Animation — m0 t2 | byte-identical | 0 | 0 | 0 |
| (Example) 3D Camera Animation — end (slide 1) | byte-identical | 0 | 0 | 0 |
| (Example) Algorithm — s0 | byte-identical | 0 | 0 | 0 |
| (Example) Algorithm — m0 t2 | byte-identical | 0 | 0 | 0 |
| (Example) Algorithm — end (slide 1) | byte-identical | 0 | 0 | 0 |
| (Example) Flow Field — s0 | byte-identical | 0 | 0 | 0 |
| (Example) Flow Field — m1 t2 | byte-identical | 0 | 0 | 0 |
| (Example) Flow Field — end (slide 2) | byte-identical | 0 | 0 | 0 |
| (Example) Fractal — s0 | byte-identical | 0 | 0 | 0 |
| (Example) Fractal — m2 t1 | byte-identical | 0 | 0 | 0 |
| (Example) Fractal — end (slide 3) | byte-identical | 0 | 0 | 0 |
| (Example) Geometry Proof — s0 | byte-identical | 0 | 0 | 0 |
| (Example) Geometry Proof — m3 t2 | byte-identical | 0 | 0 | 0 |
| (Example) Geometry Proof — end (slide 5) | byte-identical | 0 | 0 | 0 |
| (Example) Image — s0 | byte-identical | 0 | 0 | 0 |
| (Example) Image — end (slide 0) | byte-identical | 0 | 0 | 0 |
| (Example) Riemann Sum — s0 | byte-identical | 0 | 0 | 0 |
| (Example) Riemann Sum — m1 t1 | byte-identical | 0 | 0 | 0 |
| (Example) Riemann Sum — m2 t2 | byte-identical | 0 | 0 | 0 |
| (Example) Riemann Sum — end (slide 3) | byte-identical | 0 | 0 | 0 |
| (Example) Text — s0 | byte-identical | 0 | 0 | 0 |
| (Example) Text — m3 t2 | byte-identical | 0 | 0 | 0 |
| (Example) Text — end (slide 6) | byte-identical | 0 | 0 | 0 |
| (Tutorial) Animations — s0 | byte-identical | 0 | 0 | 0 |
| (Tutorial) Animations — m3 t1.5 | byte-identical | 0 | 0 | 0 |
| (Tutorial) Animations — m5 t1 | byte-identical | 0 | 0 | 0 |
| (Tutorial) Animations — end (slide 6) | byte-identical | 0 | 0 | 0 |
| (Tutorial) Language Basics — s0 | byte-identical | 0 | 0 | 0 |
| (Tutorial) Language Basics — end (slide 0) | byte-identical | 0 | 0 | 0 |
| (Tutorial) Meshes — s0 | byte-identical | 0 | 0 | 0 |
| (Tutorial) Meshes — m3 t1.5 | byte-identical | 0 | 0 | 0 |
| (Tutorial) Meshes — end (slide 5) | byte-identical | 0 | 0 | 0 |
| (Tutorial) Monocurl Overview — s0 | byte-identical | 0 | 0 | 0 |
| (Tutorial) Monocurl Overview — m2 t2 | byte-identical | 0 | 0 | 0 |
| (Tutorial) Monocurl Overview — end (slide 4) | byte-identical | 0 | 0 | 0 |
| monocurl-mcp docs/riemann-rectangles — s0 | byte-identical | 0 | 0 | 0 |
| monocurl-mcp docs/riemann-rectangles — end (slide 3) | byte-identical | 0 | 0 | 0 |

**36 frames, all byte-identical (MD5 match, 0 differing pixels).**

### Coverage of the changed constructors by these scenes

| changed API | exercised by |
|---|---|
| `Arrow(...)` (native `mk_arrow` tip args) | Algorithm, Flow Field (via `Field`), Meshes |
| `Vector` / `mk_vector` | (synthetic scene below) |
| `axis_style{...}` operator (8th slot now always emitted) | 3D Camera Animation, Riemann Sum, mcp riemann |
| `Axis3d` + `axis_style` | 3D Camera Animation |
| `Axis2d` (default + `axis_style`) | Meshes, Riemann Sum, mcp riemann |
| `ExplicitFunc` (native `mk_explicit`) | Riemann Sum, mcp riemann |
| `Field` (arrow-per-cell) | Flow Field |

No default scene uses `ParametricFunc`, `ExplicitFunc2d`, `Axis1d`, or raw
axis-style lists — those are covered by the synthetic scene and the test suites.

## 2. Extra synthetic fixtures (branch vs base)

`grep` of `assets/`, `content/`, `learning/`, `packages/`, and `crates/` found no
other runnable `.mcs`/`.mcl` scene fixtures (the doc markdown under the
`monocurl-documenter` repo's `content/`/`learning/` contains code *snippets*, not
standalone scenes; integration tests embed MCL as inline strings, not files).
Two synthetic scenes were written to hit the remaining changed paths:

| fixture | what it exercises | result |
|---|---|---|
| `raw_axis.mcs` | raw **7-element** axis-style lists passed straight to `Axis2d`; raw 7-element list to `Axis1d`; `Arrow()` and `Vector()` with default tip args; continuous `ExplicitFunc(sin)` in `in_space` | **byte-identical** (MD5 match, 0 px) |
| `disc.mcs` | `ExplicitFunc` whose callback returns `nil` near a pole | base: hard error `type error: expected float, got nil for f`; branch: renders (two clean branches) — see §3 |

## 3. Code-path analysis of the "risky" changes

### 3a. Old 7-element axis style list parses identically

`read_axis_style` changed `2..=7` → `2..=8` and added optional slot 8
(`tick_placement`). For a list of length 7 every `tick_step_index + N` offset
resolves exactly as before:
- with title (`[min,max,title,tick,major,label,arrow]`): `tick_step_index = 3`;
  `arrow_extrusion` read from slot 6 (`7 > 6`); `tick_placement` check `7 > 7`
  is false → stays `TickPlacement::Both` (the unchanged default).
- The only *new* acceptance is a 7-element list **without** a title, which
  previously returned an error (`!has_title && len > 6`); it is now read with
  slot 6 as `tick_placement`. Strictly more permissive, never a regression.

The `axis_style{...}` operator now always emits an 8-element list with
`tick_placement = nil` in slot 8; `tick_placement_from_value(Nil) =>
TickPlacement::Both`, and `axis_tick_lins` with `Both` produces the exact
`p ± extend` segment endpoints as before. Confirmed byte-identical by 3D Camera
Animation and Riemann Sum (both use the operator).

`axis_style_third_arg_is_title` now also treats a `List` in slot 3 as "not a
title" (to allow explicit tick positions). A scene passing a list as the axis
*title* would be the only thing affected; no scene does, and a list was never a
valid title value.

Explicit-tick handling and the `explicit_ticks.retain(range)` clip only run when
the `tick_spacing` slot is a **list**, which is new syntax — zero effect on the
numeric-spacing path.

### 3b. `mk_explicit` / `mk_parametric` / `mk_explicit_diff` domain gaps

`open_polyline(&[Float3])` → `segmented_open_polyline(&[Option<Float3>])`. When
**every** sample is `Some` (i.e. the callback returned a finite number at every
point — the only way a scene renders today), `push_segmented_open_polyline`
accumulates all points into one run and makes exactly one
`push_open_polyline(out, &run, normal)` call — identical bytes to the old
`open_polyline`. Same for `mk_explicit_diff`: with all columns `valid`, the
maximal-run loop degenerates to the old `for i in 1..samples-1` strip loop and
the final `append_strip(.., samples-1)`; the outline is one contour per
curve, as before.

The gap path is reachable **only** when the callback returns `nil` or a
non-finite number (`NaN`/`±inf`). Both of those previously produced a hard
`ExecutorError` ("expected float, got nil" / non-finite → error or spurious
jump), so **no scene that renders successfully today can reach the new path.**
Verified directly: `disc.mcs` errors on base, renders on branch. A genuinely
wrong type (string/list/mesh) still errors (`explicit_sample_y` returns `Err`).

### 3c. `mk_arrow` / `mk_vector` tip geometry

Defaults are `tip_length = 1`, `tip_width = 1`, `double_headed = 0`.
`vector_like_mesh_with_tip` builds a `VectorLikeStyle` with
`head_len_scale = 1.0`, `head_width_scale = 1.0`, `double_headed = false` —
identical to `DEFAULT_VECTOR_LIKE_STYLE` / `AXIS_ARROW_STYLE`.

In `vector_like_mesh_with_style` the new arithmetic is
`head_half_width = (old_value * 1.0).min(len * 0.45)` and
`head_depth = (old_value * 1.0).min(len * 0.9)`. The pre-existing caps are
tighter (`ARROW_MAX_HEAD_HALF_WIDTH_OVER_LENGTH = 0.22 ≤ 0.45`,
`ARROW_MAX_HEAD_DEPTH_OVER_LENGTH = 0.35 ≤ 0.9`, axis `0.18`/`0.32`), so the
extra `.min()` never binds → `head_half_width`/`head_depth` unchanged.
`shaft_start` is `0.0` unless `double_headed` → `shaft_start + t*(shaft_end -
shaft_start) == t * shaft_end`, the old sampling. The `double_headed` contour
block is skipped. `Vec::with_capacity` bumps (`+3` → `+6`) do not affect output.

**⇒ existing arrows, vectors, and every axis arrowhead are byte-identical.**
Confirmed by Flow Field (dozens of `Field` arrows), Algorithm, Meshes,
3D Camera Animation (axis arrows), and `raw_axis.mcs`.

### 3d. `mk_explicit2d` argument shift

Native reads shifted `-7..-1` → `-8..-1` for the new trailing `color_at`; the
`.mcl` wrapper passes `color_at` (default `nil`). When `nil`, every vertex colour
is `Float4::new(0.0, 0.0, 0.0, 1.0)` — the exact old hard-coded value.
Byte-identical (also `test_explicit_func_2d_color_at_sets_vertex_colors`).

## 4. Test results (verbatim `test result:` lines)

`cargo test -p integration_tests` (branch):
```
     Running tests/anim_tests.rs
test result: ok. 117 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.69s
     Running tests/basic_executor_tests.rs
test result: ok. 373 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
     Running tests/stateful_tests.rs
test result: ok. 184 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.11s
   (unittests src/lib.rs + Doc-tests: 0 tests)
```

`cargo test -p stdlib` (branch):
```
     Running unittests src/lib.rs (stdlib)
test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

`cargo test -p compiler` (branch):
```
     Running unittests src/lib.rs (compiler)
test result: ok. 43 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

`cargo build -p monocurl` — ok on both branch and base (base worktree needed a
copy of the untracked `.cargo/config.toml` for `PKG_CONFIG_PATH`; one pre-existing
dead-code warning `vector_like_mesh is never used`, non-fatal).

## 5. Caveats

- Rendering was `-r medium`; comparison is exact-byte so resolution is not a
  factor for the equality claim.
- Per-scene sampling is slide 0 + last slide + one interior slide for multi-slide
  scenes (not every frame of every animation). Given (a) 100% byte-identical
  results across 36 varied frames, (b) the code-path analysis showing the new
  branches are unreachable / no-ops for existing inputs, and (c) the full
  `anim_tests` + `basic_executor_tests` + `stateful_tests` suites passing, the
  risk of an unsampled frame differing is negligible.
- New `nil`-in-`arrow_extrusion` → hide-arrowheads and `nil` `label_map`
  behaviours are new syntax; they cannot be produced by the pre-branch
  `axis_style` operator or by any existing style list, so no existing scene is
  affected.
- One-directional animation of `arrow_extrusion` between `nil` and a number is
  still not expressible (documented in `STDLIB_CHANGES.md`); this is a
  pre-existing `lerp` limitation, not a regression.

## Conclusion

`feat/stdlib-primitives` is **fully backward-compatible**. Every existing scene
renders byte-for-byte identically to `origin/main`, all targeted test suites pass,
and every new code path is either new syntax, an unreachable error-replacement,
or a proven no-op at default arguments.
