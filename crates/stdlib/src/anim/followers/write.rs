use std::sync::Arc;

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{mesh::Mesh, simd::Float3};
use stdlib_macros::stdlib_func;

use super::super::helpers::{
    decompose_mesh_tree, list_value, materialize_live_value, pack_value_tree,
    prefer_single_mesh_tree_value,
};
use super::{embed_triplet, write_start_mesh};

const WRITE_STATE_KIND: i64 = 2;
const WRITE_LAG_RATIO: f32 = 0.075;
const WRITE_SUBCONTOUR_LAG_RATIO: f32 = 0.1;
const WRITE_BOUNDARY_HEADSTART: f32 = 0.3;

#[stdlib_func]
pub async fn write_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let start = materialize_live_value(executor, &start_value).await?;
    let destination = materialize_live_value(executor, &destination_value).await?;
    prepare_write_embed_triplet(&start, &destination)
}

#[stdlib_func]
pub async fn write_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start = executor.state.stack(stack_idx).read_at(-4).clone();
    let end = executor.state.stack(stack_idx).read_at(-3).clone();
    let state = executor.state.stack(stack_idx).read_at(-2).clone();
    let t = crate::read_float(executor, stack_idx, -1, "t")? as f32;
    write_tree_value(&start, &end, &state, t.clamp(0.0, 1.0))
}

#[stdlib_func]
pub async fn flash_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let destination_value = executor.state.stack(stack_idx).read_at(-3).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let u = crate::read_float(executor, stack_idx, -2, "trail")? as f32;
    let v = crate::read_float(executor, stack_idx, -1, "lead")? as f32;
    if u > v {
        return Err(ExecutorError::InvalidArgument {
            arg: "trail",
            message: "must not exceed lead",
        });
    }

    let embedded = prepare_flash_embed_triplet(&destination)?;
    let Value::List(embedded) = embedded else {
        return Err(ExecutorError::invalid_operation(
            "invalid flash embed triplet",
        ));
    };
    let start = executor::heap::with_heap(|h| h.get(embedded.elements()[0].key()).clone());
    let end = executor::heap::with_heap(|h| h.get(embedded.elements()[1].key()).clone());
    let state = executor::heap::with_heap(|h| h.get(embedded.elements()[2].key()).clone());
    flash_tree_value(&start, &end, &state, u, v)
}

pub(super) fn prepare_write_embed_triplet(
    start: &Value,
    destination: &Value,
) -> Result<Value, ExecutorError> {
    let decomposition = decompose_mesh_tree(start, destination)?;
    let insert_leaves = contour_separated_meshes(&decomposition.insert)?;
    let delete_leaves = contour_separated_meshes(&decomposition.delete)?;
    let prefer_single = prefer_single_mesh_tree_value(start, destination);

    let mut starts = Vec::with_capacity(
        insert_leaves.len() + delete_leaves.len() + decomposition.constant.len(),
    );
    let mut ends = Vec::with_capacity(starts.capacity());
    let mut states = Vec::with_capacity(starts.capacity());

    push_write_group(&mut starts, &mut ends, &mut states, &insert_leaves, false);
    push_write_group(&mut starts, &mut ends, &mut states, &delete_leaves, true);

    for mesh in decomposition.constant {
        let value = Value::Mesh(mesh);
        starts.push(value.clone());
        ends.push(value);
        states.push(Value::Nil);
    }

    Ok(embed_triplet(
        pack_value_tree(starts, prefer_single),
        pack_value_tree(ends, prefer_single),
        pack_value_tree(states, prefer_single),
    ))
}

pub(super) fn prepare_flash_embed_triplet(destination: &Value) -> Result<Value, ExecutorError> {
    let leaves = super::trans::contour_separated_leaves(destination)?;
    let prefer_single = matches!(destination, Value::Mesh(_)) && leaves.len() <= 1;

    let mut starts = Vec::with_capacity(leaves.len());
    let mut ends = Vec::with_capacity(leaves.len());
    let mut states = Vec::with_capacity(leaves.len());

    push_write_group(&mut starts, &mut ends, &mut states, &leaves, false);

    Ok(embed_triplet(
        pack_value_tree(starts, prefer_single),
        pack_value_tree(ends, prefer_single),
        pack_value_tree(states, prefer_single),
    ))
}

