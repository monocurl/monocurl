export type PlaybackMode = "preview" | "presentation";
export type Vec2 = [number, number];
export type Vec3 = [number, number, number];
export type Vec4 = [number, number, number, number];
export type ExecutionStatus = "playing" | "paused" | "runtimeError" | "compileError";

export {
  MonocurlWebGlRenderer,
  UnsupportedWebGlRendererError,
  createMonocurlWebGlRenderer,
} from "./webgl-renderer.js";
export type { MonocurlWebGlRendererOptions } from "./webgl-renderer.js";
export {
  MissingMathJaxError,
  installMonocurlMathJaxRenderer,
  renderMathJaxSvg,
} from "./mathjax-renderer.js";
export type {
  MonocurlLatexSvgRenderer,
  MonocurlMathJax,
  MonocurlMathJaxRendererOptions,
} from "./mathjax-renderer.js";

export interface Timestamp {
  slide: number;
  time: number;
}

export interface RuntimeIteration {
  snapshots: ExecutionSnapshot[];
  nextFrameInterval?: number;
}

export interface RuntimeStepResult extends RuntimeIteration {
  snapshotCount: number;
  nowSeconds: number;
  isPlaying: boolean;
  needsWork: boolean;
}

export interface CompilationReport {
  ok: boolean;
  diagnostics: CompilationDiagnostic[];
}

export interface CompilationDiagnostic {
  kind: "parseError" | "compileError" | "compileWarning";
  title: string;
  message: string;
  span: SourceSpan;
}

export interface ExecutionSnapshot {
  background?: BackgroundSnapshot;
  camera?: CameraSnapshot;
  cameraVersion?: number;
  meshes?: MeshSnapshot[];
  errors?: RuntimeErrorSnapshot[];
  currentTimestamp: Timestamp;
  status: ExecutionStatus;
  isLoading: boolean;
  slideCount: number;
  slideNames: Array<string | null>;
  slideDurations: Array<number | null>;
  minimumSlideDurations: Array<number | null>;
  parameters?: ParameterSnapshot;
  transcript?: TranscriptSection[];
}

export interface RuntimeErrorSnapshot {
  message: string;
  span: SourceSpan;
  hint?: string;
  callstack?: RuntimeCallFrameSnapshot[];
}

export interface RuntimeCallFrameSnapshot {
  section: number;
  span: SourceSpan;
}

export interface BackgroundSnapshot {
  color: Vec4;
}

export interface CameraSnapshot {
  position: Vec3;
  lookAt: Vec3;
  up: Vec3;
  near: number;
  far: number;
}

export interface MeshSnapshot {
  version: number;
  tag: number[];
  uniform: MeshUniforms;
  dots: DotSnapshot[];
  lines: LineSnapshot[];
  triangles: TriangleSnapshot[];
}

export interface MeshUniforms {
  alpha: number;
  strokeMiterRadiusScale: number;
  strokeRadius: number;
  dotRadius: number;
  dotVertexCount: number;
  smooth: boolean;
  gloss: number;
  image?: string;
  zIndex: number;
}

export interface DotSnapshot {
  position: Vec3;
  normal: Vec3;
  color: Vec4;
  inverse: number;
  isDominantSibling: boolean;
}

export interface LineVertexSnapshot {
  position: Vec3;
  color: Vec4;
}

export interface LineSnapshot {
  a: LineVertexSnapshot;
  b: LineVertexSnapshot;
  normal: Vec3;
  previous: number;
  next: number;
  inverse: number;
  isDominantSibling: boolean;
}

export interface TriangleVertexSnapshot {
  position: Vec3;
  color: Vec4;
  uv: Vec2;
}

export interface TriangleSnapshot {
  a: TriangleVertexSnapshot;
  b: TriangleVertexSnapshot;
  c: TriangleVertexSnapshot;
  edgeAb: number;
  edgeBc: number;
  edgeCa: number;
  isDominantSibling: boolean;
}

export interface ParameterSnapshot {
  params: ParameterEntrySnapshot[];
  meshes: MeshEntrySnapshot[];
}

export interface ParameterEntrySnapshot {
  target: PresentationUpdateTarget;
  name: string;
  value: ParameterValue;
  locked: boolean;
}

export interface MeshEntrySnapshot {
  leaderIndex: number;
  name: string;
  locked: boolean;
  attributes: MeshAttributeSnapshot[];
}

export interface MeshAttributeSnapshot {
  target?: PresentationUpdateTarget;
  name: string;
  value: ParameterValue;
  children: MeshAttributeSnapshot[];
}

export type PresentationUpdateTarget =
  | { kind: "param"; leaderIndex: number }
  | {
      kind: "meshAttribute";
      leaderIndex: number;
      path: MeshAttributePathSegment[];
    };

