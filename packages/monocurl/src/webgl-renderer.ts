import type {
  CameraSnapshot,
  DotSnapshot,
  ExecutionSnapshot,
  LineSnapshot,
  MeshSnapshot,
  TriangleSnapshot,
  Vec3,
  Vec4,
} from "./index.js";
import {
  DOT_VERTEX_SHADER,
  LINE_VERTEX_SHADER,
  SOLID_FRAGMENT_SHADER,
  TRIANGLE_FRAGMENT_SHADER,
  TRIANGLE_VERTEX_SHADER,
} from "./webgl-shaders.js";

export interface MonocurlWebGlRendererOptions {
  contextAttributes?: WebGLContextAttributes;
  pixelRatio?: number | (() => number);
  lineWidthPx?: number;
  dotRadiusPx?: number;
}

export class UnsupportedWebGlRendererError extends Error {
  constructor() {
    super("MonocurlWebGlRenderer requires a WebGL2 rendering context");
    this.name = "UnsupportedWebGlRendererError";
  }
}

type CameraBasis = {
  position: Vec3;
  right: Vec3;
  up: Vec3;
  forward: Vec3;
  near: number;
  far: number;
  tanHalfFov: number;
};

type ProgramInfo = {
  program: WebGLProgram;
  uniforms: Record<string, WebGLUniformLocation>;
};

type LineVertex = {
  position: Vec3;
  color: Vec4;
  tangent: Vec3;
  previousTangent: Vec3;
  extrude: number;
};

const DEFAULT_CAMERA_FOV = 1.0247789;
const MIN_CAMERA_NEAR = 0.01;
const REFERENCE_WIDTH = 1480;
const DEFAULT_LINE_MITER_SCALE = 4;
const DEPTH_STEP = 1e-6;
const EPSILON = 1e-6;

const LINE_VERTEX_INDICES = [
  0, 2, 1, 1, 2, 4, 1, 4, 3, 3, 4, 5, 6, 7, 3, 3, 7, 8, 3, 8, 1, 1, 8, 9,
] as const;

export class MonocurlWebGlRenderer {
  readonly canvas: HTMLCanvasElement;
  readonly gl: WebGL2RenderingContext;

  private readonly triangleProgram: ProgramInfo;
  private readonly lineProgram: ProgramInfo;
  private readonly dotProgram: ProgramInfo;
  private readonly triangleBuffer: WebGLBuffer;
  private readonly lineBuffer: WebGLBuffer;
  private readonly dotBuffer: WebGLBuffer;
  private readonly triangleVao: WebGLVertexArrayObject;
  private readonly lineVao: WebGLVertexArrayObject;
  private readonly dotVao: WebGLVertexArrayObject;
  private readonly pixelRatio: number | (() => number);
  private readonly lineWidthPx: number;
  private readonly dotRadiusPx: number;
  private disposed = false;

  constructor(canvas: HTMLCanvasElement, options: MonocurlWebGlRendererOptions = {}) {
    const gl = canvas.getContext("webgl2", {
      alpha: true,
      antialias: true,
      depth: true,
      premultipliedAlpha: false,
      ...options.contextAttributes,
    });
    if (gl === null) {
      throw new UnsupportedWebGlRendererError();
    }

    this.canvas = canvas;
    this.gl = gl;
    this.pixelRatio = options.pixelRatio ?? (() => globalThis.devicePixelRatio || 1);
    this.lineWidthPx = options.lineWidthPx ?? 1;
    this.dotRadiusPx = options.dotRadiusPx ?? 3.5;

    this.triangleProgram = createProgramInfo(
      gl,
      TRIANGLE_VERTEX_SHADER,
      TRIANGLE_FRAGMENT_SHADER,
      [
        "uCameraPosition",
        "uCameraRight",
        "uCameraUp",
        "uCameraForward",
        "uCameraClip",
        "uViewportScale",
        "uDepthBias",
        "uAlpha",
        "uGloss",
      ],
    );
    this.lineProgram = createProgramInfo(gl, LINE_VERTEX_SHADER, SOLID_FRAGMENT_SHADER, [
      "uCameraPosition",
      "uCameraRight",
      "uCameraUp",
      "uCameraForward",
      "uCameraClip",
      "uViewportScale",
      "uViewportAndLineWidth",
      "uDepthBiasAndMiterScale",
    ]);
    this.dotProgram = createProgramInfo(gl, DOT_VERTEX_SHADER, SOLID_FRAGMENT_SHADER, [
      "uCameraPosition",
      "uCameraRight",
      "uCameraUp",
      "uCameraForward",
      "uCameraClip",
      "uViewportScale",
      "uViewportAndRadius",
      "uDepthBias",
    ]);

    this.triangleBuffer = createBuffer(gl);
    this.lineBuffer = createBuffer(gl);
    this.dotBuffer = createBuffer(gl);
    this.triangleVao = createTriangleVao(gl, this.triangleBuffer);
    this.lineVao = createLineVao(gl, this.lineBuffer);
    this.dotVao = createDotVao(gl, this.dotBuffer);
  }