fn contour_separated_meshes(meshes: &[Arc<Mesh>]) -> Result<Vec<Arc<Mesh>>, ExecutorError> {
    super::trans::contour_separated_leaves(&list_value(meshes.iter().cloned().map(Value::Mesh)))
}

fn push_write_group(
    starts: &mut Vec<Value>,
    ends: &mut Vec<Value>,
    states: &mut Vec<Value>,
    leaves: &[Arc<Mesh>],
    reverse: bool,
) {
    let total = leaves.len().max(1);
    for (index, leaf) in leaves.iter().enumerate() {
        let hidden = Value::Mesh(Arc::new(write_start_mesh(leaf.as_ref())));
        let visible = Value::Mesh(leaf.clone());
        if reverse {
            starts.push(visible);
            ends.push(hidden);
        } else {
            starts.push(hidden);
            ends.push(visible);
        }
        states.push(write_state(index, total, reverse));
    }
}

enum WriteState {
    Constant,
    Animated {
        subset_index: usize,
        subset_count: usize,
        reverse: bool,
    },
}

fn write_state(index: usize, total: usize, reverse: bool) -> Value {
    list_value([
        Value::Integer(WRITE_STATE_KIND),
        Value::Integer(index as i64),
        Value::Integer(total as i64),
        Value::Integer(reverse as i64),
    ])
}

fn read_write_state(value: &Value) -> Result<WriteState, ExecutorError> {
    match value {
        Value::Nil => Ok(WriteState::Constant),
        Value::List(list) if matches!(list.len(), 3 | 4) => {
            let kind = executor::heap::with_heap(|h| h.get(list.elements()[0].key()).clone());
            let index = executor::heap::with_heap(|h| h.get(list.elements()[1].key()).clone());
            let total = executor::heap::with_heap(|h| h.get(list.elements()[2].key()).clone());
            let reverse = if list.len() == 4 {
                executor::heap::with_heap(|h| h.get(list.elements()[3].key()).clone())
            } else {
                Value::Integer(0)
            };
            match (
                kind.elide_lvalue_leader_rec(),
                index.elide_lvalue_leader_rec(),
                total.elide_lvalue_leader_rec(),
                reverse.elide_lvalue_leader_rec(),
            ) {
                (
                    Value::Integer(WRITE_STATE_KIND),
                    Value::Integer(index),
                    Value::Integer(total),
                    Value::Integer(reverse),
                ) if index >= 0 && total > 0 => Ok(WriteState::Animated {
                    subset_index: index as usize,
                    subset_count: total as usize,
                    reverse: reverse != 0,
                }),
                _ => Err(ExecutorError::invalid_operation("invalid write state")),
            }
        }
        _ => Err(ExecutorError::invalid_operation("invalid write state")),
    }
}

fn write_tree_value(
    start: &Value,
    end: &Value,
    state: &Value,
    t: f32,
) -> Result<Value, ExecutorError> {
    match start.clone().elide_lvalue_leader_rec() {
        Value::Mesh(start_mesh) => {
            let end_mesh = match end.clone().elide_lvalue_leader_rec() {
                Value::Mesh(mesh) => mesh,
                other => {
                    return Err(ExecutorError::type_error("mesh", other.type_name()));
                }
            };
            match read_write_state(state)? {
                WriteState::Constant => Ok(Value::Mesh(start_mesh)),
                WriteState::Animated {
                    subset_index,
                    subset_count,
                    reverse,
                } => {
                    let (mesh, write_t) = if reverse {
                        (start_mesh.as_ref(), 1.0 - t)
                    } else {
                        (end_mesh.as_ref(), t)
                    };
                    Ok(Value::Mesh(Arc::new(write_mesh(
                        mesh,
                        subset_index,
                        subset_count,
                        write_t,
                    ))))
                }
            }
        }
        Value::List(start_list) => {
            let Value::List(end_list) = end.clone().elide_lvalue_leader_rec() else {
                return Err(ExecutorError::invalid_operation(format!(
                    "cannot write list with end {}",
                    end.type_name()
                )));
            };
            let Value::List(state_list) = state.clone().elide_lvalue_leader_rec() else {
                return Err(ExecutorError::invalid_operation(format!(
                    "cannot write list with state {}",
                    state.type_name()
                )));
            };
            if start_list.len() != end_list.len() || start_list.len() != state_list.len() {
                return Err(ExecutorError::invalid_operation(format!(
                    "cannot write lists of different lengths: {} vs {} vs {}",
                    start_list.len(),
                    end_list.len(),
                    state_list.len()
                )));
            }

            let mut out = Vec::with_capacity(start_list.len());
            for ((start, end), state) in start_list
                .elements()
                .iter()
                .zip(end_list.elements().iter())
                .zip(state_list.elements().iter())
            {
                let start = executor::heap::with_heap(|h| h.get(start.key()).clone());
                let end = executor::heap::with_heap(|h| h.get(end.key()).clone());
                let state = executor::heap::with_heap(|h| h.get(state.key()).clone());
                out.push(write_tree_value(&start, &end, &state, t)?);
            }
            Ok(Value::List(executor::value::container::List::new_with(
                out.into_iter().map(executor::heap::VRc::new).collect(),
            )))
        }
        other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
    }
}

