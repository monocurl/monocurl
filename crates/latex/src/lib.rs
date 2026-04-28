mod document;
mod svg;
mod system;
mod tectonic;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, RwLock},
};

use anyhow::{Result, bail};
use geo::{
    mesh::{Mesh, make_mesh_mut},
    simd::Float3,
};

pub use document::SpanMarker;

const SYSTEM_SVG_UNITS_AT_SCALE_1: f32 = 36.0;
const DEFAULT_NUMBER_SIGNIFICANT_DIGITS: usize = 6;
const MAX_NUMBER_DECIMAL_PLACES: usize = 64;
const NUMBER_GLYPH_TRACKING_AT_SCALE_1: f32 = 0.015;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LatexBackendConfig {
    Bundled,
    System(SystemBackendConfig),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SystemBackendConfig {
    pub latex: PathBuf,
    pub dvisvgm: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemToolPaths {
    pub latex: Option<PathBuf>,
    pub dvisvgm: Option<PathBuf>,
}

impl SystemToolPaths {
    pub fn into_config(self) -> Option<SystemBackendConfig> {
        Some(SystemBackendConfig {
            latex: self.latex?,
            dvisvgm: self.dvisvgm?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BundledBackendStatus {
    pub bundle: bool,
}

impl BundledBackendStatus {
    pub fn is_available(self) -> bool {
        true
    }

    pub fn is_self_contained(self) -> bool {
        self.bundle
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SystemBackendStatus {
    pub latex: bool,
    pub dvisvgm: bool,
}

impl SystemBackendStatus {
    pub fn is_available(self) -> bool {
        self.latex && self.dvisvgm
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RenderQuality {
    Normal,
    High,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum BackendKind {
    Text,
    Tex,
    Latex,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    backend: BackendKind,
    backend_config: LatexBackendConfig,
    source: String,
    scale_bits: u32,
    quality: RenderQuality,
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
static BACKEND_CONFIG: OnceLock<RwLock<LatexBackendConfig>> = OnceLock::new();

pub fn backend_config() -> LatexBackendConfig {
    backend_config_lock().read().unwrap().clone()
}

pub fn set_backend_config(config: LatexBackendConfig) {
    let mut current = backend_config_lock().write().unwrap();
    if *current != config {
        *current = config;
        cache().lock().unwrap().clear();
    }
}

pub fn discover_system_backend() -> SystemToolPaths {
    system::discover_backend()
}

pub fn bundled_backend_status() -> BundledBackendStatus {
    BundledBackendStatus {
        bundle: tectonic::bundle_is_available(),
    }
}

pub fn system_backend_status(config: &SystemBackendConfig) -> SystemBackendStatus {
    system::backend_status(config)
}

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

pub fn format_number(
    value: f64,
    decimal_places: Option<usize>,
    include_sign: bool,
) -> Result<String> {
    if decimal_places.is_some_and(|places| places > MAX_NUMBER_DECIMAL_PLACES) {
        bail!("number decimal places must be at most {MAX_NUMBER_DECIMAL_PLACES}");
    }

    let negative = value.is_sign_negative() && value != 0.0;
    let mut out = if !value.is_finite() {
        if value.is_nan() {
            "nan".to_string()
        } else {
            "inf".to_string()
        }
    } else if let Some(decimal_places) = decimal_places {
        format!("{:.*}", decimal_places, value.abs())
    } else {
        format_general_number(value.abs())
    };

    strip_negative_zero(&mut out);
    if negative {
        out.insert(0, '-');
    } else if include_sign {
        out.insert(0, '+');
    }
    Ok(out)
}

pub fn render_number(
    value: f64,
    decimal_places: Option<usize>,
    include_sign: bool,
    scale: f32,
) -> Result<Vec<Arc<Mesh>>> {
    render_number_with_quality(
        value,
        decimal_places,
        include_sign,
        scale,
        RenderQuality::Normal,
    )
}

pub fn render_number_with_quality(
    value: f64,
    decimal_places: Option<usize>,
    include_sign: bool,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    let text = format_number(value, decimal_places, include_sign)?;
    render_number_string_with_quality(&text, scale, quality)
}

pub fn render_number_string_with_quality(
    text: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<Vec<Arc<Mesh>>> {
    validate_scale(scale)?;
    if text.is_empty() {
        return Ok(Vec::new());
    }

    let digit_advance = number_digit_advance(scale, quality)?;
    let tracking = NUMBER_GLYPH_TRACKING_AT_SCALE_1 * scale;
    let mut cursor = 0.0f32;
    let mut out = Vec::new();

    for ch in text.chars() {
        if ch.is_whitespace() {
            cursor += digit_advance;
            continue;
        }

        let mut glyph = render_number_glyph(ch, scale, quality)?;
        let Some((min, max)) = mesh_collection_bounds(&glyph) else {
            cursor += digit_advance;
            continue;
        };

        let width = max.x - min.x;
        translate_meshes(&mut glyph, Float3::new(cursor - min.x, 0.0, 0.0));
        out.extend(glyph);

        cursor += if ch.is_ascii_digit() {
            digit_advance
        } else {
            (width + tracking).max(tracking)
        };
    }

    Ok(out)
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
    render_cached(
        backend,
        backend_config.clone(),
        source,
        scale,
        quality,
        |source| {
            let (svg, flip_y) = match &backend_config {
                // after expand_glyph_uses, all paths have transforms applied once → y-down
                LatexBackendConfig::Bundled => (tectonic::render_svg_document(&source)?, true),
                // dvisvgm outputs standard SVG y-down coordinates; negate to get Monocurl y-up
                LatexBackendConfig::System(config) => {
                    (system::render_svg_document(&source, config)?, true)
                }
            };
            svg::import(
                &svg,
                scale / SYSTEM_SVG_UNITS_AT_SCALE_1,
                svg_import_options(quality, flip_y),
            )
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

fn render_cached<F>(
    backend: BackendKind,
    backend_config: LatexBackendConfig,
    source: String,
    scale: f32,
    quality: RenderQuality,
    render: F,
) -> Result<RenderedOutput>
where
    F: FnOnce(String) -> Result<svg::RenderedSvg>,
{
    let key = CacheKey {
        backend,
        backend_config,
        source: source.clone(),
        scale_bits: scale.to_bits(),
        quality,
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

fn validate_scale(scale: f32) -> Result<()> {
    if !scale.is_finite() || scale <= 0.0 {
        bail!("text scale must be a positive finite number");
    }
    Ok(())
}

fn format_general_number(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }

    let exponent = value.abs().log10().floor() as i32;
    if exponent < -4 || exponent >= DEFAULT_NUMBER_SIGNIFICANT_DIGITS as i32 {
        let mut out = format!("{:.*e}", DEFAULT_NUMBER_SIGNIFICANT_DIGITS - 1, value);
        trim_scientific_trailing_zeroes(&mut out);
        out
    } else {
        let decimal_places =
            (DEFAULT_NUMBER_SIGNIFICANT_DIGITS as i32 - 1 - exponent).max(0) as usize;
        let mut out = format!("{value:.decimal_places$}");
        trim_decimal_trailing_zeroes(&mut out);
        out
    }
}

fn trim_decimal_trailing_zeroes(out: &mut String) {
    while out.contains('.') && out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
}

fn trim_scientific_trailing_zeroes(out: &mut String) {
    let Some(e_index) = out.find('e') else {
        trim_decimal_trailing_zeroes(out);
        return;
    };

    let exponent = out.split_off(e_index);
    trim_decimal_trailing_zeroes(out);
    out.push_str(&exponent);
}

fn strip_negative_zero(out: &mut String) {
    let Some(rest) = out.strip_prefix("-0") else {
        return;
    };
    if rest.is_empty() || rest.chars().all(|ch| ch == '.' || ch == '0') {
        out.remove(0);
    }
}

fn render_number_glyph(ch: char, scale: f32, quality: RenderQuality) -> Result<Vec<Arc<Mesh>>> {
    let mut source = String::new();
    source.push(ch);
    render_tex_with_quality(&source, scale, quality)
}

fn number_digit_advance(scale: f32, quality: RenderQuality) -> Result<f32> {
    let meshes = render_number_glyph('0', scale, quality)?;
    let Some((min, max)) = mesh_collection_bounds(&meshes) else {
        return Ok(scale * 0.5);
    };
    Ok((max.x - min.x + NUMBER_GLYPH_TRACKING_AT_SCALE_1 * scale).max(scale * 0.05))
}

fn mesh_vertices(mesh: &Mesh) -> impl Iterator<Item = Float3> + '_ {
    mesh.dots
        .iter()
        .map(|dot| dot.pos)
        .chain(mesh.lins.iter().flat_map(|lin| [lin.a.pos, lin.b.pos]))
        .chain(
            mesh.tris
                .iter()
                .flat_map(|tri| [tri.a.pos, tri.b.pos, tri.c.pos]),
        )
}

fn mesh_collection_bounds(meshes: &[Arc<Mesh>]) -> Option<(Float3, Float3)> {
    let mut vertices = meshes.iter().flat_map(|mesh| mesh_vertices(mesh));
    let first = vertices.next()?;
    Some(vertices.fold((first, first), |(mut min, mut max), point| {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        min.z = min.z.min(point.z);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
        max.z = max.z.max(point.z);
        (min, max)
    }))
}

fn translate_meshes(meshes: &mut [Arc<Mesh>], delta: Float3) {
    for mesh in meshes {
        translate_mesh(make_mesh_mut(mesh), delta);
    }
}

fn translate_mesh(mesh: &mut Mesh, delta: Float3) {
    for dot in &mut mesh.dots {
        dot.pos = dot.pos + delta;
    }
    for lin in &mut mesh.lins {
        lin.a.pos = lin.a.pos + delta;
        lin.b.pos = lin.b.pos + delta;
    }
    for tri in &mut mesh.tris {
        tri.a.pos = tri.a.pos + delta;
        tri.b.pos = tri.b.pos + delta;
        tri.c.pos = tri.c.pos + delta;
    }
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

fn cache() -> &'static Mutex<HashMap<CacheKey, CacheEntry>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn backend_config_lock() -> &'static RwLock<LatexBackendConfig> {
    BACKEND_CONFIG.get_or_init(|| RwLock::new(LatexBackendConfig::Bundled))
}

fn svg_import_options(quality: RenderQuality, flip_y: bool) -> svg::ImportOptions {
    svg::ImportOptions {
        curve_sampling: match quality {
            RenderQuality::Normal => svg::CurveSampling::Normal,
            RenderQuality::High => svg::CurveSampling::High,
        },
        flip_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::simd::Float3;

    fn configure_test_backend() -> bool {
        if bundled_backend_status().is_available() {
            set_backend_config(LatexBackendConfig::Bundled);
            return true;
        }
        let Some(config) = discover_system_backend().into_config() else {
            return false;
        };
        set_backend_config(LatexBackendConfig::System(config));
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
    fn number_formatting_supports_general_fixed_and_sign_modes() {
        assert_eq!(format_number(12.345_678, None, false).unwrap(), "12.3457");
        assert_eq!(
            format_number(1_234_567.0, None, false).unwrap(),
            "1.23457e6"
        );
        assert_eq!(
            format_number(0.000_012_345, None, false).unwrap(),
            "1.2345e-5"
        );
        assert_eq!(format_number(12.3, Some(2), true).unwrap(), "+12.30");
        assert_eq!(format_number(-12.3, Some(1), true).unwrap(), "-12.3");
    }

    #[test]
    fn number_renderer_lays_out_cached_glyphs() {
        if !configure_test_backend() {
            return;
        }
        let one = render_number_string_with_quality("1", 1.0, RenderQuality::Normal).unwrap();
        let two = render_number_string_with_quality("11", 1.0, RenderQuality::Normal).unwrap();
        let (one_min, one_max) = mesh_bounds(&one).unwrap();
        let (two_min, two_max) = mesh_bounds(&two).unwrap();
        assert!(one_max.x > one_min.x);
        assert!(two_max.x - two_min.x > one_max.x - one_min.x);
    }
}
