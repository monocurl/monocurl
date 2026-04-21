use std::{collections::VecDeque, rc::Rc, sync::Arc};

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{
    mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
    simd::{Float2, Float3, Float4},
};
use stdlib_macros::stdlib_func;

use crate::mesh::helpers::{
    build_indexed_surface, mesh_position_groups, mesh_to_indexed_surface, push_closed_polyline,
    uprank_mesh,
};

use super::super::helpers::{self, list_value, materialize_live_value, mesh_center};
use super::{embed_triplet, lerp_uniforms, read_path_arc_value};

#[stdlib_func]
pub async fn trans_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let start = materialize_live_value(executor, &start_value).await?;
    let destination = materialize_live_value(executor, &destination_value).await?;
    let start_leaves = contour_separated_leaves(&start)?;
    let target_leaves = contour_separated_leaves(&destination)?;
    let (aligned, prepared_destination, state) = prepare_trans_value_like(
        &start_leaves,
        &target_leaves,
        matches!(destination, Value::Mesh(_))
            && start_leaves.len() <= 1
            && target_leaves.len() <= 1,
    )?;
    Ok(embed_triplet(aligned, prepared_destination, state))
}

#[stdlib_func]
pub async fn tag_trans_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let start = materialize_live_value(executor, &start_value).await?;
    let destination = materialize_live_value(executor, &destination_value).await?;
    let start_leaves = contour_separated_leaves(&start)?;
    let target_leaves = contour_separated_leaves(&destination)?;
    let (aligned, prepared_destination, state) = prepare_trans_value_like_by_tag(
        &start_leaves,
        &target_leaves,
        matches!(destination, Value::Mesh(_))
            && start_leaves.len() <= 1
            && target_leaves.len() <= 1,
    )?;
    Ok(embed_triplet(aligned, prepared_destination, state))
}

#[stdlib_func]
pub async fn trans_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start = executor.state.stack(stack_idx).read_at(-5).clone();
    let end = executor.state.stack(stack_idx).read_at(-4).clone();
    let state = executor.state.stack(stack_idx).read_at(-3).clone();
    let t = crate::read_float(executor, stack_idx, -2, "t")? as f32;
    let path_arc = read_path_arc_value(executor.state.stack(stack_idx).read_at(-1).clone())?;
    mesh_tree_trans_lerp(&start, &end, &state, t, path_arc)
}

fn planar_state(start_fill: Float4, end_fill: Float4) -> Value {
    list_value([
        Value::Integer(1),
        float4_value(start_fill),
        float4_value(end_fill),
    ])
}

fn float4_value(color: Float4) -> Value {
    list_value(
        color
            .to_array()
            .into_iter()
            .map(|component| Value::Float(component as f64)),
    )
}

fn read_planar_state(value: &Value) -> Result<Option<(Float4, Float4)>, ExecutorError> {
    match value {
        Value::Nil => Ok(None),
        Value::List(list) if list.len() == 3 => {
            let kind = executor::heap::with_heap(|h| h.get(list.elements()[0].key()).clone());
            match kind.elide_lvalue_leader_rec() {
                Value::Integer(1) => {
                    let start_fill = helpers::read_float4_value(
                        executor::heap::with_heap(|h| h.get(list.elements()[1].key()).clone()),
                        "state",
                    )?;
                    let end_fill = helpers::read_float4_value(
                        executor::heap::with_heap(|h| h.get(list.elements()[2].key()).clone()),
                        "state",
                    )?;
                    Ok(Some((start_fill, end_fill)))
                }
                _ => Ok(None),
            }
        }
        _ => Ok(None),
    }
}

#[derive(Clone)]
struct ClosedContour {
    points: Vec<Float3>,
    colors: Vec<Float4>,
    normal: Float3,
    signed_area: f32,
}

#[derive(Clone, Copy)]
enum MeshNode {
    Tri(usize),
    Lin(usize),
    Dot(usize),
}

#[derive(Clone)]
struct OrderedPath {
    points: Vec<Float3>,
    colors: Vec<Float4>,
    normal: Float3,
    closed: bool,
}

#[derive(Clone, Copy)]
struct FaceVisit {
    tri_idx: usize,
    incoming_edge: Option<usize>,
}

fn prepare_trans_value_like(
    source_leaves: &[Arc<Mesh>],
    target_leaves: &[Arc<Mesh>],
    prefer_single_mesh: bool,
) -> Result<(Value, Value, Value), ExecutorError> {
    let pairings = pair_leaf_indices_in_order(source_leaves.len(), target_leaves.len());
    build_prepared_trans_values(source_leaves, target_leaves, pairings, prefer_single_mesh)
}

fn prepare_trans_value_like_by_tag(
    source_leaves: &[Arc<Mesh>],
    target_leaves: &[Arc<Mesh>],
    prefer_single_mesh: bool,
) -> Result<(Value, Value, Value), ExecutorError> {
    let pairings = pair_leaf_indices_by_tag(source_leaves, &target_leaves);
    build_prepared_trans_values(source_leaves, target_leaves, pairings, prefer_single_mesh)
}

fn build_prepared_trans_values(
    source_leaves: &[Arc<Mesh>],
    target_leaves: &[Arc<Mesh>],
    pairings: Vec<(Option<usize>, Option<usize>)>,
    prefer_single_mesh: bool,
) -> Result<(Value, Value, Value), ExecutorError> {
    let mut starts = Vec::with_capacity(pairings.len());
    let mut ends = Vec::with_capacity(pairings.len());
    let mut states = Vec::with_capacity(pairings.len());

    for (source_idx, target_idx) in pairings {
        let source = source_idx.map(|idx| source_leaves[idx].as_ref());
        let target = target_idx.map(|idx| target_leaves[idx].as_ref());
        let (start, end, state) = prepare_trans_mesh_pair(source, target)?;
        starts.push(Value::Mesh(Arc::new(start)));
        ends.push(Value::Mesh(Arc::new(end)));
        states.push(state);
    }

    if prefer_single_mesh && starts.len() == 1 {
        return Ok((
            starts.pop().expect("single start"),
            ends.pop().expect("single end"),
            states.pop().expect("single state"),
        ));
    }

    Ok((list_value(starts), list_value(ends), list_value(states)))
}

fn pair_leaf_indices_in_order(
    source_len: usize,
    target_len: usize,
) -> Vec<(Option<usize>, Option<usize>)> {
    pair_index_groups(
        &(0..source_len).collect::<Vec<_>>(),
        &(0..target_len).collect::<Vec<_>>(),
    )
}

fn pair_leaf_indices_by_tag(
    source_leaves: &[Arc<Mesh>],
    target_leaves: &[Arc<Mesh>],
) -> Vec<(Option<usize>, Option<usize>)> {
    let mut source_order: Vec<_> = (0..source_leaves.len()).collect();
    source_order.sort_by(|&lhs, &rhs| {
        source_leaves[lhs]
            .tag
            .cmp(&source_leaves[rhs].tag)
            .then(lhs.cmp(&rhs))
    });

    let mut target_order: Vec<_> = (0..target_leaves.len()).collect();
    target_order.sort_by(|&lhs, &rhs| {
        target_leaves[lhs]
            .tag
            .cmp(&target_leaves[rhs].tag)
            .then(lhs.cmp(&rhs))
    });

    let mut out = Vec::new();
    let mut source_cursor = 0usize;
    let mut target_cursor = 0usize;
    while source_cursor < source_order.len() || target_cursor < target_order.len() {
        let source_tag = source_order
            .get(source_cursor)
            .map(|&idx| source_leaves[idx].tag.as_slice());
        let target_tag = target_order
            .get(target_cursor)
            .map(|&idx| target_leaves[idx].tag.as_slice());

        let ordering = match (source_tag, target_tag) {
            (Some(source_tag), Some(target_tag)) => source_tag.cmp(target_tag),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => break,
        };

        match ordering {
            std::cmp::Ordering::Less => {
                let source_end = tag_group_end(source_leaves, &source_order, source_cursor);
                out.extend(pair_index_groups(
                    &source_order[source_cursor..source_end],
                    &[],
                ));
                source_cursor = source_end;
            }
            std::cmp::Ordering::Greater => {
                let target_end = tag_group_end(target_leaves, &target_order, target_cursor);
                out.extend(pair_index_groups(
                    &[],
                    &target_order[target_cursor..target_end],
                ));
                target_cursor = target_end;
            }
            std::cmp::Ordering::Equal => {
                let source_end = tag_group_end(source_leaves, &source_order, source_cursor);
                let target_end = tag_group_end(target_leaves, &target_order, target_cursor);
                out.extend(pair_index_groups(
                    &source_order[source_cursor..source_end],
                    &target_order[target_cursor..target_end],
                ));
                source_cursor = source_end;
                target_cursor = target_end;
            }
        }
    }

    out
}

fn tag_group_end(leaves: &[Arc<Mesh>], order: &[usize], start: usize) -> usize {
    let tag = leaves[order[start]].tag.as_slice();
    let mut end = start + 1;
    while end < order.len() && leaves[order[end]].tag.as_slice() == tag {
        end += 1;
    }
    end
}

fn pair_index_groups(
    source_group: &[usize],
    target_group: &[usize],
) -> Vec<(Option<usize>, Option<usize>)> {
    if source_group.len() >= target_group.len() {
        source_group
            .iter()
            .enumerate()
            .map(|(i, &source_idx)| {
                (
                    Some(source_idx),
                    distributed_group_index(target_group, i, source_group.len()),
                )
            })
            .collect()
    } else {
        target_group
            .iter()
            .enumerate()
            .map(|(i, &target_idx)| {
                (
                    distributed_group_index(source_group, i, target_group.len()),
                    Some(target_idx),
                )
            })
            .collect()
    }
}

fn distributed_group_index(group: &[usize], large_idx: usize, large_len: usize) -> Option<usize> {
    if group.is_empty() {
        None
    } else {
        Some(group[(large_idx * group.len()) / large_len])
    }
}

fn mesh_ref(idx: usize) -> i32 {
    -2 - idx as i32
}

pub(super) fn contour_separated_leaves(value: &Value) -> Result<Vec<Arc<Mesh>>, ExecutorError> {
    let mut out = Vec::new();
    contour_separated_leaves_into(value, &mut out)?;
    Ok(out)
}

