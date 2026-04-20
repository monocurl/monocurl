use std::{collections::HashMap, rc::Rc, sync::Arc};

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

use crate::read_float;

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
            "4-vector",
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
                        ExecutorError::Other(
                            "animation variable does not belong to executor state".into(),
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

#[derive(Debug)]
struct SlotDsu {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl SlotDsu {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
            rank: vec![0; len],
        }
    }

    fn find(&mut self, idx: usize) -> usize {
        let parent = self.parent[idx];
        if parent == idx {
            idx
        } else {
            let root = self.find(parent);
            self.parent[idx] = root;
            root
        }
    }

    fn union(&mut self, a: usize, b: usize) {
        let mut ra = self.find(a);
        let mut rb = self.find(b);
        if ra == rb {
            return;
        }
        if self.rank[ra] < self.rank[rb] {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb] = ra;
        if self.rank[ra] == self.rank[rb] {
            self.rank[ra] += 1;
        }
    }
}

fn decode_mesh_ref(value: i32) -> Option<usize> {
    (value < -1).then_some((-value - 2) as usize)
}

fn dot_slot(idx: usize) -> usize {
    idx
}

fn line_a_slot(mesh: &Mesh, idx: usize) -> usize {
    mesh.dots.len() + idx * 2
}

fn line_b_slot(mesh: &Mesh, idx: usize) -> usize {
    line_a_slot(mesh, idx) + 1
}

fn tri_a_slot(mesh: &Mesh, idx: usize) -> usize {
    mesh.dots.len() + mesh.lins.len() * 2 + idx * 3
}

fn tri_b_slot(mesh: &Mesh, idx: usize) -> usize {
    tri_a_slot(mesh, idx) + 1
}

fn tri_c_slot(mesh: &Mesh, idx: usize) -> usize {
    tri_a_slot(mesh, idx) + 2
}

fn tri_edge_slots(mesh: &Mesh, tri_idx: usize, edge_idx: usize) -> (usize, usize) {
    match edge_idx {
        0 => (tri_a_slot(mesh, tri_idx), tri_b_slot(mesh, tri_idx)),
        1 => (tri_b_slot(mesh, tri_idx), tri_c_slot(mesh, tri_idx)),
        _ => (tri_c_slot(mesh, tri_idx), tri_a_slot(mesh, tri_idx)),
    }
}

fn tri_edge_for(tri: &Tri, value: i32) -> Option<usize> {
    [tri.ab, tri.bc, tri.ca].iter().position(|edge| *edge == value)
}

