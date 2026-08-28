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
the screen-space extrusion), `DECAL_SCALE = 0.02`, `DECAL_MAX_FRACTION = 0.002`.
Because `width_eye ∝ eye_depth`, `eye_bias/d` is constant: **camera-distance
invariant and self-limiting** — the property the fixed NDC step lacked.

The bias is deliberately tiny — `≈ 0.02 · stroke eye-width`, a few `1e-5` world
units at a typical camera. It does not need to out-push the geometric
`half_width·tanθ` depth sweep of a grazing fill; it only needs to break the
*tie* that base leaves to depth-buffer precision + MSAA sample noise. Once the
stroke is a deterministic epsilon in front at every matched screen pixel, every
one of its MSAA samples passes the `LessEqual` test consistently and the sparkle
is gone — verified crisp at all three `surface-stroke` camera angles including
the near-grazing one. A larger decal was what caused the x-ray regression (see
below), so it is kept as small as will clear the noise. The `DECAL_MAX_FRACTION`
clamp bounds the pull for very thick strokes / very near geometry.

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

Rendering a bumped opaque surface + a coplanar `LineGrid` wireframe on it
(`render-demo/surface-stroke.mcs`), and `(Example) 3D Camera Animation`, on the
oversized-decal revisions vs base: the **far side of the wireframe / the axis
floor + back-wall grid lines bled through the front of the opaque surface**
(x-ray wireframe). Causes, all now removed:

- `DECAL_SCALE` had been as high as `3.0` (`DECAL_MAX_FRACTION` up to `0.08`), so
  the camera-ward decal on every line was several × its half-width. That slid a
  hidden back-side stroke from behind an opaque fill to in front of it, and since
  lines don't write depth nothing stopped it. The `b7af62e` WIP tried the
  opposite tack — making lines/dots write depth again — but then the decal
  biases the depth *write* too, so an earlier coplanar stroke wrongly occludes
  later coplanar fills/strokes and flat 2D scenes with layered strokes change.
- `DECAL_SCALE` is now `0.02` (`DECAL_MAX_FRACTION 0.002`): the decal is a few
  `1e-5` world units, far smaller than the depth gap to any surface a stroke is
  genuinely hidden behind, so hidden-line removal is exactly as in base. Lines
  and dots stay depth-read-only, so 2D paint order is byte-preserved.

## Empirical before/after (`monocurl image … -r medium`, 1920×1080)

Base = `6080017`. Branch = this branch's fix (`0b5d250`). Diff = max per-channel
|Δ| > 0 over RGBA. Regenerate with `render-demo/render-all.sh` + `diff-all.py`.

### All 12 default scenes

| scene | pixels changed vs base | max Δ | note |
|---|---|---|---|
| `(Example) 3D Camera Animation` | 1924 / 2073600 (0.093 %) | 255 | diff is the z-axis arrow, the parabola on the surface, and two tick marks — coplanar strokes now crisp instead of z-fighting. Fill and surface interior byte-identical; no grid bleed-through. |
| every other default scene (11) | **0** | 0 | byte-identical to base |

### Extra 3D check scenes (`render-demo/`)

| scene | pixels changed | max Δ | note |
|---|---|---|---|
| `surface-stroke` cam A / B / C | 36942 / 33044 / 18755 (0.9–1.8 %) | 255 | opaque bumped surface + coplanar `LineGrid` wireframe. Diff is **only** the visible front wireframe strokes going from broken/z-fighting (base) to crisp (branch). The occluded back-side wireframe shows **zero** diff — no x-ray bleed-through, at any of the three camera angles incl. near-grazing (cam C). |
| `crossing` / `crossing-swapped` | 4816 / 5384 (0.23–0.26 %) | 180 | two interpenetrating translucent planes. Diff is confined to each plane's own border stroke sitting on its fill edge; the translucent fill compositing is byte-identical to base (WBOIT stays reverted). |
| `translucent` | 2599 (0.125 %) | 49 | translucent surface + grid. Diff is the surface's white edge stroke only. |

For comparison the pre-fix WBOIT revision (`172c3b9`) changed `3D Camera
Animation` by 6234 px (grid bleeding across the bowl interior), `Fractal` by
63964 px, `Image` by 111583 px, `Riemann Sum` by 112 px.

### Coplanar-stroke flicker check

`surface-stroke.mcs`: an opaque `ColorGrid` bumped surface (`fill{ORANGE}`) with
a coplanar bold `stroke{BLACK,2} LineGrid` wireframe on the exact same
`point_map`, at three camera angles (3/4 view, side, near-grazing).

- **base `6080017`**: the grazing wireframe rows lose stroke pixels — thin,
  broken, fading lines where the image-plane-parallel ribbon sinks into the
  tilted fill; visibly sparkles under camera motion.
- **branch fix**: every wireframe line uniformly crisp at all three angles; the
  back of the wireframe stays correctly hidden behind the hump.

## Test results

`cargo test -p renderer` — **17 passed, 0 failed** (unchanged from base;
`blade::pipelines::tests::blade_shader_parses_and_validates` extended to assert
the `eye_bias` parameter and `width_eye * DECAL_SCALE` stay wired in).
`cargo build --workspace` succeeds; `cargo clippy -p renderer` clean.

## Files touched vs base `6080017`

`git diff 6080017 HEAD` is exactly three files (+290 / −12):

```
crates/renderer/src/blade.wgsl          project_camera/project gain eye_bias (guarded, 0 == identity);
                                        DECAL_SCALE / DECAL_MAX_FRACTION consts; vs_line/vs_dot decal
crates/renderer/src/blade/pipelines.rs  DepthStencilState split: depth_write (tris) vs
                                        depth_read_only (lines/dots); shader test asserts eye_bias
RENDER_NOTES.md                         this file (new)
```

The branch history has five commits — `a1b019d` + `172c3b9` added an
opaque/transparent split, a back-to-front sort and a WBOIT pass (`blade/order.rs`,
`blade/oit.rs`, `mesh_centroid`, `TextureCacheEntry::has_alpha`, two GPU
transparency tests), `337669d` reverted all of it, and `b7af62e` → `0b5d250`
converged on the final decal. Net effect vs base is only the three files above;
squash before merging if a clean single commit is wanted.