fn flash_tree_value(
    start: &Value,
    end: &Value,
    state: &Value,
    u: f32,
    v: f32,
) -> Result<Value, ExecutorError> {
    match start.clone().elide_lvalue_leader_rec() {
        Value::Mesh(_start_mesh) => {
            let end_mesh = match end.clone().elide_lvalue_leader_rec() {
                Value::Mesh(mesh) => mesh,
                other => {
                    return Err(ExecutorError::type_error("mesh", other.type_name()));
                }
            };
            match read_write_state(state)? {
                WriteState::Constant => Ok(Value::Mesh(end_mesh)),
                WriteState::Animated {
                    subset_index,
                    subset_count,
                    reverse,
                } => {
                    if reverse {
                        return Err(ExecutorError::invalid_operation(
                            "flash does not support reverse write state",
                        ));
                    }
                    Ok(Value::Mesh(Arc::new(flash_mesh(
                        end_mesh.as_ref(),
                        subset_index,
                        subset_count,
                        u,
                        v,
                    ))))
                }
            }
        }
        Value::List(start_list) => {
            let Value::List(end_list) = end.clone().elide_lvalue_leader_rec() else {
                return Err(ExecutorError::invalid_operation(format!(
                    "cannot flash list with end {}",
                    end.type_name()
                )));
            };
            let Value::List(state_list) = state.clone().elide_lvalue_leader_rec() else {
                return Err(ExecutorError::invalid_operation(format!(
                    "cannot flash list with state {}",
                    state.type_name()
                )));
            };
            if start_list.len() != end_list.len() || start_list.len() != state_list.len() {
                return Err(ExecutorError::invalid_operation(format!(
                    "cannot flash lists of different lengths: {} vs {} vs {}",
                    start_list.len(),
                    end_list.len(),
                    state_list.len()
                )));
            }

            let mut out = Vec::with_capacity(start_list.len());
            for ((start, end), state) in start_list
                .elements()
                .iter()
                .zip(end_list.elements().iter())
                .zip(state_list.elements().iter())
            {
                let start = executor::heap::with_heap(|h| h.get(start.key()).clone());
                let end = executor::heap::with_heap(|h| h.get(end.key()).clone());
                let state = executor::heap::with_heap(|h| h.get(state.key()).clone());
                out.push(flash_tree_value(&start, &end, &state, u, v)?);
            }
            Ok(Value::List(executor::value::container::List::new_with(
                out.into_iter().map(executor::heap::VRc::new).collect(),
            )))
        }
        other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
    }
}

#[derive(Clone, Copy)]
struct WriteWindow {
    start: f32,
    end: f32,
}

#[derive(Clone)]
struct WriteLineGroup {
    phase_index: usize,
    lines: Vec<usize>,
}

fn write_mesh(mesh: &Mesh, subset_index: usize, subset_count: usize, t: f32) -> Mesh {
    write_mesh_window(mesh, subset_index, subset_count, 0.0, t)
}

fn flash_mesh(mesh: &Mesh, subset_index: usize, subset_count: usize, u: f32, v: f32) -> Mesh {
    write_mesh_window(mesh, subset_index, subset_count, u, v)
}

