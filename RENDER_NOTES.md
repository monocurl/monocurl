# Renderer depth-stability changes

Branch `fix/render-depth-transparency`. Targets one maintainer-reported problem:

1. **Flicker/z-fighting when a stroke is coplanar with a fill** (a line/dot
   sitting on the triangle it decorates). Fine head-on, "becomes weird" when the
   camera moves.

## Root cause of the flicker

Line and dot geometry is extruded in the vertex shader along
`cross(tangent, (0,0,1))` **in camera space**, whose z is identically 0. So a
stroke quad / dot disc is a ribbon **parallel to the image plane**, sitting at
the constant eye depth of its centre — it is *not* coplanar with the fill it
decorates. Tilt the fill by θ and its depth under the stroke sweeps
`±(stroke half-width in eye space)·tanθ`. Half the stroke sinks into the fill;
MSAA turns that into sparkle. The mismatch shrinks like `1/d` as the camera
pulls back while the old fixed NDC `z_offset` step (`DEPTH_STEP = 1e-6`) stays
constant, which is exactly why "flat is fine, moving the camera is not".

Hardware `DepthStencilState.bias` cannot help: `slope_scale` multiplies the
primitive's *own* depth slope (≈0 for an image-plane-parallel ribbon), and on
the Metal backend `blade_graphics` casts `bias.constant` (`i32`) straight to
`f32` (`vendor/blade-graphics-0.7.1/src/metal/command.rs` →
`setDepthBias_slopeScale_clamp`), unusable for the sub-unit offset a stroke
needs. Left at `Default` everywhere.

## The fix — perspective-correct eye-space "decal" bias (`blade.wgsl`)

`project_camera` / `project` take a new `eye_bias` argument in **eye-space world
units**. It is applied only to `clip.z` (via
`biased_camera_z = min(camera_z + eye_bias, -near)`); `clip.w` stays the true
depth, so the vertex does **not move on screen** — it slides along its own view
ray. This is a true depth-only decal.

`vs_line` and `vs_dot` compute
`eye_bias = min(width_eye · DECAL_SCALE, eye_depth · DECAL_MAX_FRACTION)` where
`width_eye` is the primitive's own eye-space **full** width (already computed for
the screen-space extrusion), `DECAL_SCALE = 1.5`, `DECAL_MAX_FRACTION = 0.02`.
Because `width_eye ∝ eye_depth`, `eye_bias/d` is constant: **camera-distance
invariant and self-limiting** — the property the fixed NDC step lacked. Bias
`= 2·DECAL_SCALE·half_width` covers surface tilt up to `atan(2·DECAL_SCALE) ≈
71°`; beyond that the fill can still bleed through its own stroke slightly. The
`DECAL_MAX_FRACTION` clamp bounds the pull for very thick strokes / very near
geometry.

`vs_triangle` passes `eye_bias = 0.0`. **With `eye_bias == 0.0` the projection is
bit-identical to the previous code** (`biased_camera_z` is only recomputed inside
`if (eye_bias > 0.0)`), so opaque fills are byte-for-byte unchanged.

### Depth-write split (`pipelines.rs`)

One `DepthStencilState` became two, both `LessEqual`, both `bias: Default`:

| pipeline | depth test | depth write |
|---|---|---|
| `triangles` | yes | **yes** |
| `lines` | yes | **no** (was yes) |
| `dots` | yes | **no** (was yes) |

Lines/dots no longer writing depth is what keeps the decal bias safe: a biased
stroke can only ever win against geometry drawn *before* it, it can **never
punch a hole in an opaque mesh painted afterwards** — so 2D paint order is
preserved exactly, and a stroke that is genuinely behind a separate opaque
surface is still occluded by that surface's depth (as long as the gap exceeds
the small decal). Stroke-vs-stroke is pure painter order.

Everything else in the scene pass is unchanged from `6080017`: a single render
pass, meshes drawn in `(z_index, declaration order)`, one `z_offset` NDC step
per primitive group for coplanar paint-order ties, straight alpha blending.

## Why the flat / 2D case does not regress

For every default scene with a flat / head-on camera the rendered PNG is
**byte-identical to base `6080017`** (verified for all 11 non-3D default
scenes). Head-on, every fill vertex under a stroke has the same eye depth as the
stroke, so `min(camera_z + eye_bias, -near)` only ever pulls the stroke *toward*
the camera past a `LessEqual` test it already passed — the depth values change
but no fragment's colour does, because lines/dots do not write depth and nothing
is drawn behind them that they could now wrongly occlude.

## The reverted transparency / WBOIT work

Earlier revisions of this branch (commits `a1b019d`, `172c3b9`) also added an
opaque/transparent pass split, a back-to-front transparent sort, and a
weighted-blended OIT (McGuire/Bavoil) transparent pass, to fix a second reported
problem (translucent triangles depth-occluding what should blend through).

