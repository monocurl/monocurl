import { existsSync, lstatSync, mkdirSync, rmSync, symlinkSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");
const repoRoot = resolve(packageRoot, "../..");

const assetDocs = resolve(repoRoot, "assets/monocurl-mcp/docs");

function relink(path, target, type) {
  if (existsSync(path) || lstatSync(path, { throwIfNoEntry: false })?.isSymbolicLink()) {
    rmSync(path, { recursive: true, force: true });
  }

  symlinkSync(target, path, type);
}

mkdirSync(assetDocs, { recursive: true });

relink(resolve(assetDocs, "std"), "../../std/std", "dir");
relink(resolve(packageRoot, "docs"), "../../assets/monocurl-mcp/docs", "dir");
relink(resolve(packageRoot, "icon.png"), "../../assets/img/monocurl-1024.png", "file");