fn write_mesh_window(
    mesh: &Mesh,
    subset_index: usize,
    subset_count: usize,
    u: f32,
    v: f32,
) -> Mesh {
    let mut out = mesh.clone();
    out.uniform.alpha = mesh.uniform.alpha;
    let has_visible_dots = mesh.dots.iter().any(|dot| dot.col.w > f32::EPSILON);

    let subset_count = subset_count.max(1) as f32;
    let anim_length = 1.0 / (WRITE_LAG_RATIO * (subset_count - 1.0) + 1.0);
    let base_start = subset_index as f32 * anim_length * WRITE_LAG_RATIO;
    let base_end = base_start + anim_length;

    write_dots(
        mesh,
        &mut out,
        WriteWindow {
            start: base_start,
            end: if mesh.lins.is_empty() {
                base_end
            } else {
                base_end - WRITE_BOUNDARY_HEADSTART * anim_length
            },
        },
        u,
        v,
    );

    let raw_line_end = base_end;
    write_lines(
        mesh,
        &mut out,
        WriteWindow {
            start: if !has_visible_dots {
                base_start
            } else {
                base_start + WRITE_BOUNDARY_HEADSTART * anim_length
            },
            end: if mesh.tris.is_empty() {
                raw_line_end
            } else {
                raw_line_end - WRITE_BOUNDARY_HEADSTART * anim_length
            },
        },
        raw_line_end,
        u,
        v,
    );
    sync_inverse_line_geometry(&mut out);

    write_tris(
        mesh,
        &mut out,
        WriteWindow {
            start: if mesh.lins.is_empty() {
                base_start
            } else {
                base_start + WRITE_BOUNDARY_HEADSTART * anim_length
            },
            end: base_end,
        },
        u,
        v,
    );

    debug_assert!(out.has_consistent_line_links());
    out
}

fn write_dots(mesh: &Mesh, out: &mut Mesh, window: WriteWindow, u: f32, v: f32) {
    if !mesh.dots.iter().any(|dot| dot.col.w > f32::EPSILON) {
        return;
    }

    if v < window.start || u > window.end {
        for dot in &mut out.dots {
            dot.col.w = 0.0;
        }
        return;
    }

    let sub_u = normalize_window_time(u, window);
    let sub_v = normalize_window_time(v, window);
    let sub_t = (sub_v - sub_u).clamp(0.0, 1.0);
    let unit_length =
        1.0 / (WRITE_SUBCONTOUR_LAG_RATIO * mesh.dots.len().saturating_sub(1) as f32 + 1.0);
    for (index, (src, dst)) in mesh.dots.iter().zip(out.dots.iter_mut()).enumerate() {
        let full_t = ((sub_t - index as f32 * WRITE_SUBCONTOUR_LAG_RATIO * unit_length)
            / unit_length)
            .clamp(0.0, 1.0);
        dst.col.w = src.col.w * full_t;
    }
}

fn write_lines(mesh: &Mesh, out: &mut Mesh, window: WriteWindow, raw_end: f32, u: f32, v: f32) {
    if mesh.lins.is_empty() {
        return;
    }

    if v < window.start || u > window.end {
        for (src, dst) in mesh.lins.iter().zip(out.lins.iter_mut()) {
            dst.a.pos = src.a.pos;
            dst.b.pos = src.a.pos;
            dst.a.col.w = 0.0;
            dst.b.col.w = 0.0;
        }
        return;
    }

    let groups = write_line_groups(mesh);
    let sub_u = normalize_window_time(u, window);
    let sub_v = normalize_window_time(v, window);
    let unit_length =
        1.0 / (WRITE_SUBCONTOUR_LAG_RATIO * groups.len().saturating_sub(1) as f32 + 1.0);

    for group in groups {
        let phase_offset = group.phase_index as f32 * WRITE_SUBCONTOUR_LAG_RATIO * unit_length;
        let full_u = ((sub_u - phase_offset) / unit_length).clamp(0.0, 1.0);
        let full_v = ((sub_v - phase_offset) / unit_length).clamp(0.0, 1.0);
        let count = group.lines.len().max(1) as f32;

        for (ind, &line_idx) in group.lines.iter().enumerate() {
            let src = mesh.lins[line_idx];
            let dst = &mut out.lins[line_idx];
            let segment_start = ind as f32 / count;
            let segment_end = (ind as f32 + 1.0) / count;

            dst.a.pos = write_segment_endpoint(
                src.a.pos,
                src.b.pos,
                segment_start,
                segment_end,
                full_u,
                SegmentEndpoint::Start,
            );
            dst.b.pos = write_segment_endpoint(
                src.a.pos,
                src.b.pos,
                segment_start,
                segment_end,
                full_v,
                SegmentEndpoint::End,
            );
            if v < raw_end {
                dst.a.col.w = 1.0;
                dst.b.col.w = 1.0;
            } else {
                dst.a.col.w = src.a.col.w;
                dst.b.col.w = src.b.col.w;
            }
        }
    }

    if v > window.end && v < raw_end {
        let overlap_t = ((v - window.end) / (raw_end - window.end).max(1e-6)).clamp(0.0, 1.0);
        for (src, dst) in mesh.lins.iter().zip(out.lins.iter_mut()) {
            dst.a.pos = src.a.pos;
            dst.b.pos = src.b.pos;
            dst.a.col.w = (1.0 - overlap_t) + overlap_t * src.a.col.w;
            dst.b.col.w = (1.0 - overlap_t) + overlap_t * src.b.col.w;
        }
    }
}

