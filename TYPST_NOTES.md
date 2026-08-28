# Typst markup backend

Adds a `Typst(content, scale = 1)` mesh constructor that compiles Typst markup to
mesh geometry on **desktop/native builds only**. On `wasm32` it returns an
explicit "not supported" error, mirroring `Svg` / `Image`.

## Pipeline

`Typst markup` -> (preamble wrap) -> `typst` compiler (`typst-as-lib` `World`) ->
`PagedDocument` -> `typst-svg::svg_merged` -> SVG string -> shared SVG->mesh
importer (`crates/text/src/svg.rs`, via `cache::import_svg`).

This reuses exactly the same SVG->mesh path as `Svg(...)` / the LaTeX backends.

## Files touched

| File | Change |
| --- | --- |
| `Cargo.toml` (workspace) | new `[workspace.dependencies]`: `typst`, `typst-layout`, `typst-svg`, `typst-assets`, `typst-as-lib` |
| `crates/text/Cargo.toml` | add those 5 deps under `cfg(not(target_arch = "wasm32"))` |
| `crates/text/src/typst_backend.rs` | **new** native-only module: global `TypstEngine` (built once), custom `FileResolver`, `render_typst_svg(markup) -> Result<String>` |
| `crates/text/src/lib.rs` | `mod typst_backend` (native), re-export `render_typst`, `render_typst_with_quality` |
| `crates/text/src/render.rs` | `render_typst` / `render_typst_with_quality` (native: parse tags -> apply markers -> cache + SVG import -> remap tags; wasm bails), `TYPST_SVG_UNITS_AT_SCALE_1`, unit tests |
| `crates/text/src/document.rs` | `apply_typst_text_tag_markers` (native) + `typst_math_context`; factored shared `validate_marker_spans` out of `apply_text_tag_markers` |
| `crates/text/src/types.rs` | `BackendKind::Typst` variant (+ wasm `as_str` arm) |
| `crates/text/src/backend.rs` | `unreachable!()` arm for `BackendKind::Typst` (Typst never uses the LaTeX doc pipeline) |
| `crates/stdlib/src/mesh/constructors.rs` | `mk_typst` native `#[stdlib_func]` + wasm stub |
| `assets/std/std/mesh.mcl` | `Typst` user wrapper with doc comment + example, next to `Latex` |

## Dependency additions

Pinned to the Typst **0.15** release line (resolves to 0.15.1). `typst-as-lib`
0.16 targets `typst ^0.15`.

- `typst = "0.15"` — the compiler. Pure Rust.
- `typst-layout = "0.15"` — only needed to name `PagedDocument` (not re-exported by `typst`).
- `typst-svg = "0.15"` — `svg_merged(&PagedDocument, &SvgOptions, Abs) -> String`.
- `typst-assets = { version = "0.15", features = ["fonts"] }` — embedded default fonts (New Computer Modern math/text, Libertinus Serif, DejaVu Sans Mono) so stock Typst documents render as on typst.app. `typst_assets::fonts()` yields `&'static [u8]`.
- `typst-as-lib = "0.16"` — supplies the `typst::World` impl (fonts + in-memory main source + file resolvers). Its optional `reqwest`/`packages`/`typst-kit` features are **not** enabled, so no `reqwest 0.13` / `fontconfig` / `typst-kit` gets pulled — no conflict with the workspace `reqwest 0.12` or the `tectonic_engine_xetex = "=0.4.4"` fontconfig pin. Typst is pure Rust with no C deps.

`cargo check -p text` compiled the whole typst tree with **no version conflicts**
against the existing tree (tectonic, gpui, usvg 0.45, hayro).

## What works

- `Typst("$x^2 + y^2 = z^2$")`, `Typst("hello")`, headings, etc. compile and
  import to meshes.
- Tightly cropped output: the preamble sets `page(width: auto, height: auto,
  margin: 0pt, fill: none)` so the SVG viewBox shrink-wraps the content and the
  transparent fill means no background rectangle is imported.
- Errors (parse/type/layout/`#panic`) surface as `text render failed: ...`.
- Renders are cached via `cache::render_cached(BackendKind::Typst, ...)`, keyed by
  markup + scale + quality, same as the TeX backend's in-memory cache.
- **`text_tag` recovery works for both markup and math** (added in the third
  commit — see its own section below).
- `RenderQuality` is forwarded from the executor like the other text natives.
- **Global engine** (added in the second commit): the `TypstEngine` is built
  once in a `OnceLock` — fonts parsed once, `FontBook` derived once — with
  `comemo_evict_max_age(Some(30))`. The synthetic main file is served by a custom
  `FileResolver` that reads a `Mutex<String>` holding the source for the compile
  in flight; a second `Mutex<()>` (`TYPST_LOCK`) serialises compilation (Typst
  leans on a process-global `comemo` cache, and the tectonic LaTeX backend is
  effectively serial too). Effect on `cargo test -p text typst`: run time dropped
  from ~7s to ~0.06s.
- **Size calibration**: `TYPST_SVG_UNITS_AT_SCALE_1 = 47.0`, measured in the real
  renderer (`monocurl transcript` + `mesh_width`/`mesh_height`). At scale 1:
  `Tex("x^2 + y^2 = z^2")` = 1.490 x 0.300, `Typst("$x^2 + y^2 = z^2$")` (at 47)
  ≈ 1.525 x 0.293 — within ~3% width, ~2% height. Verified visually: correct
  orientation (upright, not mirrored — `flip_y = true` is right), transparent
  background, tight crop.

## `text_tag` recovery (third commit)

