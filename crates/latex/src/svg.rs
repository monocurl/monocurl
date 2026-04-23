use std::collections::HashMap;

use anyhow::{Result, anyhow};
use geo::{
    mesh::{Lin, Mesh, Tri, Uniforms},
    mesh_build,
    simd::{Float2, Float3, Float4},
};
use libtess2::{TessellationOptions, WindingRule};
use tiny_skia_path::{Path, PathSegment, Point};
use usvg::{FillRule, Node, Paint, Path as SvgPath, Tree};

use crate::document;

pub(crate) const DEFAULT_TEXT_STROKE_RADIUS: f32 = 0.55;
const NORMAL_CURVE_SAMPLE_SPACING: f32 = 24.0;
const HIGH_QUALITY_CURVE_SAMPLE_SPACING: f32 = 14.0;
const MIN_CURVE_SAMPLES: usize = 4;
const NORMAL_MAX_CURVE_SAMPLES: usize = 96;
const HIGH_QUALITY_MAX_CURVE_SAMPLES: usize = 160;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CurveSampling {
    Normal,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImportOptions {
    pub curve_sampling: CurveSampling,
}

pub(crate) struct RenderedSvg {
    pub meshes: Vec<Mesh>,
    pub span_mesh_indices: HashMap<String, Vec<usize>>,
}

pub(crate) fn import(svg: &str, unit_scale: f32, options: ImportOptions) -> Result<RenderedSvg> {
    let tree = Tree::from_str(svg, &usvg::Options::default())?;
    let mut rendered = RenderedSvg {
        meshes: Vec::new(),
        span_mesh_indices: HashMap::new(),
    };
    collect_group(tree.root(), None, 1.0, unit_scale, options, &mut rendered)?;
    Ok(rendered)
}

fn collect_group(
    group: &usvg::Group,
    inherited_span: Option<&str>,
    inherited_opacity: f32,
    unit_scale: f32,
    options: ImportOptions,
    rendered: &mut RenderedSvg,
) -> Result<()> {
    let span = document::strip_span_prefix(group.id()).or(inherited_span);
    let opacity = inherited_opacity * group.opacity().get();

    for child in group.children() {
        match child {
            Node::Group(group) => {
                collect_group(group, span, opacity, unit_scale, options, rendered)?
            }
            Node::Path(path) => collect_path(path, span, opacity, unit_scale, options, rendered)?,
            _ => {}
        }
    }

    Ok(())
}

fn collect_path(
    path: &SvgPath,
    inherited_span: Option<&str>,
    inherited_opacity: f32,
    unit_scale: f32,
    options: ImportOptions,
    rendered: &mut RenderedSvg,
) -> Result<()> {
    if !path.is_visible() {
        return Ok(());
    }

    let span = path
        .id()
        .strip_prefix(document::SPAN_ID_PREFIX)
        .or(inherited_span);

    if let Some(fill) = path.fill() {
        if let Paint::Color(color) = fill.paint() {
            let (tag, color) =
                decode_tag_and_color(*color, fill.opacity().get() * inherited_opacity);
            let even_odd = matches!(fill.rule(), FillRule::EvenOdd);
            let contours = extract_contours(path.data(), path.abs_transform(), unit_scale, options);
            if !contours.is_empty() {
                push_mesh(
                    filled_contours(&contours, color, tag, even_odd)?,
                    span,
                    rendered,
                );
            }
        }
    }

    if let Some(stroke) = path.stroke() {
        if let Paint::Color(color) = stroke.paint() {
            if let Some(stroked_path) = path.data().stroke(&stroke.to_tiny_skia(), 1.0) {
                let (tag, color) =
                    decode_tag_and_color(*color, stroke.opacity().get() * inherited_opacity);
                let contours =
                    extract_contours(&stroked_path, path.abs_transform(), unit_scale, options);
                if !contours.is_empty() {
                    push_mesh(
                        filled_contours(&contours, color, tag, false)?,
                        span,
                        rendered,
                    );
                }
            }
        }
    }

    Ok(())
}

fn push_mesh(mesh: Mesh, span: Option<&str>, rendered: &mut RenderedSvg) {
    let mesh_idx = rendered.meshes.len();
    rendered.meshes.push(mesh);
    if let Some(span) = span {
        rendered
            .span_mesh_indices
            .entry(span.to_owned())
            .or_default()
            .push(mesh_idx);
    }
}

fn extract_contours(
    path: &Path,
    transform: tiny_skia_path::Transform,
    unit_scale: f32,
    options: ImportOptions,
) -> Vec<Vec<Float3>> {
    let mut contours = Vec::new();
    let mut current = Vec::new();
    let mut cursor = Float3::ZERO;
    let mut start = Float3::ZERO;
    let mut has_cursor = false;

    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(point) => {
                flush_current_contour(&mut contours, &mut current, start);
                let mapped = map_point(point, transform, unit_scale);
                current.push(mapped);
                cursor = mapped;
                start = mapped;
                has_cursor = true;
            }
            PathSegment::LineTo(point) => {
                if !has_cursor {
                    continue;
                }
                let mapped = map_point(point, transform, unit_scale);
                push_unique_point(&mut current, mapped);
                cursor = mapped;
            }
            PathSegment::QuadTo(ctrl, point) => {
                if !has_cursor {
                    continue;
                }
                let control = map_point(ctrl, transform, unit_scale);
                let end = map_point(point, transform, unit_scale);
                let approx_length = (control - cursor).len() + (end - control).len();
                let samples = curve_samples(approx_length, options);
                for i in 1..=samples {
                    let t = i as f32 / samples as f32;
                    let mt = 1.0 - t;
                    let sample = cursor * (mt * mt) + control * (2.0 * mt * t) + end * (t * t);
                    push_unique_point(&mut current, sample);
                }
                cursor = end;
            }
            PathSegment::CubicTo(ctrl_a, ctrl_b, point) => {
                if !has_cursor {
                    continue;
                }
                let control_a = map_point(ctrl_a, transform, unit_scale);
                let control_b = map_point(ctrl_b, transform, unit_scale);
                let end = map_point(point, transform, unit_scale);
                let approx_length = (control_a - cursor).len()
                    + (control_b - control_a).len()
                    + (end - control_b).len();
                let samples = curve_samples(approx_length, options);
                for i in 1..=samples {
                    let t = i as f32 / samples as f32;
                    let mt = 1.0 - t;
                    let sample = cursor * (mt * mt * mt)
                        + control_a * (3.0 * mt * mt * t)
                        + control_b * (3.0 * mt * t * t)
                        + end * (t * t * t);
                    push_unique_point(&mut current, sample);
                }
                cursor = end;
            }
            PathSegment::Close => {
                flush_current_contour(&mut contours, &mut current, start);
                has_cursor = false;
            }
        }
    }

    flush_current_contour(&mut contours, &mut current, start);
    contours
}

