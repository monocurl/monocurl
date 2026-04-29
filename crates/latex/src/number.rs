use std::sync::Arc;

use anyhow::{Result, bail};
use geo::{
    mesh::{Mesh, make_mesh_mut},
    simd::Float3,
};

use crate::{RenderQuality, render::render_tex_with_quality, render::validate_scale};

const DEFAULT_NUMBER_SIGNIFICANT_DIGITS: usize = 6;
const MAX_NUMBER_DECIMAL_PLACES: usize = 64;
const NUMBER_GLYPH_TRACKING_AT_SCALE_1: f32 = 0.015;

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
