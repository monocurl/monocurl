//! Native-only Typst markup backend.
//!
//! Compiles a snippet of Typst markup to a tightly cropped SVG string that the
//! shared SVG -> mesh importer (`crate::svg`) can consume. The heavy lifting is
//! done by the `typst` compiler; `typst-as-lib` supplies a minimal `World`
//! implementation (fonts + in-memory main source), and `typst-svg` renders the
//! compiled `PagedDocument` to SVG.

use std::sync::OnceLock;

use anyhow::{Result, anyhow, bail};
use typst::layout::Abs;
use typst_as_lib::TypstEngine;
use typst_layout::PagedDocument;
use typst_svg::SvgOptions;

/// Preamble prepended to every user snippet.
///
/// * `width`/`height: auto` + `margin: 0pt` make the page shrink-wrap the
///   content so the resulting SVG is tightly cropped (same spirit as the
///   `standalone`/`preview` LaTeX document class used by the TeX backend).
/// * `fill: none` keeps the page background transparent so no stray background
///   rectangle is imported as a mesh.
const TYPST_PREAMBLE: &str = "\
#set page(width: auto, height: auto, margin: 0pt, fill: none)
#set text(size: 10pt)
";

/// Bundled font data handed to the Typst compiler.
///
/// Includes the `typst-assets` default set (New Computer Modern, Libertinus
/// Serif, DejaVu Sans Mono, ...) so stock Typst documents render as they do on
/// typst.app, plus the fonts Monocurl already ships under `assets/font/`.
fn bundled_fonts() -> &'static [Vec<u8>] {
    static FONTS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    FONTS.get_or_init(|| {
        let mut fonts: Vec<Vec<u8>> = typst_assets::fonts().map(<[u8]>::to_vec).collect();
        fonts.push(include_bytes!("../../../assets/font/IBMPlexMono-Regular.ttf").to_vec());
        fonts.push(include_bytes!("../../../assets/font/IBMPlexMono-Italic.ttf").to_vec());
        fonts.push(include_bytes!("../../../assets/font/Lilex-Regular.ttf").to_vec());
        fonts
    })
}

/// Compile Typst `markup` to an SVG string.
///
/// The snippet is wrapped in [`TYPST_PREAMBLE`]; any Typst error (parse, type,
/// layout) is surfaced as an `anyhow` error with the collected diagnostics.
pub fn render_typst_svg(markup: &str) -> Result<String> {
    let source = format!("{TYPST_PREAMBLE}\n{markup}");

    let engine = TypstEngine::builder()
        .main_file(source)
        .fonts(bundled_fonts().iter().cloned())
        .build();

    let doc: PagedDocument = engine
        .compile()
        .output
        .map_err(|error| anyhow!("typst compilation failed: {error}"))?;

    if doc.pages().is_empty() {
        bail!("typst produced no pages");
    }

    Ok(typst_svg::svg_merged(&doc, &SvgOptions::default(), Abs::zero()))
}