  render(snapshot: ExecutionSnapshot): void {
    this.assertLive();
    this.resizeToDisplaySize();

    const gl = this.gl;
    gl.viewport(0, 0, this.canvas.width, this.canvas.height);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
    gl.enable(gl.DEPTH_TEST);
    gl.depthFunc(gl.LEQUAL);
    gl.depthMask(true);
    gl.clearDepth(1);

    const background = snapshot.background?.color ?? [1, 1, 1, 1];
    gl.clearColor(background[0], background[1], background[2], background[3]);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

    const meshes = sortedVisibleMeshes(snapshot.meshes ?? []);
    const camera = cameraBasis(snapshot.camera);
    const viewportScale: [number, number] = [1, 1];
    let depthBias = 0;

    for (const mesh of meshes) {
      const triangles = buildTriangleData(mesh);
      if (triangles.length > 0) {
        this.drawTriangles(triangles, mesh, camera, viewportScale, depthBias);
        depthBias += DEPTH_STEP;
      }

      const lineRadius = meshLineRadiusPx(mesh, this.canvas.width, this.lineWidthPx);
      if (lineRadius > EPSILON) {
        const lines = buildLineData(mesh);
        if (lines.length > 0) {
          this.drawLines(lines, mesh, camera, viewportScale, lineRadius, depthBias);
          depthBias += DEPTH_STEP;
        }
      }

      const dotRadius = meshDotRadiusPx(mesh, this.resolvedPixelRatio(), this.dotRadiusPx);
      if (dotRadius > EPSILON) {
        const dots = buildDotData(mesh, mesh.uniform.dotVertexCount);
        if (dots.length > 0) {
          this.drawDots(dots, mesh, camera, viewportScale, dotRadius, depthBias);
          depthBias += DEPTH_STEP;
        }
      }
    }
  }

  resizeToDisplaySize(): boolean {
    const ratio = this.resolvedPixelRatio();
    const width = Math.max(1, Math.round(this.canvas.clientWidth * ratio));
    const height = Math.max(1, Math.round(this.canvas.clientHeight * ratio));
    if (this.canvas.width === width && this.canvas.height === height) {
      return false;
    }
    this.canvas.width = width;
    this.canvas.height = height;
    return true;
  }

  dispose(): void {
    if (this.disposed) {
      return;
    }
    const gl = this.gl;
    gl.deleteVertexArray(this.triangleVao);
    gl.deleteVertexArray(this.lineVao);
    gl.deleteVertexArray(this.dotVao);
    gl.deleteBuffer(this.triangleBuffer);
    gl.deleteBuffer(this.lineBuffer);
    gl.deleteBuffer(this.dotBuffer);
    gl.deleteProgram(this.triangleProgram.program);
    gl.deleteProgram(this.lineProgram.program);
    gl.deleteProgram(this.dotProgram.program);
    this.disposed = true;
  }

  private drawTriangles(
    data: Float32Array,
    mesh: MeshSnapshot,
    camera: CameraBasis,
    viewportScale: [number, number],
    depthBias: number,
  ): void {
    const gl = this.gl;
    gl.useProgram(this.triangleProgram.program);
    setCameraUniforms(gl, this.triangleProgram, camera, this.canvas, viewportScale);
    gl.uniform1f(this.triangleProgram.uniforms.uDepthBias, depthBias);
    gl.uniform1f(this.triangleProgram.uniforms.uAlpha, mesh.uniform.alpha);
    gl.uniform1f(this.triangleProgram.uniforms.uGloss, mesh.uniform.gloss);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.triangleBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
    gl.bindVertexArray(this.triangleVao);
    gl.drawArrays(gl.TRIANGLES, 0, data.length / 12);
    gl.bindVertexArray(null);
  }

