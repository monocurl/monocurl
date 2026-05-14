use anyhow::Result;

use crate::types::{BackendKind, LatexBackendConfig};

#[cfg(not(target_arch = "wasm32"))]
use crate::document::{self, LatexDocumentStyle};

pub(crate) struct PreparedRender {
    pub config: LatexBackendConfig,
    pub source: String,
    pub file_cache: bool,
    pub svg_units_at_scale_1: f32,
    pub flip_y: bool,
}

#[cfg(not(target_arch = "wasm32"))]
const NATIVE_SVG_UNITS_AT_SCALE_1: f32 = 36.0;
#[cfg(target_arch = "wasm32")]
const BROWSER_SVG_UNITS_AT_SCALE_1: f32 = 36.0;

pub(crate) fn prepare_render(
    kind: BackendKind,
    config: LatexBackendConfig,
    source: &str,
    additional_preamble: &str,
) -> Result<PreparedRender> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = config;
        if kind == BackendKind::Latex {
            anyhow::bail!(
                "Latex(...) is not supported by the browser text backend; use Tex(...) or Text(...)for MathJax-compatible formulas"
            );
        }
        if !additional_preamble.trim().is_empty() {
            anyhow::bail!("additional LaTeX preamble is not supported by the browser text backend");
        }

        Ok(PreparedRender {
            config: LatexBackendConfig::Bundled,
            source: browser_source(kind, source),
            file_cache: false,
            svg_units_at_scale_1: BROWSER_SVG_UNITS_AT_SCALE_1,
            flip_y: true,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let style = document_style(&config, source, additional_preamble);
        Ok(PreparedRender {
            config,
            source: native_document(kind, source, additional_preamble, style),
            file_cache: true,
            svg_units_at_scale_1: NATIVE_SVG_UNITS_AT_SCALE_1,
            flip_y: true,
        })
    }
}

pub(crate) fn render_svg(
    kind: BackendKind,
    config: &LatexBackendConfig,
    source: &str,
) -> Result<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = config;
        browser::render_svg(kind, source)
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = kind;
        match config {
            LatexBackendConfig::Bundled => crate::tectonic::render_svg_document(source),
            LatexBackendConfig::System(config) => {
                crate::system::render_svg_document(source, config)
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_document(
    kind: BackendKind,
    source: &str,
    additional_preamble: &str,
    style: LatexDocumentStyle,
) -> String {
    match kind {
        BackendKind::Text => document::build_text_document(source, style),
        BackendKind::Tex => document::build_tex_document(source, style),
        BackendKind::Latex => document::build_latex_document(source, additional_preamble, style),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn document_style(
    config: &LatexBackendConfig,
    source: &str,
    additional_preamble: &str,
) -> LatexDocumentStyle {
    match config {
        LatexBackendConfig::Bundled => {
            if needs_unicode_preamble(source, additional_preamble) {
                LatexDocumentStyle::BundledUnicode
            } else {
                LatexDocumentStyle::BundledBasic
            }
        }
        LatexBackendConfig::System(_) => LatexDocumentStyle::SystemLatex,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn needs_unicode_preamble(source: &str, additional_preamble: &str) -> bool {
    !source.is_ascii() || latex_font_preamble_hint(additional_preamble)
}

#[cfg(not(target_arch = "wasm32"))]
fn latex_font_preamble_hint(additional_preamble: &str) -> bool {
    [
        "fontspec",
        "xeCJK",
        "setmainfont",
        "setsansfont",
        "setmonofont",
        "setCJK",
        "newfontfamily",
    ]
    .iter()
    .any(|hint| additional_preamble.contains(hint))
}

#[cfg(target_arch = "wasm32")]
fn browser_source(kind: BackendKind, source: &str) -> String {
    match kind {
        BackendKind::Text => format!("\\text{{{}}}", escape_tex_text(source)),
        BackendKind::Tex | BackendKind::Latex => source.to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
fn escape_tex_text(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    for ch in source.chars() {
        match ch {
            '\\' | '{' | '}' | '$' | '&' | '%' | '#' | '_' | '^' | '~' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(target_arch = "wasm32")]
mod browser {
    use anyhow::{Result, anyhow};
    use wasm_bindgen::prelude::*;

    use crate::types::BackendKind;

    #[wasm_bindgen(inline_js = r#"
export function monocurlRenderLatexSvg(kind, source) {
  const hook = globalThis.__monocurlRenderLatexSvg;
  if (typeof hook === "function") {
    const rendered = hook(kind, source);
    if (typeof rendered !== "string") {
      throw new Error("__monocurlRenderLatexSvg must return an SVG string");
    }
    return rendered;
  }

  const mathJax = globalThis.MathJax;
  if (mathJax && typeof mathJax.tex2svg === "function") {
    const node = mathJax.tex2svg(source, { display: false });
    const adaptor = mathJax.startup && mathJax.startup.adaptor;
    if (typeof SVGSVGElement !== "undefined" && node instanceof SVGSVGElement) {
      return node.outerHTML;
    }
    if (typeof Element !== "undefined" && node instanceof Element) {
      const svg = node.matches("svg") ? node : node.querySelector("svg");
      if (svg) {
        return svg.outerHTML;
      }
    }
    if (adaptor && typeof adaptor.outerHTML === "function") {
      const tagged = typeof adaptor.tags === "function" ? adaptor.tags(node, "svg") : undefined;
      const svg = tagged && tagged[0] ? tagged[0] : node;
      const markup = adaptor.outerHTML(svg);
      const match = markup.match(/<svg\b[\s\S]*<\/svg>/i);
      return match ? match[0] : markup;
    }
    if (typeof node.outerHTML === "string") {
      const match = node.outerHTML.match(/<svg\b[\s\S]*<\/svg>/i);
      return match ? match[0] : node.outerHTML;
    }
  }

  throw new Error("Monocurl wasm text rendering requires globalThis.__monocurlRenderLatexSvg(kind, source) or a loaded MathJax tex2svg runtime");
}

export function monocurlJsErrorMessage(value) {
  if (typeof value === "string") {
    return value;
  }
  if (value && typeof value.message === "string") {
    return value.message;
  }
  try {
    return String(value);
  } catch {
    return "browser text backend failed";
  }
}
"#)]
    extern "C" {
        #[wasm_bindgen(catch)]
        fn monocurlRenderLatexSvg(kind: &str, source: &str) -> Result<JsValue, JsValue>;

        #[wasm_bindgen(js_name = monocurlJsErrorMessage)]
        fn monocurl_js_error_message(value: JsValue) -> String;
    }

    pub(super) fn render_svg(kind: BackendKind, source: &str) -> Result<String> {
        let rendered = monocurlRenderLatexSvg(kind.as_str(), source)
            .map_err(|error| anyhow!(js_error_message(error)))?;
        rendered
            .as_string()
            .ok_or_else(|| anyhow!("browser text backend returned a non-string value"))
    }

    fn js_error_message(value: JsValue) -> String {
        monocurl_js_error_message(value)
    }
}
