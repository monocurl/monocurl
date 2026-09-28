//! compact, deterministic renderings of executor values, used as golden output
//! for the scene corpus. meshes are reduced to exact topology counts plus
//! fixed-precision aggregates (bounding box, centroid, mean vertex colours) so
//! the expectations stay readable and can be compared within a tolerance.

use executor::{
    heap::with_heap,
    value::{Value, container::HashableKey},
};
use geo::{
    mesh::Mesh,
    simd::{Float3, Float4},
};

const MAX_LIST_ENTRIES: usize = 8;
const MAX_MAP_ENTRIES: usize = 6;
const MAX_DEPTH: usize = 3;

/// rounded so that platform-level floating point noise does not churn goldens
fn round(value: f64) -> f64 {
    let scaled = (value * 1e6).round() / 1e6;
    if scaled == 0.0 { 0.0 } else { scaled }
}

pub fn mesh_summary(mesh: &Mesh) -> String {
    let mut out = format!(
        "dots={}, lins={}, tris={}, tag={:?}",
        mesh.dots.len(),
        mesh.lins.len(),
        mesh.tris.len(),
        mesh.tag,
    );
    if let Some(geometry) = geometry_summary(mesh) {
        out.push_str(", ");
        out.push_str(&geometry);
    }
    out
}

/// tolerant geometry statistics: aggregates average out per-coordinate float
/// noise, so they can be compared within an epsilon across platforms. triangle
/// statistics are area weighted, because the same region can be triangulated
/// differently on another platform (libtess2's sweep makes different but
/// equally valid choices) and a plain vertex mean would move with it
fn geometry_summary(mesh: &Mesh) -> Option<String> {
    let positions: Vec<[f64; 3]> = mesh
        .dots
        .iter()
        .map(|dot| dot.pos)
        .chain(mesh.lins.iter().flat_map(|lin| [lin.a.pos, lin.b.pos]))
        .chain(
            mesh.tris
                .iter()
                .flat_map(|tri| [tri.a.pos, tri.b.pos, tri.c.pos]),
        )
        .map(components3)
        .collect();
    if positions.is_empty() {
        return None;
    }

    let (min, max) = positions.iter().fold(
        ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(min, max), pos| {
            (
                std::array::from_fn(|i| min[i].min(pos[i])),
                std::array::from_fn(|i| max[i].max(pos[i])),
            )
        },
    );
    let mut out = format!("box={}..{}", tuple(&min), tuple(&max));

    let point_positions = mesh
        .dots
        .iter()
        .map(|dot| dot.pos)
        .chain(mesh.lins.iter().flat_map(|lin| [lin.a.pos, lin.b.pos]))
        .map(components3);
    let mid = mean(point_positions);
    if !mid.is_empty() {
        out.push_str(&format!(", mid={}", tuple(&mid)));
    }

    if !mesh.tris.is_empty() {
        let (area, centroid, colour) = weighted_triangle_stats(mesh);
        out.push_str(&format!(
            ", area={:.3}, tri_mid={}, tri_col={}",
            area,
            tuple(&centroid),
            tuple(&colour)
        ));
    }

    let colours = [
        (
            "dot_col",
            mean(mesh.dots.iter().map(|dot| components4(dot.col))),
        ),
        (
            "lin_col",
            mean(
                mesh.lins
                    .iter()
                    .flat_map(|lin| [lin.a.col, lin.b.col])
                    .map(components4),
            ),
        ),
    ];
    for (label, colour) in colours {
        if !colour.is_empty() {
            out.push_str(&format!(", {label}={}", tuple(&colour)));
        }
    }
    Some(out)
}

/// total area, area-weighted centroid and area-weighted mean colour of the
/// triangles. a mesh whose triangles are all degenerate falls back to plain
/// vertex means, which are then well defined anyway
fn weighted_triangle_stats(mesh: &Mesh) -> (f64, Vec<f64>, Vec<f64>) {
    let mut total = 0.0;
    let mut centroid = [0.0; 3];
    let mut colour = [0.0; 4];
    for tri in &mesh.tris {
        let [a, b, c] = [tri.a.pos, tri.b.pos, tri.c.pos].map(components3);
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let area = 0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        total += area;
        for i in 0..3 {
            centroid[i] += area * (a[i] + b[i] + c[i]) / 3.0;
        }
        let cols = [tri.a.col, tri.b.col, tri.c.col].map(components4);
        for i in 0..4 {
            colour[i] += area * (cols[0][i] + cols[1][i] + cols[2][i]) / 3.0;
        }
    }
    if total > 0.0 {
        return (
            total,
            centroid.iter().map(|v| v / total).collect(),
            colour.iter().map(|v| v / total).collect(),
        );
    }
    let vertex_mid = mean(
        mesh.tris
            .iter()
            .flat_map(|tri| [tri.a.pos, tri.b.pos, tri.c.pos])
            .map(components3),
    );
    let vertex_col = mean(
        mesh.tris
            .iter()
            .flat_map(|tri| [tri.a.col, tri.b.col, tri.c.col])
            .map(components4),
    );
    (0.0, vertex_mid, vertex_col)
}

