import type { MonocurlWasmRuntimeHandle } from "../index.js";

export class Runtime implements MonocurlWasmRuntimeHandle {
  free(): void;
  is_playing(): boolean;
  load_source(source: string, importsJson: string): string;
  load_source_with_root_path(rootPath: string, source: string, importsJson: string): string;
  needs_work(): boolean;
  seek_to(slide: number, time: number): void;
  set_presentation_mode(): void;
  set_preview_mode(): void;
  set_web_mode(): void;
  step(nowSeconds: number): Promise<number>;
  step_json(nowSeconds: number): Promise<string>;
  toggle_play(nowSeconds: number): void;
  update_parameters(updatesJson: string, nowSeconds: number): void;
}

export default function init(moduleOrPath?: unknown): Promise<unknown>;