`Typst` now supports the same tagging as `Tex`. `document::parse_text_tags`
(already backend-agnostic) strips `\text_tag{...}{...}` / `\tagN{...}` markers —
including the `text_tag{...}` list form, which the executor lowers to
`\text_tag` before the string reaches `mk_typst` — and records tagged byte
ranges. `render_typst_with_quality` then mirrors `render_tagged_backend`:

1. `indexed_marker_spans` gives each span a synthetic single-component tag `i+1`.
2. **`document::apply_typst_text_tag_markers`** (new, native-only) wraps each
   range. It classifies the syntax context at the range start by scanning for
   unescaped `$`:
   - **markup**: `#text(fill: rgb(N, 255, 255))[ ...content... ]`
   - **inline math** (`$x$`): `#text(fill: rgb(N, 255, 255))[$ ...content... $]`
     — the `#` switches to code mode so `rgb` resolves, and re-entering `$...$`
     keeps superscripts / fractions / etc. as math.
   - **display math** (`$ x $`, spaces around): same but `[$ ... $]` so the
     fragment stays display-styled.
   It reuses the shared `apply_wrappers` / `validate_nested_ranges` /
   `validate_marker_spans` helpers, so nesting (inner tag wins) works.
3. The SVG importer decodes the `rgb(N,255,255)` fill back to `mesh.tag = [N]`
   (decode path was already on).
4. `apply_backend_text_tags` remaps `mesh.tag == [i+1]` to the caller's real tag
   list `spans[i].tag` (so multi-component tags like `text_tag{[2, 7]}` work).

**Layout is preserved** — a test asserts tagged math stays within 10% of the
untagged bounds, and the visual check (`scratch/typst_tags.mcs`) shows correct
per-term coloring of `$a^2 + b^2 = c^2$`, `$\tag1{x^2} + \tag2{2 x y} +
\tag3{y^2}$`, and markup `\tag1{alpha} beta \tag2{gamma}` with no distortion.

Note: `rgb(N, 255, 255)` is emitted as an exact `#NNffff` sRGB fill in the SVG
(Typst does no color management for `rgb()`), so decoding is exact. The one
Typst gotcha found: `$ x $` (whitespace-padded) is *display* math, `$x$` is
inline — nothing to do with the wrapper.

Context detection does not track `//` / `/* */` comments or `$` inside
code-mode strings; those are not expected in `Typst(...)` snippets.

## Not supported (intentional)

- Typst `import` / packages / local files for inline snippets (the custom
  `FileResolver` only serves the synthetic main file). Fine for a text
  constructor; revisit if users want `#import`.

## wasm status

The typst deps are all under `cfg(not(target_arch = "wasm32"))` in
`crates/text/Cargo.toml`. `render_typst` / `render_typst_with_quality` on wasm
just `bail!("Typst is not supported by the browser text backend; use Tex(...) or
Text(...) instead")`, and `mk_typst` has a wasm stub returning
`ExecutorError::invalid_invocation("Typst(...) is not supported in the
WebAssembly runtime yet")`. Two `match kind { ... }` arms in `backend.rs` (native
`native_document`, wasm `browser_source`) got an `unreachable!()` arm for
`BackendKind::Typst`.

`cargo check -p text --target wasm32-unknown-unknown`: PASS (see below).

## Test / build results (after tag-recovery changes)

```
$ cargo check -p text                                 -> Finished, no warnings
$ cargo check -p stdlib                               -> Finished, no warnings
$ cargo check -p text --target wasm32-unknown-unknown -> Finished, no warnings
$ cargo build -p monocurl                             -> Finished (app links)

$ cargo test -p text document
running 12 tests
... 8 pre-existing (incl. latex \color marker tests) ... ok
test document::tests::typst_text_tag_markers_wrap_markup_and_math_differently ... ok
test document::tests::typst_text_tag_markers_preserve_display_math_and_nesting ... ok
test result: ok. 12 passed; 0 failed

$ cargo test -p text typst
running 9 tests
test render::tests::typst_empty_inputs_render_to_no_meshes ... ok
test render::tests::typst_invalid_markup_is_an_error ... ok
test render::tests::typst_math_renders_some_geometry ... ok
test render::tests::typst_hello_has_consistent_topology_and_reasonable_scale ... ok
test render::tests::typst_plain_markup_produces_no_text_tags ... ok
test render::tests::typst_markup_text_tags_are_recovered ... ok
test render::tests::typst_math_text_tags_are_recovered_and_preserve_layout ... ok
test render::tests::typst_nested_text_tags_use_inner_priority ... ok
test render::tests::typst_multi_component_text_tag_lists_are_recovered ... ok
test result: ok. 9 passed; 0 failed; finished in 0.07s
```

## Real-app verification

`./target/debug/monocurl image  scratch/typst_smoke.mcs`  — exported a PNG with
`Tex` and `Typst` versions of the Pythagorean identity: Typst renders upright,
un-mirrored, transparent background, tightly cropped.

`./target/debug/monocurl transcript scratch/typst_measure.mcs` — printed
`mesh_width`/`mesh_height` used to calibrate `TYPST_SVG_UNITS_AT_SCALE_1`.

`scratch/typst_perf.mcs` — 8 distinct Typst snippets (sums, integrals, matrices,
limits, `bold`, `nabla`) in one slide all render.

`scratch/typst_tags.mcs` — `text_tag` recovery: `palette{}` operator colors by
tag. `mesh_tags` reports `[1, 2, 3]` for both the list form
(`[text_tag{1} "$a^2$", ...]`) and the raw form
(`"$\tag1{x^2} + \tag2{2 x y} + \tag3{y^2}$"`), and `[1, 2]` for markup
`"\tag1{alpha} beta \tag2{gamma} delta"`. Rendered PNG shows each term in its
color, superscripts colored with their base, operators left black, no layout
distortion.