fn shared_position_groups(mesh: &Mesh) -> Vec<usize> {
    let slot_count = mesh.dots.len() + mesh.lins.len() * 2 + mesh.tris.len() * 3;
    let mut dsu = SlotDsu::new(slot_count);

    for (idx, dot) in mesh.dots.iter().enumerate() {
        if dot.inv >= 0 {
            let inv = dot.inv as usize;
            if inv < mesh.dots.len() {
                dsu.union(dot_slot(idx), dot_slot(inv));
            }
        }
        if dot.anti >= 0 {
            let anti = dot.anti as usize;
            if anti < mesh.dots.len() {
                dsu.union(dot_slot(idx), dot_slot(anti));
            }
        }
    }

    for (idx, lin) in mesh.lins.iter().enumerate() {
        if lin.prev >= 0 {
            let prev = lin.prev as usize;
            if prev < mesh.lins.len() {
                dsu.union(line_a_slot(mesh, idx), line_b_slot(mesh, prev));
            }
        } else if let Some(dot_idx) = decode_mesh_ref(lin.prev).filter(|&i| i < mesh.dots.len()) {
            dsu.union(line_a_slot(mesh, idx), dot_slot(dot_idx));
        }

        if lin.next >= 0 {
            let next = lin.next as usize;
            if next < mesh.lins.len() {
                dsu.union(line_b_slot(mesh, idx), line_a_slot(mesh, next));
            }
        } else if let Some(dot_idx) = decode_mesh_ref(lin.next).filter(|&i| i < mesh.dots.len()) {
            dsu.union(line_b_slot(mesh, idx), dot_slot(dot_idx));
        }

        if lin.inv >= 0 {
            let inv = lin.inv as usize;
            if inv < mesh.lins.len() {
                dsu.union(line_a_slot(mesh, idx), line_b_slot(mesh, inv));
                dsu.union(line_b_slot(mesh, idx), line_a_slot(mesh, inv));
            }
        }

        if lin.anti >= 0 {
            let anti = lin.anti as usize;
            if anti < mesh.lins.len() {
                dsu.union(line_a_slot(mesh, idx), line_a_slot(mesh, anti));
                dsu.union(line_b_slot(mesh, idx), line_b_slot(mesh, anti));
            }
        }
    }

    for (tri_idx, tri) in mesh.tris.iter().enumerate() {
        if tri.anti >= 0 {
            let anti = tri.anti as usize;
            if anti < mesh.tris.len() {
                dsu.union(tri_a_slot(mesh, tri_idx), tri_b_slot(mesh, anti));
                dsu.union(tri_b_slot(mesh, tri_idx), tri_a_slot(mesh, anti));
                dsu.union(tri_c_slot(mesh, tri_idx), tri_c_slot(mesh, anti));
            }
        }

        for (edge_idx, value) in [tri.ab, tri.bc, tri.ca].into_iter().enumerate() {
            let (lhs, rhs) = tri_edge_slots(mesh, tri_idx, edge_idx);
            if value >= 0 {
                let neighbor = value as usize;
                if neighbor < mesh.tris.len() {
                    if let Some(other_edge) = tri_edge_for(&mesh.tris[neighbor], tri_idx as i32) {
                        let (na, nb) = tri_edge_slots(mesh, neighbor, other_edge);
                        dsu.union(lhs, nb);
                        dsu.union(rhs, na);
                    }
                }
            } else if let Some(line_idx) = decode_mesh_ref(value).filter(|&i| i < mesh.lins.len()) {
                dsu.union(lhs, line_a_slot(mesh, line_idx));
                dsu.union(rhs, line_b_slot(mesh, line_idx));
            }
        }
    }

    let mut roots = Vec::with_capacity(slot_count);
    let mut root_to_group = std::collections::HashMap::<usize, usize>::new();
    for slot in 0..slot_count {
        let root = dsu.find(slot);
        let next_group = root_to_group.len();
        let group = *root_to_group.entry(root).or_insert(next_group);
        roots.push(group);
    }
    roots
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

fn sample_positions(mesh: &Mesh) -> Vec<Float3> {
    if !mesh.lins.is_empty() {
        let mut points: Vec<_> = mesh.lins.iter().map(|lin| lin.a.pos).collect();
        if mesh.lins.last().is_some_and(|lin| lin.next < 0) {
            points.push(mesh.lins.last().unwrap().b.pos);
        }
        return points;
    }

    if !mesh.dots.is_empty() {
        return mesh.dots.iter().map(|dot| dot.pos).collect();
    }

    mesh_vertices(mesh).collect()
}

fn sample_colors(mesh: &Mesh) -> Vec<Float4> {
    if !mesh.lins.is_empty() {
        let mut colors: Vec<_> = mesh.lins.iter().map(|lin| lin.a.col).collect();
        if mesh.lins.last().is_some_and(|lin| lin.next < 0) {
            colors.push(mesh.lins.last().unwrap().b.col);
        }
        return colors;
    }

    if !mesh.dots.is_empty() {
        return mesh.dots.iter().map(|dot| dot.col).collect();
    }

    mesh.tris
        .iter()
        .flat_map(|tri| [tri.a.col, tri.b.col, tri.c.col])
        .collect()
}

pub(super) fn conform_mesh_to_target(source: Option<&Mesh>, target: &Mesh) -> Mesh {
    if let Some(source) = source {
        if source.dots.len() == target.dots.len()
            && source.lins.len() == target.lins.len()
            && source.tris.len() == target.tris.len()
        {
            return source.clone();
        }
    }

    let src_center = source
        .map(mesh_center)
        .unwrap_or_else(|| mesh_center(target));
    let src_positions = source.map(sample_positions).unwrap_or_default();
    let src_colors = source.map(sample_colors).unwrap_or_default();
    let src_alpha = source.map(|mesh| mesh.uniform.alpha).unwrap_or(0.0);

    let position_groups = shared_position_groups(target);
    let target_position_count = position_groups
        .iter()
        .copied()
        .max()
        .map(|group| group + 1)
        .unwrap_or(0);
    let target_vertex_count =
        target.dots.len() + target.lins.len() * 2 + target.tris.len() * 3;
    let sample_index = |i: usize, len: usize| {
        if len == 0 || target_vertex_count == 0 {
            0
        } else {
            (i * len) / target_vertex_count
        }
    };
    let sample_position_index = |slot: usize, len: usize| {
        if len == 0 || target_position_count == 0 {
            0
        } else {
            (position_groups[slot] * len) / target_position_count
        }
    };
    let pick_pos = |slot: usize| {
        src_positions
            .get(sample_position_index(slot, src_positions.len()))
            .copied()
            .unwrap_or(src_center)
    };
    let pick_col = |i: usize, fallback: Float4| {
        src_colors
            .get(sample_index(i, src_colors.len()))
            .copied()
            .unwrap_or(fallback)
    };

    let mut vertex_index = 0usize;
    let dots = target
        .dots
        .iter()
        .map(|dot| {
            let out =
                copy_dot_template(dot, pick_pos(vertex_index), pick_col(vertex_index, dot.col));
            vertex_index += 1;
            out
        })
        .collect();
    let lins = target
        .lins
        .iter()
        .map(|lin| {
            let out = copy_lin_template(
                lin,
                pick_pos(vertex_index),
                pick_pos(vertex_index + 1),
                pick_col(vertex_index, lin.a.col),
                pick_col(vertex_index + 1, lin.b.col),
            );
            vertex_index += 2;
            out
        })
        .collect();
    let tris = target
        .tris
        .iter()
        .map(|tri| {
            let out = copy_tri_template(
                tri,
                pick_pos(vertex_index),
                pick_pos(vertex_index + 1),
                pick_pos(vertex_index + 2),
                pick_col(vertex_index, tri.a.col),
                pick_col(vertex_index + 1, tri.b.col),
                pick_col(vertex_index + 2, tri.c.col),
            );
            vertex_index += 3;
            out
        })
        .collect();

    let mut mesh = target.clone();
    mesh.dots = dots;
    mesh.lins = lins;
    mesh.tris = tris;
    mesh.uniform.alpha = src_alpha;
    if let Some(source) = source {
        mesh.uniform.img = source.uniform.img.clone();
        mesh.uniform.z_index = source.uniform.z_index;
        mesh.uniform.fixed_in_frame = source.uniform.fixed_in_frame;
    }
    debug_assert!(mesh.has_consistent_topology());
    mesh
}

pub(super) fn rebuild_mesh_value_like(
    source_leaves: &[Arc<Mesh>],
    target: &Value,
) -> Result<Value, ExecutorError> {
    fn recurse(
        source_leaves: &[Arc<Mesh>],
        cursor: &mut usize,
        target: &Value,
    ) -> Result<Value, ExecutorError> {
        match target {
            Value::Mesh(target_mesh) => {
                let source = if source_leaves.is_empty() {
                    None
                } else {
                    let mesh = source_leaves[*cursor % source_leaves.len()].clone();
                    Some(mesh)
                };
                *cursor += 1;
                Ok(Value::Mesh(Arc::new(conform_mesh_to_target(
                    source.as_deref(),
                    target_mesh,
                ))))
            }
            Value::List(list) => {
                let mut out = Vec::with_capacity(list.len());
                for elem in list.elements() {
                    let elem = with_heap(|h| h.get(elem.key()).clone());
                    out.push(recurse(source_leaves, cursor, &elem)?);
                }
                Ok(list_value(out))
            }
            other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
        }
    }

    let mut cursor = 0;
    recurse(source_leaves, &mut cursor, target)
}

pub(super) fn rebuild_mesh_value_like_by_tag(
    source_by_tag: &HashMap<Vec<isize>, Arc<Mesh>>,
    source_fallback: &[Arc<Mesh>],
    target: &Value,
) -> Result<Value, ExecutorError> {
    fn recurse(
        source_by_tag: &HashMap<Vec<isize>, Arc<Mesh>>,
        source_fallback: &[Arc<Mesh>],
        cursor: &mut usize,
        target: &Value,
    ) -> Result<Value, ExecutorError> {
        match target {
            Value::Mesh(target_mesh) => {
                let source = source_by_tag.get(&target_mesh.tag).cloned().or_else(|| {
                    if source_fallback.is_empty() {
                        None
                    } else {
                        Some(source_fallback[*cursor % source_fallback.len()].clone())
                    }
                });
                *cursor += 1;
                Ok(Value::Mesh(Arc::new(conform_mesh_to_target(
                    source.as_deref(),
                    target_mesh,
                ))))
            }
            Value::List(list) => {
                let mut out = Vec::with_capacity(list.len());
                for elem in list.elements() {
                    let elem = with_heap(|h| h.get(elem.key()).clone());
                    out.push(recurse(source_by_tag, source_fallback, cursor, &elem)?);
                }
                Ok(list_value(out))
            }
            other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
        }
    }

    let mut cursor = 0;
    recurse(source_by_tag, source_fallback, &mut cursor, target)
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

pub(super) fn mesh_tag_map(value: &Value) -> Result<HashMap<Vec<isize>, Arc<Mesh>>, ExecutorError> {
    let mut leaves = Vec::new();
    flatten_mesh_leaves(value, &mut leaves)?;
    Ok(leaves
        .into_iter()
        .map(|mesh| (mesh.tag.clone(), mesh))
        .collect())
}