export type MeshAttributePathSegment =
  | { kind: "listIndex"; index: number }
  | { kind: "functionArgument"; index: number }
  | { kind: "operatorOperand" }
  | { kind: "operatorArgument"; index: number };

export type ParameterValue =
  | { kind: "int"; value: number }
  | { kind: "vectorInt"; value: number[] }
  | { kind: "float"; value: number }
  | { kind: "vectorFloat"; value: number[] }
  | { kind: "complex"; re: number; im: number }
  | { kind: "camera"; value: CameraSnapshot }
  | { kind: "other" };

export interface TranscriptSection {
  entries: TranscriptEntry[];
}

export interface TranscriptEntry {
  span: SourceSpan;
  section: number;
  isRoot: boolean;
  text: string;
}

export interface SourceSpan {
  start: number;
  end: number;
}

export interface PackedMeshSnapshot {
  version: number;
  tag: Int32Array;
  uniform: MeshUniforms;
  dots: PackedDotBuffer;
  lines: PackedLineBuffer;
  triangles: PackedTriangleBuffer;
}

export interface PackedDotBuffer {
  count: number;
  positions: Float32Array;
  normals: Float32Array;
  colors: Float32Array;
  inverse: Int32Array;
  isDominantSibling: Uint8Array;
}

export interface PackedLineBuffer {
  count: number;
  positions: Float32Array;
  colors: Float32Array;
  normals: Float32Array;
  previous: Int32Array;
  next: Int32Array;
  inverse: Int32Array;
  isDominantSibling: Uint8Array;
}

export interface PackedTriangleBuffer {
  count: number;
  positions: Float32Array;
  colors: Float32Array;
  uvs: Float32Array;
  edges: Int32Array;
  isDominantSibling: Uint8Array;
}

export interface MonocurlWasmRuntimeHandle {
  native_function_count(): number;
  needs_work(): boolean;
  is_playing(): boolean;
  seek_to(slide: number, time: number): void;
  toggle_play(nowSeconds: number): void;
  set_presentation_mode(): void;
  set_preview_mode(): void;
  step(nowSeconds: number): Promise<number>;
  step_json?(nowSeconds: number): Promise<string>;
  load_source?(source: string, importsJson: string): string;
  load_source_with_root_path?(
    rootPath: string,
    source: string,
    importsJson: string,
  ): string;
  free?(): void;
}

export interface MonocurlWasmModule {
  Runtime: new () => MonocurlWasmRuntimeHandle;
}

export type MonocurlWasmSource =
  | MonocurlWasmModule
  | Promise<MonocurlWasmModule>
  | (() => MonocurlWasmModule | Promise<MonocurlWasmModule>);

export interface RuntimeClock {
  nowSeconds(): number;
}

export interface FrameScheduler {
  request(callback: () => void): number;
  cancel(handle: number): void;
}

export interface CreateMonocurlLoopOptions {
  wasm?: MonocurlWasmSource;
  runtime?: MonocurlWasmRuntimeHandle;
  clock?: RuntimeClock;
  scheduler?: FrameScheduler;
  onStep?: (result: RuntimeStepResult) => void;
  onIdle?: (result: RuntimeStepResult) => void;
  onError?: (error: unknown) => void;
}

export class MissingWasmRuntimeError extends Error {
  constructor() {
    super("createMonocurlLoop requires either a wasm module or a runtime handle");
    this.name = "MissingWasmRuntimeError";
  }
}

export class UnsupportedWasmMethodError extends Error {
  constructor(method: string) {
    super(`The loaded Monocurl wasm runtime does not expose ${method}`);
    this.name = "UnsupportedWasmMethodError";
  }
}

function writeVec2(out: Float32Array, offset: number, value: Vec2): void {
  out[offset] = value[0];
  out[offset + 1] = value[1];
}

function writeVec3(out: Float32Array, offset: number, value: Vec3): void {
  out[offset] = value[0];
  out[offset + 1] = value[1];
  out[offset + 2] = value[2];
}

function writeVec4(out: Float32Array, offset: number, value: Vec4): void {
  out[offset] = value[0];
  out[offset + 1] = value[1];
  out[offset + 2] = value[2];
  out[offset + 3] = value[3];
}

export function packMeshSnapshot(mesh: MeshSnapshot): PackedMeshSnapshot {
  return {
    version: mesh.version,
    tag: Int32Array.from(mesh.tag),
    uniform: mesh.uniform,
    dots: packDots(mesh.dots),
    lines: packLines(mesh.lines),
    triangles: packTriangles(mesh.triangles),
  };
}

export function packSnapshotMeshes(snapshot: ExecutionSnapshot): PackedMeshSnapshot[] {
  return (snapshot.meshes ?? []).map(packMeshSnapshot);
}

