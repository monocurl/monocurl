use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

#[cfg(not(target_arch = "wasm32"))]
use anyhow::Context;
use anyhow::{Result, bail};
use geo::mesh::{Mesh, make_mesh_mut};
use usvg::fontdb::Database as FontDatabase;
#[cfg(not(target_arch = "wasm32"))]
use usvg::fontdb::Source as FontSource;

use crate::{
    backend, cache,
    config::backend_config,
    document::{self, SpanMarker},
    types::{BackendKind, LatexBackendConfig, RenderQuality, RenderedOutput},
};

const SVG_TEXT_FONT_SIZE: f32 = 1000.0;
const SVG_TEXT_LINE_HEIGHT_EM: f32 = 1.2;
const SVG_TEXT_UNITS_AT_SCALE_1: f32 = SVG_TEXT_FONT_SIZE * 4.0;
const SVG_MESH_UNITS_AT_SCALE_1: f32 = 100.0;
const SVG_TEXT_CANVAS_SIZE: f32 = 100_000.0;

/// SVG units (Typst points, at the 10pt base size set in the Typst preamble)
/// that map to one scene unit when `scale == 1`. Calibrated against the TeX
/// backend: with this value `Typst("$x^2 + y^2 = z^2$")` renders within ~3% of
/// the width/height of the equivalent `Tex("x^2 + y^2 = z^2")` (measured in the
/// real renderer via `mesh_width`/`mesh_height`).
#[cfg(not(target_arch = "wasm32"))]
const TYPST_SVG_UNITS_AT_SCALE_1: f32 = 47.0;

pub fn render_text(text: &str, scale: f32) -> Result<Vec<Arc<Mesh>>> {
    render_text_with_quality(text, scale, RenderQuality::Normal)
}

pub fn render_text_with_quality(
    text: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    let tagged = document::parse_text_tags(text)?;
    if tagged.source.trim().is_empty() && tagged.spans.is_empty() {
        return Ok(Vec::new());
    }

    render_tagged_backend(BackendKind::Text, &tagged, "", scale, quality)
}

pub fn render_text_with_font(text: &str, scale: f32, font: &str) -> Result<Vec<Arc<Mesh>>> {
    render_text_with_font_and_quality(text, scale, font, RenderQuality::Normal)
}

pub fn render_text_with_font_and_quality(
    text: &str,
    scale: f32,
    font: &str,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    let tagged = document::parse_text_tags(text)?;
    if tagged.source.trim().is_empty() && tagged.spans.is_empty() {
        return Ok(Vec::new());
    }

    render_system_font_text(&tagged, scale, font, quality)
}

pub fn render_tex(tex: &str, scale: f32) -> Result<Vec<Arc<Mesh>>> {
    render_tex_with_quality(tex, scale, RenderQuality::Normal)
}

pub fn render_tex_with_quality(
    tex: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    let tagged = document::parse_text_tags(tex)?;
    if tagged.source.trim().is_empty() && tagged.spans.is_empty() {
        return Ok(Vec::new());
    }

    render_tagged_backend(BackendKind::Tex, &tagged, "", scale, quality)
}

pub fn render_tex_marked(tex: &str, scale: f32, markers: &[SpanMarker]) -> Result<RenderedOutput> {
    render_tex_marked_with_quality(tex, scale, markers, RenderQuality::Normal)
}

pub fn render_tex_marked_with_quality(
    tex: &str,
    scale: f32,
    markers: &[SpanMarker],
    quality: RenderQuality,
) -> Result<RenderedOutput> {
    validate_scale(scale)?;
    if tex.trim().is_empty() && markers.is_empty() {
        return Ok(RenderedOutput {
            meshes: Vec::new(),
            span_mesh_indices: HashMap::new(),
        });
    }

    render_tex_marked_backend(tex, scale, markers, quality)
}

pub fn render_latex(body: &str, scale: f32) -> Result<Vec<Arc<Mesh>>> {
    render_latex_with_quality(body, scale, RenderQuality::Normal)
}

pub fn render_latex_with_preamble(
    body: &str,
    additional_preamble: &str,
    scale: f32,
) -> Result<Vec<Arc<Mesh>>> {
    render_latex_with_preamble_and_quality(body, additional_preamble, scale, RenderQuality::Normal)
}

pub fn render_svg(svg_source: &str, scale: f32) -> Result<Vec<Arc<Mesh>>> {
    render_svg_with_quality(svg_source, scale, RenderQuality::Normal)
}