fn write_tris(mesh: &Mesh, out: &mut Mesh, window: WriteWindow, u: f32, v: f32) {
    if mesh.tris.is_empty() {
        return;
    }

    let sub_t = if v < window.start || u > window.end {
        0.0
    } else {
        (normalize_window_time(v, window) - normalize_window_time(u, window)).clamp(0.0, 1.0)
    };
    for (src, dst) in mesh.tris.iter().zip(out.tris.iter_mut()) {
        dst.a.col.w = src.a.col.w * sub_t;
        dst.b.col.w = src.b.col.w * sub_t;
        dst.c.col.w = src.c.col.w * sub_t;
    }
}

fn normalize_window_time(t: f32, window: WriteWindow) -> f32 {
    if window.end <= window.start {
        1.0
    } else {
        ((t - window.start) / (window.end - window.start)).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy)]
enum SegmentEndpoint {
    Start,
    End,
}

fn write_segment_endpoint(
    a: Float3,
    b: Float3,
    segment_start: f32,
    segment_end: f32,
    full_t: f32,
    endpoint: SegmentEndpoint,
) -> Float3 {
    match endpoint {
        SegmentEndpoint::Start => {
            if segment_start > full_t {
                a
            } else if segment_end < full_t {
                b
            } else {
                let local = ((full_t - segment_start) / (segment_end - segment_start).max(1e-6))
                    .clamp(0.0, 1.0);
                a.lerp(b, local)
            }
        }
        SegmentEndpoint::End => {
            if segment_end < full_t {
                b
            } else if segment_start > full_t {
                a
            } else {
                let local = ((full_t - segment_start) / (segment_end - segment_start).max(1e-6))
                    .clamp(0.0, 1.0);
                a.lerp(b, local)
            }
        }
    }
}

fn write_line_groups(mesh: &Mesh) -> Vec<WriteLineGroup> {
    ordered_line_components(mesh)
        .into_iter()
        .enumerate()
        .map(|(phase_index, lines)| WriteLineGroup { phase_index, lines })
        .collect()
}

fn sync_inverse_line_geometry(mesh: &mut Mesh) {
    for line_idx in 0..mesh.lins.len() {
        let line = mesh.lins[line_idx];
        if !line.is_dom_sib || line.inv < 0 {
            continue;
        }
        let inv_idx = line.inv as usize;
        if inv_idx >= mesh.lins.len() || mesh.lins[inv_idx].is_dom_sib {
            continue;
        }

        let inverse = &mut mesh.lins[inv_idx];
        inverse.a.pos = line.b.pos;
        inverse.b.pos = line.a.pos;
        inverse.a.col = line.b.col;
        inverse.b.col = line.a.col;
    }
}

