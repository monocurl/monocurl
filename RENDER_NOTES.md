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
| `lines` | yes | **no** (was yes) |
| `dots` | yes | **no** (was yes) |
| `triangles_oit` / `lines_oit` / `dots_oit` (transparent, §6) | yes | **no** |

Lines/dots no longer writing depth is what makes the decal bias safe: a stroke
can only ever win against its own fill, it can **never occlude geometry painted
after it** (so 2D paint order is preserved exactly). Stroke-vs-stroke is now pure
painter order — same as Manim. (An interim `triangles_blend` simple-blend
pipeline existed before §6 replaced it with the OIT pipelines.)

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

### 6. Weighted-blended OIT for the transparent pass (`blade.wgsl`, `pipelines.rs`, `resources.rs`, `renderer.rs`; follow-up commit)

The single depth-write-off transparent pass was replaced by a McGuire/Bavoil
weighted-blended OIT pipeline so overlapping / interpenetrating translucent
surfaces (and a translucent mesh's own back faces) no longer depend on per-mesh
or per-triangle draw order.

- **Structure.** When a frame has ≥1 transparent mesh the renderer now runs
  three passes instead of one:
  1. `renderer-opaque` — background + opaque geometry, writes depth (depth
     `finish_op` promoted `Discard` → `Store` only in this case).
  2. `renderer-oit` — transparent meshes (tris + their lines + their dots) into
     two MRT targets: `accum` `Rgba16Float` (additive,
     `Σ vec4(rgb·a, a)·w(z,a)`) and `revealage` `R16Float` (`dst *= 1−a`, cleared
     to 1). Depth **tests** against the opaque buffer, never writes.
  3. `renderer-oit-composite` — full-screen triangle strip, `textureLoad`s the
     resolved `accum`/`revealage`, emits `vec4(accum.rgb/max(accum.a,ε), reveal)`
     blended `src=1−srcα, dst=srcα` over the resolved opaque colour.
- **Weight** `w = a·clamp(10/(1e-5 + (z/5)² + (z/200)⁶), 1e-2, 3e3)`,
  `z = |camera-space depth|` (`in.model.z` for tris, `1/pos.w` for lines/dots).
- **MSAA.** `accum`/`revealage` are allocated at the renderer's sample count and
  `ResolveTo` single-sample copies that the composite samples; at sample_count 1
  they are the direct targets. Scratch targets are **allocated lazily** on the
  first transparent frame (an opaque-only scene allocates nothing and takes the
  unchanged single pass — `!transparent_present ⇒ early return` after pass 1).
- **Classification unchanged** (§3): `uniform.alpha < 1 || any partial-alpha
  vertex || texture has sub-opaque texel`. A transparent mesh routes *all* its
  primitives (tris + lines + dots) through OIT, so its own stroke is composited
  with the fill rather than washed by it.
- The old `triangles_blend` pipeline is removed (superseded).
- `blade/oit.rs` (test-only) is a CPU reference for the weight + accumulate +
  composite algebra; its tests pin the compositing math and its
  order-independence.

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
| WBOIT 3-pass path changes an opaque-only scene | not entered — `transparent_present` gates it; opaque-only frames run the exact single pass and allocate no OIT targets |

For two coplanar opaque meshes head-on, every vertex has identical eye depth, so
the rasterized depth plane is an exact constant for every fragment and every MSAA
sample; ties are exact, `LessEqual` passes, later draw wins — same as today. Under
camera rotation two differently-tessellated coplanar fills can disagree by ~1 ULP
(≤1e-8 NDC), still 100× below the rank-bias step, so the bias still decides.

## Known limitations

- **Intersecting / overlapping translucent surfaces** and **a translucent mesh's
  own back faces** are now handled per-pixel and order-independently by
  weighted-blended OIT (§6). The remaining error is inherent to WBOIT: it is a
  weighted *average*, not exact compositing — deep stacks of very different
  colours or extreme depth ranges lose some ordering fidelity. Full per-pixel
  exact OIT (depth peeling / linked-list) is out of scope.
- Transparent geometry is composited as one layer *over* the opaque result, so a
  translucent surface that is partially behind opaque geometry is correctly
  depth-clipped but the *visible* part is still averaged, not exactly ordered,
  against other transparent fragments at that pixel.
- **An opaque mesh declared later but geometrically behind an earlier stroke**
  will overpaint that stroke (painter's-algorithm cost of decals-without-writes).
  Matches Manim.
- Surface tilt beyond ~80° of grazing (`DECAL_SCALE = 3.0`, i.e. bias ≈ 6×
  stroke half-width) can still let a fill poke through its own stroke. Raising it
  further trades against strokes floating in front of nearby geometry.
- WBOIT `accum` is `fp16`; the weight is clamped to `3e3` and the composite
  guards against `inf`/`NaN`, but pathological scenes could still band slightly.

## Empirical before/after (real scenes, `monocurl image ... -r small`)

Rendered the same scene file from the base commit (`6080017`) and from this
branch (all three passes: decal fix, transparency pass, WBOIT), byte-comparing
the PNGs:

| scene | result |
|---|---|
| `(Example) Geometry Proof` (2D: fills + strokes + text, head-on camera) | **byte-identical** |
| `(Tutorial) Meshes` (2D) | **byte-identical** |
| `(Example) 3D Camera Animation` (tilted surface + translucent grid walls + axes + text) | differs — see below |

The 3D scene changes: (1) the vertical z-axis line, which the old renderer always
painted on top of everything, is now correctly **occluded by the front lip of
the surface**; the surface's own black outline stroke stays crisp on its fill
(decal bias). (2) The translucent grid walls now composite through WBOIT — the
image is visually the same but ~660 / 921 600 pixels change (max channel Δ 48,
mean Δ 3.6), all at antialiased edges where the translucent grid crosses the
surface silhouette. No gross change.

WBOIT order-independence is covered directly by
`tests::intersecting_translucent_planes_are_draw_order_independent`: two
interpenetrating α=0.5 planes rendered with the meshes in each declaration order
produce a **pixel-identical** composite (the previous sorted per-mesh blend
flipped the crossing region on swap).

## Test results

`cargo test -p renderer` — **37 passed, 0 failed** (17 pre-existing incl. the
`blade_shader_parses_and_validates` naga check, + 20 new):

```
running 37 tests
test blade::geometry::tests::dot_radius_falls_back_to_style_and_sanitizes_scale ... ok
test blade::geometry::tests::line_indices_fit_shared_endpoint_layout ... ok
test blade::geometry::tests::line_vertices_do_not_fall_back_to_non_dominant_visible_inverse ... ok
test blade::geometry::tests::dot_radius_scales_by_raster_scale ... ok
test blade::geometry::tests::line_vertices_prefer_dominant_sibling_orientation ... ok
test blade::geometry::tests::line_vertices_keep_butt_caps_without_explicit_neighbors ... ok
test blade::geometry::tests::line_vertices_reverse_second_half_tangents ... ok
test blade::geometry::tests::line_vertices_skip_non_dominant_inverse_pairs ... ok
test blade::geometry::tests::mesh_centroid_averages_tris_lines_and_dots_per_vertex ... ok
test blade::geometry::tests::mesh_centroid_of_empty_mesh_is_origin ... ok
test blade::geometry::tests::translucent_vertex_detection_ignores_fully_opaque_and_fully_transparent ... ok
test blade::oit::tests::fully_opaque_transparent_fragment_replaces_destination ... ok
test blade::oit::tests::no_fragments_leaves_destination_untouched ... ok
test blade::oit::tests::resolve_is_independent_of_fragment_order ... ok
test blade::oit::tests::revealage_tracks_combined_coverage ... ok
test blade::oit::tests::single_half_alpha_fragment_is_half_colour_half_destination ... ok
test blade::oit::tests::weight_is_positive_and_bounded ... ok
test blade::order::tests::camera_depth_positive_in_front_rotated_basis ... ok
test blade::order::tests::classifier_truth_table ... ok
test blade::order::tests::flat_scene_is_pure_declaration_order_within_z_index ... ok
test blade::order::tests::nan_depth_does_not_panic_and_stays_total ... ok
test blade::order::tests::opaque_before_transparent_and_opaque_keeps_declaration_order ... ok
test blade::order::tests::phase_split_is_scoped_to_each_z_index_group ... ok
test blade::order::tests::rank_bias_is_globally_monotonic_across_items_and_groups ... ok
test blade::order::tests::rank_bias_stays_below_step_cap_for_huge_scenes ... ok
test blade::order::tests::transparent_is_farthest_first_with_stable_tiebreak ... ok
test blade::resources::tests::chooses_highest_supported_count_not_exceeding_desired ... ok
test blade::pipelines::tests::blade_shader_parses_and_validates ... ok
test tests::farther_transparent_surface_shows_through_nearer_transparent_surface ... ok
test tests::linked_polyline_adds_corner_pixels_over_disconnected_segments ... ok
test tests::higher_z_index_wins_for_overlapping_geometry ... ok
test tests::renders_background_when_scene_is_empty ... ok
test tests::intersecting_translucent_planes_are_draw_order_independent ... ok
test tests::renders_standalone_line_pixels ... ok
test tests::renders_clockwise_triangle_pixels ... ok
test tests::renders_stroked_triangle_pixels ... ok
test tests::linked_polyline_populates_outer_corner_miter_region ... ok

test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
```

`cargo check -p renderer` — clean, no warnings. `cargo build -p monocurl` —
succeeds.

New tests:
- `blade::oit::*` (6) — CPU reference for the WBOIT math: weight positivity /
  clamp / alpha-linearity, empty resolve, fully-opaque fragment, single
  half-alpha fragment algebra, revealage = combined coverage, and
  **`resolve_is_independent_of_fragment_order`**.
- `blade::order::*` (9) — flat-scene invariance, per-z-group phase split,
  opaque-before-transparent + opaque declaration order, transparent
  farthest-first + stable tiebreak, NaN-depth safety, classifier truth table,
  `camera_depth` sign convention (rotated basis), `rank_bias` global monotonicity,
  `rank_bias` bounded for 10k-item scenes.
- `blade::geometry::*` (3) — `mesh_centroid` per-vertex mean over tris/lines/dots
  + empty-mesh origin; `mesh_has_translucent_vertex` ignores fully-opaque and
  fully-transparent (`w = 0`) vertices.
- `tests::farther_transparent_surface_shows_through_nearer_transparent_surface`
  — GPU render: a near red α=0.5 triangle declared first no longer depth-occludes
  a far blue α=0.5 triangle.
- `tests::intersecting_translucent_planes_are_draw_order_independent` — GPU
  render: swapping the declaration order of two interpenetrating translucent
  planes leaves the composite pixel-identical.
- `pipelines::blade_shader_parses_and_validates` — extended to assert the
  `eye_bias` parameter, `width_eye * DECAL_SCALE`, and the four OIT entry points
  stay wired in; the fragment-output check now understands MRT struct outputs.

## Files touched

```
crates/renderer/src/blade.wgsl          project_camera/project eye_bias; vs_line/vs_dot decal + consts;
                                        triangle_shaded refactor; OitOut/oit_weight/oit_fragment;
                                        fs_{triangle,line,dot}_oit; vs_/fs_oit_composite
crates/renderer/src/blade/pipelines.rs  depth_write vs depth_read_only; triangles/lines/dots_oit +
                                        oit_composite pipelines; OIT blend states; MRT-aware wgsl test
crates/renderer/src/blade/order.rs      NEW — SortKey, draw_order_cmp, is_transparent, camera_depth,
                                        rank_bias + tests
crates/renderer/src/blade/oit.rs        NEW (test-only) — CPU reference for the WBOIT compositing math
crates/renderer/src/blade/geometry.rs   mesh_centroid, mesh_has_translucent_vertex + tests
crates/renderer/src/blade/resources.rs  CachedMesh {centroid, translucent_vertices};
                                        TextureCacheEntry {has_alpha}; OitTargets + create/destroy
crates/renderer/src/blade/mod.rs        mod order/oit; OIT format consts; MeshWorkItem fields; sort_key()
crates/renderer/src/blade/renderer.rs   classify meshes; rank biases; final draw_order_cmp sort;
                                        3-pass draw_meshes (opaque / WBOIT accum / composite); DrawCtx;
                                        lazy OIT target alloc; texture alpha scan
crates/renderer/src/lib.rs              transparency + order-independence GPU render tests + helpers
```
