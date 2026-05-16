import { installMonocurlMathJaxRenderer } from "./mathjax-renderer.js";
import type { MonocurlMathJax } from "./mathjax-renderer.js";
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

export interface PlayOptions {
  until?: Timestamp;
}

export interface CompilationReport {
  ok: boolean;
  diagnostics: CompilationDiagnostic[];
  slides: SlideMetadata[];
}

export interface SlideMetadata {
  index: number;
  name: string | null;
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
  target: PresentationUpdateTarget;
  name: string;
  value: ParameterValue;
}

export type PresentationUpdateTarget =
  | { kind: "param"; leaderIndex: number }
  | {
      kind: "meshAttribute";
      leaderIndex: number;
      name: string;
    };

export type ParameterValue =
  | { kind: "int"; value: number }
  | { kind: "vectorInt"; value: number[] }
  | { kind: "float"; value: number }
  | { kind: "vectorFloat"; value: number[] }
  | { kind: "complex"; re: number; im: number }
  | { kind: "camera"; value: CameraSnapshot }
  | { kind: "other" };

export interface ParameterUpdate {
  target: PresentationUpdateTarget;
  value: ParameterValue;
}

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
  needs_work(): boolean;
  is_playing(): boolean;
  seek_to(slide: number, time: number): void;
  toggle_play(nowSeconds: number): void;
  set_web_mode(): void;
  update_parameters?(updatesJson: string, nowSeconds: number): void;
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

export type MonocurlWasmInitInput = unknown;

export interface RuntimeClock {
  nowSeconds(): number;
}

export interface FrameScheduler {
  request(callback: () => void): number;
  cancel(handle: number): void;
}

export type MonocurlMathJaxSource =
  | MonocurlMathJax
  | PromiseLike<MonocurlMathJax | undefined>
  | false;

export interface CreateMonocurlLoopOptions {
  /**
   * Optional override for tests or custom bundlers. By default the packaged
   * Monocurl wasm runtime is initialized and used automatically.
   */
  wasm?: MonocurlWasmSource;
  /**
   * Optional input passed to wasm-bindgen initialization. Leave unset to fetch
   * the packaged `web_runtime_bg.wasm` next to this package's wasm JS glue.
   */
  wasmInit?: MonocurlWasmInitInput;
  runtime?: MonocurlWasmRuntimeHandle;
  /**
   * MathJax instance, or readiness promise, to install for Text/Tex rendering.
   * By default, an already-ready global `MathJax` is installed when available.
   * Pass `false` to opt out.
   */
  mathJax?: MonocurlMathJaxSource;
  mathJaxDisplay?: boolean;
  clock?: RuntimeClock;
  scheduler?: FrameScheduler;
  onStep?: (result: RuntimeStepResult) => void;
  onIdle?: (result: RuntimeStepResult) => void;
  onError?: (error: unknown) => void;
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
    slides: parsed.slides ?? [],
  };
}

function timestampCompare(a: Timestamp, b: Timestamp): number {
  return a.slide === b.slide ? a.time - b.time : a.slide - b.slide;
}

function validateTimestamp(timestamp: Timestamp, label: string): void {
  if (!Number.isInteger(timestamp.slide) || timestamp.slide < 0) {
    throw new TypeError(`${label}.slide must be a non-negative integer`);
  }
  if (Number.isNaN(timestamp.time)) {
    throw new TypeError(`${label}.time must not be NaN`);
  }
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
  options: CreateMonocurlLoopOptions = {},
): Promise<MonocurlLoop> {
  const uninstallMathJax = await installMathJaxIfAvailable(options);
  try {
    const runtime = options.runtime ?? new (await resolveWasmModule(options)).Runtime();
    const loop = new MonocurlLoop(runtime, options);
    if (uninstallMathJax !== undefined) {
      loopCleanup.set(loop, uninstallMathJax);
    }
    return loop;
  } catch (error) {
    uninstallMathJax?.();
    throw error;
  }
}

async function resolveWasmModule(
  options: Pick<CreateMonocurlLoopOptions, "wasm" | "wasmInit">,
): Promise<MonocurlWasmModule> {
  if (options.wasm === undefined) {
    return initPackagedWasmInstance(options.wasmInit);
  }

  if (typeof options.wasm === "function") {
    return await options.wasm();
  }

  return await options.wasm;
}

type PackagedWasmModule = MonocurlWasmModule & {
  default: (input?: MonocurlWasmInitInput) => Promise<unknown>;
};

let packagedWasmInstanceId = 0;

async function initPackagedWasmInstance(
  wasmInit: MonocurlWasmInitInput | undefined,
): Promise<PackagedWasmModule> {
  const glueUrl = new URL("./wasm/web_runtime.js", import.meta.url);
  glueUrl.searchParams.set("monocurlInstance", String(packagedWasmInstanceId++));

  const wasmModule = (await import(glueUrl.href)) as PackagedWasmModule;
  await wasmModule.default(wasmInit);
  return wasmModule;
}

