# `@enigmurl/monocurl-web-runtime`

Browser-side controller for the Monocurl WebAssembly runtime.

This package does not import a concrete wasm artifact. Instead, initialize the
`wasm-bindgen` package produced from `crates/web_runtime`, then pass the module
namespace into `createMonocurlLoop`.

```ts
import init, * as wasm from "./pkg/web_runtime.js";
import {
  createMonocurlLoop,
  packSnapshotMeshes,
} from "@enigmurl/monocurl-web-runtime";

await init();

const loop = await createMonocurlLoop({
  wasm,
  onStep(result) {
    for (const snapshot of result.snapshots) {
      console.log(snapshot.currentTimestamp, packSnapshotMeshes(snapshot));
    }
  },
});

loop.seekTo({ slide: 1, time: 0 });
loop.play();
```

`createMonocurlLoop` checks `expectedVersion` when passed. If it is omitted, the
package reads `MONOCURL_VERSION` from `globalThis`, `import.meta.env`, Vite's
`VITE_MONOCURL_VERSION`, or `process.env`.

The Rust wasm object is treated as a low-level handle. This package owns the
JavaScript-side scheduling policy: requestAnimationFrame integration, command
helpers, version checks, snapshot JSON decoding, mesh typed-array packing, and
future browser rendering hooks.
