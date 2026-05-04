export type PlaybackMode = "preview" | "presentation";

export interface Timestamp {
  slide: number;
  time: number;
}

export interface RuntimeStepResult {
  snapshotCount: number;
  nowSeconds: number;
  isPlaying: boolean;
  needsWork: boolean;
}

export interface MonocurlWasmRuntimeHandle {
  monocurl_version(): string;
  supports_monocurl_version(version: string): boolean;
  native_function_count(): number;
  bytecode_instruction_size(): number;
  needs_work(): boolean;
  is_playing(): boolean;
  seek_to(slide: number, time: number): void;
  toggle_play(nowSeconds: number): void;
  set_presentation_mode(): void;
  set_preview_mode(): void;
  step(nowSeconds: number): Promise<number>;
  load_bytecode_json?(json: string): void;
  free?(): void;
}

export interface MonocurlWasmModule {
  Runtime: new () => MonocurlWasmRuntimeHandle;
  monocurl_version?: () => string;
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
  expectedVersion?: string;
  clock?: RuntimeClock;
  scheduler?: FrameScheduler;
  onStep?: (result: RuntimeStepResult) => void;
  onIdle?: (result: RuntimeStepResult) => void;
  onError?: (error: unknown) => void;
}

export class MonocurlVersionError extends Error {
  constructor(
    readonly expected: string,
    readonly actual: string,
  ) {
    super(`Monocurl version mismatch: expected ${expected}, got ${actual}`);
    this.name = "MonocurlVersionError";
  }
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
  const loop = new MonocurlLoop(runtime, options);

  if (
    options.expectedVersion !== undefined &&
    !loop.supportsVersion(options.expectedVersion)
  ) {
    throw new MonocurlVersionError(options.expectedVersion, loop.monocurlVersion);
  }

  return loop;
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

  get monocurlVersion(): string {
    return this.runtime.monocurl_version();
  }

  get nativeFunctionCount(): number {
    return this.runtime.native_function_count();
  }

  get bytecodeInstructionSize(): number {
    return this.runtime.bytecode_instruction_size();
  }

  get isPlaying(): boolean {
    return this.runtime.is_playing();
  }

  get needsWork(): boolean {
    return this.runtime.needs_work();
  }

  supportsVersion(version: string): boolean {
    return this.runtime.supports_monocurl_version(version);
  }

  loadBytecodeJson(json: string): void {
    if (this.runtime.load_bytecode_json === undefined) {
      throw new UnsupportedWasmMethodError("load_bytecode_json");
    }

    this.runtime.load_bytecode_json(json);
    this.requestStep();
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
    const snapshotCount = await this.runtime.step(nowSeconds);
    const result: RuntimeStepResult = {
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
