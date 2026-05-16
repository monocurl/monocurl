use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, bail};
use geo::mesh::{Mesh, make_mesh_mut};

use crate::{
    backend, cache,
    config::backend_config,
    document::{self, SpanMarker},
    types::{BackendKind, LatexBackendConfig, RenderQuality, RenderedOutput},
};

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

pub fn render_latex_with_quality(
    body: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    render_latex_with_preamble_and_quality(body, "", scale, quality)
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
    let marker_spans = tagged
        .spans
        .iter()
        .enumerate()
        .map(|(index, span)| document::TaggedSpan {
            tag: vec![index as isize + 1],
            range: span.range.clone(),
        })
        .collect::<Vec<_>>();
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

    #[test]
    fn empty_inputs_render_to_no_meshes() {
        assert!(render_text("", 1.0).unwrap().is_empty());
        assert!(render_text("   ", 1.0).unwrap().is_empty());
        assert!(render_tex("", 1.0).unwrap().is_empty());
        assert!(render_tex("   ", 1.0).unwrap().is_empty());
        assert!(render_latex("", 1.0).unwrap().is_empty());
        assert!(render_latex("   ", 1.0).unwrap().is_empty());
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