  private drawLines(
    data: Float32Array,
    mesh: MeshSnapshot,
    camera: CameraBasis,
    viewportScale: [number, number],
    lineRadius: number,
    depthBias: number,
  ): void {
    const gl = this.gl;
    gl.useProgram(this.lineProgram.program);
    setCameraUniforms(gl, this.lineProgram, camera, this.canvas, viewportScale);
    gl.uniform4f(
      this.lineProgram.uniforms.uViewportAndLineWidth,
      this.canvas.width,
      this.canvas.height,
      lineRadius,
      mesh.uniform.alpha,
    );
    gl.uniform2f(
      this.lineProgram.uniforms.uDepthBiasAndMiterScale,
      depthBias,
      meshLineMiterScale(mesh),
    );
    gl.bindBuffer(gl.ARRAY_BUFFER, this.lineBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
    gl.bindVertexArray(this.lineVao);
    gl.drawArrays(gl.TRIANGLES, 0, data.length / 14);
    gl.bindVertexArray(null);
  }

  private drawDots(
    data: Float32Array,
    mesh: MeshSnapshot,
    camera: CameraBasis,
    viewportScale: [number, number],
    dotRadius: number,
    depthBias: number,
  ): void {
    const gl = this.gl;
    gl.useProgram(this.dotProgram.program);
    setCameraUniforms(gl, this.dotProgram, camera, this.canvas, viewportScale);
    gl.uniform4f(
      this.dotProgram.uniforms.uViewportAndRadius,
      this.canvas.width,
      this.canvas.height,
      dotRadius,
      mesh.uniform.alpha,
    );
    gl.uniform1f(this.dotProgram.uniforms.uDepthBias, depthBias);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.dotBuffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
    gl.bindVertexArray(this.dotVao);
    gl.drawArrays(gl.TRIANGLES, 0, data.length / 9);
    gl.bindVertexArray(null);
  }

  private resolvedPixelRatio(): number {
    const ratio = typeof this.pixelRatio === "function" ? this.pixelRatio() : this.pixelRatio;
    return Number.isFinite(ratio) ? Math.max(1, ratio) : 1;
  }

  private assertLive(): void {
    if (this.disposed) {
      throw new Error("MonocurlWebGlRenderer has been disposed");
    }
  }
}

export function createMonocurlWebGlRenderer(
  canvas: HTMLCanvasElement,
  options?: MonocurlWebGlRendererOptions,
): MonocurlWebGlRenderer {
  return new MonocurlWebGlRenderer(canvas, options);
}

function createProgramInfo(
  gl: WebGL2RenderingContext,
  vertexSource: string,
  fragmentSource: string,
  uniformNames: string[],
): ProgramInfo {
  const program = createProgram(gl, vertexSource, fragmentSource);
  const uniforms: Record<string, WebGLUniformLocation> = {};
  for (const name of uniformNames) {
    const location = gl.getUniformLocation(program, name);
    if (location === null) {
      throw new Error(`WebGL program is missing uniform ${name}`);
    }
    uniforms[name] = location;
  }
  return { program, uniforms };
}

function createProgram(
  gl: WebGL2RenderingContext,
  vertexSource: string,
  fragmentSource: string,
): WebGLProgram {
  const vertex = createShader(gl, gl.VERTEX_SHADER, vertexSource);
  const fragment = createShader(gl, gl.FRAGMENT_SHADER, fragmentSource);
  const program = gl.createProgram();
  if (program === null) {
    throw new Error("failed to create WebGL program");
  }
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  gl.deleteShader(vertex);
  gl.deleteShader(fragment);

  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    const log = gl.getProgramInfoLog(program) ?? "unknown link error";
    gl.deleteProgram(program);
    throw new Error(`failed to link WebGL program: ${log}`);
  }

  return program;
}

function createShader(
  gl: WebGL2RenderingContext,
  kind: GLenum,
  source: string,
): WebGLShader {
  const shader = gl.createShader(kind);
  if (shader === null) {
    throw new Error("failed to create WebGL shader");
  }
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    const log = gl.getShaderInfoLog(shader) ?? "unknown compile error";
    gl.deleteShader(shader);
    throw new Error(`failed to compile WebGL shader: ${log}`);
  }
  return shader;
}

function createBuffer(gl: WebGL2RenderingContext): WebGLBuffer {
  const buffer = gl.createBuffer();
  if (buffer === null) {
    throw new Error("failed to create WebGL buffer");
  }
  return buffer;
}

function createVertexArray(gl: WebGL2RenderingContext): WebGLVertexArrayObject {
  const vao = gl.createVertexArray();
  if (vao === null) {
    throw new Error("failed to create WebGL vertex array");
  }
  return vao;
}

