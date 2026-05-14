# `monocurl`

Browser-side controller for the Monocurl WebAssembly runtime.

This package includes the `wasm-bindgen` output produced from
`crates/web_runtime`. `createMonocurlLoop()` initializes that packaged wasm
runtime automatically; the wasm glue fetches `web_runtime_bg.wasm` from the
package's `dist/wasm` directory unless you pass a custom `wasmInit` input.

```ts
import {
  MonocurlWebGlRenderer,
  createMonocurlLoop,
} from "monocurl";

const canvas = document.querySelector("canvas");
if (!(canvas instanceof HTMLCanvasElement)) {
  throw new Error("missing canvas");
}
const renderer = new MonocurlWebGlRenderer(canvas);

const loop = await createMonocurlLoop({
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
console.log(compile.slides);

loop.seekTo({ slide: 1, time: 0 });
loop.play();
```

The Rust wasm object is treated as a low-level handle. This package owns the
JavaScript-side scheduling policy: requestAnimationFrame integration, command
helpers, source loading, snapshot JSON decoding, mesh typed-array packing, and a
WebGL2 renderer for drawing runtime snapshots. For custom deployment setups,
pass `wasmInit` to `createMonocurlLoop()` to control how wasm-bindgen loads the
packaged `.wasm`, or pass `runtime` / `wasm` only when testing alternate wasm
handles. Source imports are supplied as a string map whose keys can be module
names such as `std.scene` or paths such as `lib/helpers.mcl`. The wasm runtime
embeds the default `std.*` modules; caller-supplied imports can override or
extend that set.

Presentation controls are surfaced through the `parameters` field on execution
snapshots. Pass those existing `target` and updated `value` objects back to
`loop.updateParameter(target, value)` or `loop.updateParameters([...])` to drive
the same runtime path used by native presentation mode.

`loop.loadSource(...)` returns compile diagnostics and static slide metadata.
Use `report.slides` to build navigation controls immediately after compiling;
each slide entry includes the `index` to pass to `loop.seekTo(...)` and the
optional declared slide `name`. Runtime snapshots also include slide names and
duration information as execution discovers it.

Playback can be bounded to a timestamp:

```ts
loop.play({ until: { slide: report.slides[1].index, time: 0 } });
```

When the loop reaches or crosses that timestamp, it seeks to the exact limit,
emits a final paused snapshot there, and stops scheduling playback frames.

Text/Tex rendering in wasm does not bundle a TeX distribution. Load a MathJax
runtime with synchronous `tex2svg` before creating the loop. If `globalThis.MathJax`
is available, `createMonocurlLoop()` installs the Monocurl text hook
automatically; you can also pass `{ mathJax }` explicitly or pass
`{ mathJax: false }` to opt out. Alternatively, define
`globalThis.__monocurlRenderLatexSvg(kind, source)` yourself and return an SVG
string. The hook receives `kind` as `"text"` or `"tex"`. Full `Latex(...)` body
fragments, `Image(...)`, and image texturing through `textured{...}` are not
supported by the wasm runtime yet; failures are surfaced on runtime snapshots
through `snapshot.errors`. `Label(...)` uses the same browser-compatible text
path as `Text(...)`.

`MonocurlWebGlRenderer` is a browser-side WebGL2 renderer that ports the same
camera projection, triangle lighting, pixel-space line extrusion, dot rendering,
z-index ordering, and depth-bias conventions used by the native blade shader
path. It consumes the `ExecutionSnapshot` objects returned by `step_json`.