fn contour_separated_leaves_into(
    value: &Value,
    out: &mut Vec<Arc<Mesh>>,
) -> Result<(), ExecutorError> {
    match value {
        Value::Mesh(mesh) => {
            out.extend(split_mesh_contours(mesh.as_ref()));
            Ok(())
        }
        Value::List(list) => {
            for elem in list.elements() {
                let elem = executor::heap::with_heap(|h| h.get(elem.key()).clone());
                contour_separated_leaves_into(&elem, out)?;
            }
            Ok(())
        }
        other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
    }
}

fn split_mesh_contours(mesh: &Mesh) -> Vec<Arc<Mesh>> {
    if !mesh_has_content(mesh) {
        return vec![Arc::new(mesh.clone())];
    }

    let mut seen_tris = vec![false; mesh.tris.len()];
    let mut seen_lins = vec![false; mesh.lins.len()];
    let mut seen_dots = vec![false; mesh.dots.len()];
    let mut out = Vec::new();

    for tri_idx in 0..mesh.tris.len() {
        if seen_tris[tri_idx] {
            continue;
        }
        out.push(Arc::new(extract_component_mesh(
            mesh,
            MeshNode::Tri(tri_idx),
            &mut seen_tris,
            &mut seen_lins,
            &mut seen_dots,
        )));
    }

    for line_idx in 0..mesh.lins.len() {
        if seen_lins[line_idx] {
            continue;
        }
        out.push(Arc::new(extract_component_mesh(
            mesh,
            MeshNode::Lin(line_idx),
            &mut seen_tris,
            &mut seen_lins,
            &mut seen_dots,
        )));
    }

    for dot_idx in 0..mesh.dots.len() {
        if seen_dots[dot_idx] {
            continue;
        }
        out.push(Arc::new(extract_component_mesh(
            mesh,
            MeshNode::Dot(dot_idx),
            &mut seen_tris,
            &mut seen_lins,
            &mut seen_dots,
        )));
    }

    if out.is_empty() {
        out.push(Arc::new(mesh.clone()));
    }
    out
}

fn extract_component_mesh(
    mesh: &Mesh,
    start: MeshNode,
    seen_tris: &mut [bool],
    seen_lins: &mut [bool],
    seen_dots: &mut [bool],
) -> Mesh {
    let mut stack = vec![start];
    let mut tri_indices = Vec::new();
    let mut line_indices = Vec::new();
    let mut dot_indices = Vec::new();

    while let Some(node) = stack.pop() {
        match node {
            MeshNode::Tri(idx) => {
                if seen_tris[idx] {
                    continue;
                }
                seen_tris[idx] = true;
                tri_indices.push(idx);

                let tri = mesh.tris[idx];
                if tri.anti >= 0 {
                    stack.push(MeshNode::Tri(tri.anti as usize));
                }
                for edge in [tri.ab, tri.bc, tri.ca] {
                    if edge >= 0 {
                        stack.push(MeshNode::Tri(edge as usize));
                    } else if let Some(line_idx) = decode_mesh_ref(edge) {
                        if line_idx < mesh.lins.len() {
                            stack.push(MeshNode::Lin(line_idx));
                        }
                    }
                }
            }
            MeshNode::Lin(idx) => {
                if seen_lins[idx] {
                    continue;
                }
                seen_lins[idx] = true;
                line_indices.push(idx);

                let line = mesh.lins[idx];
                if line.anti >= 0 {
                    stack.push(MeshNode::Lin(line.anti as usize));
                }
                if line.inv >= 0 {
                    stack.push(MeshNode::Lin(line.inv as usize));
                } else if let Some(tri_idx) = decode_mesh_ref(line.inv) {
                    if tri_idx < mesh.tris.len() {
                        stack.push(MeshNode::Tri(tri_idx));
                    }
                }

                for endpoint in [line.prev, line.next] {
                    if endpoint >= 0 {
                        stack.push(MeshNode::Lin(endpoint as usize));
                    } else if let Some(dot_idx) = decode_mesh_ref(endpoint) {
                        if dot_idx < mesh.dots.len() {
                            stack.push(MeshNode::Dot(dot_idx));
                        }
                    }
                }
            }
            MeshNode::Dot(idx) => {
                if seen_dots[idx] {
                    continue;
                }
                seen_dots[idx] = true;
                dot_indices.push(idx);

                let dot = mesh.dots[idx];
                if dot.anti >= 0 {
                    stack.push(MeshNode::Dot(dot.anti as usize));
                }
                if dot.inv >= 0 {
                    stack.push(MeshNode::Dot(dot.inv as usize));
                } else if let Some(line_idx) = decode_mesh_ref(dot.inv) {
                    if line_idx < mesh.lins.len() {
                        stack.push(MeshNode::Lin(line_idx));
                    }
                }
            }
        }
    }

    let tri_map = build_index_map(mesh.tris.len(), &tri_indices);
    let line_map = build_index_map(mesh.lins.len(), &line_indices);
    let dot_map = build_index_map(mesh.dots.len(), &dot_indices);

    let mut dots: Vec<_> = dot_indices.iter().map(|&idx| mesh.dots[idx]).collect();
    let mut lins: Vec<_> = line_indices.iter().map(|&idx| mesh.lins[idx]).collect();
    let mut tris: Vec<_> = tri_indices.iter().map(|&idx| mesh.tris[idx]).collect();

    for dot in &mut dots {
        dot.inv = remap_dot_ref(dot.inv, &dot_map, &line_map);
        dot.anti = remap_index_ref(dot.anti, &dot_map);
    }

    for line in &mut lins {
        line.prev = remap_mesh_ref(line.prev, &line_map, &dot_map);
        line.next = remap_mesh_ref(line.next, &line_map, &dot_map);
        line.inv = remap_mesh_ref(line.inv, &line_map, &tri_map);
        line.anti = remap_index_ref(line.anti, &line_map);
    }

    for tri in &mut tris {
        tri.ab = remap_mesh_ref(tri.ab, &tri_map, &line_map);
        tri.bc = remap_mesh_ref(tri.bc, &tri_map, &line_map);
        tri.ca = remap_mesh_ref(tri.ca, &tri_map, &line_map);
        tri.anti = remap_index_ref(tri.anti, &tri_map);
    }

    let component = Mesh {
        dots,
        lins,
        tris,
        uniform: mesh.uniform.clone(),
        tag: mesh.tag.clone(),
    };
    debug_assert!(component.has_consistent_topology());
    component
}

fn build_index_map(len: usize, indices: &[usize]) -> Vec<Option<usize>> {
    let mut out = vec![None; len];
    for (new_idx, &old_idx) in indices.iter().enumerate() {
        out[old_idx] = Some(new_idx);
    }
    out
}

fn decode_mesh_ref(value: i32) -> Option<usize> {
    (value < -1).then_some((-value - 2) as usize)
}

fn remap_index_ref(value: i32, map: &[Option<usize>]) -> i32 {
    if value < 0 {
        return value;
    }
    map.get(value as usize)
        .and_then(|entry| *entry)
        .map(|idx| idx as i32)
        .unwrap_or(-1)
}

fn remap_dot_ref(value: i32, dot_map: &[Option<usize>], line_map: &[Option<usize>]) -> i32 {
    if value >= 0 {
        return remap_index_ref(value, dot_map);
    }
    decode_mesh_ref(value)
        .and_then(|idx| line_map.get(idx).and_then(|entry| *entry))
        .map(mesh_ref)
        .unwrap_or(-1)
}

fn remap_mesh_ref(value: i32, direct_map: &[Option<usize>], mesh_map: &[Option<usize>]) -> i32 {
    if value >= 0 {
        return remap_index_ref(value, direct_map);
    }
    decode_mesh_ref(value)
        .and_then(|idx| mesh_map.get(idx).and_then(|entry| *entry))
        .map(mesh_ref)
        .unwrap_or(-1)
}

fn prepare_trans_mesh_pair(
    source: Option<&Mesh>,
    target: Option<&Mesh>,
) -> Result<(Mesh, Mesh, Value), ExecutorError> {
    match (source, target) {
        (Some(source), Some(target)) if mesh_has_content(source) && mesh_has_content(target) => {
            if same_mesh_topology(source, target) {
                return Ok((source.clone(), target.clone(), Value::Nil));
            }

            if let Some((start, end, state)) = prepare_planar_trans_mesh_pair(source, target)? {
                return Ok((start, end, state));
            }

            let prepared = match (mesh_rank(source), mesh_rank(target)) {
                (2, 2) => match_tri_tri(source, target),
                (2, 1) => match_tri_lin(source, target)?,
                (1, 2) => {
                    let (end, start) = match_tri_lin(target, source)?;
                    (start, end)
                }
                (1, 1) => match_lin_lin(source, target),
                (2, 0) => match_tri_dot(source, target),
                (0, 2) => {
                    let (end, start) = match_tri_dot(target, source);
                    (start, end)
                }
                (1, 0) => match_lin_dot(source, target),
                (0, 1) => {
                    let (end, start) = match_lin_dot(target, source);
                    (start, end)
                }
                (0, 0) => match_dot_dot(source, target),
                _ => unreachable!("contentful meshes must have a non-negative rank"),
            };

            Ok((prepared.0, prepared.1, Value::Nil))
        }
        (Some(source), Some(target)) if mesh_has_content(source) => {
            Ok((source.clone(), zero_alpha_mesh(source), Value::Nil))
        }
        (Some(source), Some(target)) if mesh_has_content(target) => {
            Ok((zero_alpha_mesh(target), target.clone(), Value::Nil))
        }
        (Some(source), Some(target)) => {
            Ok((zero_alpha_mesh(source), zero_alpha_mesh(target), Value::Nil))
        }
        (Some(source), None) => Ok((source.clone(), zero_alpha_mesh(source), Value::Nil)),
        (None, Some(target)) => Ok((zero_alpha_mesh(target), target.clone(), Value::Nil)),
        (None, None) => unreachable!("trans pairing must contain at least one mesh"),
    }
}

fn mesh_has_content(mesh: &Mesh) -> bool {
    !mesh.dots.is_empty() || !mesh.lins.is_empty() || !mesh.tris.is_empty()
}

fn zero_alpha_mesh(mesh: &Mesh) -> Mesh {
    let mut out = mesh.clone();
    out.uniform.alpha = 0.0;
    out
}

