use std::{rc::Rc, sync::Arc};

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{
    mesh::Mesh,
    simd::Float3,
};
use stdlib_macros::stdlib_func;

use super::{embed_triplet, write_start_mesh};
use super::super::helpers::{list_value, materialize_live_value};

const WRITE_STATE_KIND: i64 = 2;
const WRITE_LAG_RATIO: f32 = 0.075;
const WRITE_SUBCONTOUR_LAG_RATIO: f32 = 0.1;
const WRITE_BOUNDARY_HEADSTART: f32 = 0.3;

#[stdlib_func]
pub async fn write_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    prepare_write_embed_triplet(&destination)
}

#[stdlib_func]
pub async fn write_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let end = executor.state.stack(stack_idx).read_at(-3).clone();
    let state = executor.state.stack(stack_idx).read_at(-2).clone();
    let t = crate::read_float(executor, stack_idx, -1, "t")? as f32;
    write_tree_value(&end, &state, t.clamp(0.0, 1.0))
}

pub(super) fn prepare_write_embed_triplet(destination: &Value) -> Result<Value, ExecutorError> {
    let leaves = super::trans::contour_separated_leaves(destination)?;

    let mut starts = Vec::with_capacity(leaves.len());
    let mut ends = Vec::with_capacity(leaves.len());
    let mut states = Vec::with_capacity(leaves.len());
    let total = leaves.len().max(1);
    for (index, leaf) in leaves.iter().enumerate() {
        starts.push(Value::Mesh(Arc::new(write_start_mesh(leaf.as_ref()))));
        ends.push(Value::Mesh(leaf.clone()));
        states.push(write_state(index, total));
    }

    if matches!(destination, Value::Mesh(_)) && leaves.len() <= 1 {
        return Ok(embed_triplet(
            starts.into_iter().next().unwrap_or_else(|| list_value([])),
            ends.into_iter().next().unwrap_or_else(|| list_value([])),
            states.into_iter().next().unwrap_or(Value::Nil),
        ));
    }

    Ok(embed_triplet(
        list_value(starts),
        list_value(ends),
        list_value(states),
    ))
}

fn write_state(index: usize, total: usize) -> Value {
    list_value([
        Value::Integer(WRITE_STATE_KIND),
        Value::Integer(index as i64),
        Value::Integer(total as i64),
    ])
}

fn read_write_state(value: &Value) -> Result<(usize, usize), ExecutorError> {
    match value {
        Value::Nil => Ok((0, 1)),
        Value::List(list) if list.len() == 3 => {
            let kind = executor::heap::with_heap(|h| h.get(list.elements()[0].key()).clone());
            let index = executor::heap::with_heap(|h| h.get(list.elements()[1].key()).clone());
            let total = executor::heap::with_heap(|h| h.get(list.elements()[2].key()).clone());
            match (
                kind.elide_lvalue_leader_rec(),
                index.elide_lvalue_leader_rec(),
                total.elide_lvalue_leader_rec(),
            ) {
                (
                    Value::Integer(WRITE_STATE_KIND),
                    Value::Integer(index),
                    Value::Integer(total),
                ) if index >= 0 && total > 0 => Ok((index as usize, total as usize)),
                _ => Err(ExecutorError::Other("invalid write state".into())),
            }
        }
        _ => Err(ExecutorError::Other("invalid write state".into())),
    }
}