pub fn render_svg_with_quality(
    svg_source: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    render_svg_with_quality_and_resources_dir(svg_source, scale, quality, None)
}

pub fn render_svg_with_quality_and_resources_dir(
    svg_source: &str,
    scale: f32,
    quality: RenderQuality,
    resources_dir: Option<PathBuf>,
) -> Result<Vec<Arc<Mesh>>> {
    validate_scale(scale)?;
    if svg_source.trim().is_empty() {
        return Ok(Vec::new());
    }

    let options = svg_mesh_options(resources_dir);
    let rendered = cache::import_svg_with_options_and_tag_decoding(
        svg_source,
        scale,
        quality,
        SVG_MESH_UNITS_AT_SCALE_1,
        true,
        &options,
        false,
    )?;
    Ok(rendered.meshes.into_iter().map(Arc::new).collect())
}

pub fn render_latex_with_quality(
    body: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    render_latex_with_preamble_and_quality(body, "", scale, quality)
}

pub fn render_typst(markup: &str, scale: f32) -> Result<Vec<Arc<Mesh>>> {
    render_typst_with_quality(markup, scale, RenderQuality::Normal)
}

/// Render Typst markup to mesh geometry (native/desktop only).
///
/// TODO: `\text_tag{...}` fragment tag recovery is not wired up for Typst yet;
/// the whole snippet renders as untagged contours. Tag it from the outside with
/// `tag{...}` for now.
#[cfg(not(target_arch = "wasm32"))]
pub fn render_typst_with_quality(
    markup: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    validate_scale(scale)?;
    if markup.trim().is_empty() {
        return Ok(Vec::new());
    }

    let output = cache::render_cached(
        BackendKind::Typst,
        LatexBackendConfig::Bundled,
        markup.to_owned(),
        scale,
        quality,
        |markup| {
            let svg = crate::typst_backend::render_typst_svg(&markup)?;
            cache::import_svg(&svg, scale, quality, TYPST_SVG_UNITS_AT_SCALE_1, true)
        },
    )?;
    Ok(output.meshes)
}

#[cfg(target_arch = "wasm32")]
pub fn render_typst_with_quality(
    _markup: &str,
    _scale: f32,
    _quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    bail!("Typst is not supported by the browser text backend; use Tex(...) or Text(...) instead")
}

pub fn render_latex_with_preamble_and_quality(
    body: &str,
    additional_preamble: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    let tagged = document::parse_text_tags(body)?;
    if tagged.source.trim().is_empty() && tagged.spans.is_empty() {
        return Ok(Vec::new());
    }

    render_tagged_backend(
        BackendKind::Latex,
        &tagged,
        additional_preamble,
        scale,
        quality,
    )
}

fn render_tagged_backend(
    backend: BackendKind,
    tagged: &document::TaggedSource,
    additional_preamble: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    let marker_spans = indexed_marker_spans(&tagged.spans);
    let backend_config = backend_config();
    let source = document::apply_text_tag_markers(&tagged.source, &marker_spans)?;
    let output = render_tagged_document(
        backend,
        backend_config,
        &source,
        additional_preamble,
        scale,
        quality,
    )?;
    Ok(apply_backend_text_tags(output, &tagged.spans))
}

fn render_tex_marked_backend(
    tex: &str,
    scale: f32,
    markers: &[SpanMarker],
    quality: RenderQuality,
) -> Result<RenderedOutput> {
    let tagged_markers = markers
        .iter()
        .enumerate()
        .map(|(index, marker)| document::TaggedSpan {
            tag: vec![index as isize + 1],
            range: marker.range.clone(),
        })
        .collect::<Vec<_>>();
    let source = document::apply_text_tag_markers(tex, &tagged_markers)?;
    let backend_config = backend_config();
    let mut output = render_tagged_document(
        BackendKind::Tex,
        backend_config,
        &source,
        "",
        scale,
        quality,
    )?;
    let mut span_mesh_indices = HashMap::new();

    for (mesh_index, mesh) in output.meshes.iter_mut().enumerate() {
        let Some(&tag) = mesh.tag.first() else {
            continue;
        };
        if mesh.tag.len() != 1 || tag <= 0 {
            continue;
        }
        let marker_index = (tag - 1) as usize;
        let Some(marker) = markers.get(marker_index) else {
            continue;
        };
        span_mesh_indices
            .entry(marker.id.clone())
            .or_insert_with(Vec::new)
            .push(mesh_index);
        make_mesh_mut(mesh).tag.clear();
    }

    output.span_mesh_indices = span_mesh_indices;
    Ok(output)
}

