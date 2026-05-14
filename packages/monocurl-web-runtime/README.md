# `@enigmurl/monocurl-web-runtime`

Browser-side controller for the Monocurl WebAssembly runtime.

This package does not import a concrete wasm artifact. Instead, initialize the
`wasm-bindgen` package produced from `crates/web_runtime`, then pass the module
namespace into `createMonocurlLoop`.

```ts
import init, * as wasm from "./pkg/web_runtime.js";
import {
  MonocurlWebGlRenderer,
  createMonocurlLoop,
  installMonocurlMathJaxRenderer,
} from "@enigmurl/monocurl-web-runtime";

await init();
installMonocurlMathJaxRenderer();

const canvas = document.querySelector("canvas");
if (!(canvas instanceof HTMLCanvasElement)) {
  throw new Error("missing canvas");
}
const renderer = new MonocurlWebGlRenderer(canvas);

const loop = await createMonocurlLoop({
  wasm,
  onStep(result) {
    for (const snapshot of result.snapshots) {
      renderer.render(snapshot);
    }
  },
});

const compile = loop.loadSource(`
import std.scene

slide
`);
if (!compile.ok) {
  console.error(compile.diagnostics);
}

loop.seekTo({ slide: 1, time: 0 });
loop.play();
```

The Rust wasm object is treated as a low-level handle. This package owns the
JavaScript-side scheduling policy: requestAnimationFrame integration, command
helpers, source loading, snapshot JSON decoding, mesh typed-array packing, and a
WebGL2 renderer for drawing runtime snapshots. Source imports are supplied as a
string map whose keys can be module names such as `std.scene` or paths such as
`lib/helpers.mcl`. The wasm runtime embeds the default `std.*` modules;
caller-supplied imports can override or extend that set.

Presentation controls are surfaced through the `parameters` field on execution
snapshots. Pass those existing `target` and updated `value` objects back to
`loop.updateParameter(target, value)` or `loop.updateParameters([...])` to drive
the same runtime path used by native presentation mode.

Text/Tex rendering in wasm does not bundle a TeX distribution. Load a MathJax
runtime with synchronous `tex2svg` and call
`installMonocurlMathJaxRenderer()`, or define
`globalThis.__monocurlRenderLatexSvg(kind, source)` and return an SVG string.
The hook receives `kind` as `"text"` or `"tex"`. Full `Latex(...)` body
fragments, `Image(...)`, and image texturing through `retextured{...}` are not
supported by the wasm runtime yet; failures are surfaced on runtime snapshots
through `snapshot.errors`. `Label(...)` uses the same browser-compatible text
path as `Text(...)`.

`MonocurlWebGlRenderer` is a browser-side WebGL2 renderer that ports the same
camera projection, triangle lighting, pixel-space line extrusion, dot rendering,
z-index ordering, and depth-bias conventions used by the native blade shader
path. It consumes the `ExecutionSnapshot` objects returned by `step_json`.
