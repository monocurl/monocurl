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
| `crates/text/src/typst_backend.rs` | **new** native-only module: bundled fonts + `render_typst_svg(markup) -> Result<String>` |
| `crates/text/src/lib.rs` | `mod typst_backend` (native), re-export `render_typst`, `render_typst_with_quality` |
| `crates/text/src/render.rs` | `render_typst` / `render_typst_with_quality` (native routes through cache + SVG import; wasm bails), `TYPST_SVG_UNITS_AT_SCALE_1`, unit tests |
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
- `RenderQuality` is forwarded from the executor like the other text natives.

## Stubbed / TODO

- **`text_tag{...}` fragment tag recovery is NOT implemented for Typst** (v1).
  The whole snippet renders as untagged contours; use `tag{...}` on the outside.
  Wiring this would need injecting per-fragment marker colors into the Typst
  markup (Typst has `#text(fill: rgb(...))`) and decoding them like the LaTeX
  `\text_tag` path — left as a follow-up. TODO comment in `render.rs`.
- The `TypstEngine` (and therefore the `FontBook`) is rebuilt on every
  `render_typst_svg` call. Font *bytes* are loaded once (`OnceLock`), but the
  book is re-parsed each call. Fine for v1 (the SVG->mesh cache dedupes repeats);
  a global engine + `comemo_evict_max_age(Some(N))` would speed up cold renders.
- `TYPST_SVG_UNITS_AT_SCALE_1 = 36.0` chosen to match the TeX backend's
  `NATIVE_SVG_UNITS_AT_SCALE_1`; eyeballed against `Text`/`Tex`, not precisely
  calibrated.

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

## Test / build results

```
$ cargo check -p text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.05s

$ cargo check -p stdlib
    Checking stdlib v0.1.0 (.../crates/stdlib)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.64s

$ cargo test -p text typst
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1m 56s
     Running unittests src/lib.rs
running 4 tests
test render::tests::typst_empty_inputs_render_to_no_meshes ... ok
test render::tests::typst_invalid_markup_is_an_error ... ok
test render::tests::typst_math_renders_some_geometry ... ok
test render::tests::typst_hello_has_consistent_topology_and_reasonable_scale ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 6.76s

$ cargo check -p text --target wasm32-unknown-unknown
    Checking text v0.1.0 (.../crates/text)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.61s
```

```
$ cargo build -p monocurl
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 40s
```

The whole app links with the Typst backend included.