fn render_system_font_text(
    tagged: &document::TaggedSource,
    scale: f32,
    font: &str,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    validate_scale(scale)?;
    let marker_spans = indexed_marker_spans(&tagged.spans);
    let (usvg_options, font_family) = font_svg_options(font)?;
    let svg = build_svg_text_document(&tagged.source, &marker_spans, &font_family, font);
    let output = cache::render_cached(
        BackendKind::Text,
        LatexBackendConfig::Bundled,
        svg,
        scale,
        quality,
        |svg| {
            cache::import_svg_with_options(
                &svg,
                scale,
                quality,
                SVG_TEXT_UNITS_AT_SCALE_1,
                true,
                &usvg_options,
            )
        },
    )?;
    Ok(apply_backend_text_tags(output, &tagged.spans))
}

fn indexed_marker_spans(spans: &[document::TaggedSpan]) -> Vec<document::TaggedSpan> {
    spans
        .iter()
        .enumerate()
        .map(|(index, span)| document::TaggedSpan {
            tag: vec![index as isize + 1],
            range: span.range.clone(),
        })
        .collect()
}

fn build_svg_text_document(
    source: &str,
    marker_spans: &[document::TaggedSpan],
    font_family: &str,
    font_source: &str,
) -> String {
    let body = build_svg_text_body(source, marker_spans);

    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{SVG_TEXT_CANVAS_SIZE}\" height=\"{SVG_TEXT_CANVAS_SIZE}\" overflow=\"visible\" data-monocurl-font-source=\"{}\"><text x=\"0\" y=\"0\" xml:space=\"preserve\" font-family=\"{}\" font-size=\"{SVG_TEXT_FONT_SIZE}\" fill=\"black\">{body}</text></svg>",
        escape_xml_attr(font_source),
        escape_xml_attr(font_family)
    )
}

fn build_svg_text_body(source: &str, marker_spans: &[document::TaggedSpan]) -> String {
    let runs = svg_text_runs(source, marker_spans);
    let mut body = String::new();
    let mut pending_line_breaks = 0usize;
    for run in runs {
        for chunk in source[run.range].split_inclusive('\n') {
            let has_line_break = chunk.ends_with('\n');
            let text = if has_line_break {
                &chunk[..chunk.len() - '\n'.len_utf8()]
            } else {
                chunk
            };

            if !text.is_empty() {
                push_svg_text_segment(&mut body, text, run.tag, &mut pending_line_breaks);
            }
            if has_line_break {
                pending_line_breaks += 1;
            }
        }
    }
    body
}

fn push_svg_text_segment(
    body: &mut String,
    source: &str,
    tag: Option<isize>,
    pending_line_breaks: &mut usize,
) {
    let escaped = escape_xml_text(source);
    let needs_position = body.is_empty() || *pending_line_breaks > 0;
    if needs_position {
        body.push_str("<tspan x=\"0\"");
        if *pending_line_breaks > 0 {
            body.push_str(&format!(
                " dy=\"{}em\"",
                *pending_line_breaks as f32 * SVG_TEXT_LINE_HEIGHT_EM
            ));
        }
        if let Some(tag) = tag {
            body.push_str(" fill=\"");
            body.push_str(&text_tag_color(tag));
            body.push('"');
        }
        body.push('>');
        body.push_str(&escaped);
        body.push_str("</tspan>");
    } else if let Some(tag) = tag {
        body.push_str("<tspan fill=\"");
        body.push_str(&text_tag_color(tag));
        body.push_str("\">");
        body.push_str(&escaped);
        body.push_str("</tspan>");
    } else {
        body.push_str(&escaped);
    }
    *pending_line_breaks = 0;
}

struct SvgTextRun {
    range: std::ops::Range<usize>,
    tag: Option<isize>,
}

