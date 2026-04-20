use std::{collections::HashMap, rc::Rc, sync::Arc};

use executor::{
    error::ExecutorError,
    executor::Executor,
    state::LeaderKind,
    value::Value,
};
use geo::{
    mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
    simd::{Float3, Float4},
};
use stdlib_macros::stdlib_func;

use crate::mesh::helpers::{push_closed_polyline, tessellate_planar_loops};

use super::helpers::{
    build_lerp, collapse_mesh, fade_start_mesh, flatten_mesh_leaves, list_value,
    map_mesh_tree, materialize_live_value, mesh_center, mesh_tag_map, progression_from,
    read_time, resolve_targets, conform_mesh_to_target,
};

#[stdlib_func]
pub async fn grow_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let center = mesh_tree_center(&destination)?;
    let start = map_mesh_tree(&destination, &mut |mesh| collapse_mesh(mesh, center))?;
    Ok(embed_triplet(start, destination, Value::Nil))
}

#[stdlib_func]
pub async fn fade_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-3).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let delta = read_vec3_value(
        executor.state.stack(stack_idx).read_at(-1).clone(),
        "delta",
    )?;
    let start = map_mesh_tree(&destination, &mut |mesh| fade_start_mesh(mesh, delta))?;
    Ok(embed_triplet(start, destination, Value::Nil))
}

#[stdlib_func]
pub async fn write_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let start = map_mesh_tree(&destination, &mut write_start_mesh)?;
    Ok(embed_triplet(start, destination, Value::Nil))
}

#[stdlib_func]
pub async fn trans_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let start_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let start = materialize_live_value(executor, &start_value).await?;
    let destination = materialize_live_value(executor, &destination_value).await?;
    let mut start_leaves = Vec::new();
    flatten_mesh_leaves(&start, &mut start_leaves)?;
    let (aligned, prepared_destination, state) = prepare_trans_value_like(&start_leaves, &destination)?;
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
    let mut start_leaves = Vec::new();
    flatten_mesh_leaves(&start, &mut start_leaves)?;
    let by_tag: HashMap<_, _> = mesh_tag_map(&start)?;
    let (aligned, prepared_destination, state) =
        prepare_trans_value_like_by_tag(&by_tag, &start_leaves, &destination)?;
    Ok(embed_triplet(aligned, prepared_destination, state))
}

#[stdlib_func]
pub async fn bend_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    trans_embed(executor, stack_idx).await
}

#[stdlib_func]
pub async fn tag_bend_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    tag_trans_embed(executor, stack_idx).await
}

#[stdlib_func]
pub async fn highlight_embed(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-3).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let color = super::helpers::read_float4_value(
        executor.state.stack(stack_idx).read_at(-1).clone(),
        "color",
    )?;
    let highlighted = map_mesh_tree(&destination, &mut |mesh| {
        let mut mesh = mesh.clone();
        for dot in &mut mesh.dots {
            dot.col = color;
        }
        for lin in &mut mesh.lins {
            lin.a.col = color;
            lin.b.col = color;
        }
        for tri in &mut mesh.tris {
            tri.a.col = color;
            tri.b.col = color;
            tri.c.col = color;
        }
        mesh
    })?;
    Ok(embed_triplet(highlighted, destination, Value::Nil))
}

#[stdlib_func]
pub async fn flash_embed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let _start = executor.state.stack(stack_idx).read_at(-2).clone();
    let destination_value = executor.state.stack(stack_idx).read_at(-1).clone();
    let destination = materialize_live_value(executor, &destination_value).await?;
    let start = map_mesh_tree(&destination, &mut write_start_mesh)?;
    Ok(embed_triplet(start, destination, Value::Nil))
}

#[stdlib_func]
pub async fn mesh_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start = executor.state.stack(stack_idx).read_at(-3).clone();
    let end = executor.state.stack(stack_idx).read_at(-2).clone();
    let t = crate::read_float(executor, stack_idx, -1, "t")? as f32;
    mesh_tree_patharc_lerp(&start, &end, t, Float3::ZERO)
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

