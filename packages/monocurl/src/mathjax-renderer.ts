export interface MonocurlMathJax {
  tex2svg(source: string, options?: { display?: boolean }): unknown;
  startup?: {
    promise?: Promise<unknown>;
    adaptor?: {
      outerHTML(node: unknown): string;
      tags?: (node: unknown, name: string) => unknown[];
    };
  };
}

export interface MonocurlMathJaxRendererOptions {
  mathJax?: MonocurlMathJax;
  display?: boolean;
}

export type MonocurlLatexSvgRenderer = (kind: string, source: string) => string;

interface InstalledRenderer {
  renderer: MonocurlLatexSvgRenderer;
}

declare global {
  // eslint-disable-next-line no-var
  var MathJax: MonocurlMathJax | undefined;
  // eslint-disable-next-line no-var
  var __monocurlRenderLatexSvg: MonocurlLatexSvgRenderer | undefined;
}

export class MissingMathJaxError extends Error {
  constructor() {
    super(
      "MathJax tex2svg is not available; load MathJax before installing the Monocurl renderer",
    );
    this.name = "MissingMathJaxError";
  }
}

const installedRenderers: InstalledRenderer[] = [];
let baseRenderer: MonocurlLatexSvgRenderer | undefined;
const MATHJAX_SVG_UNITS_PER_TEX_POINT = 100;

export function installMonocurlMathJaxRenderer(
  options: MonocurlMathJaxRendererOptions = {},
): () => void {
  const mathJax = options.mathJax ?? globalThis.MathJax;
  if (mathJax === undefined || typeof mathJax.tex2svg !== "function") {
    throw new MissingMathJaxError();
  }

  if (installedRenderers.length === 0) {
    baseRenderer = globalThis.__monocurlRenderLatexSvg;
  }

  const entry: InstalledRenderer = {
    renderer: (_kind, source) => renderMathJaxSvg(mathJax, source, options.display ?? false),
  };

  installedRenderers.push(entry);
  globalThis.__monocurlRenderLatexSvg = entry.renderer;

  return () => {
    const index = installedRenderers.indexOf(entry);
    if (index === -1) {
      return;
    }

    installedRenderers.splice(index, 1);
    const current = installedRenderers[installedRenderers.length - 1];
    if (current !== undefined) {
      globalThis.__monocurlRenderLatexSvg = current.renderer;
      return;
    }

    globalThis.__monocurlRenderLatexSvg = baseRenderer;
    baseRenderer = undefined;
  };
}

export function renderMathJaxSvg(
  mathJax: MonocurlMathJax,
  source: string,
  display = false,
): string {
  const node = mathJax.tex2svg(source, { display });
  const svgNode = findSvgNode(mathJax, node);
  const html = outerHtml(mathJax, svgNode ?? node);
  const svg = extractSvgMarkup(html);
  if (svg === undefined) {
    throw new Error("MathJax tex2svg did not return SVG markup");
  }
  return normalizeMathJaxSvg(svg);
}

export function normalizeMathJaxSvg(svg: string): string {
  const match = svg.match(/<svg\b([^>]*)>([\s\S]*)<\/svg>/i);
  if (match === null) {
    return svg;
  }

  const [, rawAttributes, body] = match;
  const viewBox = readViewBox(rawAttributes);
  if (viewBox === undefined) {
    return svg;
  }

  const unitScale = 1 / MATHJAX_SVG_UNITS_PER_TEX_POINT;
  const scaledViewBox = viewBox.map((value) => value * unitScale) as ViewBox;
  const attributes = writeSvgAttribute(
    writeSvgAttribute(
      writeSvgAttribute(rawAttributes, "viewBox", formatViewBox(scaledViewBox)),
      "width",
      formatNumber(scaledViewBox[2]),
    ),
    "height",
    formatNumber(scaledViewBox[3]),
  );

  return `<svg${attributes}><g transform="scale(${formatNumber(unitScale)})">${body}</g></svg>`;
}

type ViewBox = [number, number, number, number];

function readViewBox(attributes: string): ViewBox | undefined {
  const match = attributes.match(/\bviewBox\s*=\s*(['"])(.*?)\1/i);
  if (match === null) {
    return undefined;
  }

  const values = match[2].trim().split(/[\s,]+/).map(Number);
  if (values.length !== 4 || values.some((value) => !Number.isFinite(value))) {
    return undefined;
  }

  return values as ViewBox;
}

function writeSvgAttribute(attributes: string, name: string, value: string): string {
  const pattern = new RegExp(`\\s${name}\\s*=\\s*(['"]).*?\\1`, "i");
  const replacement = ` ${name}="${value}"`;
  if (pattern.test(attributes)) {
    return attributes.replace(pattern, replacement);
  }

  return `${attributes}${replacement}`;
}

function formatViewBox(viewBox: ViewBox): string {
  return viewBox.map(formatNumber).join(" ");
}

function formatNumber(value: number): string {
  return Number.parseFloat(value.toFixed(6)).toString();
}

function findSvgNode(mathJax: MonocurlMathJax, node: unknown): unknown | undefined {
  if (typeof SVGSVGElement !== "undefined" && node instanceof SVGSVGElement) {
    return node;
  }
  if (typeof Element !== "undefined" && node instanceof Element) {
    return node.matches("svg") ? node : node.querySelector("svg") ?? undefined;
  }

  const adaptor = mathJax.startup?.adaptor;
  const tagged = adaptor?.tags?.(node, "svg");
  return tagged?.[0];
}

function outerHtml(mathJax: MonocurlMathJax, node: unknown): string {
  const adaptor = mathJax.startup?.adaptor;
  if (adaptor !== undefined) {
    return adaptor.outerHTML(node);
  }
  if (typeof Element !== "undefined" && node instanceof Element) {
    return node.outerHTML;
  }
  if (typeof node === "string") {
    return node;
  }
  throw new Error("MathJax returned a node that cannot be serialized to SVG");
}

function extractSvgMarkup(html: string): string | undefined {
  const trimmed = html.trim();
  if (trimmed.startsWith("<svg")) {
    return trimmed;
  }

  if (typeof DOMParser !== "undefined") {
    const document = new DOMParser().parseFromString(trimmed, "text/html");
    const svg = document.querySelector("svg");
    if (svg !== null) {
      return svg.outerHTML;
    }
  }

  return trimmed.match(/<svg\b[\s\S]*<\/svg>/i)?.[0];
}