fn write_tree_value(value: &Value, state: &Value, t: f32) -> Result<Value, ExecutorError> {
    match value.clone().elide_lvalue_leader_rec() {
        Value::Mesh(mesh) => {
            let (subset_index, subset_count) = read_write_state(state)?;
            Ok(Value::Mesh(Arc::new(write_mesh(
                mesh.as_ref(),
                subset_index,
                subset_count,
                t,
            ))))
        }
        Value::List(list) => {
            let Value::List(state_list) = state.clone().elide_lvalue_leader_rec() else {
                return Err(ExecutorError::Other(format!(
                    "cannot write list with state {}",
                    state.type_name()
                )));
            };
            if list.len() != state_list.len() {
                return Err(ExecutorError::Other(format!(
                    "cannot write lists of different lengths: {} vs {}",
                    list.len(),
                    state_list.len()
                )));
            }

            let mut out = Vec::with_capacity(list.len());
            for (elem, state) in list.elements().iter().zip(state_list.elements().iter()) {
                let elem = executor::heap::with_heap(|h| h.get(elem.key()).clone());
                let state = executor::heap::with_heap(|h| h.get(state.key()).clone());
                out.push(write_tree_value(&elem, &state, t)?);
            }
            Ok(Value::List(Rc::new(
                executor::value::container::List::new_with(
                    out.into_iter().map(executor::heap::VRc::new).collect(),
                ),
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
    let mut out = mesh.clone();
    out.uniform.alpha = mesh.uniform.alpha;

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
        t,
    );

    let raw_line_end = base_end;
    write_lines(
        mesh,
        &mut out,
        WriteWindow {
            start: if mesh.dots.is_empty() {
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
        t,
    );

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
        t,
    );

    debug_assert!(out.has_consistent_line_links());
    out
}

fn write_dots(mesh: &Mesh, out: &mut Mesh, window: WriteWindow, t: f32) {
    if mesh.dots.is_empty() {
        return;
    }

    if t < window.start {
        for dot in &mut out.dots {
            dot.col.w = 0.0;
        }
        return;
    }

    let sub_t = normalize_window_time(t, window);
    let unit_length =
        1.0 / (WRITE_SUBCONTOUR_LAG_RATIO * mesh.dots.len().saturating_sub(1) as f32 + 1.0);
    for (index, (src, dst)) in mesh.dots.iter().zip(out.dots.iter_mut()).enumerate() {
        let full_t = ((sub_t - index as f32 * WRITE_SUBCONTOUR_LAG_RATIO * unit_length)
            / unit_length)
            .clamp(0.0, 1.0);
        dst.col.w = src.col.w * full_t;
    }
}

fn write_lines(mesh: &Mesh, out: &mut Mesh, window: WriteWindow, raw_end: f32, t: f32) {
    if mesh.lins.is_empty() {
        return;
    }

    if t < window.start {
        for (src, dst) in mesh.lins.iter().zip(out.lins.iter_mut()) {
            dst.a.pos = src.a.pos;
            dst.b.pos = src.a.pos;
            dst.a.col.w = 0.0;
            dst.b.col.w = 0.0;
        }
        return;
    }

    if t >= raw_end {
        for (src, dst) in mesh.lins.iter().zip(out.lins.iter_mut()) {
            dst.a.pos = src.a.pos;
            dst.b.pos = src.b.pos;
            dst.a.col.w = src.a.col.w;
            dst.b.col.w = src.b.col.w;
        }
        return;
    }

    if t > window.end {
        let overlap_t = ((t - window.end) / (raw_end - window.end).max(1e-6)).clamp(0.0, 1.0);
        for (src, dst) in mesh.lins.iter().zip(out.lins.iter_mut()) {
            dst.a.pos = src.a.pos;
            dst.b.pos = src.b.pos;
            dst.a.col.w = (1.0 - overlap_t) + overlap_t * src.a.col.w;
            dst.b.col.w = (1.0 - overlap_t) + overlap_t * src.b.col.w;
        }
        return;
    }

    let groups = write_line_groups(mesh);
    let sub_t = normalize_window_time(t, window);
    let unit_length =
        1.0 / (WRITE_SUBCONTOUR_LAG_RATIO * groups.len().saturating_sub(1) as f32 + 1.0);

    for group in groups {
        let full_t = ((sub_t
            - group.phase_index as f32 * WRITE_SUBCONTOUR_LAG_RATIO * unit_length)
            / unit_length)
            .clamp(0.0, 1.0);
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
                full_t,
                false,
            );
            dst.b.pos = write_segment_endpoint(
                src.a.pos,
                src.b.pos,
                segment_start,
                segment_end,
                full_t,
                true,
            );
            dst.a.col.w = 1.0;
            dst.b.col.w = 1.0;
        }
    }
}

fn write_tris(mesh: &Mesh, out: &mut Mesh, window: WriteWindow, t: f32) {
    if mesh.tris.is_empty() {
        return;
    }

    let sub_t = if t < window.start {
        0.0
    } else {
        normalize_window_time(t, window)
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

fn write_segment_endpoint(
    a: Float3,
    b: Float3,
    segment_start: f32,
    segment_end: f32,
    full_t: f32,
    use_end: bool,
) -> Float3 {
    if segment_end < full_t {
        if use_end { b } else { a }
    } else if segment_start > full_t {
        a
    } else {
        let local =
            ((full_t - segment_start) / (segment_end - segment_start).max(1e-6)).clamp(0.0, 1.0);
        let point = a.lerp(b, local);
        if use_end { point } else { a }
    }
}

fn write_line_groups(mesh: &Mesh) -> Vec<WriteLineGroup> {
    ordered_line_components(mesh)
        .into_iter()
        .enumerate()
        .map(|(phase_index, lines)| WriteLineGroup { phase_index, lines })
        .collect()
}

fn ordered_line_components(mesh: &Mesh) -> Vec<Vec<usize>> {
    let mut visited = vec![false; mesh.lins.len()];
    let mut components = Vec::new();

    for line_idx in 0..mesh.lins.len() {
        if visited[line_idx] {
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
            if prev >= mesh.lins.len() || prev == start || prev == cursor || prev == line_idx {
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
            anti: -1,
            is_dom_sib: false,
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
            anti: -1,
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
                anti: -1,
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

        let embedded = prepare_write_embed_triplet(&Value::Mesh(Arc::new(mesh)))
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
