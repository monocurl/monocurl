# Renderer depth-stability + transparency changes

Branch `fix/render-depth-transparency`. Targets two maintainer-reported problems:

1. **Flicker/z-fighting when geometry shares a z value** (a line coplanar with a
   triangle). Fine head-on, "becomes weird" when the camera moves.
2. **3D partial transparency wrong** — a translucent object shows only part of
   itself / of other translucent objects; depth writes from translucent
   triangles occlude regions that should blend through.

## Root cause (from the Opus design consult)

- The old ordering knob (`z_offset`, `DEPTH_STEP = 1e-6`) is applied in the
  shader as `clip.z -= bias * clip.w`, i.e. a **constant offset in NDC z (0..1)**.
  NDC z is very non-linear in eye space (`d(ndc_z)/dd ≈ near·far/((far-near)·d²)`),
  so a fixed step is worth ~1.6e-4 world units at eye depth 4 and ~4e-3 at depth
  20 — the wobble you feel when the camera moves.
- That is **not** the main flicker source. Line and dot geometry is extruded in
  the vertex shader along `cross(tangent, (0,0,1))` **in camera space**, whose
  z is identically 0. So a stroke quad / dot disc is a ribbon **parallel to the
  image plane**, sitting at the constant eye depth of its center — it is *not*
  coplanar with the fill it decorates. Tilt the fill by θ and its depth under
  the stroke sweeps `±(stroke half-width in eye space)·tanθ`, which for a 4px
  stroke at a typical camera is ≈1e-4 NDC ≈ **100× `DEPTH_STEP`**. Half the
  stroke sinks into the fill; MSAA turns that into sparkle. The mismatch shrinks
  like `1/d` while `DEPTH_STEP` is constant, which is exactly why "flat is fine,
  moving the camera is not".
- Hardware `DepthStencilState.bias` cannot help: `slope_scale` multiplies the
  primitive's *own* depth slope (≈0 for an image-plane-parallel ribbon), and on
  the Metal backend `bias.constant` is an `i32` cast straight to `f32`, unusable
  for sub-unit offsets. Left at `Default` everywhere.
- Transparency: all three primitive pipelines wrote depth
  (`depth_write_enabled: true`). A translucent triangle drawn first therefore
  depth-occludes anything translucent behind it, instead of blending. Manim
  disables the depth write mask for transparent geometry and draws it
  back-to-front (no OIT); that is the target.

## What changed

### 1. Perspective-correct eye-space "decal" bias for lines & dots (`blade.wgsl`)

`project_camera` / `project` take a new `eye_bias` argument in **eye-space world
units**. It is applied only to `clip.z` (via `biased_camera_z = min(camera_z +
eye_bias, -near)`); `clip.w` stays the true depth, so the vertex does **not move
on screen** — it slides along its own view ray. This is a true depth-only decal.

`vs_line` and `vs_dot` compute
`eye_bias = min(width_eye · DECAL_SCALE, eye_depth · DECAL_MAX_FRACTION)` where
`width_eye` is the primitive's own eye-space half-width (already computed for the
screen-space extrusion), `DECAL_SCALE = 2.0` (covers surface tilt up to
`atan(2) ≈ 63°`), `DECAL_MAX_FRACTION = 0.05` (guard, essentially never binds).
Because `width_eye ∝ eye_depth`, `eye_bias/d` is constant (~0.008 for a 4px
stroke): **camera-distance invariant and self-limiting** — the property the
fixed NDC step lacked.

`vs_triangle` passes `eye_bias = 0.0`. **With `eye_bias == 0.0` the projection is
bit-identical to the previous code** (`biased_camera_z` is only recomputed inside
`if (eye_bias > 0.0)`), so opaque fills are unchanged.

### 2. Depth-write split (`pipelines.rs`)

One `DepthStencilState` became two, both `LessEqual`, both `bias: Default`:

| pipeline | depth test | depth write |
|---|---|---|
| `triangles` (opaque) | yes | **yes** |
| `triangles_blend` (new, transparent) | yes | **no** |
| `lines` | yes | **no** (was yes) |
| `dots` | yes | **no** (was yes) |

Lines/dots no longer writing depth is what makes the decal bias safe: a stroke
can only ever win against its own fill, it can **never occlude geometry painted
after it** (so 2D paint order is preserved exactly). Stroke-vs-stroke is now pure
painter order — same as Manim.

### 3. Opaque / transparent classification (`geometry.rs`, `resources.rs`, `renderer.rs`)

A mesh renders in the transparent pass iff
`is_transparent = uniform.alpha < 1.0 || any_partial_alpha_vertex || texture_has_alpha`:

- `mesh_has_translucent_vertex` — any tri/line/dot vertex `col.w ∈ (EPSILON, 1.0)`.
  Fully-transparent primitives are already dropped from the GPU buffers, so
  `w ≤ EPSILON` deliberately does **not** count.