fn mesh_rank(mesh: &Mesh) -> i32 {
    if !mesh.tris.is_empty() {
        2
    } else if !mesh.lins.is_empty() {
        1
    } else if !mesh.dots.is_empty() {
        0
    } else {
        -1
    }
}

fn gcd_usize(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let rem = a % b;
        a = b;
        b = rem;
    }
    a
}

fn lcm_usize(a: usize, b: usize) -> usize {
    if a == 0 || b == 0 {
        0
    } else {
        a / gcd_usize(a, b) * b
    }
}

fn same_mesh_topology(start: &Mesh, end: &Mesh) -> bool {
    start.dots.len() == end.dots.len()
        && start.lins.len() == end.lins.len()
        && start.tris.len() == end.tris.len()
        && start
            .dots
            .iter()
            .zip(&end.dots)
            .all(|(a, b)| (a.inv, a.anti, a.is_dom_sib) == (b.inv, b.anti, b.is_dom_sib))
        && start.lins.iter().zip(&end.lins).all(|(a, b)| {
            (a.prev, a.next, a.inv, a.anti, a.is_dom_sib)
                == (b.prev, b.next, b.inv, b.anti, b.is_dom_sib)
        })
        && start.tris.iter().zip(&end.tris).all(|(a, b)| {
            (a.ab, a.bc, a.ca, a.anti, a.is_dom_sib) == (b.ab, b.bc, b.ca, b.anti, b.is_dom_sib)
        })
}

fn prepare_planar_trans_mesh_pair(
    start: &Mesh,
    end: &Mesh,
) -> Result<Option<(Mesh, Mesh, Value)>, ExecutorError> {
    let start_contours = match extract_closed_contours(start) {
        Some(contours) => contours,
        None => return Ok(None),
    };
    let end_contours = match extract_closed_contours(end) {
        Some(contours) => contours,
        None => return Ok(None),
    };

    let Some(start_fill) = planar_fill_color(start) else {
        return Ok(None);
    };
    let Some(end_fill) = planar_fill_color(end) else {
        return Ok(None);
    };

    let loop_count = lcm_usize(start_contours.len(), end_contours.len());
    if loop_count == 0 {
        return Ok(None);
    }

    let mut start_mesh = Mesh {
        dots: Vec::new(),
        lins: Vec::new(),
        tris: Vec::new(),
        uniform: start.uniform.clone(),
        tag: start.tag.clone(),
    };
    let mut end_mesh = Mesh {
        dots: Vec::new(),
        lins: Vec::new(),
        tris: Vec::new(),
        uniform: end.uniform.clone(),
        tag: end.tag.clone(),
    };

    for i in 0..loop_count {
        let start_contour = &start_contours[i % start_contours.len()];
        let end_contour = &end_contours[i % end_contours.len()];
        let (sampled_start, sampled_end) = align_closed_contours(start_contour, end_contour);
        append_closed_contour(&mut start_mesh, &sampled_start);
        append_closed_contour(&mut end_mesh, &sampled_end);
    }

    debug_assert!(start_mesh.has_consistent_topology());
    debug_assert!(end_mesh.has_consistent_topology());
    Ok(Some((
        start_mesh,
        end_mesh,
        planar_state(start_fill, end_fill),
    )))
}

fn extract_closed_contours(mesh: &Mesh) -> Option<Vec<ClosedContour>> {
    if mesh.lins.is_empty()
        || mesh.dots.len() != 0
        || mesh.lins.iter().any(|lin| lin.prev < 0 || lin.next < 0)
    {
        return None;
    }

    let mut visited = vec![false; mesh.lins.len()];
    let mut contours = Vec::new();
    for start in 0..mesh.lins.len() {
        if visited[start] {
            continue;
        }

        let mut points = Vec::new();
        let mut colors = Vec::new();
        let mut cursor = start;
        loop {
            if visited[cursor] {
                return None;
            }

            let line = mesh.lins[cursor];
            visited[cursor] = true;
            points.push(line.a.pos);
            colors.push(line.a.col);

            let next = line.next as usize;
            if next >= mesh.lins.len()
                || mesh.lins[next].prev != cursor as i32
                || mesh.lins[next].a.pos != line.b.pos
            {
                return None;
            }

            cursor = next;
            if cursor == start {
                break;
            }
        }

        let normal = mesh.lins[start].norm;
        contours.push(ClosedContour {
            signed_area: signed_contour_area(&points, normal),
            points,
            colors,
            normal,
        });
    }

    Some(contours)
}

fn planar_fill_color(mesh: &Mesh) -> Option<Float4> {
    if mesh.tris.is_empty() {
        return Some(Float4::ZERO);
    }

    let color = mesh.tris.first()?.a.col;
    mesh.tris
        .iter()
        .all(|tri| tri.a.col == color && tri.b.col == color && tri.c.col == color)
        .then_some(color)
}

fn align_closed_contours(
    start: &ClosedContour,
    end: &ClosedContour,
) -> (ClosedContour, ClosedContour) {
    let start_is_large = start.points.len() >= end.points.len();
    let (mut large, mut small) = if start_is_large {
        (start.clone(), end.clone())
    } else {
        (end.clone(), start.clone())
    };

    if large.signed_area * small.signed_area < 0.0 {
        reverse_closed_contour(&mut small);
    }

    let large_pivot = choose_anchor_edge(&large, &small);
    let small_pivot = best_matching_edge(&large, &small, large_pivot);
    rotate_vec(&mut large.points, large_pivot);
    rotate_vec(&mut large.colors, large_pivot);
    rotate_vec(&mut small.points, small_pivot);
    rotate_vec(&mut small.colors, small_pivot);

    let sampled_small = split_closed_contour_to_count(&small, large.points.len().max(3));

    if start_is_large {
        (large, sampled_small)
    } else {
        (sampled_small, large)
    }
}

fn reverse_closed_contour(contour: &mut ClosedContour) {
    contour.points.reverse();
    contour.colors.reverse();
    contour.signed_area = -contour.signed_area;
}

fn split_closed_contour_to_count(contour: &ClosedContour, target_segments: usize) -> ClosedContour {
    if contour.points.len() == target_segments {
        return contour.clone();
    }
    if contour.points.is_empty() {
        return contour.clone();
    }

    let source_segments = contour.points.len();
    let mut points = Vec::with_capacity(target_segments);
    let mut colors = Vec::with_capacity(target_segments);
    let mut source_segment = 0usize;
    for target_segment in 0..target_segments {
        while source_segment + 1 < source_segments
            && target_segment >= target_segments * (source_segment + 1) / source_segments
        {
            source_segment += 1;
        }
        let start = target_segments * source_segment / source_segments;
        let end = target_segments * (source_segment + 1) / source_segments;
        let denom = (end.saturating_sub(start)).max(1);
        let local = (target_segment.saturating_sub(start)) as f32 / denom as f32;
        let next = (source_segment + 1) % source_segments;
        points.push(contour.points[source_segment].lerp(contour.points[next], local));
        colors.push(contour.colors[source_segment].lerp(contour.colors[next], local));
    }

    ClosedContour {
        signed_area: signed_contour_area(&points, contour.normal),
        points,
        colors,
        normal: contour.normal,
    }
}

fn rotate_vec<T>(values: &mut [T], shift: usize) {
    if !values.is_empty() {
        values.rotate_left(shift % values.len());
    }
}

fn contour_center(contour: &ClosedContour) -> Float3 {
    let sum = contour
        .points
        .iter()
        .copied()
        .fold(Float3::ZERO, |acc, point| acc + point);
    sum / contour.points.len().max(1) as f32
}

fn contour_edge_midpoint(contour: &ClosedContour, idx: usize) -> Float3 {
    (contour.points[idx] + contour.points[(idx + 1) % contour.points.len()]) / 2.0
}

fn contour_edge_direction(contour: &ClosedContour, idx: usize) -> Float3 {
    let direction = contour.points[(idx + 1) % contour.points.len()] - contour.points[idx];
    let len = direction.len();
    if len <= 1e-6 {
        Float3::ZERO
    } else {
        direction / len
    }
}

