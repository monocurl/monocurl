import languageSemantics from "../docs/language-semantics.md";
import aiOverview from "../docs/ai-overview.md";
import languageBasics from "../docs/language-basics.md";
import meshesAndOperators from "../docs/meshes-and-operators.md";
import animations from "../docs/animations.md";
import paramsCameraBackground from "../docs/params-camera-background.md";
import debuggingPatterns from "../docs/debugging-patterns.md";
import cheatSheet from "../docs/cheat-sheet.md";
import stdlibDocs from "../docs/stdlib.md";
import cliDocs from "../docs/cli.md";
import riemannRectanglesExample from "../docs/riemann-rectangles.mcs";
import stdUtil from "../docs/std/util.mcl";
import stdMath from "../docs/std/math.mcl";
import stdColor from "../docs/std/color.mcl";
import stdMesh from "../docs/std/mesh.mcl";
import stdAnim from "../docs/std/anim.mcl";
import stdScene from "../docs/std/scene.mcl";

export const SERVER_NAME = "monocurl-mcp";
export const SERVER_TITLE = "Monocurl Documentation";
export const SERVER_VERSION = "0.2.1";
export const SERVER_INSTRUCTIONS =
  "Use resources/list and resources/read to load split Monocurl authoring context, stdlib documentation, CLI invocation guidance, and raw stdlib wrapper sources. Validation and execution should be handled outside this documentation server with the monocurl binary.";

type ResourceDefinition = {
  uri: string;
  name: string;
  title: string;
  description: string;
  mimeType: string;
  text: string;
};

export type ListedResource = {
  uri: string;
  name: string;
  title: string;
  description: string;
  mimeType: string;
  size: number;
  annotations: {
    audience: ["assistant"];
    priority: number;
  };
};

export type ResourcePayload = ListedResource & {
  text: string;
};

