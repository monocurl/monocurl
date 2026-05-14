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

export function installMonocurlMathJaxRenderer(
  options: MonocurlMathJaxRendererOptions = {},
): () => void {
  const mathJax = options.mathJax ?? globalThis.MathJax;
  if (mathJax === undefined || typeof mathJax.tex2svg !== "function") {
    throw new MissingMathJaxError();
  }

  const previous = globalThis.__monocurlRenderLatexSvg;
  globalThis.__monocurlRenderLatexSvg = (_kind, source) =>
    renderMathJaxSvg(mathJax, source, options.display ?? false);

  return () => {
    globalThis.__monocurlRenderLatexSvg = previous;
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
  return svg;
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
