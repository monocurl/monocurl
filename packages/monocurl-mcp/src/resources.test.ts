import test from "node:test";
import assert from "node:assert/strict";

import { findResource, listResources, readResource } from "./resources.js";

test("lists the split docs and stdlib resources", () => {
  const resources = listResources();

  assert.equal(resources.length, 17);
  assert.ok(
    resources.some(
      (resource) => resource.uri === "monocurl://docs/language-semantics",
    ),
  );
  assert.ok(
    resources.some((resource) => resource.uri === "monocurl://stdlib/math"),
  );
  assert.ok(
    resources.some(
      (resource) => resource.uri === "monocurl://examples/riemann-rectangles",
    ),
  );
});

test("applies assistant annotations and doc priorities", () => {
  const overview = findResource("monocurl://docs/ai-overview");
  const math = findResource("monocurl://stdlib/math");

  assert.equal(overview?.annotations.audience[0], "assistant");
  assert.equal(overview?.annotations.priority, 1.0);
  assert.equal(math?.annotations.priority, 0.8);
});

test("reads markdown and Monocurl resources", () => {
  const overview = readResource("monocurl://docs/ai-overview");
  const math = readResource("monocurl://stdlib/math");

  assert.equal(overview.mimeType, "text/markdown");
  assert.match(overview.text, /Monocurl/i);
  assert.equal(math.mimeType, "text/x-monocurl");
  assert.match(math.text, /let\s+norm/);
});

test("throws for unknown resources", () => {
  assert.throws(
    () => readResource("monocurl://docs/missing"),
    /resource_not_found/,
  );
});
