mod document;
mod svg;
mod system;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::{Result, bail};
use geo::mesh::Mesh;

pub use document::SpanMarker;

const FONT_SIZE_AT_SCALE_1: f64 = 36.0;
const MATHJAX_UNITS_PER_EM: f32 = 1000.0;
const SYSTEM_SVG_UNITS_AT_SCALE_1: f32 = 36.0;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum BackendKind {
    Text,
    Tex,
    Latex,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    backend: BackendKind,
    source: String,
    scale_bits: u32,
}

#[derive(Clone)]
struct CacheEntry {
    meshes: Arc<Vec<Arc<Mesh>>>,
    span_mesh_indices: Arc<HashMap<String, Vec<usize>>>,
}

#[derive(Clone, Debug)]
pub struct RenderedOutput {
    pub meshes: Vec<Arc<Mesh>>,
    pub span_mesh_indices: HashMap<String, Vec<usize>>,
}

static CACHE: OnceLock<Mutex<HashMap<CacheKey, CacheEntry>>> = OnceLock::new();

pub fn render_text(text: &str, scale: f32) -> Result<Vec<Arc<Mesh>>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    render_system(
        BackendKind::Text,
        document::build_text_document(text),
        scale,
    )
    .map(|output| output.meshes)
}

pub fn render_tex(tex: &str, scale: f32) -> Result<Vec<Arc<Mesh>>> {
    render_tex_marked(tex, scale, &[]).map(|output| output.meshes)
}

pub fn render_tex_marked(tex: &str, scale: f32, markers: &[SpanMarker]) -> Result<RenderedOutput> {
    validate_scale(scale)?;
    if tex.trim().is_empty() && markers.is_empty() {
        return Ok(RenderedOutput {
            meshes: Vec::new(),
            span_mesh_indices: HashMap::new(),
        });
    }

    let source = document::build_mathjax_source(tex, markers)?;
    let font_size = FONT_SIZE_AT_SCALE_1 * scale as f64;
    render_cached(BackendKind::Tex, source, scale, |source| {
        let svg = mathjax_svg::render_svg(&source, mathjax_svg::RenderOptions::new(font_size))?;
        svg::import(&svg, font_size as f32 / MATHJAX_UNITS_PER_EM)
    })
}

pub fn render_latex(body: &str, scale: f32) -> Result<Vec<Arc<Mesh>>> {
    if body.trim().is_empty() {
        return Ok(Vec::new());
    }

    render_system(
        BackendKind::Latex,
        document::build_latex_document(body),
        scale,
    )
    .map(|output| output.meshes)
}

fn render_system(backend: BackendKind, source: String, scale: f32) -> Result<RenderedOutput> {
    validate_scale(scale)?;
    render_cached(backend, source, scale, |source| {
        let svg = system::render_svg_document(&source)?;
        svg::import(&svg, scale / SYSTEM_SVG_UNITS_AT_SCALE_1)
    })
}

fn render_cached<F>(
    backend: BackendKind,
    source: String,
    scale: f32,
    render: F,
) -> Result<RenderedOutput>
where
    F: FnOnce(String) -> Result<svg::RenderedSvg>,
{
    let key = CacheKey {
        backend,
        source: source.clone(),
        scale_bits: scale.to_bits(),
    };

    if let Some(entry) = cache().lock().unwrap().get(&key).cloned() {
        return Ok(RenderedOutput {
            meshes: entry.meshes.iter().cloned().collect(),
            span_mesh_indices: (*entry.span_mesh_indices).clone(),
        });
    }

    let rendered = render(source)?;
    let entry = CacheEntry {
        meshes: Arc::new(rendered.meshes.into_iter().map(Arc::new).collect()),
        span_mesh_indices: Arc::new(rendered.span_mesh_indices),
    };

    cache().lock().unwrap().insert(key, entry.clone());

    Ok(RenderedOutput {
        meshes: entry.meshes.iter().cloned().collect(),
        span_mesh_indices: (*entry.span_mesh_indices).clone(),
    })
}

fn validate_scale(scale: f32) -> Result<()> {
    if !scale.is_finite() || scale <= 0.0 {
        bail!("text scale must be a positive finite number");
    }
    Ok(())
}

fn cache() -> &'static Mutex<HashMap<CacheKey, CacheEntry>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::simd::Float3;

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
    fn empty_inputs_render_to_no_meshes() {
        assert!(render_text("", 1.0).unwrap().is_empty());
        assert!(render_text("   ", 1.0).unwrap().is_empty());
        assert!(render_tex("", 1.0).unwrap().is_empty());
        assert!(render_tex("   ", 1.0).unwrap().is_empty());
        assert!(render_latex("", 1.0).unwrap().is_empty());
        assert!(render_latex("   ", 1.0).unwrap().is_empty());
    }
}