function createTriangleVao(
  gl: WebGL2RenderingContext,
  buffer: WebGLBuffer,
): WebGLVertexArrayObject {
  const vao = createVertexArray(gl);
  gl.bindVertexArray(vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  vertexAttrib(gl, 0, 3, 12, 0);
  vertexAttrib(gl, 1, 3, 12, 3);
  vertexAttrib(gl, 2, 4, 12, 6);
  vertexAttrib(gl, 3, 2, 12, 10);
  gl.bindVertexArray(null);
  return vao;
}

function createLineVao(
  gl: WebGL2RenderingContext,
  buffer: WebGLBuffer,
): WebGLVertexArrayObject {
  const vao = createVertexArray(gl);
  gl.bindVertexArray(vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  vertexAttrib(gl, 0, 3, 14, 0);
  vertexAttrib(gl, 1, 4, 14, 3);
  vertexAttrib(gl, 2, 3, 14, 7);
  vertexAttrib(gl, 3, 3, 14, 10);
  vertexAttrib(gl, 4, 1, 14, 13);
  gl.bindVertexArray(null);
  return vao;
}

function createDotVao(
  gl: WebGL2RenderingContext,
  buffer: WebGLBuffer,
): WebGLVertexArrayObject {
  const vao = createVertexArray(gl);
  gl.bindVertexArray(vao);
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  vertexAttrib(gl, 0, 3, 9, 0);
  vertexAttrib(gl, 1, 4, 9, 3);
  vertexAttrib(gl, 2, 2, 9, 7);
  gl.bindVertexArray(null);
  return vao;
}

function vertexAttrib(
  gl: WebGL2RenderingContext,
  index: number,
  size: number,
  strideFloats: number,
  offsetFloats: number,
): void {
  gl.enableVertexAttribArray(index);
  gl.vertexAttribPointer(index, size, gl.FLOAT, false, strideFloats * 4, offsetFloats * 4);
}

function sortedVisibleMeshes(meshes: MeshSnapshot[]): MeshSnapshot[] {
  return meshes
    .map((mesh, order) => ({ mesh, order }))
    .filter(({ mesh }) => mesh.uniform.alpha > 0)
    .sort(
      (a, b) =>
        a.mesh.uniform.zIndex - b.mesh.uniform.zIndex ||
        a.order - b.order,
    )
    .map(({ mesh }) => mesh);
}

function setCameraUniforms(
  gl: WebGL2RenderingContext,
  program: ProgramInfo,
  camera: CameraBasis,
  canvas: HTMLCanvasElement,
  viewportScale: [number, number],
): void {
  gl.uniform3fv(program.uniforms.uCameraPosition, camera.position);
  gl.uniform3fv(program.uniforms.uCameraRight, camera.right);
  gl.uniform3fv(program.uniforms.uCameraUp, camera.up);
  gl.uniform3fv(program.uniforms.uCameraForward, camera.forward);
  gl.uniform4f(
    program.uniforms.uCameraClip,
    camera.near,
    camera.far,
    camera.tanHalfFov,
    canvas.width / Math.max(1, canvas.height),
  );
  gl.uniform2f(program.uniforms.uViewportScale, viewportScale[0], viewportScale[1]);
}

function cameraBasis(camera?: CameraSnapshot): CameraBasis {
  const snapshot = camera ?? {
    position: [0, 0, 4] as Vec3,
    lookAt: [0, 0, 0] as Vec3,
    up: [0, 1, 0] as Vec3,
    near: 0.1,
    far: 100,
  };
  const forward = normalizedOr(sub(snapshot.lookAt, snapshot.position), [0, 0, -1]);
  const upHint = normalizedOr(snapshot.up, [0, 1, 0]);
  let right = cross(forward, upHint);
  if (lengthSquared(right) <= 1e-6) {
    const fallbackUp = lengthSquared(cross(forward, [0, 1, 0])) > 1e-6 ? [0, 1, 0] : [0, 0, 1];
    right = cross(forward, fallbackUp as Vec3);
  }
  right = normalize(right);
  const up = normalize(cross(right, forward));
  const near = Math.max(MIN_CAMERA_NEAR, snapshot.near);
  return {
    position: snapshot.position,
    right,
    up,
    forward,
    near,
    far: Math.max(near, snapshot.far),
    tanHalfFov: Math.max(0.05, Math.tan(DEFAULT_CAMERA_FOV * 0.5)),
  };
}

function buildTriangleData(mesh: MeshSnapshot): Float32Array {
  const smoothNormals = mesh.uniform.smooth ? averagedTriangleNormals(mesh) : undefined;
  const out: number[] = [];
  for (const triangle of mesh.triangles) {
    if (
      triangle.a.color[3] <= EPSILON &&
      triangle.b.color[3] <= EPSILON &&
      triangle.c.color[3] <= EPSILON
    ) {
      continue;
    }

    const faceNormal = triangleFaceNormal(
      triangle.a.position,
      triangle.b.position,
      triangle.c.position,
    );
    pushTriangleVertex(out, triangle.a, triangleVertexNormal(smoothNormals, triangle.a.position, faceNormal));
    pushTriangleVertex(out, triangle.b, triangleVertexNormal(smoothNormals, triangle.b.position, faceNormal));
    pushTriangleVertex(out, triangle.c, triangleVertexNormal(smoothNormals, triangle.c.position, faceNormal));
  }
  return new Float32Array(out);
}

function pushTriangleVertex(
  out: number[],
  vertex: TriangleSnapshot["a"],
  normal: Vec3,
): void {
  out.push(
    vertex.position[0],
    vertex.position[1],
    vertex.position[2],
    normal[0],
    normal[1],
    normal[2],
    vertex.color[0],
    vertex.color[1],
    vertex.color[2],
    vertex.color[3],
    vertex.uv[0],
    vertex.uv[1],
  );
}

function averagedTriangleNormals(mesh: MeshSnapshot): Map<string, Vec3> {
  const normals = new Map<string, Vec3>();
  for (const triangle of mesh.triangles) {
    if (
      triangle.a.color[3] <= EPSILON &&
      triangle.b.color[3] <= EPSILON &&
      triangle.c.color[3] <= EPSILON
    ) {
      continue;
    }
    const areaNormal = cross(
      sub(triangle.b.position, triangle.a.position),
      sub(triangle.c.position, triangle.a.position),
    );
    if (lengthSquared(areaNormal) <= 1e-12) {
      continue;
    }
    for (const position of [triangle.a.position, triangle.b.position, triangle.c.position]) {
      const key = positionKey(position);
      const current = normals.get(key);
      normals.set(key, current === undefined ? areaNormal : add(current, areaNormal));
    }
  }
  return normals;
}

function triangleVertexNormal(
  smoothNormals: Map<string, Vec3> | undefined,
  position: Vec3,
  fallback: Vec3,
): Vec3 {
  const normal = smoothNormals?.get(positionKey(position));
  if (normal !== undefined && lengthSquared(normal) > 1e-12) {
    return normalize(normal);
  }
  return fallback;
}

function triangleFaceNormal(a: Vec3, b: Vec3, c: Vec3): Vec3 {
  const normal = cross(sub(b, a), sub(c, a));
  return lengthSquared(normal) <= 1e-12 ? [0, 0, 1] : normalize(normal);
}

function buildLineData(mesh: MeshSnapshot): Float32Array {
  const out: number[] = [];
  for (const source of mesh.lines) {
    if (!lineVisible(source) || !source.isDominantSibling) {
      continue;
    }

    const previous = source.previous >= 0 ? mesh.lines[source.previous] : source;
    const next = source.next >= 0 ? mesh.lines[source.next] : source;
    const tangent = sub(source.b.position, source.a.position);
    const previousTangent = sub(source.a.position, previous?.a.position ?? source.a.position);
    const nextTangent = sub(next?.b.position ?? source.b.position, source.b.position);
    const reverseTangent = negate(tangent);
    const reversePreviousTangent = negate(nextTangent);
    const reverseNextTangent = negate(previousTangent);

    const vertices: LineVertex[] = [
      lineVertex(source.a.position, source.a.color, tangent, previousTangent, 1),
      lineVertex(source.a.position, source.a.color, tangent, tangent, 0),
      lineVertex(source.a.position, source.a.color, tangent, tangent, 1),
      lineVertex(source.b.position, source.b.color, tangent, tangent, 0),
      lineVertex(source.b.position, source.b.color, tangent, tangent, 1),
      lineVertex(source.b.position, source.b.color, tangent, nextTangent, 1),
      lineVertex(source.b.position, source.b.color, reverseTangent, reversePreviousTangent, 1),
      lineVertex(source.b.position, source.b.color, reverseTangent, reverseTangent, 1),
      lineVertex(source.a.position, source.a.color, reverseTangent, reverseTangent, 1),
      lineVertex(source.a.position, source.a.color, reverseTangent, reverseNextTangent, 1),
    ];

    for (const index of LINE_VERTEX_INDICES) {
      pushLineVertex(out, vertices[index]);
    }
  }
  return new Float32Array(out);
}

function lineVisible(line: LineSnapshot): boolean {
  return line.a.color[3] > EPSILON || line.b.color[3] > EPSILON;
}

function lineVertex(
  position: Vec3,
  color: Vec4,
  tangent: Vec3,
  previousTangent: Vec3,
  extrude: number,
): LineVertex {
  return { position, color, tangent, previousTangent, extrude };
}

function pushLineVertex(out: number[], vertex: LineVertex): void {
  out.push(
    vertex.position[0],
    vertex.position[1],
    vertex.position[2],
    vertex.color[0],
    vertex.color[1],
    vertex.color[2],
    vertex.color[3],
    vertex.tangent[0],
    vertex.tangent[1],
    vertex.tangent[2],
    vertex.previousTangent[0],
    vertex.previousTangent[1],
    vertex.previousTangent[2],
    vertex.extrude,
  );
}

function buildDotData(mesh: MeshSnapshot, vertexCount: number): Float32Array {
  const out: number[] = [];
  const count = Math.max(3, Math.floor(vertexCount));
  const local = Array.from({ length: count }, (_, index) => {
    const angle = (2 * Math.PI * index) / count;
    return [Math.cos(angle), Math.sin(angle)] as const;
  });

  for (const dot of mesh.dots) {
    if (!dot.isDominantSibling || dot.color[3] <= EPSILON) {
      continue;
    }
    for (let index = 1; index < count - 1; index += 1) {
      pushDotVertex(out, dot, local[0]);
      pushDotVertex(out, dot, local[index]);
      pushDotVertex(out, dot, local[index + 1]);
    }
  }
  return new Float32Array(out);
}

function pushDotVertex(out: number[], dot: DotSnapshot, local: readonly [number, number]): void {
  out.push(
    dot.position[0],
    dot.position[1],
    dot.position[2],
    dot.color[0],
    dot.color[1],
    dot.color[2],
    dot.color[3],
    local[0],
    local[1],
  );
}

function meshLineRadiusPx(mesh: MeshSnapshot, width: number, fallbackLineWidthPx: number): number {
  const radius = Number.isFinite(mesh.uniform.strokeRadius)
    ? Math.max(0, mesh.uniform.strokeRadius)
    : Math.max(0, fallbackLineWidthPx) * 0.5;
  return radius * Math.max(1, width) / REFERENCE_WIDTH;
}

function meshLineMiterScale(mesh: MeshSnapshot): number {
  return Number.isFinite(mesh.uniform.strokeMiterRadiusScale)
    ? Math.max(0, mesh.uniform.strokeMiterRadiusScale)
    : DEFAULT_LINE_MITER_SCALE;
}

function meshDotRadiusPx(
  mesh: MeshSnapshot,
  pixelRatio: number,
  fallbackDotRadiusPx: number,
): number {
  const rasterScale = Number.isFinite(pixelRatio) ? Math.max(1, pixelRatio) : 1;
  if (Number.isFinite(mesh.uniform.dotRadius)) {
    return Math.max(0, mesh.uniform.dotRadius) * rasterScale;
  }
  return Math.max(0, fallbackDotRadiusPx) * rasterScale;
}

function positionKey(position: Vec3): string {
  return `${position[0]},${position[1]},${position[2]}`;
}

function add(a: Vec3, b: Vec3): Vec3 {
  return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
}

function sub(a: Vec3, b: Vec3): Vec3 {
  return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
}

function negate(value: Vec3): Vec3 {
  return [-value[0], -value[1], -value[2]];
}

function cross(a: Vec3, b: Vec3): Vec3 {
  return [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
}

function lengthSquared(value: Vec3): number {
  return value[0] * value[0] + value[1] * value[1] + value[2] * value[2];
}

function normalize(value: Vec3): Vec3 {
  const length = Math.sqrt(lengthSquared(value));
  if (length <= EPSILON) {
    return [0, 0, 0];
  }
  return [value[0] / length, value[1] / length, value[2] / length];
}

function normalizedOr(value: Vec3, fallback: Vec3): Vec3 {
  return lengthSquared(value) <= 1e-6 ? fallback : normalize(value);
}