fn svg_text_runs(source: &str, marker_spans: &[document::TaggedSpan]) -> Vec<SvgTextRun> {
    let mut boundaries = vec![0, source.len()];
    for span in marker_spans {
        boundaries.push(span.range.start);
        boundaries.push(span.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();

    let mut runs: Vec<SvgTextRun> = Vec::new();
    for window in boundaries.windows(2) {
        let [start, end] = [window[0], window[1]];
        if start == end {
            continue;
        }
        let tag = innermost_marker_tag(start, end, marker_spans);
        if let Some(last) = runs.last_mut()
            && last.tag == tag
            && last.range.end == start
        {
            last.range.end = end;
            continue;
        }
        runs.push(SvgTextRun {
            range: start..end,
            tag,
        });
    }
    runs
}

fn innermost_marker_tag(
    start: usize,
    end: usize,
    marker_spans: &[document::TaggedSpan],
) -> Option<isize> {
    marker_spans
        .iter()
        .filter(|span| span.range.start <= start && end <= span.range.end)
        .min_by_key(|span| span.range.end - span.range.start)
        .and_then(|span| span.tag.first().copied())
}

fn text_tag_color(tag: isize) -> String {
    let tag = tag.clamp(0, u8::MAX as isize);
    format!("rgb({tag},255,255)")
}

fn escape_xml_text(source: &str) -> String {
    let mut out = String::new();
    for ch in source.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_xml_attr(source: &str) -> String {
    let mut out = String::new();
    for ch in source.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn font_svg_options(font: &str) -> Result<(usvg::Options<'static>, String)> {
    let mut options = usvg::Options::default();
    let mut db = system_font_db().as_ref().clone();
    let font_family = resolve_font_family(font, &mut db)?;
    options.font_family = font_family.clone();
    options.fontdb = Arc::new(db);
    Ok((options, font_family))
}

fn svg_mesh_options(resources_dir: Option<PathBuf>) -> usvg::Options<'static> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut options = usvg::Options {
            resources_dir,
            ..usvg::Options::default()
        };
        options.fontdb = system_font_db().clone();
        options
    }

    #[cfg(target_arch = "wasm32")]
    {
        usvg::Options {
            resources_dir,
            ..usvg::Options::default()
        }
    }
}

fn system_font_db() -> &'static Arc<FontDatabase> {
    static DB: OnceLock<Arc<FontDatabase>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = FontDatabase::new();
        db.load_system_fonts();
        Arc::new(db)
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_font_family(font: &str, db: &mut FontDatabase) -> Result<String> {
    let path = Path::new(font);
    if path.exists() {
        db.load_font_file(path)
            .with_context(|| format!("failed to load font file `{}`", path.display()))?;
        if let Some(family) = loaded_font_family(path, db) {
            return Ok(family);
        }
        bail!(
            "font file `{}` does not contain a usable font family",
            path.display()
        );
    }

    Ok(font.to_owned())
}

#[cfg(target_arch = "wasm32")]
fn resolve_font_family(font: &str, _db: &mut FontDatabase) -> Result<String> {
    Ok(font.to_owned())
}

#[cfg(not(target_arch = "wasm32"))]
fn loaded_font_family(path: &Path, db: &FontDatabase) -> Option<String> {
    db.faces()
        .find(|face| match &face.source {
            FontSource::File(source_path) => source_path == path,
            FontSource::SharedFile(source_path, _) => source_path == path,
            FontSource::Binary(_) => false,
        })
        .and_then(|face| face.families.first().map(|(family, _)| family.clone()))
}

fn render_tagged_document(
    backend: BackendKind,
    backend_config: LatexBackendConfig,
    source: &str,
    additional_preamble: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<RenderedOutput> {
    validate_scale(scale)?;
    let prepared = backend::prepare_render(backend, backend_config, source, additional_preamble)?;
    let source = prepared.source.clone();
    cache::render_cached(
        backend,
        prepared.config.clone(),
        source,
        scale,
        quality,
        |source| {
            render_prepared_svg(
                &prepared.config,
                backend,
                &source,
                scale,
                quality,
                &prepared,
            )
        },
    )
}

fn render_prepared_svg(
    config: &LatexBackendConfig,
    backend: BackendKind,
    source: &str,
    scale: f32,
    quality: RenderQuality,
    prepared: &backend::PreparedRender,
) -> Result<crate::svg::RenderedSvg> {
    if prepared.file_cache {
        cache::render_svg_with_file_cache(
            config,
            source,
            scale,
            quality,
            prepared.svg_units_at_scale_1,
            prepared.flip_y,
            |source| backend::render_svg(backend, config, source),
        )
    } else {
        let svg = backend::render_svg(backend, config, source)?;
        cache::import_svg(
            &svg,
            scale,
            quality,
            prepared.svg_units_at_scale_1,
            prepared.flip_y,
        )
    }
}

pub(crate) fn validate_scale(scale: f32) -> Result<()> {
    if !scale.is_finite() || scale <= 0.0 {
        bail!("text scale must be a positive finite number");
    }
    Ok(())
}

fn apply_backend_text_tags(
    output: RenderedOutput,
    spans: &[document::TaggedSpan],
) -> Vec<Arc<Mesh>> {
    let mut meshes = output.meshes;
    for mesh in &mut meshes {
        let Some(&tag) = mesh.tag.first() else {
            continue;
        };
        if mesh.tag.len() != 1 || tag <= 0 {
            continue;
        }
        let marker_index = (tag - 1) as usize;
        let Some(span) = spans.get(marker_index) else {
            continue;
        };
        make_mesh_mut(mesh).tag = span.tag.clone();
    }
    meshes
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use std::path::PathBuf;

    use geo::simd::Float3;

    use super::*;
    use crate::{LatexBackendConfig, document::LatexDocumentStyle, set_backend_config};

    fn configure_test_backend() -> bool {
        set_backend_config(LatexBackendConfig::Bundled);
        true
    }

    fn mesh_bounds(meshes: &[Arc<Mesh>]) -> Option<(Float3, Float3)> {
        let mut bounds: Option<(Float3, Float3)> = None;
        for mesh in meshes {
            let points = mesh
                .dots
                .iter()
                .map(|dot| dot.pos)
                .chain(mesh.lins.iter().flat_map(|lin| [lin.a.pos, lin.b.pos]))
                .chain(
                    mesh.tris
                        .iter()
                        .flat_map(|tri| [tri.a.pos, tri.b.pos, tri.c.pos]),
                );
            for point in points {
                bounds = Some(match bounds {
                    Some((min, max)) => (
                        Float3::new(min.x.min(point.x), min.y.min(point.y), min.z.min(point.z)),
                        Float3::new(max.x.max(point.x), max.y.max(point.y), max.z.max(point.z)),
                    ),
                    None => (point, point),
                });
            }
        }
        bounds
    }

    fn mesh_triangle_count(meshes: &[Arc<Mesh>]) -> usize {
        meshes.iter().map(|mesh| mesh.tris.len()).sum()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn bundled_font_path() -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/font/IBMPlexMono-Regular.ttf")
            .display()
            .to_string()
    }

    #[test]
    fn tex_and_text_have_similar_scale() {
        if !configure_test_backend() {
            return;
        }
        let text = render_text("2 + 4", 1.0).unwrap();
        let tex = render_tex("2 + 4", 1.0).unwrap();
        let (text_min, text_max) = mesh_bounds(&text).unwrap();
        let (tex_min, tex_max) = mesh_bounds(&tex).unwrap();
        let text_size = text_max - text_min;
        let tex_size = tex_max - tex_min;

        let width_ratio = tex_size.x / text_size.x;
        let height_ratio = tex_size.y.abs() / text_size.y.abs();

        assert!((0.5..=2.0).contains(&width_ratio));
        assert!((0.5..=2.0).contains(&height_ratio));
    }

    #[test]
    fn tex_digits_and_letters_keep_expected_bounds() {
        if !configure_test_backend() {
            return;
        }
        for source in ["4", "Hello"] {
            let text = render_text(source, 1.0).unwrap();
            let tex = render_tex(source, 1.0).unwrap();
            let (text_min, text_max) = mesh_bounds(&text).unwrap();
            let (tex_min, tex_max) = mesh_bounds(&tex).unwrap();
            let text_size = text_max - text_min;
            let tex_size = tex_max - tex_min;

            let width_ratio = tex_size.x / text_size.x;
            let height_ratio = tex_size.y.abs() / text_size.y.abs();

            assert!(
                (0.5..=2.0).contains(&width_ratio),
                "{source} width ratio {width_ratio}"
            );
            assert!(
                (0.5..=2.0).contains(&height_ratio),
                "{source} height ratio {height_ratio}"
            );
        }
    }

    #[test]
    fn tex_initial_origin_stays_near_formula_bounds() {
        if !configure_test_backend() {
            return;
        }
        let tex = render_tex(r"a_i \to A", 1.0).unwrap();
        let (min, max) = mesh_bounds(&tex).unwrap();
        let size = max - min;

        assert!(
            min.x.abs() <= size.x,
            "tex x origin drifted too far from formula bounds: min={}, max={}, width={}",
            min.x,
            max.x,
            size.x
        );
    }

    #[test]
    fn text_monocurl_has_consistent_topology() {
        if !configure_test_backend() {
            return;
        }
        let meshes = render_text("Monocurl", 1.5).unwrap();
        for mesh in meshes {
            assert!(
                mesh.has_consistent_topology(),
                "{}",
                mesh.topology_mismatch_report()
                    .unwrap_or_else(|| "no mismatch report".into())
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn font_text_from_file_keeps_text_tags() {
        let meshes = render_text_with_font(r"\tag1{Hi}", 1.0, &bundled_font_path()).unwrap();
        let latex = render_text("Hi", 1.0).unwrap();
        let (font_min, font_max) = mesh_bounds(&meshes).unwrap();
        let (latex_min, latex_max) = mesh_bounds(&latex).unwrap();
        let font_size = font_max - font_min;
        let latex_size = latex_max - latex_min;
        let width_ratio = font_size.x / latex_size.x;
        let height_ratio = font_size.y.abs() / latex_size.y.abs();

        assert!(
            (0.8..=1.25).contains(&width_ratio),
            "font text width ratio {width_ratio}"
        );
        assert!(
            (0.8..=1.25).contains(&height_ratio),
            "font text height ratio {height_ratio}"
        );
        assert!(!meshes.is_empty());
        assert!(meshes.iter().any(|mesh| mesh.tag == vec![1]));
        for mesh in meshes {
            assert!(
                mesh.has_consistent_topology(),
                "{}",
                mesh.topology_mismatch_report()
                    .unwrap_or_else(|| "no mismatch report".into())
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn font_text_newlines_create_line_breaks() {
        let single_line = render_text_with_font("Hi", 1.0, &bundled_font_path()).unwrap();
        let multi_line =
            render_text_with_font("\\tag1{Hi\n\nHi}", 1.0, &bundled_font_path()).unwrap();
        let (single_min, single_max) = mesh_bounds(&single_line).unwrap();
        let (multi_min, multi_max) = mesh_bounds(&multi_line).unwrap();
        let single_size = single_max - single_min;
        let multi_size = multi_max - multi_min;

        assert!(
            multi_size.y.abs() > single_size.y.abs() * 2.0,
            "multi-line height {} should exceed single-line height {}",
            multi_size.y.abs(),
            single_size.y.abs()
        );
        assert!(multi_line.iter().any(|mesh| mesh.tag == vec![1]));
        for mesh in multi_line {
            assert!(
                mesh.has_consistent_topology(),
                "{}",
                mesh.topology_mismatch_report()
                    .unwrap_or_else(|| "no mismatch report".into())
            );
        }
    }

    #[test]
    fn svg_import_scales_basic_shapes() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect x="0" y="0" width="100" height="100" fill="red"/></svg>"#;
        let meshes = render_svg(svg, 1.0).unwrap();
        let (min, max) = mesh_bounds(&meshes).unwrap();
        let size = max - min;

        assert!((size.x - 1.0).abs() < 1e-4, "svg width {}", size.x);
        assert!(
            (size.y.abs() - 1.0).abs() < 1e-4,
            "svg height {}",
            size.y.abs()
        );
        assert!(!meshes.is_empty());
        for mesh in meshes {
            assert!(
                mesh.has_consistent_topology(),
                "{}",
                mesh.topology_mismatch_report()
                    .unwrap_or_else(|| "no mismatch report".into())
            );
        }
    }

    #[test]
    fn svg_import_preserves_colors_that_look_like_text_tag_markers() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><rect width="100" height="100" fill="#FFF5EB"/></svg>"##;
        let meshes = render_svg(svg, 1.0).unwrap();
        let color = meshes
            .iter()
            .flat_map(|mesh| &mesh.tris)
            .map(|tri| tri.a.col)
            .next()
            .unwrap();

        assert!((color.x - 1.0).abs() < 1e-6, "red {}", color.x);
        assert!((color.y - 245.0 / 255.0).abs() < 1e-6, "green {}", color.y);
        assert!((color.z - 235.0 / 255.0).abs() < 1e-6, "blue {}", color.z);
        assert!(
            meshes.iter().all(|mesh| mesh.tag.is_empty()),
            "plain SVG colors should not be decoded as text tags"
        );
    }

    #[test]
    fn high_quality_svg_import_samples_curves_more_densely() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><path d="M 0 50 C 25 0 75 0 100 50 C 75 100 25 100 0 50 Z" fill="red"/></svg>"#;
        let normal = render_svg_with_quality(svg, 1.0, RenderQuality::Normal).unwrap();
        let high = render_svg_with_quality(svg, 1.0, RenderQuality::High).unwrap();
        let normal_tris = mesh_triangle_count(&normal);
        let high_tris = mesh_triangle_count(&high);

        assert!(
            high_tris > normal_tris,
            "high-quality SVG import should emit more triangles, got normal={normal_tris}, high={high_tris}"
        );
    }

    #[test]
    fn high_quality_tex_uses_toned_down_curve_sampling() {
        if !configure_test_backend() {
            return;
        }
        let normal = render_tex_with_quality("S", 1.0, RenderQuality::Normal).unwrap();
        let high = render_tex_with_quality("S", 1.0, RenderQuality::High).unwrap();
        let normal_tris = mesh_triangle_count(&normal);
        let high_tris = mesh_triangle_count(&high);

        assert!(
            high_tris >= normal_tris,
            "high-quality TeX import should not be coarser than normal, got normal={normal_tris}, high={high_tris}"
        );
        assert!(
            high_tris <= normal_tris * 2,
            "high-quality TeX import should stay close to normal density, got normal={normal_tris}, high={high_tris}"
        );
    }

    #[test]
    fn empty_inputs_render_to_no_meshes() {
        assert!(render_text("", 1.0).unwrap().is_empty());
        assert!(render_text("   ", 1.0).unwrap().is_empty());
        assert!(render_tex("", 1.0).unwrap().is_empty());
        assert!(render_tex("   ", 1.0).unwrap().is_empty());
        assert!(render_latex("", 1.0).unwrap().is_empty());
        assert!(render_latex("   ", 1.0).unwrap().is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn typst_empty_inputs_render_to_no_meshes() {
        assert!(render_typst("", 1.0).unwrap().is_empty());
        assert!(render_typst("   ", 1.0).unwrap().is_empty());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn typst_hello_has_consistent_topology_and_reasonable_scale() {
        let typst = render_typst("hello", 1.0).unwrap();
        assert!(!typst.is_empty());

        let text = render_text("hello", 1.0).unwrap();
        let (typst_min, typst_max) = mesh_bounds(&typst).unwrap();
        let (text_min, text_max) = mesh_bounds(&text).unwrap();
        let typst_size = typst_max - typst_min;
        let text_size = text_max - text_min;

        let width_ratio = typst_size.x / text_size.x;
        let height_ratio = typst_size.y.abs() / text_size.y.abs();
        assert!(
            (0.3..=3.0).contains(&width_ratio),
            "typst/text width ratio {width_ratio}"
        );
        assert!(
            (0.3..=3.0).contains(&height_ratio),
            "typst/text height ratio {height_ratio}"
        );

        for mesh in typst {
            assert!(
                mesh.has_consistent_topology(),
                "{}",
                mesh.topology_mismatch_report()
                    .unwrap_or_else(|| "no mismatch report".into())
            );
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn typst_math_renders_some_geometry() {
        let meshes = render_typst("$x^2$", 1.0).unwrap();
        assert!(!meshes.is_empty());
        let (min, max) = mesh_bounds(&meshes).unwrap();
        let size = max - min;
        assert!(size.x > 0.0 && size.y.abs() > 0.0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn typst_invalid_markup_is_an_error() {
        assert!(render_typst("#panic(\"boom\")", 1.0).is_err());
    }

    #[test]
    fn bundled_backend_uses_basic_preamble_for_ascii_sources() {
        assert_eq!(
            backend::document_style(&LatexBackendConfig::Bundled, "x^2 + y^2", ""),
            LatexDocumentStyle::BundledBasic,
        );
    }

    #[test]
    fn bundled_backend_uses_unicode_preamble_when_needed() {
        assert_eq!(
            backend::document_style(&LatexBackendConfig::Bundled, "文", ""),
            LatexDocumentStyle::BundledUnicode,
        );
        assert_eq!(
            backend::document_style(
                &LatexBackendConfig::Bundled,
                "hello",
                r"\usepackage{fontspec}"
            ),
            LatexDocumentStyle::BundledUnicode,
        );
    }
}
