//! compact, deterministic renderings of executor values, used as golden output
//! for the scene corpus. meshes are fingerprinted rather than dumped so the
//! expectations stay readable while still failing on geometry changes.

use executor::{
    heap::with_heap,
    value::{Value, container::HashableKey},
};
use geo::mesh::Mesh;

const MAX_LIST_ENTRIES: usize = 8;
const MAX_MAP_ENTRIES: usize = 6;
const MAX_DEPTH: usize = 3;

/// rounded so that platform-level floating point noise does not churn goldens
fn round(value: f64) -> f64 {
    let scaled = (value * 1e6).round() / 1e6;
    if scaled == 0.0 { 0.0 } else { scaled }
}

/// geometry is quantized far more coarsely than values are printed, because the
/// fingerprint has to agree across machines. an f32 coordinate in the range a
/// scene occupies carries roughly 2e-7 per unit in the last place, so rounding
/// at 1e-6 left only a few of them of headroom -- far less than two targets'
/// trigonometry and vectorization differ by, and aarch64 and x86_64 duly
/// disagreed. 1e-3 keeps thousands of ulp of slack while any real geometry
/// change stays orders of magnitude larger than the step
fn quantize_coordinate(value: f32) -> i64 {
    let scaled = (value as f64 * 1e3).round();
    if scaled == 0.0 { 0 } else { scaled as i64 }
}

fn fold(accumulator: &mut u64, value: f32) {
    *accumulator = accumulator
        .rotate_left(7)
        .wrapping_add(quantize_coordinate(value) as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15);
}

pub fn mesh_summary(mesh: &Mesh) -> String {
    format!(
        "dots={}, lins={}, tris={}, tag={:?}, fp={:016x}",
        mesh.dots.len(),
        mesh.lins.len(),
        mesh.tris.len(),
        mesh.tag,
        mesh_fingerprint(mesh),
    )
}

/// order-sensitive fingerprint of a mesh's geometry and vertex colours
fn mesh_fingerprint(mesh: &Mesh) -> u64 {
    let mut accumulator = 0xcbf2_9ce4_8422_2325u64;
    for dot in &mesh.dots {
        for component in [dot.pos.x, dot.pos.y, dot.pos.z] {
            fold(&mut accumulator, component);
        }
    }
    for lin in &mesh.lins {
        for vertex in [lin.a, lin.b] {
            for component in [vertex.pos.x, vertex.pos.y, vertex.pos.z] {
                fold(&mut accumulator, component);
            }
        }
    }
    for tri in &mesh.tris {
        for vertex in [tri.a, tri.b, tri.c] {
            for component in [vertex.pos.x, vertex.pos.y, vertex.pos.z] {
                fold(&mut accumulator, component);
            }
            for component in [vertex.col.x, vertex.col.y, vertex.col.z, vertex.col.w] {
                fold(&mut accumulator, component);
            }
        }
    }
    accumulator
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
                .map(|element| {
                    summary_at(&with_heap(|h| h.get(element.key()).clone()), depth - 1)
                })
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