- `texture_has_alpha` — the mesh's image has any texel with `alpha < 255`
  (scanned once at load; a cut-out PNG writing depth was one of the reported
  bugs). Computed in `ensure_texture`, cached on `TextureCacheEntry`.
- Both mesh-side facts are computed once per mesh version in `ensure_mesh` and
  cached on `CachedMesh` (`centroid`, `translucent_vertices`).

### 4. Sort keys (`order.rs`, new pure module)

`draw_order_cmp` orders draw items by:

```
(z_index asc, opaque before transparent, [transparent only] farthest-first, declaration order)
```

- **`z_index` stays the dominant explicit key.**
- Opaque-before-transparent is scoped **per `z_index` group**.
- Opaque half: **declaration order, no front-to-back sort** (offline renderer;
  front-to-back would break coplanar 2D paint order for no benefit).
- Transparent half: **back-to-front** by
  `depth = (centroid_world − camera.position)·camera.forward` (cached
  camera-independent centroid, one dot product per item per frame).
- Ties: declaration order (`total_cmp`, so NaN/degenerate centroids can't panic
  or produce a non-total order).

### 5. Depth bias keyed to declaration rank, not draw sequence (`order::rank_bias`)

The old `z_offset` was a mutable counter incremented in *draw* order. Since the
transparent split now **reorders** draws, that would silently change which
coplanar mesh wins. Instead, after sorting canonically by `(z_index, order)`,
each item gets `rank_bias(rank, item_count, DEPTH_STEP)` →
`{ tri: 3·rank·step, line: +step, dot: +2·step }`, `step = DEPTH_STEP.min(2e-3 /
(3·item_count))` so the total pull is bounded for huge scenes and degrades
gracefully to ties (which `LessEqual` + draw order still resolve as "later
wins"). The bias is then independent of the final paint order.

## Why the 2D / flat head-on case does not regress

Scene: filled polygon + its stroke + a text mesh, all `z_index` 0, camera head-on.

| risk | why it can't happen |
|---|---|
| triangle depth values shift | `eye_bias == 0` ⇒ projection identical to before |
| reordering changes which coplanar mesh wins | biases keyed to **declaration rank**, not draw sequence |
| stroke's decal bias occludes later text | lines/dots never write depth |
| translucent overlay jumps above later opaque content | its rank bias is smaller ⇒ `LessEqual` rejects it where covered |
| depth-key sort shuffles flat scenes | all centroid depths equal ⇒ that key is inert; result falls back to `(z_index, transparent, order)` |
| opaque front-to-back sort breaks paint order | not done — opaque stays in declaration order |

For two coplanar opaque meshes head-on, every vertex has identical eye depth, so
the rasterized depth plane is an exact constant for every fragment and every MSAA
sample; ties are exact, `LessEqual` passes, later draw wins — same as today. Under
camera rotation two differently-tessellated coplanar fills can disagree by ~1 ULP
(≤1e-8 NDC), still 100× below the rank-bias step, so the bias still decides.

## Known limitations

- **Intersecting translucent surfaces** are still imperfect — no full OIT. Two
  translucent meshes that interpenetrate blend by whole-mesh centroid order, not
  per-pixel. Matches Manim.
- **Back faces of a single translucent mesh** blend in vertex-buffer order (no
  per-triangle sort — that would defeat the cached-buffer design). Matches Manim.
- **An opaque mesh declared later but geometrically behind an earlier stroke**
  will overpaint that stroke (painter's-algorithm cost of decals-without-writes).
  Matches Manim.
- Surface tilt beyond ~63° relative to the camera can still let a fill poke
  through its stroke (the `DECAL_SCALE = 2.0` ceiling). Raising it trades against
  strokes floating in front of nearer geometry; 2.0 is the Opus recommendation.

## Empirical before/after (real scenes, `monocurl image ... -r small`)

Rendered the same scene file from the base commit (`6080017`) and from this
branch, byte-comparing the PNGs:

| scene | result |
|---|---|
| `(Example) Geometry Proof` (2D: fills + strokes + text, head-on camera) | **byte-identical** |
| `(Tutorial) Meshes` (2D) | **byte-identical** |
| `(Example) 3D Camera Animation` (tilted 3D surface + axes + text) | differs — see below |

The 3D scene change: the vertical z-axis line, which the old renderer always
painted on top of everything, is now correctly **occluded by the front lip of
the surface** where the surface is genuinely nearer the camera. The surface's
own black base-outline stroke stays crisply on top of its own fill (the decal
bias). This is the intended behavior — lines now respect true depth against
opaque fills while still winning against their own coplanar fill.

## Test results

`cargo test -p renderer` — **30 passed, 0 failed** (17 pre-existing incl. the
`blade_shader_parses_and_validates` naga check, + 13 new):

```
running 30 tests
test blade::geometry::tests::dot_radius_falls_back_to_style_and_sanitizes_scale ... ok
test blade::geometry::tests::dot_radius_scales_by_raster_scale ... ok
test blade::geometry::tests::mesh_centroid_of_empty_mesh_is_origin ... ok
test blade::geometry::tests::line_indices_fit_shared_endpoint_layout ... ok
test blade::geometry::tests::mesh_centroid_averages_tris_lines_and_dots_per_vertex ... ok
test blade::geometry::tests::translucent_vertex_detection_ignores_fully_opaque_and_fully_transparent ... ok
test blade::geometry::tests::line_vertices_skip_non_dominant_inverse_pairs ... ok
test blade::geometry::tests::line_vertices_do_not_fall_back_to_non_dominant_visible_inverse ... ok
test blade::order::tests::classifier_truth_table ... ok
test blade::order::tests::camera_depth_positive_in_front_rotated_basis ... ok
test blade::geometry::tests::line_vertices_keep_butt_caps_without_explicit_neighbors ... ok
test blade::geometry::tests::line_vertices_prefer_dominant_sibling_orientation ... ok
test blade::geometry::tests::line_vertices_reverse_second_half_tangents ... ok
test blade::order::tests::rank_bias_stays_below_step_cap_for_huge_scenes ... ok
test blade::resources::tests::chooses_highest_supported_count_not_exceeding_desired ... ok
test blade::order::tests::rank_bias_is_globally_monotonic_across_items_and_groups ... ok
test blade::order::tests::flat_scene_is_pure_declaration_order_within_z_index ... ok
test blade::order::tests::transparent_is_farthest_first_with_stable_tiebreak ... ok
test blade::order::tests::opaque_before_transparent_and_opaque_keeps_declaration_order ... ok
test blade::order::tests::phase_split_is_scoped_to_each_z_index_group ... ok
test blade::order::tests::nan_depth_does_not_panic_and_stays_total ... ok
test blade::pipelines::tests::blade_shader_parses_and_validates ... ok
test tests::renders_background_when_scene_is_empty ... ok
test tests::farther_transparent_surface_shows_through_nearer_transparent_surface ... ok
test tests::higher_z_index_wins_for_overlapping_geometry ... ok
test tests::renders_standalone_line_pixels ... ok
test tests::renders_clockwise_triangle_pixels ... ok
test tests::renders_stroked_triangle_pixels ... ok
test tests::linked_polyline_adds_corner_pixels_over_disconnected_segments ... ok
test tests::linked_polyline_populates_outer_corner_miter_region ... ok

test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
```

`cargo build -p monocurl` — succeeds (workspace builds clean).

New tests:
- `blade::order::*` — flat-scene invariance, per-z-group phase split,
  opaque-before-transparent + opaque declaration order, transparent
  farthest-first + stable tiebreak, NaN-depth safety, classifier truth table,
  `camera_depth` sign convention (rotated basis), `rank_bias` global monotonicity,
  `rank_bias` bounded for 10k-item scenes.
- `blade::geometry::*` — `mesh_centroid` per-vertex mean over tris/lines/dots +
  empty-mesh origin; `mesh_has_translucent_vertex` ignores fully-opaque and
  fully-transparent (`w = 0`) vertices.
- `tests::farther_transparent_surface_shows_through_nearer_transparent_surface`
  — GPU render: a near red α=0.5 triangle declared first no longer depth-occludes
  a far blue α=0.5 triangle; the overlap pixel carries both R and B.
- `pipelines::blade_shader_parses_and_validates` extended to assert the
  `eye_bias` parameter and `width_eye * DECAL_SCALE` stay wired in.

## Files touched

```
crates/renderer/src/blade.wgsl          project_camera/project eye_bias; vs_line/vs_dot decal; consts
crates/renderer/src/blade/pipelines.rs  depth_write vs depth_read_only; triangles_blend; test asserts
crates/renderer/src/blade/order.rs      NEW — SortKey, draw_order_cmp, is_transparent, camera_depth, rank_bias + tests
crates/renderer/src/blade/geometry.rs   mesh_centroid, mesh_has_translucent_vertex + tests
crates/renderer/src/blade/resources.rs  CachedMesh {centroid, translucent_vertices}; TextureCacheEntry {has_alpha}
crates/renderer/src/blade/mod.rs        mod order; MeshWorkItem {transparent, depth, tri/line/dot_bias}; sort_key()
crates/renderer/src/blade/renderer.rs   classify meshes; rank biases; final draw_order_cmp sort; pipeline select; texture alpha scan
crates/renderer/src/lib.rs              transparency regression render test + helper
```