**That work was reverted on this branch.** It changed the compositing of *flat
2D* scenes that merely contain translucent geometry — `(Example) Image` blew its
overlapping translucent shapes out to white, `(Example) Fractal` washed out its
overlapping translucent leaf dots, `(Example) Riemann Sum` shifted — because
WBOIT is a weighted *average*, not ordered alpha compositing, and the
back-to-front per-mesh sort reorders draws that base composited in declaration
order. Keeping 2D scenes byte-identical to base outranked the transparency
improvement, so the transparent pass is gone and translucent geometry again
renders exactly as in base `6080017` (single pass, declaration order, straight
alpha blend — including base's known translucent-depth-write limitation).

If the translucent-depth problem is picked up again it needs an approach that is
provably inert for flat scenes (e.g. depth-write-off for translucent tris but
**no** reorder and **no** weighted blend, or a proper per-pixel OIT gated on the
frame actually having interpenetrating 3D translucent surfaces).

## The regression this branch's WBOIT revision introduced (now fixed)

Rendering `(Example) 3D Camera Animation` on `172c3b9` vs base: the axis floor
grid + back-wall grid lines **bled through the front of the opaque paraboloid
surface**. Two causes, both now removed:

- `DECAL_SCALE` had been raised to `3.0` and `DECAL_MAX_FRACTION` to `0.08`, so
  the camera-ward decal on every line was `≈ 6× its half-width` (or up to 8 % of
  eye depth). The scene's axis mesh is nudged only `shift{[0,0,-0.01]}` below
  the surface; a decal that large slid the grid lines from behind the surface to
  in front of it, and since lines don't write depth nothing stopped them.
- `DECAL_SCALE` is now `1.5` (`DECAL_MAX_FRACTION 0.02`), so the decal at that
  camera is `≈ 0.005` world units — comfortably inside the `0.01` gap, so the
  grid stays occluded exactly as in base, while still covering ~71° of fill tilt
  for a stroke sitting on *its own* fill.

## Empirical before/after (all 12 default scenes, `monocurl image … -r medium`)

Base = `6080017`. Branch = this branch's fix.

| scene | pixels changed vs base | max Δ | note |
|---|---|---|---|
| `(Example) 3D Camera Animation` | 3448 / 2073600 (0.17 %) | 255 | regression fixed; diff is the surface's own rim stroke now sitting crisply on its fill + front-facing axis lines slightly bolder. Bowl interior identical to base (no grid bleed-through). |
| every other default scene (11) | **0** | 0 | byte-identical to base |

For comparison the pre-fix branch (`172c3b9`) changed `3D Camera Animation` by
6234 px (grid bleeding across the bowl interior), `Fractal` by 63964 px,
`Image` by 111583 px, `Riemann Sum` by 112 px.

### Coplanar-stroke flicker check

Test scene: opaque `Square(2.2)` (`fill{ORANGE} stroke{RED,3}`) + a coplanar
`stroke{BLACK,1.5} LineGrid([-1,1,9],[-1,1,9])`, camera tilted 45° / 60° / 72°
about the x-axis (`/tmp/coplanar/cp_{45,60,72}.mcs`).

- **base `6080017`**: the far (grazing) grid rows lose stroke pixels — thin,
  broken, fading lines where the ribbon sinks into the tilted fill.
- **branch fix**: every grid line is uniformly crisp at all three angles; the
  red outline stays solid. ~11k stroke pixels that base drops are kept.

## Test results

`cargo test -p renderer` — **17 passed, 0 failed** (unchanged from base;
`blade::pipelines::tests::blade_shader_parses_and_validates` extended to assert
the `eye_bias` parameter and `width_eye * DECAL_SCALE` stay wired in).
`cargo build --workspace` succeeds; `cargo clippy -p renderer` clean.

## Files touched vs base `6080017`

```
crates/renderer/src/blade.wgsl          project_camera/project gain eye_bias (guarded, 0 == identity);
                                        DECAL_SCALE / DECAL_MAX_FRACTION consts; vs_line/vs_dot decal
crates/renderer/src/blade/pipelines.rs  DepthStencilState split: depth_write (tris) vs
                                        depth_read_only (lines/dots); shader test asserts eye_bias
```

(The large deletions in `git diff HEAD` are the revert of `a1b019d` + `172c3b9`:
`blade/order.rs`, `blade/oit.rs`, the OIT pipelines/targets/formats, the
transparent classification + sort, `mesh_centroid` / `mesh_has_translucent_vertex`,
`TextureCacheEntry::has_alpha`, and the two GPU transparency tests in `lib.rs`.)