fn map_point(point: Point, transform: tiny_skia_path::Transform, unit_scale: f32) -> Float3 {
    let mut point = point;
    transform.map_point(&mut point);
    Float3::new(point.x * unit_scale, -point.y * unit_scale, 0.0)
}

fn curve_samples(approx_length: f32, options: ImportOptions) -> usize {
    let (spacing, max_samples) = match options.curve_sampling {
        CurveSampling::Normal => (NORMAL_CURVE_SAMPLE_SPACING, NORMAL_MAX_CURVE_SAMPLES),
        CurveSampling::High => (
            HIGH_QUALITY_CURVE_SAMPLE_SPACING,
            HIGH_QUALITY_MAX_CURVE_SAMPLES,
        ),
    };

    (approx_length / spacing)
        .ceil()
        .clamp(MIN_CURVE_SAMPLES as f32, max_samples as f32) as usize
}

fn push_unique_point(points: &mut Vec<Float3>, point: Float3) {
    if points.last().is_none_or(|last| *last != point) {
        points.push(point);
    }
}

fn flush_current_contour(
    contours: &mut Vec<Vec<Float3>>,
    current: &mut Vec<Float3>,
    start: Float3,
) {
    if current.len() >= 2 && current.last() == Some(&start) {
        current.pop();
    }
    if current.len() >= 3 {
        contours.push(std::mem::take(current));
    } else {
        current.clear();
    }
}

fn decode_tag_and_color(color: usvg::Color, alpha: f32) -> (Vec<isize>, Float4) {
    if color.green == u8::MAX && color.blue == u8::MAX {
        return (vec![color.red as isize], Float4::new(0.0, 0.0, 0.0, alpha));
    }
    if color.red == u8::MAX {
        return (
            vec![color.green as isize, color.blue as isize],
            Float4::new(0.0, 0.0, 0.0, alpha),
        );
    }

    (
        Vec::new(),
        Float4::new(
            color.red as f32 / 255.0,
            color.green as f32 / 255.0,
            color.blue as f32 / 255.0,
            alpha,
        ),
    )
}

fn filled_contours(
    contours: &[Vec<Float3>],
    color: Float4,
    tag: Vec<isize>,
    even_odd: bool,
) -> Result<Mesh> {
    let (lins, tris) = tessellate_planar_loops(contours, Float3::Z, color, even_odd)?;
    let mesh = Mesh {
        dots: Vec::new(),
        lins,
        tris,
        uniform: Uniforms {
            stroke_radius: DEFAULT_TEXT_STROKE_RADIUS,
            ..Uniforms::default()
        },
        tag,
    };
    mesh.debug_assert_consistent_topology();
    Ok(mesh)
}

fn tessellate_planar_loops(
    contours: &[Vec<Float3>],
    normal: Float3,
    color: Float4,
    even_odd: bool,
) -> Result<(Vec<Lin>, Vec<Tri>)> {
    let contours: Vec<_> = contours
        .iter()
        .filter(|contour| contour.len() >= 3)
        .cloned()
        .collect();
    if contours.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let tess = libtess2::triangulate(
        contours.iter().map(Vec::as_slice),
        TessellationOptions {
            winding_rule: if even_odd {
                WindingRule::Odd
            } else {
                WindingRule::NonZero
            },
            normal: Some(normal),
            constrained_delaunay: true,
            reverse_contours: false,
            normalize_input: false,
        },
    )
    .map_err(|error| anyhow!("failed to tessellate glyph outline: {error}"))?;

    let vertices: Vec<_> = tess
        .vertices
        .iter()
        .copied()
        .map(|pos| mesh_build::SurfaceVertex {
            pos,
            col: color,
            uv: Float2::ZERO,
        })
        .collect();
    let (mut lins, tris) =
        mesh_build::build_indexed_surface(&vertices, &tess.triangles, &HashMap::new());
    for line in &mut lins {
        line.norm = normal;
    }

    Ok((lins, tris))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_basic_mathjax_tex_with_consistent_topology() {
        let svg = mathjax_svg::render_svg("2 + 4", mathjax_svg::RenderOptions::new(36.0)).unwrap();
        let rendered = import(
            &svg,
            1.0 / super::super::MATHJAX_UNITS_PER_EM,
            ImportOptions {
                curve_sampling: CurveSampling::Normal,
            },
        )
        .unwrap();

        assert!(!rendered.meshes.is_empty());
        assert!(rendered.meshes.iter().all(Mesh::has_consistent_topology));
    }
}