function packDots(dots: DotSnapshot[]): PackedDotBuffer {
  const count = dots.length;
  const positions = new Float32Array(count * 3);
  const normals = new Float32Array(count * 3);
  const colors = new Float32Array(count * 4);
  const inverse = new Int32Array(count);
  const isDominantSibling = new Uint8Array(count);

  for (const [index, dot] of dots.entries()) {
    writeVec3(positions, index * 3, dot.position);
    writeVec3(normals, index * 3, dot.normal);
    writeVec4(colors, index * 4, dot.color);
    inverse[index] = dot.inverse;
    isDominantSibling[index] = dot.isDominantSibling ? 1 : 0;
  }

  return { count, positions, normals, colors, inverse, isDominantSibling };
}

function packLines(lines: LineSnapshot[]): PackedLineBuffer {
  const count = lines.length;
  const positions = new Float32Array(count * 6);
  const colors = new Float32Array(count * 8);
  const normals = new Float32Array(count * 3);
  const previous = new Int32Array(count);
  const next = new Int32Array(count);
  const inverse = new Int32Array(count);
  const isDominantSibling = new Uint8Array(count);

  for (const [index, line] of lines.entries()) {
    writeVec3(positions, index * 6, line.a.position);
    writeVec3(positions, index * 6 + 3, line.b.position);
    writeVec4(colors, index * 8, line.a.color);
    writeVec4(colors, index * 8 + 4, line.b.color);
    writeVec3(normals, index * 3, line.normal);
    previous[index] = line.previous;
    next[index] = line.next;
    inverse[index] = line.inverse;
    isDominantSibling[index] = line.isDominantSibling ? 1 : 0;
  }

  return {
    count,
    positions,
    colors,
    normals,
    previous,
    next,
    inverse,
    isDominantSibling,
  };
}

function packTriangles(triangles: TriangleSnapshot[]): PackedTriangleBuffer {
  const count = triangles.length;
  const positions = new Float32Array(count * 9);
  const colors = new Float32Array(count * 12);
  const uvs = new Float32Array(count * 6);
  const edges = new Int32Array(count * 3);
  const isDominantSibling = new Uint8Array(count);

  for (const [index, triangle] of triangles.entries()) {
    writeVec3(positions, index * 9, triangle.a.position);
    writeVec3(positions, index * 9 + 3, triangle.b.position);
    writeVec3(positions, index * 9 + 6, triangle.c.position);
    writeVec4(colors, index * 12, triangle.a.color);
    writeVec4(colors, index * 12 + 4, triangle.b.color);
    writeVec4(colors, index * 12 + 8, triangle.c.color);
    writeVec2(uvs, index * 6, triangle.a.uv);
    writeVec2(uvs, index * 6 + 2, triangle.b.uv);
    writeVec2(uvs, index * 6 + 4, triangle.c.uv);
    edges[index * 3] = triangle.edgeAb;
    edges[index * 3 + 1] = triangle.edgeBc;
    edges[index * 3 + 2] = triangle.edgeCa;
    isDominantSibling[index] = triangle.isDominantSibling ? 1 : 0;
  }

  return { count, positions, colors, uvs, edges, isDominantSibling };
}

export function parseRuntimeIterationJson(json: string): RuntimeIteration {
  const parsed = JSON.parse(json) as Partial<RuntimeIteration>;

  return {
    snapshots: parsed.snapshots ?? [],
    nextFrameInterval: parsed.nextFrameInterval,
  };
}

export function parseCompilationReport(json: string): CompilationReport {
  const parsed = JSON.parse(json) as Partial<CompilationReport>;

  return {
    ok: parsed.ok === true,
    diagnostics: parsed.diagnostics ?? [],
  };
}

export const performanceClock: RuntimeClock = {
  nowSeconds(): number {
    return globalThis.performance.now() / 1000;
  },
};

export const animationFrameScheduler: FrameScheduler = {
  request(callback: () => void): number {
    if (typeof globalThis.requestAnimationFrame === "function") {
      return globalThis.requestAnimationFrame(() => callback());
    }

    return globalThis.setTimeout(callback, 16);
  },

  cancel(handle: number): void {
    if (typeof globalThis.cancelAnimationFrame === "function") {
      globalThis.cancelAnimationFrame(handle);
      return;
    }

    globalThis.clearTimeout(handle);
  },
};

export async function createMonocurlLoop(
  options: CreateMonocurlLoopOptions,
): Promise<MonocurlLoop> {
  const runtime = options.runtime ?? new (await resolveWasmModule(options.wasm)).Runtime();
  return new MonocurlLoop(runtime, options);
}

async function resolveWasmModule(
  wasm: MonocurlWasmSource | undefined,
): Promise<MonocurlWasmModule> {
  if (wasm === undefined) {
    throw new MissingWasmRuntimeError();
  }

  if (typeof wasm === "function") {
    return await wasm();
  }

  return await wasm;
}