#[stdlib_func]
pub async fn bend_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let start = executor.state.stack(stack_idx).read_at(-3).clone();
    let end = executor.state.stack(stack_idx).read_at(-2).clone();
    let t = crate::read_float(executor, stack_idx, -1, "t")? as f32;
    mesh_tree_bend_lerp(&start, &end, t)
}

#[stdlib_func]
pub async fn write_lerp_value(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let end = executor.state.stack(stack_idx).read_at(-2).clone();
    let t = crate::read_float(executor, stack_idx, -1, "t")? as f32;
    write_tree_value(&end, t.clamp(0.0, 1.0))
}

#[stdlib_func]
pub async fn camera_lerp_anim(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let candidates = executor.state.stack(stack_idx).read_at(-3).clone();
    let time = read_time(executor, stack_idx, -2)?;
    let rate = executor.state.stack(stack_idx).read_at(-1).clone();
    let targets = resolve_targets(executor, stack_idx, &candidates, Some(LeaderKind::Param))?;
    Ok(build_lerp(
        &targets,
        time,
        progression_from(rate),
        None,
        None,
    ))
}

fn embed_triplet(start: Value, destination: Value, state: Value) -> Value {
    list_value([start, destination, state])
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
                    let start_fill = super::helpers::read_float4_value(
                        executor::heap::with_heap(|h| h.get(list.elements()[1].key()).clone()),
                        "state",
                    )?;
                    let end_fill = super::helpers::read_float4_value(
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

fn prepare_trans_value_like(
    source_leaves: &[Arc<Mesh>],
    target: &Value,
) -> Result<(Value, Value, Value), ExecutorError> {
    fn recurse(
        source_leaves: &[Arc<Mesh>],
        cursor: &mut usize,
        target: &Value,
    ) -> Result<(Value, Value, Value), ExecutorError> {
        match target {
            Value::Mesh(target_mesh) => {
                let source = if source_leaves.is_empty() {
                    None
                } else {
                    Some(source_leaves[*cursor % source_leaves.len()].as_ref())
                };
                *cursor += 1;
                let (start, end, state) = prepare_trans_mesh_pair(source, target_mesh)?;
                Ok((
                    Value::Mesh(Arc::new(start)),
                    Value::Mesh(Arc::new(end)),
                    state,
                ))
            }
            Value::List(list) => {
                let mut starts = Vec::with_capacity(list.len());
                let mut ends = Vec::with_capacity(list.len());
                let mut states = Vec::with_capacity(list.len());
                for elem in list.elements() {
                    let elem = executor::heap::with_heap(|h| h.get(elem.key()).clone());
                    let (start, end, state) = recurse(source_leaves, cursor, &elem)?;
                    starts.push(start);
                    ends.push(end);
                    states.push(state);
                }
                Ok((list_value(starts), list_value(ends), list_value(states)))
            }
            other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
        }
    }

    let mut cursor = 0;
    recurse(source_leaves, &mut cursor, target)
}

fn prepare_trans_value_like_by_tag(
    source_by_tag: &HashMap<Vec<isize>, Arc<Mesh>>,
    source_fallback: &[Arc<Mesh>],
    target: &Value,
) -> Result<(Value, Value, Value), ExecutorError> {
    fn recurse(
        source_by_tag: &HashMap<Vec<isize>, Arc<Mesh>>,
        source_fallback: &[Arc<Mesh>],
        cursor: &mut usize,
        target: &Value,
    ) -> Result<(Value, Value, Value), ExecutorError> {
        match target {
            Value::Mesh(target_mesh) => {
                let source = source_by_tag.get(&target_mesh.tag).map(Arc::as_ref).or_else(|| {
                    if source_fallback.is_empty() {
                        None
                    } else {
                        Some(source_fallback[*cursor % source_fallback.len()].as_ref())
                    }
                });
                *cursor += 1;
                let (start, end, state) = prepare_trans_mesh_pair(source, target_mesh)?;
                Ok((
                    Value::Mesh(Arc::new(start)),
                    Value::Mesh(Arc::new(end)),
                    state,
                ))
            }
            Value::List(list) => {
                let mut starts = Vec::with_capacity(list.len());
                let mut ends = Vec::with_capacity(list.len());
                let mut states = Vec::with_capacity(list.len());
                for elem in list.elements() {
                    let elem = executor::heap::with_heap(|h| h.get(elem.key()).clone());
                    let (start, end, state) =
                        recurse(source_by_tag, source_fallback, cursor, &elem)?;
                    starts.push(start);
                    ends.push(end);
                    states.push(state);
                }
                Ok((list_value(starts), list_value(ends), list_value(states)))
            }
            other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
        }
    }

    let mut cursor = 0;
    recurse(source_by_tag, source_fallback, &mut cursor, target)
}

fn prepare_trans_mesh_pair(
    source: Option<&Mesh>,
    target: &Mesh,
) -> Result<(Mesh, Mesh, Value), ExecutorError> {
    if let Some(source) = source {
        if let Some((start, end, state)) = prepare_planar_trans_mesh_pair(source, target)? {
            return Ok((start, end, state));
        }
    }

    Ok((
        conform_mesh_to_target(source, target),
        target.clone(),
        Value::Nil,
    ))
}

fn prepare_planar_trans_mesh_pair(
    start: &Mesh,
    end: &Mesh,
) -> Result<Option<(Mesh, Mesh, Value)>, ExecutorError> {
    let mut start_contours = match extract_closed_contours(start) {
        Some(contours) => contours,
        None => return Ok(None),
    };
    let mut end_contours = match extract_closed_contours(end) {
        Some(contours) => contours,
        None => return Ok(None),
    };

    let Some(start_fill) = planar_fill_color(start) else {
        return Ok(None);
    };
    let Some(end_fill) = planar_fill_color(end) else {
        return Ok(None);
    };

    start_contours.sort_by(|a, b| {
        b.signed_area
            .abs()
            .partial_cmp(&a.signed_area.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    end_contours.sort_by(|a, b| {
        b.signed_area
            .abs()
            .partial_cmp(&a.signed_area.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let loop_count = match (start_contours.len(), end_contours.len()) {
        (0, _) | (_, 0) => 0,
        (a, b) if a.is_multiple_of(b) => a,
        (a, b) if b.is_multiple_of(a) => b,
        (a, b) => a.checked_mul(b).expect("contour count overflow"),
    };
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
        let sample_count = start_contour.points.len().max(end_contour.points.len()).max(3);

        let sampled_start = resample_closed_contour(start_contour, sample_count);
        let sampled_end = align_resampled_contour(&sampled_start, end_contour, sample_count);
        append_closed_contour(&mut start_mesh, &sampled_start);
        append_closed_contour(&mut end_mesh, &sampled_end);
    }

    debug_assert!(start_mesh.has_consistent_topology());
    debug_assert!(end_mesh.has_consistent_topology());
    Ok(Some((start_mesh, end_mesh, planar_state(start_fill, end_fill))))
}

fn extract_closed_contours(mesh: &Mesh) -> Option<Vec<ClosedContour>> {
    if mesh.lins.is_empty() || mesh.dots.len() != 0 || mesh.lins.iter().any(|lin| lin.prev < 0 || lin.next < 0) {
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
    mesh.tris.iter().all(|tri| tri.a.col == color && tri.b.col == color && tri.c.col == color)
        .then_some(color)
}

fn resample_closed_contour(contour: &ClosedContour, sample_count: usize) -> ClosedContour {
    if contour.points.len() == sample_count {
        return contour.clone();
    }

    let segment_lengths: Vec<_> = contour
        .points
        .iter()
        .enumerate()
        .map(|(i, point)| (*point - contour.points[(i + 1) % contour.points.len()]).len())
        .collect();
    let total_length: f32 = segment_lengths.iter().sum();
    if total_length <= 1e-6 {
        return ClosedContour {
            points: vec![contour.points[0]; sample_count],
            colors: vec![contour.colors[0]; sample_count],
            normal: contour.normal,
            signed_area: contour.signed_area,
        };
    }

    let mut points = Vec::with_capacity(sample_count);
    let mut colors = Vec::with_capacity(sample_count);
    let mut segment = 0usize;
    let mut consumed = 0.0f32;
    for i in 0..sample_count {
        let target = total_length * i as f32 / sample_count as f32;
        while consumed + segment_lengths[segment] < target && segment + 1 < segment_lengths.len() {
            consumed += segment_lengths[segment];
            segment += 1;
        }

        let len = segment_lengths[segment].max(1e-6);
        let local = ((target - consumed) / len).clamp(0.0, 1.0);
        let next = (segment + 1) % contour.points.len();
        points.push(contour.points[segment].lerp(contour.points[next], local));
        colors.push(contour.colors[segment].lerp(contour.colors[next], local));
    }

    ClosedContour {
        signed_area: signed_contour_area(&points, contour.normal),
        points,
        colors,
        normal: contour.normal,
    }
}

fn align_resampled_contour(
    reference: &ClosedContour,
    contour: &ClosedContour,
    sample_count: usize,
) -> ClosedContour {
    let mut sampled = resample_closed_contour(contour, sample_count);
    if reference.signed_area * sampled.signed_area < 0.0 {
        sampled.points.reverse();
        sampled.colors.reverse();
        sampled.signed_area = -sampled.signed_area;
    }

    let shift = best_contour_shift(&reference.points, &sampled.points);
    rotate_vec(&mut sampled.points, shift);
    rotate_vec(&mut sampled.colors, shift);
    sampled
}

fn best_contour_shift(reference: &[Float3], candidate: &[Float3]) -> usize {
    (0..candidate.len())
        .min_by(|&lhs, &rhs| {
            contour_shift_cost(reference, candidate, lhs)
                .partial_cmp(&contour_shift_cost(reference, candidate, rhs))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0)
}

fn contour_shift_cost(reference: &[Float3], candidate: &[Float3], shift: usize) -> f32 {
    reference
        .iter()
        .enumerate()
        .map(|(i, point)| (*point - candidate[(i + shift) % candidate.len()]).len_sq())
        .sum()
}

fn rotate_vec<T>(values: &mut [T], shift: usize) {
    if !values.is_empty() {
        values.rotate_left(shift % values.len());
    }
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

fn mesh_tree_center(value: &Value) -> Result<Float3, ExecutorError> {
    let mut leaves = Vec::new();
    flatten_mesh_leaves(value, &mut leaves)?;
    let mut min = None::<Float3>;
    let mut max = None::<Float3>;
    for leaf in leaves {
        let c = mesh_center(&leaf);
        min = Some(
            min.map(|m| Float3::new(m.x.min(c.x), m.y.min(c.y), m.z.min(c.z)))
                .unwrap_or(c),
        );
        max = Some(
            max.map(|m| Float3::new(m.x.max(c.x), m.y.max(c.y), m.z.max(c.z)))
                .unwrap_or(c),
        );
    }
    Ok((min.unwrap_or(Float3::ZERO) + max.unwrap_or(Float3::ZERO)) / 2.0)
}

fn read_vec3_value(value: Value, name: &'static str) -> Result<Float3, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::List(list) if list.elements().len() == 3 => {
            let comps = list
                .elements()
                .iter()
                .map(|key| match executor::heap::with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => Ok(n as f32),
                    Value::Float(f) => Ok(f as f32),
                    other => Err(ExecutorError::type_error_for(
                        "number",
                        other.type_name(),
                        name,
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Float3::new(comps[0], comps[1], comps[2]))
        }
        other => Err(ExecutorError::type_error_for(
            "3-vector",
            other.type_name(),
            name,
        )),
    }
}

fn read_path_arc_value(value: Value) -> Result<Float3, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::Integer(0) => Ok(Float3::ZERO),
        Value::Float(f) if f == 0.0 => Ok(Float3::ZERO),
        other => read_vec3_value(other, "path_arc"),
    }
}

fn write_start_mesh(mesh: &Mesh) -> Mesh {
    let mut out = mesh.clone();
    out.uniform.alpha = 0.0;
    for lin in &mut out.lins {
        lin.a.col.w = 0.0;
        lin.b.col.w = 0.0;
    }
    for tri in &mut out.tris {
        tri.a.col.w = 0.0;
        tri.b.col.w = 0.0;
        tri.c.col.w = 0.0;
    }
    for dot in &mut out.dots {
        dot.col.w = 0.0;
    }
    debug_assert!(out.has_consistent_line_links());
    out
}

fn write_tree_value(value: &Value, t: f32) -> Result<Value, ExecutorError> {
    match value.clone().elide_lvalue_leader_rec() {
        Value::Mesh(mesh) => Ok(Value::Mesh(Arc::new(write_mesh(mesh.as_ref(), t)))),
        Value::List(list) => {
            let mut out = Vec::with_capacity(list.len());
            for elem in list.elements() {
                let elem = executor::heap::with_heap(|h| h.get(elem.key()).clone());
                out.push(write_tree_value(&elem, t)?);
            }
            Ok(list_value(out))
        }
        other => Err(ExecutorError::type_error("mesh / list", other.type_name())),
    }
}

fn write_mesh(mesh: &Mesh, t: f32) -> Mesh {
    let mut out = mesh.clone();
    out.uniform.alpha = mesh.uniform.alpha;

    for dot in &mut out.dots {
        dot.col.w *= t;
    }

    let count = out.lins.len().max(1) as f32;
    for (i, lin) in out.lins.iter_mut().enumerate() {
        let start_t = i as f32 / count;
        let end_t = (i as f32 + 1.0) / count;
        if t <= start_t {
            lin.b.pos = lin.a.pos;
            lin.a.col.w = 0.0;
            lin.b.col.w = 0.0;
        } else if t < end_t {
            let local = (t - start_t) / (end_t - start_t);
            lin.b.pos = lin.a.pos.lerp(lin.b.pos, local);
            lin.a.col.w *= local;
            lin.b.col.w *= local;
        }
    }

    for tri in &mut out.tris {
        tri.a.col.w *= t;
        tri.b.col.w *= t;
        tri.c.col.w *= t;
    }

    debug_assert!(out.has_consistent_line_links());
    out
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
                return Err(ExecutorError::Other(format!(
                    "cannot trans vectors of different lengths: {} vs {} vs {}",
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
            Ok(Value::List(Rc::new(executor::value::container::List::new_with(
                elements,
            ))))
        }
        (start, end, state) => Err(ExecutorError::Other(format!(
            "cannot trans {} and {} with state {}",
            start.type_name(),
            end.type_name(),
            state.type_name()
        ))),
    }
}

fn mesh_tree_patharc_lerp(
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
                return Err(ExecutorError::Other(format!(
                    "cannot trans vectors of different lengths: {} vs {}",
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
            Ok(Value::List(Rc::new(executor::value::container::List::new_with(
                elements,
            ))))
        }
        (start, end) => Err(ExecutorError::Other(format!(
            "cannot trans {} and {}",
            start.type_name(),
            end.type_name()
        ))),
    }
}

fn mesh_tree_bend_lerp(start: &Value, end: &Value, t: f32) -> Result<Value, ExecutorError> {
    mesh_tree_patharc_lerp(start, end, t, Float3::ZERO)
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
    let contours = extract_closed_contours(&boundary).ok_or_else(|| {
        ExecutorError::Other("planar trans produced a non-closed contour".into())
    })?;
    let contour_points: Vec<_> = contours.iter().map(|contour| contour.points.clone()).collect();
    let normal = boundary
        .lins
        .first()
        .map(|line| line.norm)
        .unwrap_or(Float3::Z);
    let (mut lins, mut tris) = tessellate_planar_loops(&contour_points, normal)?;

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

fn ensure_same_mesh_topology(start: &Mesh, end: &Mesh, op: &'static str) -> Result<(), ExecutorError> {
    if start.dots.len() != end.dots.len()
        || start.lins.len() != end.lins.len()
        || start.tris.len() != end.tris.len()
        || start
            .dots
            .iter()
            .zip(&end.dots)
            .any(|(a, b)| (a.inv, a.anti, a.is_dom_sib) != (b.inv, b.anti, b.is_dom_sib))
        || start
            .lins
            .iter()
            .zip(&end.lins)
            .any(|(a, b)| {
                (a.prev, a.next, a.inv, a.anti, a.is_dom_sib)
                    != (b.prev, b.next, b.inv, b.anti, b.is_dom_sib)
            })
        || start
            .tris
            .iter()
            .zip(&end.tris)
            .any(|(a, b)| {
                (a.ab, a.bc, a.ca, a.anti, a.is_dom_sib)
                    != (b.ab, b.bc, b.ca, b.anti, b.is_dom_sib)
            })
    {
        return Err(ExecutorError::Other(format!(
            "cannot {} meshes with different topology",
            op
        )));
    }
    Ok(())
}

fn lerp_uniforms(start: &Uniforms, end: &Uniforms, t: f32) -> Uniforms {
    Uniforms {
        alpha: start.alpha + (end.alpha - start.alpha) * t as f64,
        ..end.clone()
    }
}

#[cfg(test)]
mod tests {
    use geo::{
        mesh::{Lin, LinVertex, Mesh, Uniforms},
        simd::{Float3, Float4},
    };

    use super::write_mesh;

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

        let written = write_mesh(&mesh, 0.0);

        assert_eq!(written.lins[0].prev, -1);
        assert_eq!(written.lins[0].next, 1);
        assert_eq!(written.lins[1].prev, 0);
        assert_eq!(written.lins[1].next, -1);
        assert!(written.has_consistent_line_links());
    }
}

fn vec3_norm_lerp(start: Float3, t: f32, end: Float3) -> Float3 {
    let raw = start.lerp(end, t);
    let len = raw.len();
    if len <= 1e-6 {
        end
    } else {
        raw / len
    }
}

fn vec3_patharc_lerp(start: Float3, t: f32, end: Float3, path_arc: Float3) -> Float3 {
    if path_arc.len_sq() <= 1e-12 {
        return start.lerp(end, t);
    }

    let delta = end - start;
    let half = delta / 2.0;
    let normal = path_arc;
    let normal_len = normal.len();
    if normal_len <= 1e-6 {
        return start.lerp(end, t);
    }
    let normal = normal / normal_len;
    let binormal = half.cross(normal);
    if binormal.len_sq() <= 1e-12 {
        return start.lerp(end, t);
    }

    let center = start + half + path_arc;
    let from = start - center;
    let to = end - center;
    let denom = from.len() * to.len();
    if denom <= 1e-6 {
        return start.lerp(end, t);
    }
    let cos_theta = (from.dot(to) / denom).clamp(-1.0, 1.0);
    let angle = cos_theta.acos();
    if !angle.is_finite() || angle.abs() <= 1e-6 {
        return start.lerp(end, t);
    }

    let axis = from.cross(to);
    if axis.len_sq() <= 1e-12 {
        return start.lerp(end, t);
    }
    let axis = axis / axis.len();

    let theta = angle * t;
    let rotated =
        from * theta.cos() + axis.cross(from) * theta.sin() + axis * axis.dot(from) * (1.0 - theta.cos());
    center + rotated
}