const RESOURCE_DEFINITIONS: readonly ResourceDefinition[] = [
  {
    uri: "monocurl://docs/language-semantics",
    name: "language-semantics",
    title: "Monocurl AI Context Index",
    description: "Index of split Monocurl authoring context resources.",
    mimeType: "text/markdown",
    text: languageSemantics,
  },
  {
    uri: "monocurl://docs/ai-overview",
    name: "ai-overview",
    title: "Monocurl AI Overview",
    description:
      "Project overview, scene skeleton, init/slide rules, UI notes, and timeline shortcuts.",
    mimeType: "text/markdown",
    text: aiOverview,
  },
  {
    uri: "monocurl://docs/language-basics",
    name: "language-basics",
    title: "Monocurl Language Basics",
    description:
      "Values, assignment, control flow, lambdas, block accumulation, calls, operators, set_default, and references.",
    mimeType: "text/markdown",
    text: languageBasics,
  },
  {
    uri: "monocurl://docs/meshes",
    name: "meshes",
    title: "Monocurl Meshes And Operators",
    description: "Mesh values, mesh trees, tags, filters, and text tags.",
    mimeType: "text/markdown",
    text: meshesAndOperators,
  },
  {
    uri: "monocurl://docs/animations",
    name: "animations",
    title: "Monocurl Animations",
    description:
      "Leader/follower semantics, Wait, Set, Lerp, morphs, animation blocks, parallelism, and rates.",
    mimeType: "text/markdown",
    text: animations,
  },
  {
    uri: "monocurl://docs/params-camera",
    name: "params-camera",
    title: "Monocurl Params, Camera, And Background",
    description:
      "Params, stateful values, presentation controls, camera, and background.",
    mimeType: "text/markdown",
    text: paramsCameraBackground,
  },
  {
    uri: "monocurl://docs/debugging-patterns",
    name: "debugging-patterns",
    title: "Monocurl Debugging, Patterns, And Examples",
    description:
      "Print transcripts, authoring patterns, anti-patterns, examples to imitate, and formatting conventions.",
    mimeType: "text/markdown",
    text: debuggingPatterns,
  },
  {
    uri: "monocurl://docs/cheat-sheet",
    name: "cheat-sheet",
    title: "Monocurl Cheat Sheet",
    description:
      "Compact imports, common constructors, operators, animations, utilities, colors, and scene constants.",
    mimeType: "text/markdown",
    text: cheatSheet,
  },
  {
    uri: "monocurl://docs/stdlib",
    name: "stdlib-overview",
    title: "Monocurl Standard Library Overview",
    description:
      "Overview of the public stdlib wrapper modules and authoring conventions.",
    mimeType: "text/markdown",
    text: stdlibDocs,
  },
  {
    uri: "monocurl://docs/cli",
    name: "cli-usage",
    title: "Monocurl Binary and CLI",
    description:
      "How to launch the shared GUI/CLI binary and run image, video, and transcript commands.",
    mimeType: "text/markdown",
    text: cliDocs,
  },
  {
    uri: "monocurl://examples/riemann-rectangles",
    name: "riemann-rectangles-example",
    title: "Riemann Rectangles Example Scene",
    description:
      "Complete Monocurl scene demonstrating graph helpers, tags, text tags, transcript prints, and multi-slide animation flow.",
    mimeType: "text/x-monocurl",
    text: riemannRectanglesExample,
  },
  {
    uri: "monocurl://stdlib/util",
    name: "std-util",
    title: "std.util Source",
    description:
      "Public utility wrappers for collections, strings, conversion, predicates, and live defaults.",
    mimeType: "text/x-monocurl",
    text: stdUtil,
  },
  {
    uri: "monocurl://stdlib/math",
    name: "std-math",
    title: "std.math Source",
    description:
      "Public scalar, vector, interpolation, statistics, and combinatorics wrappers.",
    mimeType: "text/x-monocurl",
    text: stdMath,
  },
  {
    uri: "monocurl://stdlib/color",
    name: "std-color",
    title: "std.color Source",
    description: "Public color constants and color helper wrappers.",
    mimeType: "text/x-monocurl",
    text: stdColor,
  },
  {
    uri: "monocurl://stdlib/mesh",
    name: "std-mesh",
    title: "std.mesh Source",
    description:
      "Public mesh constructors, graphing helpers, styling operators, transforms, tags, and queries.",
    mimeType: "text/x-monocurl",
    text: stdMesh,
  },
  {
    uri: "monocurl://stdlib/anim",
    name: "std-anim",
    title: "std.anim Source",
    description:
      "Public rate functions, primitive animations, follower animations, and animation composition wrappers.",
    mimeType: "text/x-monocurl",
    text: stdAnim,
  },
  {
    uri: "monocurl://stdlib/scene",
    name: "std-scene",
    title: "std.scene Source",
    description: "Public scene, camera, and background wrappers.",
    mimeType: "text/x-monocurl",
    text: stdScene,
  },
];

function priorityFor(uri: string): number {
  return uri.startsWith("monocurl://docs/") ? 1.0 : 0.8;
}

function loadResource(definition: ResourceDefinition): ResourcePayload {
  return {
    uri: definition.uri,
    name: definition.name,
    title: definition.title,
    description: definition.description,
    mimeType: definition.mimeType,
    size: Buffer.byteLength(definition.text, "utf8"),
    annotations: {
      audience: ["assistant"],
      priority: priorityFor(definition.uri),
    },
    text: definition.text,
  };
}

const RESOURCE_PAYLOADS: readonly ResourcePayload[] =
  RESOURCE_DEFINITIONS.map(loadResource);

export function listResources(): ListedResource[] {
  return RESOURCE_PAYLOADS.map(({ text: _text, ...resource }) => resource);
}

export function findResource(uri: string): ResourcePayload | undefined {
  return RESOURCE_PAYLOADS.find((resource) => resource.uri === uri);
}

export function readResource(uri: string): ResourcePayload {
  const resource = findResource(uri);

  if (!resource) {
    throw new Error(`resource_not_found: ${uri}`);
  }

  return resource;
}