fn ordered_line_components(mesh: &Mesh) -> Vec<Vec<usize>> {
    let mut visited = vec![false; mesh.lins.len()];
    let mut components = Vec::new();

    for line_idx in 0..mesh.lins.len() {
        if visited[line_idx] || !mesh.lins[line_idx].is_dom_sib {
            continue;
        }

        let mut start = line_idx;
        let mut cursor = line_idx;
        loop {
            let prev = mesh.lins[cursor].prev;
            if prev < 0 {
                break;
            }
            let prev = prev as usize;
            if prev >= mesh.lins.len()
                || !mesh.lins[prev].is_dom_sib
                || prev == start
                || prev == cursor
                || prev == line_idx
            {
                break;
            }
            cursor = prev;
            start = cursor;
        }

        let mut component = Vec::new();
        let mut cursor = start;
        loop {
            if cursor >= mesh.lins.len() || visited[cursor] {
                break;
            }
            visited[cursor] = true;
            component.push(cursor);

            let next = mesh.lins[cursor].next;
            if next < 0 {
                break;
            }
            let next = next as usize;
            if next >= mesh.lins.len() || !mesh.lins[next].is_dom_sib {
                break;
            }
            if next == start {
                break;
            }
            cursor = next;
        }

        if !component.is_empty() {
            components.push(component);
        }
    }

    components
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::anim::helpers::list_value;
    use executor::value::Value;
    use geo::{
        mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
        simd::{Float3, Float4},
    };

    use super::{prepare_write_embed_triplet, write_mesh};

    fn line(a: Float3, b: Float3, prev: i32, next: i32) -> Lin {
        Lin {
            a: LinVertex {
                pos: a,
                col: Float4::ONE,
            },
            b: LinVertex {
                pos: b,
                col: Float4::ONE,
            },
            norm: Float3::Z,
            prev,
            next,
            inv: -1,
            is_dom_sib: true,
        }
    }

    fn tri(a: Float3, b: Float3, c: Float3, ab: i32, bc: i32, ca: i32) -> Tri {
        Tri {
            a: TriVertex {
                pos: a,
                col: Float4::ONE,
                uv: Default::default(),
            },
            b: TriVertex {
                pos: b,
                col: Float4::ONE,
                uv: Default::default(),
            },
            c: TriVertex {
                pos: c,
                col: Float4::ONE,
                uv: Default::default(),
            },
            ab,
            bc,
            ca,
            is_dom_sib: false,
        }
    }

    #[test]
    fn write_mesh_preserves_authored_line_links() {
        let mesh = Mesh {
            dots: vec![],
            lins: vec![
                line(Float3::ZERO, Float3::X, -1, 1),
                line(Float3::X, Float3::new(1.0, 1.0, 0.0), 0, -1),
            ],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
        };

        let written = write_mesh(&mesh, 0, 1, 0.0);

        assert_eq!(written.lins[0].prev, -1);
        assert_eq!(written.lins[0].next, 1);
        assert_eq!(written.lins[1].prev, 0);
        assert_eq!(written.lins[1].next, -1);
        assert!(written.has_consistent_line_links());
    }

    #[test]
    fn prepare_write_embed_triplet_contour_separates_single_mesh() {
        let mesh = Mesh {
            dots: vec![Dot {
                pos: Float3::new(20.0, 0.0, 0.0),
                norm: Float3::Z,
                col: Float4::ONE,
                inv: -1,
                is_dom_sib: false,
            }],
            lins: vec![
                line(Float3::ZERO, Float3::X, -1, 1),
                line(Float3::X, Float3::new(2.0, 0.0, 0.0), 0, -1),
            ],
            tris: vec![tri(
                Float3::new(10.0, 0.0, 0.0),
                Float3::new(11.0, 0.0, 0.0),
                Float3::new(10.0, 1.0, 0.0),
                -1,
                -1,
                -1,
            )],
            uniform: Uniforms::default(),
            tag: vec![],
        };

        let embedded = prepare_write_embed_triplet(&list_value([]), &Value::Mesh(Arc::new(mesh)))
            .expect("write embed prep should succeed");
        let Value::List(embedded) = embedded else {
            panic!("expected embed triplet");
        };

        let ends = executor::heap::with_heap(|h| h.get(embedded.elements()[1].key()).clone());
        let states = executor::heap::with_heap(|h| h.get(embedded.elements()[2].key()).clone());

        let Value::List(ends) = ends else {
            panic!("expected separated destination list");
        };
        let Value::List(states) = states else {
            panic!("expected separated state list");
        };

        assert_eq!(ends.len(), 3);
        assert_eq!(states.len(), 3);

        let mut signature = Vec::new();
        for key in ends.elements() {
            let Value::Mesh(mesh) = executor::heap::with_heap(|h| h.get(key.key()).clone()) else {
                panic!("expected mesh contour");
            };
            signature.push((mesh.dots.len(), mesh.lins.len(), mesh.tris.len()));
        }
        signature.sort_unstable();

        assert_eq!(signature, vec![(0, 0, 1), (0, 2, 0), (1, 0, 0)]);
    }
}