fn choose_anchor_edge(anchor: &ClosedContour, other: &ClosedContour) -> usize {
    let delta = contour_center(anchor) - contour_center(other);
    if delta.len_sq() <= 1e-6 {
        return 0;
    }

    (0..anchor.points.len())
        .max_by(|&lhs, &rhs| {
            contour_edge_midpoint(anchor, lhs)
                .dot(delta)
                .partial_cmp(&contour_edge_midpoint(anchor, rhs).dot(delta))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0)
}

fn best_matching_edge(
    anchor: &ClosedContour,
    candidate: &ClosedContour,
    anchor_edge: usize,
) -> usize {
    const TRANSLATION_BIAS: f32 = 20.0;

    let unit = contour_edge_direction(anchor, anchor_edge);
    let comp_point = contour_edge_midpoint(anchor, anchor_edge);
    (0..candidate.points.len())
        .min_by(|&lhs, &rhs| {
            contour_edge_match_cost(candidate, lhs, unit, comp_point, TRANSLATION_BIAS)
                .partial_cmp(&contour_edge_match_cost(
                    candidate,
                    rhs,
                    unit,
                    comp_point,
                    TRANSLATION_BIAS,
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0)
}

fn contour_edge_match_cost(
    contour: &ClosedContour,
    edge_idx: usize,
    unit: Float3,
    comp_point: Float3,
    translation_bias: f32,
) -> f32 {
    let candidate_unit = contour_edge_direction(contour, edge_idx);
    let midpoint = contour_edge_midpoint(contour, edge_idx);
    (translation_bias - unit.dot(candidate_unit)) * (midpoint - comp_point).len()
}

fn append_closed_contour(mesh: &mut Mesh, contour: &ClosedContour) {
    let range = push_closed_polyline(&mut mesh.lins, &contour.points, contour.normal);
    for i in 0..contour.points.len() {
        let line = &mut mesh.lins[range.start + i];
        line.a.col = contour.colors[i];
        line.b.col = contour.colors[(i + 1) % contour.colors.len()];
        line.norm = contour.normal;
    }
}

fn signed_contour_area(points: &[Float3], normal: Float3) -> f32 {
    let mut area = 0.0;
    for i in 0..points.len() {
        area += points[i].cross(points[(i + 1) % points.len()]).dot(normal);
    }
    area * 0.5
}

fn mesh_vertex_samples(mesh: &Mesh) -> Vec<(Float3, Float4)> {
    let mut samples =
        Vec::with_capacity(mesh.dots.len() + mesh.lins.len() * 2 + mesh.tris.len() * 3);
    samples.extend(mesh.dots.iter().map(|dot| (dot.pos, dot.col)));
    samples.extend(
        mesh.lins
            .iter()
            .flat_map(|line| [(line.a.pos, line.a.col), (line.b.pos, line.b.col)]),
    );
    samples.extend(mesh.tris.iter().flat_map(|tri| {
        [
            (tri.a.pos, tri.a.col),
            (tri.b.pos, tri.b.col),
            (tri.c.pos, tri.c.col),
        ]
    }));
    samples
}

fn conform_samples_to_template(
    samples: &[(Float3, Float4)],
    uniform: &Uniforms,
    tag: &[isize],
    template: &Mesh,
) -> Mesh {
    if samples.is_empty() {
        return conform_constant_to_template(
            mesh_center(template),
            Float4::ZERO,
            uniform,
            tag,
            template,
        );
    }

    let groups = mesh_position_groups(template);
    let group_count = groups
        .iter()
        .copied()
        .max()
        .map(|group| group + 1)
        .unwrap_or(0)
        .max(1);
    let sample_for_group = |group: usize| {
        let idx = (group * samples.len()) / group_count;
        idx.min(samples.len() - 1)
    };

    let mut dots = template.dots.clone();
    let mut slot = 0usize;
    for dot in &mut dots {
        let (pos, col) = samples[sample_for_group(groups[slot])];
        dot.pos = pos;
        dot.col = col;
        slot += 1;
    }

    if !template.tris.is_empty() {
        let mut surface = mesh_to_indexed_surface(template);
        for (group, vertex) in surface.vertices.iter_mut().enumerate() {
            let (pos, col) = samples[sample_for_group(group)];
            vertex.pos = pos;
            vertex.col = col;
            vertex.uv = Float2::ZERO;
        }
        let (lins, tris) =
            build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
        let out = Mesh {
            dots,
            lins,
            tris,
            uniform: uniform.clone(),
            tag: tag.to_vec(),
        };
        debug_assert!(out.has_consistent_topology());
        return out;
    }

    let mut out = template.clone();
    out.dots = dots;
    out.uniform = uniform.clone();
    out.tag = tag.to_vec();
    for line in &mut out.lins {
        let (a_pos, a_col) = samples[sample_for_group(groups[slot])];
        let (b_pos, b_col) = samples[sample_for_group(groups[slot + 1])];
        line.a.pos = a_pos;
        line.a.col = a_col;
        line.b.pos = b_pos;
        line.b.col = b_col;
        slot += 2;
    }
    debug_assert!(out.has_consistent_topology());
    out
}

fn conform_line_mesh_to_template(source: &Mesh, template: &Mesh) -> Mesh {
    if let Some(path) = ordered_path_from_mesh(source) {
        conform_path_to_template(&path, template, &source.uniform, &source.tag)
    } else {
        conform_samples_to_template(
            &mesh_vertex_samples(source),
            &source.uniform,
            &source.tag,
            template,
        )
    }
}

fn match_tri_tri(source: &Mesh, target: &Mesh) -> (Mesh, Mesh) {
    if surface_template_score(source) >= surface_template_score(target) {
        let template = canonicalize_surface_template(source);
        (
            template.clone(),
            conform_surface_to_template(target, &template),
        )
    } else {
        let template = canonicalize_surface_template(target);
        (
            conform_surface_to_template(source, &template),
            template,
        )
    }
}

fn match_tri_lin(surface: &Mesh, line: &Mesh) -> Result<(Mesh, Mesh), ExecutorError> {
    if let Some(mut upranked) = uprank_mesh(line)? {
        for tri in &mut upranked.tris {
            tri.a.col = Float4::ZERO;
            tri.b.col = Float4::ZERO;
            tri.c.col = Float4::ZERO;
        }
        if !upranked.tris.is_empty() {
            return Ok(match_tri_tri(surface, &upranked));
        }
    }
    Ok((
        surface.clone(),
        conform_line_mesh_to_template(line, surface),
    ))
}

fn match_lin_lin(source: &Mesh, target: &Mesh) -> (Mesh, Mesh) {
    match (
        ordered_path_from_mesh(source),
        ordered_path_from_mesh(target),
    ) {
        (Some(source_path), Some(target_path)) if source_path.closed && target_path.closed => {
            let (start, end) = align_closed_paths(&source_path, &target_path);
            (
                mesh_from_ordered_path(&start, &source.uniform, &source.tag),
                mesh_from_ordered_path(&end, &target.uniform, &target.tag),
            )
        }
        (Some(source_path), Some(target_path)) if !source_path.closed && !target_path.closed => {
            let (start, end) = align_open_paths(&source_path, &target_path);
            (
                mesh_from_ordered_path(&start, &source.uniform, &source.tag),
                mesh_from_ordered_path(&end, &target.uniform, &target.tag),
            )
        }
        (Some(source_path), Some(target_path)) => {
            if source_path.points.len() >= target_path.points.len() {
                (
                    source.clone(),
                    conform_path_to_template(&target_path, source, &target.uniform, &target.tag),
                )
            } else {
                (
                    conform_path_to_template(&source_path, target, &source.uniform, &source.tag),
                    target.clone(),
                )
            }
        }
        _ if source.lins.len() >= target.lins.len() => (
            source.clone(),
            conform_line_mesh_to_template(target, source),
        ),
        _ => (
            conform_line_mesh_to_template(source, target),
            target.clone(),
        ),
    }
}

fn match_tri_dot(surface: &Mesh, dot: &Mesh) -> (Mesh, Mesh) {
    (
        surface.clone(),
        conform_constant_to_template(
            representative_point(dot),
            representative_color(dot),
            &dot.uniform,
            &dot.tag,
            surface,
        ),
    )
}

fn match_lin_dot(line: &Mesh, dot: &Mesh) -> (Mesh, Mesh) {
    let end = ordered_path_from_mesh(line)
        .map(|path| {
            conform_constant_to_template(
                representative_point(dot),
                representative_color(dot),
                &dot.uniform,
                &dot.tag,
                &mesh_from_ordered_path(&path, &line.uniform, &line.tag),
            )
        })
        .unwrap_or_else(|| {
            conform_constant_to_template(
                representative_point(dot),
                representative_color(dot),
                &dot.uniform,
                &dot.tag,
                line,
            )
        });
    (line.clone(), end)
}

fn match_dot_dot(source: &Mesh, target: &Mesh) -> (Mesh, Mesh) {
    if same_mesh_topology(source, target) {
        return (source.clone(), target.clone());
    }
    if source.dots.len() >= target.dots.len() {
        (
            source.clone(),
            conform_constant_to_template(
                representative_point(target),
                representative_color(target),
                &target.uniform,
                &target.tag,
                source,
            ),
        )
    } else {
        (
            conform_constant_to_template(
                representative_point(source),
                representative_color(source),
                &source.uniform,
                &source.tag,
                target,
            ),
            target.clone(),
        )
    }
}

fn surface_template_score(mesh: &Mesh) -> (usize, usize, usize) {
    (mesh.tris.len(), mesh.lins.len(), mesh.dots.len())
}

fn canonicalize_surface_template(mesh: &Mesh) -> Mesh {
    let surface = mesh_to_indexed_surface(mesh);
    let (lins, tris) = build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
    let out = Mesh {
        dots: mesh.dots.clone(),
        lins,
        tris,
        uniform: mesh.uniform.clone(),
        tag: mesh.tag.clone(),
    };
    debug_assert!(out.has_consistent_topology());
    out
}

fn representative_point(mesh: &Mesh) -> Float3 {
    mesh.dots
        .first()
        .map(|dot| dot.pos)
        .or_else(|| {
            mesh.lins
                .first()
                .map(|line| (line.a.pos + line.b.pos) / 2.0)
        })
        .or_else(|| mesh.tris.first().map(triangle_centroid))
        .unwrap_or_else(|| mesh_center(mesh))
}

fn representative_color(mesh: &Mesh) -> Float4 {
    mesh.dots
        .first()
        .map(|dot| dot.col)
        .or_else(|| mesh.lins.first().map(|line| line.a.col))
        .or_else(|| mesh.tris.first().map(|tri| tri.a.col))
        .unwrap_or(Float4::ONE)
}

fn triangle_centroid(tri: &Tri) -> Float3 {
    (tri.a.pos + tri.b.pos + tri.c.pos) / 3.0
}

fn tri_edge_value(tri: &Tri, edge_idx: usize) -> i32 {
    match edge_idx {
        0 => tri.ab,
        1 => tri.bc,
        2 => tri.ca,
        _ => unreachable!(),
    }
}

fn tri_edge_positions_with_color(tri: &Tri, edge_idx: usize) -> (Float3, Float3, Float4, Float4) {
    match edge_idx {
        0 => (tri.a.pos, tri.b.pos, tri.a.col, tri.b.col),
        1 => (tri.b.pos, tri.c.pos, tri.b.col, tri.c.col),
        2 => (tri.c.pos, tri.a.pos, tri.c.col, tri.a.col),
        _ => unreachable!(),
    }
}

fn tri_edge_for_neighbor(tri: &Tri, neighbor: i32) -> Option<usize> {
    [tri.ab, tri.bc, tri.ca]
        .iter()
        .position(|edge| *edge == neighbor)
}

fn triangle_bfs_visits(mesh: &Mesh) -> Vec<FaceVisit> {
    if mesh.tris.is_empty() {
        return Vec::new();
    }

    let centroid_sum = mesh
        .tris
        .iter()
        .map(triangle_centroid)
        .fold(Float3::ZERO, |acc, center| acc + center);
    let centroid = centroid_sum / mesh.tris.len() as f32;
    let pivot = mesh
        .tris
        .iter()
        .enumerate()
        .min_by(|(_, lhs), (_, rhs)| {
            (triangle_centroid(lhs) - centroid)
                .len_sq()
                .partial_cmp(&(triangle_centroid(rhs) - centroid).len_sq())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(idx, _)| idx)
        .unwrap_or(0);

    let mut seen = vec![false; mesh.tris.len()];
    let mut queue = VecDeque::from([FaceVisit {
        tri_idx: pivot,
        incoming_edge: None,
    }]);
    let mut visits = Vec::with_capacity(mesh.tris.len());

    while let Some(visit) = queue.pop_front() {
        if seen[visit.tri_idx] {
            continue;
        }
        seen[visit.tri_idx] = true;
        visits.push(visit);

        let tri = &mesh.tris[visit.tri_idx];
        for edge_idx in 0..3 {
            let neighbor = tri_edge_value(tri, edge_idx);
            if neighbor < 0 {
                continue;
            }
            let neighbor = neighbor as usize;
            if seen[neighbor] {
                continue;
            }
            queue.push_back(FaceVisit {
                tri_idx: neighbor,
                incoming_edge: tri_edge_for_neighbor(&mesh.tris[neighbor], visit.tri_idx as i32),
            });
        }
    }

    for tri_idx in 0..mesh.tris.len() {
        if !seen[tri_idx] {
            queue.push_back(FaceVisit {
                tri_idx,
                incoming_edge: None,
            });
            while let Some(visit) = queue.pop_front() {
                if seen[visit.tri_idx] {
                    continue;
                }
                seen[visit.tri_idx] = true;
                visits.push(visit);
                let tri = &mesh.tris[visit.tri_idx];
                for edge_idx in 0..3 {
                    let neighbor = tri_edge_value(tri, edge_idx);
                    if neighbor < 0 {
                        continue;
                    }
                    let neighbor = neighbor as usize;
                    if seen[neighbor] {
                        continue;
                    }
                    queue.push_back(FaceVisit {
                        tri_idx: neighbor,
                        incoming_edge: tri_edge_for_neighbor(
                            &mesh.tris[neighbor],
                            visit.tri_idx as i32,
                        ),
                    });
                }
            }
        }
    }

    visits
}

fn rotated_triangle_vertices(tri: &Tri, rotation: usize) -> [TriVertex; 3] {
    let vertices = [tri.a, tri.b, tri.c];
    [
        vertices[rotation % 3],
        vertices[(rotation + 1) % 3],
        vertices[(rotation + 2) % 3],
    ]
}

fn mirrored_triangle_vertices(tri: &Tri, rotation: usize) -> [TriVertex; 3] {
    let vertices = [tri.a, tri.b, tri.c];
    [
        vertices[rotation % 3],
        vertices[(rotation + 2) % 3],
        vertices[(rotation + 1) % 3],
    ]
}

fn choose_triangle_rotation(
    template: &Tri,
    template_incoming: Option<usize>,
    source: &Tri,
    source_incoming: Option<usize>,
) -> [TriVertex; 3] {
    let source_incoming_positions = source_incoming.map(|edge| {
        let (a, b, _, _) = tri_edge_positions_with_color(source, edge);
        (a, b)
    });

    (0..3)
        .flat_map(|rotation| {
            [
                rotated_triangle_vertices(source, rotation),
                mirrored_triangle_vertices(source, rotation),
            ]
        })
        .filter_map(|vertices| {
            let rotated = Tri {
                a: vertices[0],
                b: vertices[1],
                c: vertices[2],
                ab: source.ab,
                bc: source.bc,
                ca: source.ca,
                anti: source.anti,
                is_dom_sib: source.is_dom_sib,
            };

            if let (Some(template_edge), Some((source_a, source_b))) =
                (template_incoming, source_incoming_positions)
            {
                let matches_edge = (0..3).any(|candidate_edge| {
                    let (a, b, _, _) = tri_edge_positions_with_color(&rotated, candidate_edge);
                    a == source_a && b == source_b && candidate_edge == template_edge
                });
                if !matches_edge {
                    return None;
                }
            }

            let cost = [template.a.pos, template.b.pos, template.c.pos]
                .into_iter()
                .zip(vertices.into_iter().map(|vertex| vertex.pos))
                .map(|(a, b)| (a - b).len_sq())
                .sum::<f32>();
            Some((cost, vertices))
        })
        .min_by(|(lhs, _), (rhs, _)| lhs.partial_cmp(rhs).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, vertices)| vertices)
        .unwrap_or_else(|| rotated_triangle_vertices(source, 0))
}

fn conform_surface_to_template(source: &Mesh, template: &Mesh) -> Mesh {
    let template_visits = triangle_bfs_visits(template);
    let source_visits = triangle_bfs_visits(source);
    if template_visits.is_empty() || source_visits.is_empty() {
        return conform_samples_to_template(
            &mesh_vertex_samples(source),
            &source.uniform,
            &source.tag,
            template,
        );
    }

    let groups = mesh_position_groups(template);
    let group_count = groups
        .iter()
        .copied()
        .max()
        .map(|group| group + 1)
        .unwrap_or(0);
    let mut group_pos = vec![Float3::ZERO; group_count];
    let mut group_col = vec![Float4::ZERO; group_count];
    let mut group_uv = vec![Float2::ZERO; group_count];
    let mut group_hits = vec![0u32; group_count];

    for (idx, template_visit) in template_visits.iter().enumerate() {
        let source_visit = source_visits[(idx * source_visits.len()) / template_visits.len()];
        let source_tri = &source.tris[source_visit.tri_idx];
        let template_tri = &template.tris[template_visit.tri_idx];
        let rotated = choose_triangle_rotation(
            template_tri,
            template_visit.incoming_edge,
            source_tri,
            source_visit.incoming_edge,
        );
        let slot = template.dots.len() + template.lins.len() * 2 + template_visit.tri_idx * 3;
        for (group, vertex) in [
            (groups[slot], rotated[0]),
            (groups[slot + 1], rotated[1]),
            (groups[slot + 2], rotated[2]),
        ] {
            group_pos[group] += vertex.pos;
            group_col[group] += vertex.col;
            group_uv[group] += vertex.uv;
            group_hits[group] += 1;
        }
    }

    for group in 0..group_count {
        if group_hits[group] == 0 {
            continue;
        }
        let scale = 1.0 / group_hits[group] as f32;
        group_pos[group] *= scale;
        group_col[group] *= scale;
        group_uv[group] *= scale;
    }

    let mut dots = template.dots.clone();
    let mut slot = 0usize;
    for dot in &mut dots {
        let group = groups[slot];
        if group_hits[group] > 0 {
            dot.pos = group_pos[group];
            dot.col = group_col[group];
        }
        slot += 1;
    }

    let mut surface = mesh_to_indexed_surface(template);
    for (group, vertex) in surface.vertices.iter_mut().enumerate() {
        if group_hits[group] > 0 {
            vertex.pos = group_pos[group];
            vertex.col = group_col[group];
            vertex.uv = group_uv[group];
        }
    }
    let (lins, tris) = build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
    let out = Mesh {
        dots,
        lins,
        tris,
        uniform: source.uniform.clone(),
        tag: source.tag.clone(),
    };

    debug_assert!(out.has_consistent_topology());
    out
}

fn ordered_path_from_mesh(mesh: &Mesh) -> Option<OrderedPath> {
    if mesh.lins.is_empty() || !mesh.tris.is_empty() {
        return None;
    }

    let closed = mesh
        .lins
        .iter()
        .all(|line| line.prev >= 0 && line.next >= 0);
    let start = if closed {
        0
    } else {
        mesh.lins.iter().position(|line| line.prev < 0)?
    };

    let mut points = Vec::with_capacity(mesh.lins.len() + (!closed as usize));
    let mut colors = Vec::with_capacity(mesh.lins.len() + (!closed as usize));
    let mut seen = vec![false; mesh.lins.len()];
    let mut cursor = start;
    loop {
        if seen[cursor] {
            return closed.then_some(OrderedPath {
                points,
                colors,
                normal: mesh.lins[start].norm,
                closed,
            });
        }
        let line = mesh.lins[cursor];
        seen[cursor] = true;
        if points.is_empty() {
            points.push(line.a.pos);
            colors.push(line.a.col);
        }
        points.push(line.b.pos);
        colors.push(line.b.col);

        if line.next < 0 {
            break;
        }
        cursor = line.next as usize;
        if closed && cursor == start {
            break;
        }
    }

    if closed {
        points.pop();
        colors.pop();
    } else if seen.iter().any(|visited| !visited) {
        return grouped_ordered_path_from_mesh(mesh);
    }

    Some(OrderedPath {
        points,
        colors,
        normal: mesh.lins[start].norm,
        closed,
    })
}

fn grouped_ordered_path_from_mesh(mesh: &Mesh) -> Option<OrderedPath> {
    let groups = mesh_position_groups(mesh);
    let group_count = groups
        .iter()
        .copied()
        .max()
        .map(|group| group + 1)
        .unwrap_or(0);
    if group_count == 0 {
        return None;
    }

    let mut adjacency = vec![Vec::<usize>::new(); group_count];
    let mut endpoints = Vec::with_capacity(mesh.lins.len());
    for (line_idx, line) in mesh.lins.iter().enumerate() {
        let a_group = groups[mesh.dots.len() + line_idx * 2];
        let b_group = groups[mesh.dots.len() + line_idx * 2 + 1];
        adjacency[a_group].push(line_idx);
        adjacency[b_group].push(line_idx);
        endpoints.push((
            a_group, b_group, line.a.pos, line.b.pos, line.a.col, line.b.col,
        ));
    }

    let degree_one: Vec<_> = adjacency
        .iter()
        .enumerate()
        .filter_map(|(group, edges)| (edges.len() == 1).then_some(group))
        .collect();
    let closed = degree_one.is_empty();
    if closed {
        if adjacency.iter().any(|edges| edges.len() != 2) {
            return None;
        }
    } else if degree_one.len() != 2 || adjacency.iter().any(|edges| !matches!(edges.len(), 1 | 2)) {
        return None;
    }

    let mut visited = vec![false; mesh.lins.len()];
    let start_group = degree_one.first().copied().unwrap_or(endpoints[0].0);
    let mut current_group = start_group;
    let mut points = Vec::with_capacity(mesh.lins.len() + (!closed as usize));
    let mut colors = Vec::with_capacity(mesh.lins.len() + (!closed as usize));

    loop {
        let next_line = adjacency[current_group]
            .iter()
            .copied()
            .find(|&line_idx| !visited[line_idx]);
        let Some(line_idx) = next_line else {
            break;
        };
        visited[line_idx] = true;

        let (a_group, b_group, a_pos, b_pos, a_col, b_col) = endpoints[line_idx];
        let (next_group, start_pos, start_col, end_pos, end_col) = if current_group == a_group {
            (b_group, a_pos, a_col, b_pos, b_col)
        } else if current_group == b_group {
            (a_group, b_pos, b_col, a_pos, a_col)
        } else {
            return None;
        };

        if points.is_empty() {
            points.push(start_pos);
            colors.push(start_col);
        }
        points.push(end_pos);
        colors.push(end_col);
        current_group = next_group;

        if closed && current_group == start_group {
            break;
        }
    }

    if visited.iter().any(|visited| !visited) || points.len() < 2 {
        return None;
    }
    if closed {
        points.pop();
        colors.pop();
    }

    Some(OrderedPath {
        points,
        colors,
        normal: mesh.lins[0].norm,
        closed,
    })
}

fn reverse_open_path(path: &mut OrderedPath) {
    path.points.reverse();
    path.colors.reverse();
}

fn align_closed_paths(source: &OrderedPath, target: &OrderedPath) -> (OrderedPath, OrderedPath) {
    let start_contour = ClosedContour {
        signed_area: signed_contour_area(&source.points, source.normal),
        points: source.points.clone(),
        colors: source.colors.clone(),
        normal: source.normal,
    };
    let end_contour = ClosedContour {
        signed_area: signed_contour_area(&target.points, target.normal),
        points: target.points.clone(),
        colors: target.colors.clone(),
        normal: target.normal,
    };
    let (start, end) = align_closed_contours(&start_contour, &end_contour);
    (
        OrderedPath {
            points: start.points,
            colors: start.colors,
            normal: start.normal,
            closed: true,
        },
        OrderedPath {
            points: end.points,
            colors: end.colors,
            normal: end.normal,
            closed: true,
        },
    )
}

fn align_open_paths(source: &OrderedPath, target: &OrderedPath) -> (OrderedPath, OrderedPath) {
    let source_is_large = source_segment_count(source) >= target_segment_count(target);
    let (large, mut small) = if source_is_large {
        (source.clone(), target.clone())
    } else {
        (target.clone(), source.clone())
    };

    let forward_cost = endpoint_alignment_cost(&large, &small);
    let mut reversed = small.clone();
    reverse_open_path(&mut reversed);
    if endpoint_alignment_cost(&large, &reversed) < forward_cost {
        small = reversed;
    }

    let sampled = split_open_path_to_count(&small, source_segment_count(&large));
    if source_is_large {
        (large, sampled)
    } else {
        (sampled, large)
    }
}

fn source_segment_count(path: &OrderedPath) -> usize {
    if path.closed {
        path.points.len()
    } else {
        path.points.len().saturating_sub(1)
    }
}

fn target_segment_count(path: &OrderedPath) -> usize {
    source_segment_count(path)
}

fn endpoint_alignment_cost(lhs: &OrderedPath, rhs: &OrderedPath) -> f32 {
    if lhs.points.is_empty() || rhs.points.is_empty() {
        return 0.0;
    }
    (lhs.points[0] - rhs.points[0]).len_sq()
        + (lhs.points[lhs.points.len() - 1] - rhs.points[rhs.points.len() - 1]).len_sq()
}

fn split_open_path_to_count(path: &OrderedPath, target_segments: usize) -> OrderedPath {
    let source_segments = path.points.len().saturating_sub(1);
    if source_segments == 0 {
        return OrderedPath {
            points: vec![path.points.first().copied().unwrap_or(Float3::ZERO); target_segments + 1],
            colors: vec![path.colors.first().copied().unwrap_or(Float4::ONE); target_segments + 1],
            normal: path.normal,
            closed: false,
        };
    }
    if source_segments == target_segments {
        return path.clone();
    }

    let mut points = Vec::with_capacity(target_segments + 1);
    let mut colors = Vec::with_capacity(target_segments + 1);
    let mut source_segment = 0usize;
    for vertex_idx in 0..=target_segments {
        while source_segment + 1 < source_segments
            && vertex_idx > target_segments * (source_segment + 1) / source_segments
        {
            source_segment += 1;
        }
        let start = target_segments * source_segment / source_segments;
        let end = target_segments * (source_segment + 1) / source_segments;
        let denom = (end.saturating_sub(start)).max(1);
        let local = (vertex_idx.saturating_sub(start)) as f32 / denom as f32;
        let next = (source_segment + 1).min(path.points.len() - 1);
        points.push(path.points[source_segment].lerp(path.points[next], local.clamp(0.0, 1.0)));
        colors.push(path.colors[source_segment].lerp(path.colors[next], local.clamp(0.0, 1.0)));
    }

    OrderedPath {
        points,
        colors,
        normal: path.normal,
        closed: false,
    }
}

fn mesh_from_ordered_path(path: &OrderedPath, uniform: &Uniforms, tag: &[isize]) -> Mesh {
    let mut lins = Vec::new();
    if path.closed {
        push_closed_polyline(&mut lins, &path.points, path.normal);
        for idx in 0..path.points.len() {
            lins[idx].a.col = path.colors[idx];
            lins[idx].b.col = path.colors[(idx + 1) % path.colors.len()];
            lins[idx].norm = path.normal;
        }
    } else if path.points.len() >= 2 {
        for idx in 0..path.points.len() - 1 {
            let mut line = Lin {
                a: LinVertex {
                    pos: path.points[idx],
                    col: path.colors[idx],
                },
                b: LinVertex {
                    pos: path.points[idx + 1],
                    col: path.colors[idx + 1],
                },
                norm: path.normal,
                prev: if idx == 0 { -1 } else { idx as i32 - 1 },
                next: if idx + 1 == path.points.len() - 1 {
                    -1
                } else {
                    idx as i32 + 1
                },
                inv: -1,
                anti: -1,
                is_dom_sib: false,
            };
            if idx > 0 {
                line.prev = idx as i32 - 1;
            }
            lins.push(line);
        }
    }

    Mesh {
        dots: Vec::new(),
        lins,
        tris: Vec::new(),
        uniform: uniform.clone(),
        tag: tag.to_vec(),
    }
}

fn conform_path_to_template(
    path: &OrderedPath,
    template: &Mesh,
    uniform: &Uniforms,
    tag: &[isize],
) -> Mesh {
    let groups = mesh_position_groups(template);
    let group_count = groups
        .iter()
        .copied()
        .max()
        .map(|group| group + 1)
        .unwrap_or(0);
    if group_count == 0 {
        let mut out = template.clone();
        out.uniform = uniform.clone();
        out.tag = tag.to_vec();
        return out;
    }

    let samples: Vec<_> = (0..group_count)
        .map(|group_idx| {
            let t = if path.closed {
                group_idx as f32 / group_count as f32
            } else if group_count <= 1 {
                0.0
            } else {
                group_idx as f32 / (group_count - 1) as f32
            };
            sample_ordered_path(path, t)
        })
        .collect();

    let mut out = template.clone();
    out.uniform = uniform.clone();
    out.tag = tag.to_vec();

    let mut slot = 0usize;
    for dot in &mut out.dots {
        let (pos, col) = samples[groups[slot]];
        dot.pos = pos;
        dot.col = col;
        slot += 1;
    }
    for line in &mut out.lins {
        let (a_pos, a_col) = samples[groups[slot]];
        let (b_pos, b_col) = samples[groups[slot + 1]];
        line.a.pos = a_pos;
        line.a.col = a_col;
        line.b.pos = b_pos;
        line.b.col = b_col;
        slot += 2;
    }
    for tri in &mut out.tris {
        let (a_pos, a_col) = samples[groups[slot]];
        let (b_pos, b_col) = samples[groups[slot + 1]];
        let (c_pos, c_col) = samples[groups[slot + 2]];
        tri.a.pos = a_pos;
        tri.a.col = a_col;
        tri.b.pos = b_pos;
        tri.b.col = b_col;
        tri.c.pos = c_pos;
        tri.c.col = c_col;
        slot += 3;
    }

    debug_assert!(out.has_consistent_topology());
    out
}

fn conform_constant_to_template(
    point: Float3,
    color: Float4,
    uniform: &Uniforms,
    tag: &[isize],
    template: &Mesh,
) -> Mesh {
    let constant = OrderedPath {
        points: vec![point],
        colors: vec![color],
        normal: template
            .lins
            .first()
            .map(|line| line.norm)
            .unwrap_or(Float3::ZERO),
        closed: false,
    };
    conform_path_to_template(&constant, template, uniform, tag)
}

fn sample_ordered_path(path: &OrderedPath, t: f32) -> (Float3, Float4) {
    if path.points.is_empty() {
        return (Float3::ZERO, Float4::ONE);
    }
    if path.points.len() == 1 {
        return (path.points[0], path.colors[0]);
    }

    let segment_count = source_segment_count(path);
    if segment_count == 0 {
        return (path.points[0], path.colors[0]);
    }

    let scaled = if path.closed {
        (t.rem_euclid(1.0)) * segment_count as f32
    } else {
        t.clamp(0.0, 1.0) * segment_count as f32
    };
    let segment = scaled.floor() as usize;
    let local = (scaled - segment as f32).clamp(0.0, 1.0);
    let start_idx = segment.min(path.points.len() - 1);
    let end_idx = if path.closed {
        (segment + 1) % path.points.len()
    } else {
        (segment + 1).min(path.points.len() - 1)
    };
    (
        path.points[start_idx].lerp(path.points[end_idx], local),
        path.colors[start_idx].lerp(path.colors[end_idx], local),
    )
}

fn mesh_tree_trans_lerp(
    start: &Value,
    end: &Value,
    state: &Value,
    t: f32,
    path_arc: Float3,
) -> Result<Value, ExecutorError> {
    match (
        start.clone().elide_lvalue_leader_rec(),
        end.clone().elide_lvalue_leader_rec(),
        state.clone().elide_lvalue_leader_rec(),
    ) {
        (Value::Mesh(start), Value::Mesh(end), state) => {
            if let Some((start_fill, end_fill)) = read_planar_state(&state)? {
                Ok(Value::Mesh(Arc::new(planar_mesh_patharc_lerp(
                    start.as_ref(),
                    end.as_ref(),
                    start_fill,
                    end_fill,
                    t,
                    path_arc,
                )?)))
            } else {
                Ok(Value::Mesh(Arc::new(mesh_patharc_lerp(
                    start.as_ref(),
                    end.as_ref(),
                    t,
                    path_arc,
                )?)))
            }
        }
        (Value::List(start), Value::List(end), Value::List(state)) => {
            if start.len() != end.len() || start.len() != state.len() {
                return Err(ExecutorError::invalid_interpolation(format!(
                    "cannot trans lists of different lengths: {} vs {} vs {}",
                    start.len(),
                    end.len(),
                    state.len()
                )));
            }

            let elements = start
                .elements()
                .iter()
                .zip(end.elements().iter())
                .zip(state.elements().iter())
                .map(|((start, end), state)| {
                    let start = executor::heap::with_heap(|h| h.get(start.key()).clone());
                    let end = executor::heap::with_heap(|h| h.get(end.key()).clone());
                    let state = executor::heap::with_heap(|h| h.get(state.key()).clone());
                    mesh_tree_trans_lerp(&start, &end, &state, t, path_arc)
                        .map(executor::heap::VRc::new)
                })
                .collect::<Result<_, _>>()?;
            Ok(Value::List(Rc::new(
                executor::value::container::List::new_with(elements),
            )))
        }
        (start, end, state) => Err(ExecutorError::invalid_interpolation(format!(
            "cannot trans {} and {} with state {}",
            start.type_name(),
            end.type_name(),
            state.type_name()
        ))),
    }
}

pub(super) fn mesh_tree_patharc_lerp(
    start: &Value,
    end: &Value,
    t: f32,
    path_arc: Float3,
) -> Result<Value, ExecutorError> {
    match (
        start.clone().elide_lvalue_leader_rec(),
        end.clone().elide_lvalue_leader_rec(),
    ) {
        (Value::Mesh(start), Value::Mesh(end)) => Ok(Value::Mesh(Arc::new(mesh_patharc_lerp(
            start.as_ref(),
            end.as_ref(),
            t,
            path_arc,
        )?))),
        (Value::List(start), Value::List(end)) => {
            if start.len() != end.len() {
                return Err(ExecutorError::invalid_interpolation(format!(
                    "cannot trans lists of different lengths: {} vs {}",
                    start.len(),
                    end.len()
                )));
            }

            let elements = start
                .elements()
                .iter()
                .zip(end.elements().iter())
                .map(|(start, end)| {
                    let start = executor::heap::with_heap(|h| h.get(start.key()).clone());
                    let end = executor::heap::with_heap(|h| h.get(end.key()).clone());
                    mesh_tree_patharc_lerp(&start, &end, t, path_arc).map(executor::heap::VRc::new)
                })
                .collect::<Result<_, _>>()?;
            Ok(Value::List(Rc::new(
                executor::value::container::List::new_with(elements),
            )))
        }
        (start, end) => Err(ExecutorError::invalid_interpolation(format!(
            "cannot trans {} and {}",
            start.type_name(),
            end.type_name()
        ))),
    }
}

fn mesh_patharc_lerp(
    start: &Mesh,
    end: &Mesh,
    t: f32,
    path_arc: Float3,
) -> Result<Mesh, ExecutorError> {
    ensure_same_mesh_topology(start, end, "trans")?;

    let mesh = Mesh {
        dots: start
            .dots
            .iter()
            .zip(&end.dots)
            .map(|(start, end)| Dot {
                pos: vec3_patharc_lerp(start.pos, t, end.pos, path_arc),
                norm: vec3_norm_lerp(start.norm, t, end.norm),
                col: start.col.lerp(end.col, t),
                inv: end.inv,
                anti: end.anti,
                is_dom_sib: end.is_dom_sib,
            })
            .collect(),
        lins: start
            .lins
            .iter()
            .zip(&end.lins)
            .map(|(start, end)| Lin {
                a: LinVertex {
                    pos: vec3_patharc_lerp(start.a.pos, t, end.a.pos, path_arc),
                    col: start.a.col.lerp(end.a.col, t),
                },
                b: LinVertex {
                    pos: vec3_patharc_lerp(start.b.pos, t, end.b.pos, path_arc),
                    col: start.b.col.lerp(end.b.col, t),
                },
                norm: vec3_norm_lerp(start.norm, t, end.norm),
                prev: end.prev,
                next: end.next,
                inv: end.inv,
                anti: end.anti,
                is_dom_sib: end.is_dom_sib,
            })
            .collect(),
        tris: start
            .tris
            .iter()
            .zip(&end.tris)
            .map(|(start, end)| Tri {
                a: TriVertex {
                    pos: vec3_patharc_lerp(start.a.pos, t, end.a.pos, path_arc),
                    col: start.a.col.lerp(end.a.col, t),
                    uv: start.a.uv.lerp(end.a.uv, t),
                },
                b: TriVertex {
                    pos: vec3_patharc_lerp(start.b.pos, t, end.b.pos, path_arc),
                    col: start.b.col.lerp(end.b.col, t),
                    uv: start.b.uv.lerp(end.b.uv, t),
                },
                c: TriVertex {
                    pos: vec3_patharc_lerp(start.c.pos, t, end.c.pos, path_arc),
                    col: start.c.col.lerp(end.c.col, t),
                    uv: start.c.uv.lerp(end.c.uv, t),
                },
                ab: end.ab,
                bc: end.bc,
                ca: end.ca,
                anti: end.anti,
                is_dom_sib: end.is_dom_sib,
            })
            .collect(),
        uniform: lerp_uniforms(&start.uniform, &end.uniform, t),
        tag: end.tag.clone(),
    };
    debug_assert!(mesh.has_consistent_topology());
    Ok(mesh)
}

fn planar_mesh_patharc_lerp(
    start: &Mesh,
    end: &Mesh,
    start_fill: Float4,
    end_fill: Float4,
    t: f32,
    path_arc: Float3,
) -> Result<Mesh, ExecutorError> {
    let boundary = mesh_patharc_lerp(start, end, t, path_arc)?;
    extract_closed_contours(&boundary).ok_or_else(|| {
        ExecutorError::invalid_interpolation("planar trans produced a non-closed contour")
    })?;
    let Some(upranked) = uprank_mesh(&boundary)? else {
        return Err(ExecutorError::invalid_interpolation(
            "planar trans produced a non-closed contour",
        ));
    };
    let mut lins = upranked.lins;
    let mut tris = upranked.tris;

    if lins.len() == boundary.lins.len() {
        for (line, template) in lins.iter_mut().zip(&boundary.lins) {
            line.a.col = template.a.col;
            line.b.col = template.b.col;
            line.norm = template.norm;
        }
    }

    let fill = start_fill.lerp(end_fill, t);
    for tri in &mut tris {
        tri.a.col = fill;
        tri.b.col = fill;
        tri.c.col = fill;
    }

    let mesh = Mesh {
        dots: Vec::new(),
        lins,
        tris,
        uniform: boundary.uniform,
        tag: boundary.tag,
    };
    debug_assert!(mesh.has_consistent_topology());
    Ok(mesh)
}

fn ensure_same_mesh_topology(
    start: &Mesh,
    end: &Mesh,
    op: &'static str,
) -> Result<(), ExecutorError> {
    if start.dots.len() != end.dots.len()
        || start.lins.len() != end.lins.len()
        || start.tris.len() != end.tris.len()
        || start
            .dots
            .iter()
            .zip(&end.dots)
            .any(|(a, b)| (a.inv, a.anti, a.is_dom_sib) != (b.inv, b.anti, b.is_dom_sib))
        || start.lins.iter().zip(&end.lins).any(|(a, b)| {
            (a.prev, a.next, a.inv, a.anti, a.is_dom_sib)
                != (b.prev, b.next, b.inv, b.anti, b.is_dom_sib)
        })
        || start.tris.iter().zip(&end.tris).any(|(a, b)| {
            (a.ab, a.bc, a.ca, a.anti, a.is_dom_sib) != (b.ab, b.bc, b.ca, b.anti, b.is_dom_sib)
        })
    {
        return Err(ExecutorError::invalid_interpolation(format!(
            "cannot {} meshes with different topology",
            op
        )));
    }
    Ok(())
}

fn vec3_norm_lerp(start: Float3, t: f32, end: Float3) -> Float3 {
    let raw = start.lerp(end, t);
    let len = raw.len();
    if len <= 1e-6 { end } else { raw / len }
}

fn vec3_patharc_lerp(start: Float3, t: f32, end: Float3, path_arc: Float3) -> Float3 {
    if path_arc.len_sq() <= 1e-12 {
        return start.lerp(end, t);
    }

    let delta = end - start;
    if delta.len_sq() <= 1e-12 {
        return start.lerp(end, t);
    }

    let cross = path_arc.cross(delta);
    let cross_len = cross.len();
    if cross_len <= 1e-6 {
        return start.lerp(end, t);
    }

    let alpha = path_arc.len();
    let tan_half = (alpha / 2.0).tan();
    if !alpha.is_finite() || alpha.abs() <= 1e-6 || !tan_half.is_finite() || tan_half.abs() <= 1e-6
    {
        return start.lerp(end, t);
    }

    let pivot = (start + end) / 2.0 + cross * (delta.len() / (2.0 * tan_half * cross_len));
    let radius_vec = start - pivot;
    let radius = radius_vec.len();
    if radius <= 1e-6 {
        return start.lerp(end, t);
    }

    let a_prime = radius_vec / radius;
    let a_prime_norm = path_arc.cross(a_prime);
    let a_prime_norm_len = a_prime_norm.len();
    if a_prime_norm_len <= 1e-6 {
        return start.lerp(end, t);
    }

    let theta = t * alpha;
    let cos = theta.cos() * radius;
    let sin = theta.sin() * radius;
    pivot + a_prime * cos + (a_prime_norm / a_prime_norm_len) * sin
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;
    use std::sync::Arc;

    use geo::{
        mesh::{Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
        simd::{Float3, Float4},
    };

    use crate::mesh::helpers::tessellate_planar_loops;

    use super::{
        ClosedContour, append_closed_contour, extract_closed_contours, pair_leaf_indices_by_tag,
        prepare_planar_trans_mesh_pair, prepare_trans_mesh_pair, same_mesh_topology,
        split_mesh_contours,
        vec3_patharc_lerp,
    };

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

    fn tagged_mesh(tag: Vec<isize>) -> Arc<Mesh> {
        Arc::new(Mesh {
            dots: vec![],
            lins: vec![],
            tris: vec![],
            uniform: Uniforms::default(),
            tag,
        })
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

    fn tessellated_mesh(contours: &[Vec<Float3>]) -> Mesh {
        let (lins, tris) =
            tessellate_planar_loops(contours, Float3::Z).expect("planar tessellation should work");
        Mesh {
            dots: vec![],
            lins,
            tris,
            uniform: Uniforms::default(),
            tag: vec![],
        }
    }

    fn circle_points(radius: f32, samples: usize) -> Vec<Float3> {
        (0..samples)
            .map(|i| {
                let theta = std::f32::consts::TAU * i as f32 / samples as f32;
                Float3::new(radius * theta.cos(), radius * theta.sin(), 0.0)
            })
            .collect()
    }

    fn regular_polygon_points(radius: f32, samples: usize) -> Vec<Float3> {
        circle_points(radius, samples)
    }

    fn annulus_mesh(inner: f32, outer: f32, samples: usize) -> Mesh {
        let mut inner_pts = circle_points(inner, samples);
        inner_pts.reverse();
        tessellated_mesh(&[circle_points(outer, samples), inner_pts])
    }

    fn capsule_mesh(half_len: f32, radius: f32, arc_samples: usize) -> Mesh {
        let mut contour = Vec::with_capacity(arc_samples * 2);
        for i in 0..arc_samples {
            let theta = -std::f32::consts::FRAC_PI_2
                + std::f32::consts::PI * i as f32 / (arc_samples - 1) as f32;
            contour.push(Float3::new(
                half_len + radius * theta.cos(),
                radius * theta.sin(),
                0.0,
            ));
        }
        for i in 0..arc_samples {
            let theta = std::f32::consts::FRAC_PI_2
                + std::f32::consts::PI * i as f32 / (arc_samples - 1) as f32;
            contour.push(Float3::new(
                -half_len + radius * theta.cos(),
                radius * theta.sin(),
                0.0,
            ));
        }
        tessellated_mesh(&[contour])
    }

    #[test]
    fn vec3_patharc_lerp_matches_old_c_semantics() {
        let point = vec3_patharc_lerp(
            Float3::ZERO,
            0.5,
            Float3::X,
            Float3::new(0.0, 0.0, FRAC_PI_2),
        );

        assert!((point.x - 0.5).abs() < 1e-5, "unexpected x: {}", point.x);
        assert!(
            (point.y + 0.20710677).abs() < 1e-5,
            "unexpected y: {}",
            point.y
        );
        assert!(point.z.abs() < 1e-5, "unexpected z: {}", point.z);
    }

    #[test]
    fn pair_leaf_indices_by_tag_keeps_unmatched_source_groups() {
        let source = vec![tagged_mesh(vec![0]), tagged_mesh(vec![1])];
        let target = vec![tagged_mesh(vec![0])];

        let pairings = pair_leaf_indices_by_tag(&source, &target);

        assert_eq!(pairings, vec![(Some(0), Some(0)), (Some(1), None)]);
    }

    #[test]
    fn split_mesh_contours_splits_disconnected_line_components() {
        let mesh = Mesh {
            dots: vec![],
            lins: vec![
                line(Float3::ZERO, Float3::X, -1, 1),
                line(Float3::X, Float3::new(2.0, 0.0, 0.0), 0, -1),
                line(
                    Float3::new(10.0, 0.0, 0.0),
                    Float3::new(11.0, 0.0, 0.0),
                    -1,
                    3,
                ),
                line(
                    Float3::new(11.0, 0.0, 0.0),
                    Float3::new(12.0, 0.0, 0.0),
                    2,
                    -1,
                ),
            ],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
        };

        let parts = split_mesh_contours(&mesh);

        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].lins.len(), 2);
        assert_eq!(parts[1].lins.len(), 2);
    }

    #[test]
    fn prepare_planar_trans_mesh_pair_preserves_loop_traversal_order() {
        let small = ClosedContour {
            points: vec![
                Float3::new(-4.0, -0.5, 0.0),
                Float3::new(-3.0, -0.5, 0.0),
                Float3::new(-3.0, 0.5, 0.0),
                Float3::new(-4.0, 0.5, 0.0),
            ],
            colors: vec![Float4::ONE; 4],
            normal: Float3::Z,
            signed_area: 1.0,
        };
        let large = ClosedContour {
            points: vec![
                Float3::new(2.0, -2.0, 0.0),
                Float3::new(6.0, -2.0, 0.0),
                Float3::new(6.0, 2.0, 0.0),
                Float3::new(2.0, 2.0, 0.0),
            ],
            colors: vec![Float4::ONE; 4],
            normal: Float3::Z,
            signed_area: 16.0,
        };

        let mut start = Mesh {
            dots: vec![],
            lins: vec![],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
        };
        append_closed_contour(&mut start, &small);
        append_closed_contour(&mut start, &large);

        let mut end = Mesh {
            dots: vec![],
            lins: vec![],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
        };
        append_closed_contour(&mut end, &large);
        append_closed_contour(&mut end, &small);

        let (prepared_start, _, _) = prepare_planar_trans_mesh_pair(&start, &end)
            .expect("planar prep should succeed")
            .unwrap();
        let contours =
            extract_closed_contours(&prepared_start).expect("prepared start should stay closed");

        let first_center = contours[0]
            .points
            .iter()
            .copied()
            .fold(Float3::ZERO, |acc, point| acc + point)
            / contours[0].points.len() as f32;
        let second_center = contours[1]
            .points
            .iter()
            .copied()
            .fold(Float3::ZERO, |acc, point| acc + point)
            / contours[1].points.len() as f32;

        assert!(first_center.x < second_center.x);
    }

    #[test]
    fn prepare_trans_mesh_pair_keeps_larger_surface_topology() {
        let source = Mesh {
            dots: vec![],
            lins: vec![],
            tris: vec![tri(
                Float3::new(0.0, 0.0, 0.0),
                Float3::new(1.0, 0.0, 0.0),
                Float3::new(0.0, 1.0, 0.0),
                -1,
                -1,
                -1,
            )],
            uniform: Uniforms::default(),
            tag: vec![],
        };
        let target = Mesh {
            dots: vec![],
            lins: vec![],
            tris: vec![
                tri(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(1.0, 0.0, 0.0),
                    Float3::new(1.0, 1.0, 0.0),
                    -1,
                    -1,
                    1,
                ),
                tri(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(1.0, 1.0, 0.0),
                    Float3::new(0.0, 1.0, 0.0),
                    0,
                    -1,
                    -1,
                ),
            ],
            uniform: Uniforms::default(),
            tag: vec![],
        };

        let (aligned_source, aligned_target, _) =
            prepare_trans_mesh_pair(Some(&target), Some(&source))
                .expect("pair prep should succeed");

        assert_eq!(aligned_source.tris.len(), 2);
        assert_eq!(aligned_target.tris.len(), 2);
    }

    #[test]
    fn prepare_trans_mesh_pair_handles_polygon_to_annulus() {
        let source = tessellated_mesh(&[vec![
            Float3::new(-1.0, -0.7, 0.0),
            Float3::new(1.1, -0.9, 0.0),
            Float3::new(0.0, 1.2, 0.0),
        ]]);
        let target = annulus_mesh(0.4, 1.1, 24);

        let (aligned_source, aligned_target, _) =
            prepare_trans_mesh_pair(Some(&source), Some(&target))
                .expect("pair prep should succeed");

        assert!(aligned_source.has_consistent_topology());
        assert!(aligned_target.has_consistent_topology());
        assert!(same_mesh_topology(&aligned_source, &aligned_target));
    }

    #[test]
    fn prepare_trans_mesh_pair_handles_polygon_to_capsule_like_surface() {
        let source = tessellated_mesh(&[vec![
            Float3::new(-0.9, -0.9, 0.0),
            Float3::new(0.9, -0.9, 0.0),
            Float3::new(0.9, 0.9, 0.0),
            Float3::new(-0.9, 0.9, 0.0),
        ]]);
        let target = capsule_mesh(1.0, 0.35, 16);

        let (aligned_source, aligned_target, _) =
            prepare_trans_mesh_pair(Some(&source), Some(&target))
                .expect("pair prep should succeed");

        assert!(aligned_source.has_consistent_topology());
        assert!(aligned_target.has_consistent_topology());
        assert!(same_mesh_topology(&aligned_source, &aligned_target));
    }

    #[test]
    fn prepare_trans_mesh_pair_handles_circle_to_regular_polygon() {
        let source = tessellated_mesh(&[circle_points(0.8, 32)]);
        let target = tessellated_mesh(&[regular_polygon_points(0.9, 5)]);

        let (aligned_source, aligned_target, _) =
            prepare_trans_mesh_pair(Some(&source), Some(&target))
                .expect("pair prep should succeed");

        assert!(aligned_source.has_consistent_topology());
        assert!(aligned_target.has_consistent_topology());
        assert!(same_mesh_topology(&aligned_source, &aligned_target));
    }

    #[test]
    fn prepare_trans_mesh_pair_handles_clockwise_triangle_to_annulus() {
        let mut lins = vec![
            line(
                Float3::new(1.5, 1.8, 0.0),
                Float3::new(2.5, 3.4, 0.0),
                -1,
                -1,
            ),
            line(
                Float3::new(2.5, 3.4, 0.0),
                Float3::new(3.3, 1.7, 0.0),
                -1,
                -1,
            ),
            line(
                Float3::new(3.3, 1.7, 0.0),
                Float3::new(1.5, 1.8, 0.0),
                -1,
                -1,
            ),
        ];
        let mut triangle = tri(
            Float3::new(1.5, 1.8, 0.0),
            Float3::new(2.5, 3.4, 0.0),
            Float3::new(3.3, 1.7, 0.0),
            super::mesh_ref(0),
            super::mesh_ref(1),
            super::mesh_ref(2),
        );
        for line in &mut lins {
            line.inv = super::mesh_ref(0);
        }
        triangle.a.col = Float4::ONE;
        triangle.b.col = Float4::ONE;
        triangle.c.col = Float4::ONE;
        let source = Mesh {
            dots: vec![],
            lins,
            tris: vec![triangle],
            uniform: Uniforms::default(),
            tag: vec![],
        };
        let target = annulus_mesh(0.34, 0.82, 64);

        let (aligned_source, aligned_target, _) =
            prepare_trans_mesh_pair(Some(&source), Some(&target))
                .expect("pair prep should succeed");

        assert!(aligned_source.has_consistent_topology());
        assert!(aligned_target.has_consistent_topology());
        assert!(same_mesh_topology(&aligned_source, &aligned_target));
    }
}
