use std::{rc::Rc, sync::Arc};

use crate::read_float;
use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{VRc, heap_replace, with_heap, with_heap_mut},
    state::LeaderKind,
    value::{Value, container::List, leader::Leader, primitive_anim::PrimitiveAnim},
};
use geo::{
    mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex},
    simd::{Float3, Float4},
};

pub(super) fn read_time(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
) -> Result<f64, ExecutorError> {
    let time = read_float(executor, stack_idx, index, "time")?;
    if time < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "time",
            message: "must be non-negative",
        });
    }
    Ok(time)
}

pub(super) fn progression_from(value: Value) -> Option<Box<Value>> {
    matches!(value, Value::Lambda(_) | Value::Operator(_)).then(|| Box::new(value))
}

pub(super) fn read_float4_value(value: Value, name: &'static str) -> Result<Float4, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::List(list) if list.elements().len() == 4 => {
            let mut out = [0.0; 4];
            for (slot, key) in out.iter_mut().zip(list.elements().iter()) {
                *slot = match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => n as f32,
                    Value::Float(f) => f as f32,
                    other => {
                        return Err(ExecutorError::type_error_for(
                            "number",
                            other.type_name(),
                            name,
                        ));
                    }
                };
            }
            Ok(Float4::from_array(out))
        }
        other => Err(ExecutorError::type_error_for(
            "list of length 4",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn list_value(values: impl IntoIterator<Item = Value>) -> Value {
    Value::List(Rc::new(List::new_with(
        values.into_iter().map(VRc::new).collect(),
    )))
}

pub(super) fn targets_to_value(targets: &[VRc]) -> Value {
    list_value(targets.iter().cloned().map(Value::Lvalue))
}

fn dedup_targets(values: &mut Vec<VRc>) {
    let mut out = Vec::with_capacity(values.len());
    for value in values.drain(..) {
        if !out
            .iter()
            .any(|existing: &VRc| existing.key() == value.key())
        {
            out.push(value);
        }
    }
    *values = out;
}

fn push_leader_candidate(
    executor: &Executor,
    value: &Value,
    kind: Option<LeaderKind>,
    out: &mut Vec<VRc>,
) -> Result<(), ExecutorError> {
    match value {
        Value::Leader(leader) => {
            if kind.is_none_or(|kind| leader.kind == kind) {
                out.push(
                    find_leader_cell_from_value(executor, leader).ok_or_else(|| {
                        ExecutorError::internal(
                            "animation variable does not belong to executor state",
                        )
                    })?,
                );
            }
            Ok(())
        }
        Value::Lvalue(vrc) => {
            let inner = with_heap(|h| h.get(vrc.key()).clone());
            match inner {
                Value::Leader(leader) => {
                    if kind.is_none_or(|kind| leader.kind == kind) {
                        out.push(VRc::retain_key(vrc.key()));
                    }
                    Ok(())
                }
                other => Err(ExecutorError::type_error(
                    "leader variable reference",
                    other.type_name(),
                )),
            }
        }
        Value::WeakLvalue(vweak) => {
            let inner = with_heap(|h| h.get(vweak.key()).clone());
            match inner {
                Value::Leader(leader) => {
                    if kind.is_none_or(|kind| leader.kind == kind) {
                        out.push(VRc::retain_key(vweak.key()));
                    }
                    Ok(())
                }
                other => Err(ExecutorError::type_error(
                    "leader variable reference",
                    other.type_name(),
                )),
            }
        }
        other => Err(ExecutorError::type_error(
            "leader variable reference",
            other.type_name(),
        )),
    }
}

fn find_leader_cell_from_value(executor: &Executor, needle: &Leader) -> Option<VRc> {
    executor.state.leaders.iter().find_map(|entry| {
        let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
        let Value::Leader(leader) = cell_val else {
            return None;
        };
        ((leader.leader_rc.key(), leader.follower_rc.key())
            == (needle.leader_rc.key(), needle.follower_rc.key()))
            .then(|| entry.leader_cell.clone())
    })
}

fn flatten_candidate_tree(
    executor: &Executor,
    value: &Value,
    kind: Option<LeaderKind>,
    out: &mut Vec<VRc>,
) -> Result<(), ExecutorError> {
    match value {
        Value::List(list) => {
            for elem in list.elements() {
                let elem = with_heap(|h| h.get(elem.key()).clone());
                flatten_candidate_tree(executor, &elem, kind, out)?;
            }
            Ok(())
        }
        value => push_leader_candidate(executor, value, kind, out),
    }
}

pub(super) fn resolve_targets(
    executor: &Executor,
    stack_idx: usize,
    candidates: &Value,
    kind: Option<LeaderKind>,
) -> Result<Vec<VRc>, ExecutorError> {
    let mut out = Vec::new();
    flatten_candidate_tree(executor, candidates, kind, &mut out)?;

    if out.is_empty() {
        let stack_id = executor.state.stack_id(stack_idx);
        for entry in &executor.state.leaders {
            if kind.is_some_and(|kind| entry.kind != kind) {
                continue;
            }
            let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
            let Value::Leader(leader) = cell_val else {
                continue;
            };
            if leader.last_modified_stack.is_some_and(|modified| {
                executor
                    .state
                    .is_stack_id_ancestor_of_stack(modified, stack_idx)
            }) {
                out.push(entry.leader_cell.clone());
            }
        }
        let _ = stack_id;
    }

    dedup_targets(&mut out);
    Ok(out)
}

pub(super) fn leader_from_cell(cell: &VRc) -> Result<Leader, ExecutorError> {
    let value = with_heap(|h| h.get(cell.key()).clone());
    match value {
        Value::Leader(leader) => Ok(leader),
        other => Err(ExecutorError::type_error("leader", other.type_name())),
    }
}

pub(super) fn follower_value(cell: &VRc) -> Result<Value, ExecutorError> {
    let leader = leader_from_cell(cell)?;
    Ok(with_heap(|h| h.get(leader.follower_rc.key()).clone()))
}

pub(super) fn replace_leader_and_follower(
    executor: &Executor,
    stack_idx: usize,
    cell: &VRc,
    value: Value,
) -> Result<(), ExecutorError> {
    let leader = leader_from_cell(cell)?;
    let follower = value.clone().to_follower_stateful();
    heap_replace(leader.leader_rc.key(), value);
    heap_replace(leader.follower_rc.key(), follower);
    let stack_id = executor.state.stack_id(stack_idx);
    with_heap_mut(|h| {
        if let Value::Leader(leader) = &mut *h.get_mut(cell.key()) {
            leader.leader_version += 1;
            leader.follower_version += 1;
            leader.last_modified_stack = Some(stack_id);
        }
    });
    Ok(())
}

pub(super) fn build_lerp(
    targets: &[VRc],
    time: f64,
    progression: Option<Box<Value>>,
    embed: Option<Box<Value>>,
    lerp: Option<Box<Value>>,
) -> Value {
    Value::PrimitiveAnim(PrimitiveAnim::Lerp {
        candidates: Box::new(targets_to_value(targets)),
        time,
        progression,
        embed,
        lerp,
    })
}

pub(super) fn build_set(targets: &[VRc]) -> Value {
    Value::PrimitiveAnim(PrimitiveAnim::Set {
        candidates: Box::new(targets_to_value(targets)),
    })
}

pub(super) fn scale_primitive_time(anim: Value, factor: f64) -> Result<Value, ExecutorError> {
    match anim {
        Value::PrimitiveAnim(PrimitiveAnim::Lerp {
            candidates,
            time,
            progression,
            embed,
            lerp,
        }) => Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
            candidates,
            time: time * factor,
            progression,
            embed,
            lerp,
        })),
        Value::PrimitiveAnim(PrimitiveAnim::Wait { time }) => {
            Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait {
                time: time * factor,
            }))
        }
        Value::PrimitiveAnim(PrimitiveAnim::Set { .. }) => Ok(anim),
        other => Err(ExecutorError::type_error_for(
            "primitive_anim",
            other.type_name(),
            "target",
        )),
    }
}