export class MonocurlLoop {
  private readonly clock: RuntimeClock;
  private readonly scheduler: FrameScheduler;
  private readonly onStep?: (result: RuntimeStepResult) => void;
  private readonly onIdle?: (result: RuntimeStepResult) => void;
  private readonly onError?: (error: unknown) => void;
  private scheduledFrame: number | undefined;
  private pendingStep: Promise<RuntimeStepResult> | undefined;
  private disposed = false;

  constructor(
    readonly runtime: MonocurlWasmRuntimeHandle,
    options: Omit<CreateMonocurlLoopOptions, "wasm" | "runtime"> = {},
  ) {
    this.clock = options.clock ?? performanceClock;
    this.scheduler = options.scheduler ?? animationFrameScheduler;
    this.onStep = options.onStep;
    this.onIdle = options.onIdle;
    this.onError = options.onError;
  }

  get nativeFunctionCount(): number {
    return this.runtime.native_function_count();
  }

  get isPlaying(): boolean {
    return this.runtime.is_playing();
  }

  get needsWork(): boolean {
    return this.runtime.needs_work();
  }

  loadSource(
    source: string,
    imports: Record<string, string> = {},
    rootPath?: string,
  ): CompilationReport {
    const importsJson = JSON.stringify(imports);
    let reportJson: string;
    if (rootPath === undefined) {
      if (this.runtime.load_source === undefined) {
        throw new UnsupportedWasmMethodError("load_source");
      }
      reportJson = this.runtime.load_source(source, importsJson);
    } else {
      if (this.runtime.load_source_with_root_path === undefined) {
        throw new UnsupportedWasmMethodError("load_source_with_root_path");
      }
      reportJson = this.runtime.load_source_with_root_path(
        rootPath,
        source,
        importsJson,
      );
    }

    const report = parseCompilationReport(reportJson);
    this.requestStep();
    return report;
  }

  setPlaybackMode(mode: PlaybackMode): void {
    if (mode === "presentation") {
      this.runtime.set_presentation_mode();
    } else {
      this.runtime.set_preview_mode();
    }
    this.requestStep();
  }

  seekTo(timestamp: Timestamp): void {
    this.runtime.seek_to(timestamp.slide, timestamp.time);
    this.requestStep();
  }

  togglePlay(nowSeconds = this.clock.nowSeconds()): void {
    this.runtime.toggle_play(nowSeconds);
    this.requestStep();
  }

  play(nowSeconds = this.clock.nowSeconds()): void {
    if (!this.runtime.is_playing()) {
      this.togglePlay(nowSeconds);
    }
  }

  pause(nowSeconds = this.clock.nowSeconds()): void {
    if (this.runtime.is_playing()) {
      this.togglePlay(nowSeconds);
    }
  }

  requestStep(): void {
    this.assertLive();
    if (this.scheduledFrame !== undefined || this.pendingStep !== undefined) {
      return;
    }

    this.scheduledFrame = this.scheduler.request(() => {
      this.scheduledFrame = undefined;
      void this.step().catch((error: unknown) => {
        this.onError?.(error);
      });
    });
  }

  async step(nowSeconds = this.clock.nowSeconds()): Promise<RuntimeStepResult> {
    this.assertLive();
    if (this.pendingStep !== undefined) {
      return this.pendingStep;
    }

    this.pendingStep = this.runStep(nowSeconds);
    try {
      return await this.pendingStep;
    } finally {
      this.pendingStep = undefined;
    }
  }

  stop(): void {
    if (this.scheduledFrame !== undefined) {
      this.scheduler.cancel(this.scheduledFrame);
      this.scheduledFrame = undefined;
    }
  }

  dispose(): void {
    if (this.disposed) {
      return;
    }

    this.stop();
    this.runtime.free?.();
    this.disposed = true;
  }

  private async runStep(nowSeconds: number): Promise<RuntimeStepResult> {
    let iteration: RuntimeIteration;
    let snapshotCount: number;

    if (this.runtime.step_json !== undefined) {
      iteration = parseRuntimeIterationJson(await this.runtime.step_json(nowSeconds));
      snapshotCount = iteration.snapshots.length;
    } else {
      snapshotCount = await this.runtime.step(nowSeconds);
      iteration = { snapshots: [] };
    }

    const result: RuntimeStepResult = {
      ...iteration,
      snapshotCount,
      nowSeconds,
      isPlaying: this.runtime.is_playing(),
      needsWork: this.runtime.needs_work(),
    };

    this.onStep?.(result);

    if (result.isPlaying || result.needsWork) {
      this.requestStep();
    } else {
      this.onIdle?.(result);
    }

    return result;
  }

  private assertLive(): void {
    if (this.disposed) {
      throw new Error("MonocurlLoop has been disposed");
    }
  }
}
