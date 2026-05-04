# `@enigmurl/monocurl-web-runtime`

Browser-side controller for the Monocurl WebAssembly runtime.

This package does not import a concrete wasm artifact. Instead, initialize the
`wasm-bindgen` package produced from `crates/web_runtime`, then pass the module
namespace into `createMonocurlLoop`.

```ts
import init, * as wasm from "./pkg/web_runtime.js";
import { createMonocurlLoop } from "@enigmurl/monocurl-web-runtime";

await init();

const loop = await createMonocurlLoop({
  wasm,
  expectedVersion: "0.1.0-dev",
  onStep(result) {
    // Snapshot payloads will be exposed here once the wasm API returns them.
    console.log(result.snapshotCount);
  },
});

loop.seekTo({ slide: 1, time: 0 });
loop.play();
```

The Rust wasm object is treated as a low-level handle. This package owns the
JavaScript-side scheduling policy: requestAnimationFrame integration, command
helpers, version checks, and future browser rendering hooks.