async function installMathJaxIfAvailable(
  options: Pick<CreateMonocurlLoopOptions, "mathJax" | "mathJaxDisplay">,
): Promise<(() => void) | undefined> {
  if (options.mathJax === false) {
    return undefined;
  }

  const mathJax =
    options.mathJax !== undefined
      ? await options.mathJax
      : readyGlobalMathJax();
  if (mathJax === undefined) {
    return undefined;
  }

  await mathJax.startup?.promise;
  return installMonocurlMathJaxRenderer({
    mathJax,
    display: options.mathJaxDisplay,
  });
}

function readyGlobalMathJax(): MonocurlMathJax | undefined {
  const mathJax = globalThis.MathJax;
  return typeof mathJax?.tex2svg === "function" ? mathJax : undefined;
}

type MonocurlLoopOptions = Omit<
  CreateMonocurlLoopOptions,
  "wasm" | "wasmInit" | "runtime" | "mathJax" | "mathJaxDisplay"
>;

const loopCleanup = new WeakMap<MonocurlLoop, () => void>();

export class MonocurlLoop {
  private readonly clock: RuntimeClock;
  private readonly scheduler: FrameScheduler;
  private readonly onStep?: (result: RuntimeStepResult) => void;
  private readonly onIdle?: (result: RuntimeStepResult) => void;
  private readonly onError?: (error: unknown) => void;
  private scheduledFrame: number | undefined;
  private pendingStep: Promise<RuntimeStepResult> | undefined;
  private needsNextFrame = false;
  private playUntil: Timestamp | undefined;
  private disposed = false;

  constructor(
    readonly runtime: MonocurlWasmRuntimeHandle,
    options: MonocurlLoopOptions = {},
  ) {
    this.clock = options.clock ?? performanceClock;
    this.scheduler = options.scheduler ?? animationFrameScheduler;
    this.onStep = options.onStep;
    this.onIdle = options.onIdle;
    this.onError = options.onError;
    this.runtime.set_web_mode();
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
    this.playUntil = undefined;
    this.requestStep();
    return report;
  }

  seekTo(timestamp: Timestamp): void {
    validateTimestamp(timestamp, "timestamp");
    this.playUntil = undefined;
    this.runtime.seek_to(timestamp.slide, timestamp.time);
    this.requestStep();
  }

  updateParameter(
    target: PresentationUpdateTarget,
    value: ParameterValue,
    nowSeconds = this.clock.nowSeconds(),
  ): void {
    this.updateParameters([{ target, value }], nowSeconds);
  }

  updateParameters(
    updates: ParameterUpdate[],
    nowSeconds = this.clock.nowSeconds(),
  ): void {
    if (this.runtime.update_parameters === undefined) {
      throw new UnsupportedWasmMethodError("update_parameters");
    }

    this.runtime.update_parameters(JSON.stringify(updates), nowSeconds);
    this.requestStep();
  }

  togglePlay(): void {
    this.playUntil = undefined;
    this.runtime.toggle_play(this.clock.nowSeconds());
    this.requestStep();
  }

  play(options: PlayOptions = {}): void {
    this.playUntil = options.until;
    if (this.playUntil !== undefined) {
      validateTimestamp(this.playUntil, "options.until");
    }

    if (!this.runtime.is_playing()) {
      this.runtime.toggle_play(this.clock.nowSeconds());
    }
    this.requestStep();
  }

  pause(): void {
    this.playUntil = undefined;
    if (this.runtime.is_playing()) {
      this.runtime.toggle_play(this.clock.nowSeconds());
      this.requestStep();
    }
  }

  private requestStep(): void {
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
      if (this.needsNextFrame) {
        this.needsNextFrame = false;
        this.requestStep();
      }
    }
  }

  private stop(): void {
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
    loopCleanup.get(this)?.();
    loopCleanup.delete(this);
    this.disposed = true;
  }

  private async runStep(nowSeconds: number): Promise<RuntimeStepResult> {
    let iteration: RuntimeIteration;
    let snapshotCount: number;

    if (this.runtime.step_json !== undefined) {
      iteration = parseRuntimeIterationJson(await this.runtime.step_json(nowSeconds));
      iteration = await this.applyPlayUntil(iteration, nowSeconds);
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
      this.needsNextFrame = true;
    } else {
      this.needsNextFrame = false;
      this.onIdle?.(result);
    }

    return result;
  }

  private async applyPlayUntil(
    iteration: RuntimeIteration,
    nowSeconds: number,
  ): Promise<RuntimeIteration> {
    if (this.playUntil === undefined) {
      return iteration;
    }

    const until = this.playUntil;
    const reachedIndex = iteration.snapshots.findIndex(
      (snapshot) => timestampCompare(snapshot.currentTimestamp, until) >= 0,
    );
    if (reachedIndex === -1) {
      return iteration;
    }

    this.playUntil = undefined;
    const beforeLimit = iteration.snapshots.slice(0, reachedIndex);
    this.runtime.seek_to(until.slide, until.time);
    const stepJson = this.runtime.step_json;
    if (stepJson === undefined) {
      return { snapshots: beforeLimit };
    }

    const stopped = parseRuntimeIterationJson(await stepJson.call(this.runtime, nowSeconds));
    return {
      snapshots: beforeLimit.concat(stopped.snapshots),
      nextFrameInterval: stopped.nextFrameInterval,
    };
  }

  private assertLive(): void {
    if (this.disposed) {
      throw new Error("MonocurlLoop has been disposed");
    }
  }
}