fn components3(value: Float3) -> [f64; 3] {
    [value.x, value.y, value.z].map(f64::from)
}

fn components4(value: Float4) -> [f64; 4] {
    [value.x, value.y, value.z, value.w].map(f64::from)
}

/// componentwise mean, empty when there is nothing to average
fn mean<const N: usize>(values: impl Iterator<Item = [f64; N]>) -> Vec<f64> {
    let (count, sum) = values.fold((0usize, [0.0; N]), |(count, sum), value| {
        (count + 1, std::array::from_fn(|i| sum[i] + value[i]))
    });
    if count == 0 {
        return Vec::new();
    }
    sum.iter().map(|total| total / count as f64).collect()
}

fn tuple(values: &[f64]) -> String {
    let entries: Vec<_> = values.iter().map(|&value| fixed(value)).collect();
    format!("({})", entries.join(", "))
}

/// fixed precision, with negative zero folded into zero
fn fixed(value: f64) -> String {
    let text = format!("{value:.3}");
    match text.strip_prefix('-') {
        Some(rest) if rest.bytes().all(|b| b == b'0' || b == b'.') => rest.to_string(),
        _ => text,
    }
}

fn key_summary(key: &HashableKey) -> String {
    match key {
        HashableKey::Integer(value) => value.to_string(),
        HashableKey::Float(bits) => format!("{:?}", round(HashableKey::float_value(*bits))),
        HashableKey::String(value) => format!("{value:?}"),
        HashableKey::List(keys) => {
            let entries: Vec<_> = keys.iter().map(key_summary).collect();
            format!("[{}]", entries.join(", "))
        }
    }
}

pub fn value_summary(value: &Value) -> String {
    summary_at(value, MAX_DEPTH)
}

fn summary_at(value: &Value, depth: usize) -> String {
    if depth == 0 {
        return format!("<{}>", value.type_name());
    }

    match value {
        Value::Nil => "nil".into(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => format!("{:?}", round(*value)),
        Value::Complex { re, im } => format!("{:?}+{:?}i", round(*re), round(*im)),
        Value::String(value) => format!("{value:?}"),
        Value::Mesh(mesh) => format!("mesh({})", mesh_summary(mesh)),
        Value::List(list) => {
            let entries: Vec<_> = list
                .elements()
                .iter()
                .take(MAX_LIST_ENTRIES)
                .map(|element| summary_at(&with_heap(|h| h.get(element.key()).clone()), depth - 1))
                .collect();
            let ellipsis = if list.len() > MAX_LIST_ENTRIES {
                ", .."
            } else {
                ""
            };
            format!("[{}{ellipsis}] (len {})", entries.join(", "), list.len())
        }
        Value::Map(map) => {
            let entries: Vec<_> = map
                .iter()
                .take(MAX_MAP_ENTRIES)
                .map(|(key, element)| {
                    format!(
                        "{} -> {}",
                        key_summary(key),
                        summary_at(&with_heap(|h| h.get(element.key()).clone()), depth - 1)
                    )
                })
                .collect();
            let ellipsis = if map.len() > MAX_MAP_ENTRIES {
                ", .."
            } else {
                ""
            };
            format!("[{}{ellipsis}] (len {})", entries.join(", "), map.len())
        }
        Value::Leader(leader) => summary_at(
            &with_heap(|h| h.get(leader.follower_rc.key()).clone()),
            depth,
        ),
        Value::Lvalue(reference) => {
            summary_at(&with_heap(|h| h.get(reference.key()).clone()), depth)
        }
        Value::WeakLvalue(reference) => {
            summary_at(&with_heap(|h| h.get(reference.key()).clone()), depth)
        }
        Value::Lambda(lambda) => format!("lambda/{}", lambda.arg_names.len()),
        Value::Operator(_) => "operator".into(),
        Value::AnimBlock(_) => "anim".into(),
        Value::PrimitiveAnim(_) => "primitive anim".into(),
        Value::Stateful(_) => "stateful".into(),
        Value::InvokedFunction(_) => "live function".into(),
        Value::InvokedOperator(_) => "live operator".into(),
    }
}
