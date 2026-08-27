# RESUME — feat/stdlib-primitives

Worktree: `/Users/manubhat/home/enigmadux/monocurl/wt-stdlib`, branch
`feat/stdlib-primitives`. Do NOT touch `../monocurl`.

## State of the world

| Commit | Contents | Verified? |
|--------|----------|-----------|
| `4417b6b` | mesh.mcl: Angle, RightAngle, Brace, DashedLine, NumberLine | yes (tests) |
| `a3be690` | Angle reflex fix + integration tests + STDLIB_CHANGES.md | yes — `cargo test -p integration_tests --test basic_executor_tests` = **347 passed, 0 failed** (incl. 9 new `stdlib_primitives::*`) |
| *(uncommitted at pause — commit as WIP)* | **native**: axis `arrow_extrusion = nil` hides arrowheads (`graphs.rs`) + mesh.mcl doc line + STDLIB_CHANGES/RESUME | **NOT compiled** |

## DONE (areas 1, 2, part of 3)

Pure-`.mcl`, fully tested, ready to keep:
- `Angle(vertex, a, b, radius=0.4, samples=32, reflex=0)` — angle arc marker
- `RightAngle(vertex, a, b, size=0.2)` — square corner marker
- `Brace(start, end, depth=0.25, direction=nil, samples=101, label=nil, label_scale=1, label_buffer=0.12)` — smooth curly brace, optional label → `[brace, text]`
- `DashedLine(start, end, lengths=[0.2,0.1], offset=0, normal=1b)` — `dashed{} Line()` convenience
- `NumberLine(min=-5, max=5, tick_spacing=1, major_tick_rate=1, axis_title=nil, label_map=(|x|x), basis=1r, color=[0,0,0,1])` — friendlier `Axis1d`

All doc `##` blocks written. Tests: `crates/integration_tests/tests/basic_executor_tests/stdlib_primitives.rs` (registered in `basic_executor_tests.rs`).

## MID-FLIGHT — native "hide axis arrowheads" (uncommitted / unbuilt)

Edits already made to `crates/stdlib/src/mesh/graphs.rs`:
1. `struct AxisStyle` — added `draw_arrows: bool` field.
2. `AxisStyle::from_range` — sets `draw_arrows: true`.
3. `read_axis_style` List branch — added `let mut draw_arrows = true;`; the
   `arrow_extrusion` slot now checks `Value::Nil` → `draw_arrows = false;
   arrow_extrusion = 0.0`, else parses the number as before.
4. `Ok(AxisStyle { ... draw_arrows })` construction updated.
5. `push_axis_arrows` — new `draw_arrows: bool` last param, early-returns when false.
6. All 6 `push_axis_arrows(...)` call sites pass `<style>.draw_arrows`.

Also: `assets/std/std/mesh.mcl` `axis_style` doc — `arrow_extrusion` param line
notes `nil` hides arrowheads. `axis_style` operator body unchanged (already
threads `arrow_extrusion` positionally, so `nil` flows through).

### NEXT STEPS, in order

1. `git add -A && git commit` (WIP) with co-author trailer — DONE at pause if you
   see a 3rd commit; otherwise do it.
2. `timeout 1200 cargo check -p stdlib 2>&1 | tee /tmp/stdlib.log` — fix any
   compile errors (likely: `elide_cached_wrappers_rec` call form on `raw`, or a
   missed call site).
3. Add a unit test near the other `test_axis_style_*` tests in
   `crates/integration_tests/tests/basic_executor_tests/live_values.rs`:
   build `Axis2d` with `axis_style{... , nil}` and assert the mesh count /
   x-bounds drop vs the default (arrowheads gone). Pattern: copy
   `test_axis_style_arrow_extrusion_controls_bounds`.
4. `timeout 1800 cargo test -p integration_tests --test basic_executor_tests 2>&1 | tail -20`
   — expect 348+ passing.
5. Commit the native change as its own focused commit (area 3) if not already.
6. If it does NOT compile cleanly in ~15 min of effort: `git revert` / drop the
   graphs.rs hunk, mark "hide arrowheads" back under "skipped" in
   STDLIB_CHANGES.md, and ship areas 1–2 + NumberLine only. The pure-.mcl work
   is the guaranteed-good deliverable.

## NOT STARTED (candidate follow-ups, see STDLIB_CHANGES.md "skipped")

- explicit tick-position list / ticks both sides / on-axis number formatting
  (all in `read_axis_style`, `graphs.rs`)
- `Field` length-normalisation modes + colour-by-magnitude (native `mk_field`)
- `Arrow`/`Vector` tip-ratio knobs (native `VectorLikeStyle`)
- `ExplicitFunc` fill-to-axis flag (native `mk_explicit`; today: `ExplicitFuncDiff(f, |x| 0, ...)`)
- `outline{}` / `round_corners{}` operators (need native geometry ops)
- grid major/minor distinction + grid opacity (native `mk_line_grid` / axis grid)

## Open design questions

- **Brace shape**: current profile is a single smooth lobe + outward tip, not a
  true double-ogee `\underbrace`. Fine for grouping; maintainer may want the real
  4-arc brace or a TeX-backed one. Signature is designed to survive that swap.
- **NumberLine `label_map` default** is `|x| x` (compact `%g` via stringify), not
  the axis `DefaultFormat` (which adds sci-notation for huge/tiny values). Chosen
  for predictability on integer number lines. Revisit if wanted.
- **Animating arrowheads on/off** (`nil ↔ number` in an animated `axis_style`)
  may raise an interpolation error. Acceptable? Or make hidden = extrusion `0`
  with a separate bool that lerps as a step function.

## Build notes

- Worktree has its own `target/` (no shared target dir). First cold build of
  `integration_tests` done — cache is warm.
- `.cargo/config.toml` sets `MONOCURL_ASSETS_DIR = assets` (relative), so tests
  read `assets/std/std/*.mcl` from this worktree. Good.
- mcl comments are `#` not `//` (`//` is integer division).
- mcl stdlib modules are NOT cross-imported: inside `mesh.mcl` use
  `__monocurl__native__ <name>` for math (`sin`, `cos`, `arctan2`, `sqrt`,
  `floor`, `min`, `max`, `exp`, `dot`, `range`, `is_nil`, `is_list`).