pub(super) fn delay_primitive(anim: Value, delay: f64) -> Result<Value, ExecutorError> {
    match anim {
        Value::PrimitiveAnim(PrimitiveAnim::Lerp {
            candidates,
            time,
            progression,
            embed,
            lerp,
        }) => Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
            candidates,
            time: time + delay,
            progression,
            embed,
            lerp,
        })),
        Value::PrimitiveAnim(PrimitiveAnim::Wait { time }) => {
            Ok(Value::PrimitiveAnim(PrimitiveAnim::Wait {
                time: time + delay,
            }))
        }
        Value::PrimitiveAnim(PrimitiveAnim::Set { candidates }) => {
            Ok(Value::PrimitiveAnim(PrimitiveAnim::Lerp {
                candidates,
                time: delay,
                progression: None,
                embed: None,
                lerp: None,
            }))
        }
        other => Err(ExecutorError::type_error_for(
            "primitive_anim",
            other.type_name(),
            "target",
        )),
    }
}

pub(super) async fn eval_unit_map(
    executor: &mut Executor,
    map: &Value,
    t: f64,
) -> Result<f64, ExecutorError> {
    let raw = match map.clone().elide_lvalue() {
        Value::Lambda(lambda) => {
            executor
                .invoke_lambda(&lambda, vec![Value::Float(t)])
                .await?
        }
        Value::Operator(operator) => {
            executor
                .invoke_lambda(&operator.0, vec![Value::Float(t)])
                .await?
        }
        other => {
            return Err(ExecutorError::type_error(
                "lambda / operator",
                other.type_name(),
            ));
        }
    }
    .elide_wrappers(executor)
    .await?;

    match raw {
        Value::Float(f) => Ok(f),
        Value::Integer(n) => Ok(n as f64),
        other => Err(ExecutorError::type_error_for(
            "float",
            other.type_name(),
            "unit_map",
        )),
    }
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

fn mesh_bounds(mesh: &Mesh) -> Option<(Float3, Float3)> {
    let mut vertices = mesh_vertices(mesh);
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

pub(super) fn mesh_center(mesh: &Mesh) -> Float3 {
    mesh_bounds(mesh)
        .map(|(min, max)| (min + max) / 2.0)
        .unwrap_or(Float3::ZERO)
}

pub(super) fn flatten_mesh_leaves(
    value: &Value,
    out: &mut Vec<Arc<Mesh>>,
) -> Result<(), ExecutorError> {
    match value {
        Value::Mesh(mesh) => {
            out.push(mesh.clone());
            Ok(())
        }
        Value::List(list) => {
            for elem in list.elements() {
                let elem = with_heap(|h| h.get(elem.key()).clone());
                flatten_mesh_leaves(&elem, out)?;
            }
            Ok(())
        }
        other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
    }
}

pub(super) fn map_mesh_tree(
    value: &Value,
    f: &mut impl FnMut(&Mesh) -> Mesh,
) -> Result<Value, ExecutorError> {
    match value {
        Value::Mesh(mesh) => Ok(Value::Mesh(Arc::new(f(mesh)))),
        Value::List(list) => {
            let mut out = Vec::with_capacity(list.len());
            for elem in list.elements() {
                let elem = with_heap(|h| h.get(elem.key()).clone());
                out.push(map_mesh_tree(&elem, f)?);
            }
            Ok(list_value(out))
        }
        other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
    }
}

fn copy_dot_template(dot: &Dot, pos: Float3, col: Float4) -> Dot {
    Dot {
        pos,
        norm: dot.norm,
        col,
        inv: dot.inv,
        anti: dot.anti,
        is_dom_sib: dot.is_dom_sib,
    }
}

fn copy_lin_template(lin: &Lin, a: Float3, b: Float3, col_a: Float4, col_b: Float4) -> Lin {
    Lin {
        a: LinVertex { pos: a, col: col_a },
        b: LinVertex { pos: b, col: col_b },
        norm: lin.norm,
        prev: lin.prev,
        next: lin.next,
        inv: lin.inv,
        anti: lin.anti,
        is_dom_sib: lin.is_dom_sib,
    }
}

fn copy_tri_template(
    tri: &Tri,
    a: Float3,
    b: Float3,
    c: Float3,
    col_a: Float4,
    col_b: Float4,
    col_c: Float4,
) -> Tri {
    Tri {
        a: TriVertex {
            pos: a,
            col: col_a,
            uv: tri.a.uv,
        },
        b: TriVertex {
            pos: b,
            col: col_b,
            uv: tri.b.uv,
        },
        c: TriVertex {
            pos: c,
            col: col_c,
            uv: tri.c.uv,
        },
        ab: tri.ab,
        bc: tri.bc,
        ca: tri.ca,
        anti: tri.anti,
        is_dom_sib: tri.is_dom_sib,
    }
}

pub(super) fn collapse_mesh(mesh: &Mesh, center: Float3) -> Mesh {
    let mesh = Mesh {
        dots: mesh
            .dots
            .iter()
            .map(|dot| copy_dot_template(dot, center, dot.col))
            .collect(),
        lins: mesh
            .lins
            .iter()
            .map(|lin| copy_lin_template(lin, center, center, lin.a.col, lin.b.col))
            .collect(),
        tris: mesh
            .tris
            .iter()
            .map(|tri| {
                copy_tri_template(tri, center, center, center, tri.a.col, tri.b.col, tri.c.col)
            })
            .collect(),
        uniform: mesh.uniform.clone(),
        tag: mesh.tag.clone(),
    };
    debug_assert!(mesh.has_consistent_topology());
    mesh
}

pub(super) fn fade_start_mesh(mesh: &Mesh, delta: Float3) -> Mesh {
    let mut out = mesh.clone();
    for dot in &mut out.dots {
        dot.pos = dot.pos - delta;
    }
    for lin in &mut out.lins {
        lin.a.pos = lin.a.pos - delta;
        lin.b.pos = lin.b.pos - delta;
    }
    for tri in &mut out.tris {
        tri.a.pos = tri.a.pos - delta;
        tri.b.pos = tri.b.pos - delta;
        tri.c.pos = tri.c.pos - delta;
    }
    out.uniform.alpha = 0.0;
    debug_assert!(out.has_consistent_topology());
    out
}

pub(super) fn materialize_current_value(value: &Value) -> Result<Value, ExecutorError> {
    match value {
        Value::List(list) => {
            let mut out = Vec::with_capacity(list.len());
            for elem in list.elements() {
                let elem = with_heap(|h| h.get(elem.key()).clone());
                out.push(materialize_current_value(&elem)?);
            }
            Ok(list_value(out))
        }
        Value::Lvalue(vrc) => materialize_current_value(&with_heap(|h| h.get(vrc.key()).clone())),
        Value::WeakLvalue(vweak) => {
            materialize_current_value(&with_heap(|h| h.get(vweak.key()).clone()))
        }
        Value::Leader(leader) => Ok(with_heap(|h| h.get(leader.follower_rc.key()).clone())),
        other => Ok(other.clone().elide_lvalue_leader_rec()),
    }
}

pub(super) async fn materialize_live_value(
    executor: &mut Executor,
    value: &Value,
) -> Result<Value, ExecutorError> {
    let resolved = value.clone().elide_wrappers(executor).await?;
    materialize_current_value(&resolved)
}

pub(super) fn merge_transfer_value(dst: Value, transfer: Value) -> Value {
    match dst.elide_lvalue_leader_rec() {
        Value::List(dst_list) if dst_list.is_empty() => transfer,
        Value::List(dst_list) => match transfer {
            Value::List(src_list) => {
                let mut out = Vec::with_capacity(dst_list.len() + src_list.len());
                for elem in dst_list.elements() {
                    out.push(with_heap(|h| h.get(elem.key()).clone()));
                }
                for elem in src_list.elements() {
                    out.push(with_heap(|h| h.get(elem.key()).clone()));
                }
                list_value(out)
            }
            other => {
                let mut out = Vec::with_capacity(dst_list.len() + 1);
                for elem in dst_list.elements() {
                    out.push(with_heap(|h| h.get(elem.key()).clone()));
                }
                out.push(other);
                list_value(out)
            }
        },
        _ => transfer,
    }
}

pub(super) fn empty_mesh_tree() -> Value {
    list_value([])
}
