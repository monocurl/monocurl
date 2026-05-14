import { rmSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const packageRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(packageRoot, "../..");
const wasmInput = path.join(
  repoRoot,
  "target",
  "wasm32-unknown-unknown",
  "release",
  "web_runtime.wasm",
);
const wasmOutDir = path.join(packageRoot, "dist", "wasm");

run("cargo", ["build", "-p", "web_runtime", "--target", "wasm32-unknown-unknown", "--release"], {
  cwd: repoRoot,
});

rmSync(wasmOutDir, { recursive: true, force: true });
mkdirSync(wasmOutDir, { recursive: true });

run(
  "wasm-bindgen",
  [
    wasmInput,
    "--target",
    "web",
    "--out-dir",
    wasmOutDir,
    "--out-name",
    "web_runtime",
  ],
  { cwd: repoRoot },
);

function run(command, args, options) {
  const result = spawnSync(command, args, {
    ...options,
    stdio: "inherit",
  });
  if (result.error !== undefined) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}
