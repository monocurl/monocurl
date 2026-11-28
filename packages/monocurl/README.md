# `monocurl`

Browser-side controller for the Monocurl WebAssembly runtime.

This package includes the `wasm-bindgen` output produced from
`crates/web_runtime`. `createMonocurlLoop()` initializes that packaged wasm
runtime automatically; the wasm glue fetches `web_runtime_bg.wasm` from the
package's `dist/wasm` directory.

```sh
npm install monocurl
```

**Minimal Browser Example**

```html
<canvas id="viewport" style="width: 900px; height: 520px"></canvas>
<div id="slides"></div>
<button id="play">Play/Pause</button>
```

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
const slides = document.querySelector("#slides");
const play = document.querySelector("#play");

const loop = await createMonocurlLoop();
loop.addSnapshotListener((snapshot) => {
  renderer.render(snapshot);
});

const report = loop.loadSource(`
import std.color
import std.mesh
import std.anim
import std.scene


mesh marker =
    center{0r}
    fill{alpha{0.28} BLUE}
    stroke{BLUE, 2}
    Circle(0.55)

slide "Intro"
    play Wait(0.5)

slide "Move"
    marker =
        center{1.4r}
        fill{alpha{0.28} ORANGE}
        stroke{ORANGE, 2}
        Circle(0.55)
    play Trans(1)
`);
if (!report.ok) {
  console.error(report.diagnostics);
  throw new Error("Monocurl compile failed");
}

for (const slide of report.slides) {
  const button = document.createElement("button");
  button.textContent = slide.name ?? `Slide ${slide.index}`;
  button.onclick = () => loop.seekTo({ slide: slide.index, time: 0 });
  slides?.append(button);
}


play?.addEventListener("click", () => {
  loop.togglePlay();
});
```

For an interactive 3D viewport with camera panning, let the package own the
canvas renderer and camera override state:

```ts
import {
  createMonocurlLoop,
  installMonocurlCameraController,
} from "monocurl";

const loop = await createMonocurlLoop();
installMonocurlCameraController(canvas, loop);
loop.loadSource(source);
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

## Sliders and parameters in a page

A `param` declared at the top level of a scene shows up in
`snapshot.parameters.params`. Each entry carries a `name`, a `value` (with a
`kind` such as `float` or `vectorFloat`) and a `target`. Build a control from
those, then write the new value back with `loop.updateParameter(target, value)`:

```ts
loop.addSnapshotListener((snapshot) => {
  for (const entry of snapshot.parameters?.params ?? []) {
    // create or update a slider for entry.name using entry.value
  }
});

slider.addEventListener("input", () => {
  loop.updateParameter(entry.target, { kind: "float", value: Number(slider.value) });
});
```

The snapshot does not include slider bounds, so the page chooses min, max and
step for each control. Scene code reacts to the change through `$name`, for
example `mesh ball = Sphere(radius: $radius)`. Only `param` values can be
driven this way. Use `mesh` attributes and `target.kind = "meshAttribute"`
for those.

A full working page is in [`examples/interactive-params.html`](examples/interactive-params.html).
It builds with `npm run build` and runs from any static file server at the
package root. It shows a 3D sphere and a cylinder driven by sliders, with orbit
and shift-drag pan on the canvas.

## Camera controls

`installMonocurlCameraController(canvas, loop)` lets viewers orbit the scene by
dragging the canvas. Holding Shift while dragging pans instead. The camera
controller also accepts `enabled: false` to turn off pointer input, and
`reset()` returns to the scene's own camera. Scene code can still set the
camera with `Camera(...)` and `CameraLerp`.

Text/Tex rendering in wasm uses the package's `mathjax-full` dependency.
`createMonocurlLoop()` installs that renderer automatically; callers do not
provide, load, or configure MathJax. The renderer emits standalone SVG markup
before Monocurl imports it. Full `Latex(...)` body fragments,
`Image(...)`, and image texturing through `textured{...}` are not supported by
the wasm runtime yet; failures are surfaced on runtime snapshots through
`snapshot.errors`. `Label(...)` uses the same browser-compatible text path as
`Text(...)`.

`MonocurlWebGlRenderer` is a browser-side WebGL2 renderer that ports the same
camera projection, triangle lighting, pixel-space line extrusion, dot rendering,
z-index ordering, and depth-bias conventions used by the native blade shader
path. It consumes the `ExecutionSnapshot` objects returned by `step_json`.
