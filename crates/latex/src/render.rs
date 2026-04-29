use std::{collections::HashMap, sync::Arc};

use anyhow::{Result, bail};
use geo::mesh::{Mesh, make_mesh_mut};

use crate::{
    cache,
    config::backend_config,
    document::{self, SpanMarker},
    system, tectonic,
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

    render_tagged_backend(
        BackendKind::Text,
        &tagged,
        scale,
        quality,
        document::build_text_document,
    )
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

    render_tagged_backend(
        BackendKind::Tex,
        &tagged,
        scale,
        quality,
        document::build_tex_document,
    )
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

pub fn render_latex_with_quality(
    body: &str,
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
        scale,
        quality,
        document::build_latex_document,
    )
}

fn render_document(
    backend: BackendKind,
    source: String,
    scale: f32,
    quality: RenderQuality,
) -> Result<RenderedOutput> {
    validate_scale(scale)?;
    let backend_config = backend_config();
    cache::render_cached(
        backend,
        backend_config.clone(),
        source,
        scale,
        quality,
        |source| {
            cache::render_svg_with_file_cache(&backend_config, &source, scale, quality, |source| {
                match &backend_config {
                    LatexBackendConfig::Bundled => tectonic::render_svg_document(source),
                    LatexBackendConfig::System(config) => {
                        system::render_svg_document(source, config)
                    }
                }
            })
        },
    )
}

fn render_tagged_backend<F>(
    backend: BackendKind,
    tagged: &document::TaggedSource,
    scale: f32,
    quality: RenderQuality,
    build_document: F,
) -> Result<Vec<Arc<Mesh>>>
where
    F: FnOnce(&str) -> String,
{
    let marker_spans = tagged
        .spans
        .iter()
        .enumerate()
        .map(|(index, span)| document::TaggedSpan {
            tag: vec![index as isize + 1],
            range: span.range.clone(),
        })
        .collect::<Vec<_>>();
    let source = document::apply_legacy_text_tags(&tagged.source, &marker_spans)?;
    let output = render_document(backend, build_document(&source), scale, quality)?;
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
    let source = document::apply_legacy_text_tags(tex, &tagged_markers)?;
    let mut output = render_document(
        BackendKind::Tex,
        document::build_tex_document(&source),
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
    use crate::{LatexBackendConfig, set_backend_config};

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
}
