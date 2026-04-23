use std::{collections::HashSet, sync::Arc};

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{
    mesh::{Dot, Lin, Mesh, Tri},
    simd::Float3,
};
use stdlib_macros::stdlib_func;

use super::helpers::*;

fn decode_mesh_ref(value: i32) -> Option<usize> {
    (value < -1).then_some((-value - 2) as usize)
}

fn shift_dot_refs(dots: &mut [Dot], delta: usize) {
    let delta = delta as i32;
    for dot in dots {
        if dot.inv >= 0 {
            dot.inv += delta;
        }
        if dot.anti >= 0 {
            dot.anti += delta;
        }
    }
}

fn shift_line_point_ref(value: &mut i32, dot_delta: usize, line_delta: usize) {
    if *value >= 0 {
        *value += line_delta as i32;
    } else if let Some(dot_idx) = decode_mesh_ref(*value) {
        *value = mesh_ref(dot_idx + dot_delta);
    }
}

fn shift_line_surface_ref(value: &mut i32, tri_delta: usize, line_delta: usize) {
    if *value >= 0 {
        *value += line_delta as i32;
    } else if let Some(tri_idx) = decode_mesh_ref(*value) {
        *value = mesh_ref(tri_idx + tri_delta);
    }
}

fn shift_line_refs(lines: &mut [Lin], dot_delta: usize, line_delta: usize, tri_delta: usize) {
    for line in lines {
        shift_line_point_ref(&mut line.prev, dot_delta, line_delta);
        shift_line_point_ref(&mut line.next, dot_delta, line_delta);
        shift_line_surface_ref(&mut line.inv, tri_delta, line_delta);
        if line.anti >= 0 {
            line.anti += line_delta as i32;
        }
    }
}

fn shift_tri_edge_ref(value: &mut i32, line_delta: usize, tri_delta: usize) {
    if *value >= 0 {
        *value += tri_delta as i32;
    } else if let Some(line_idx) = decode_mesh_ref(*value) {
        *value = mesh_ref(line_idx + line_delta);
    }
}

fn shift_tri_refs(tris: &mut [Tri], line_delta: usize, tri_delta: usize) {
    for tri in tris {
        shift_tri_edge_ref(&mut tri.ab, line_delta, tri_delta);
        shift_tri_edge_ref(&mut tri.bc, line_delta, tri_delta);
        shift_tri_edge_ref(&mut tri.ca, line_delta, tri_delta);
        if tri.anti >= 0 {
            tri.anti += tri_delta as i32;
        }
    }
}

fn append_mesh_into(out: &mut Mesh, mesh: &Mesh) {
    let dot_delta = out.dots.len();
    let line_delta = out.lins.len();
    let tri_delta = out.tris.len();

    let mut dots = mesh.dots.clone();
    let mut lins = mesh.lins.clone();
    let mut tris = mesh.tris.clone();

    shift_dot_refs(&mut dots, dot_delta);
    shift_line_refs(&mut lins, dot_delta, line_delta, tri_delta);
    shift_tri_refs(&mut tris, line_delta, tri_delta);

    out.dots.extend(dots);
    out.lins.extend(lins);
    out.tris.extend(tris);
    for &tag in &mesh.tag {
        if !out.tag.contains(&tag) {
            out.tag.push(tag);
        }
    }
}

#[stdlib_func]
pub async fn tag_filter(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let filter = read_tag_filter(executor, stack_idx, -1, "filter")?;
    Ok(filter_tree_by_tag_filter(executor, tree, &filter)
        .await?
        .map(MeshTree::into_value)
        .unwrap_or_else(|| list_value([])))
}

#[stdlib_func]
pub async fn tag_split(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let (matched, unmatched) = match filter.as_ref() {
        Some(filter) => split_tree_by_tag_filter(executor, tree, filter).await?,
        None => (Some(tree), None),
    };
    Ok(list_value([
        matched
            .map(MeshTree::into_value)
            .unwrap_or_else(|| list_value([])),
        unmatched
            .map(MeshTree::into_value)
            .unwrap_or_else(|| list_value([])),
    ]))
}

#[stdlib_func]
pub async fn mesh_collapse(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "target").await?;
    let mut iter = tree.iter();
    let Some(first) = iter.next() else {
        return Ok(mesh_from_parts(vec![], vec![], vec![]));
    };

    let mut out = first.clone();
    for mesh in iter {
        append_mesh_into(&mut out, mesh);
    }
    out.debug_assert_consistent_topology();
    Ok(Value::Mesh(out.into()))
}

#[stdlib_func]
pub async fn mesh_left(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(-1.0, 0.0, 0.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_right(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(1.0, 0.0, 0.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_up(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(0.0, 1.0, 0.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_down(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(0.0, -1.0, 0.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_forward(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(0.0, 0.0, 1.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_backward(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(0.0, 0.0, -1.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_direc(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "mesh").await?;
    let direction = read_float3(executor, stack_idx, -1, "direc")?;
    require_point(extremal_point(&tree, direction), "mesh")
}

#[stdlib_func]
pub async fn mesh_width(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let (min, max) = bounds_of(&tree).ok_or(ExecutorError::InvalidArgument {
        arg: "mesh",
        message: "mesh tree must contain at least one vertex",
    })?;
    Ok(Value::Float((max.x - min.x) as f64))
}

#[stdlib_func]
pub async fn mesh_height(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let (min, max) = bounds_of(&tree).ok_or(ExecutorError::InvalidArgument {
        arg: "mesh",
        message: "mesh tree must contain at least one vertex",
    })?;
    Ok(Value::Float((max.y - min.y) as f64))
}

#[stdlib_func]
pub async fn mesh_center(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let (min, max) = bounds_of(&tree).ok_or(ExecutorError::InvalidArgument {
        arg: "mesh",
        message: "mesh tree must contain at least one vertex",
    })?;
    Ok(point_value((min + max) / 2.0))
}

#[stdlib_func]
pub async fn mesh_rank(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    Ok(Value::Integer(
        tree.iter().map(axis_aligned_rank).max().unwrap_or(-1),
    ))
}

#[stdlib_func]
pub async fn mesh_tags(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for mesh in tree.iter() {
        for &tag in &mesh.tag {
            if seen.insert(tag) {
                tags.push(Value::Integer(tag as i64));
            }
        }
    }
    Ok(list_value(tags))
}

#[stdlib_func]
pub async fn mesh_sample(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "mesh").await?;
    let t = crate::read_float(executor, stack_idx, -1, "t")?.clamp(0.0, 1.0) as f32;

    let mut lines = Vec::new();
    let mut tris = Vec::new();
    let mut dots = Vec::new();
    for mesh in tree.iter() {
        dots.extend(mesh.dots.iter().map(|dot| dot.pos));
        lines.extend(mesh.lins.iter().map(|lin| (lin.a.pos, lin.b.pos)));
        tris.extend(
            mesh.tris
                .iter()
                .map(|tri| (tri.a.pos, tri.b.pos, tri.c.pos)),
        );
    }

    if !lines.is_empty() {
        let lengths: Vec<_> = lines.iter().map(|(a, b)| (*b - *a).len()).collect();
        let total = lengths.iter().sum::<f32>().max(1e-6);
        let mut target = t * total;
        for ((a, b), len) in lines.into_iter().zip(lengths) {
            if target <= len {
                return Ok(point_value(a.lerp(b, target / len.max(1e-6))));
            }
            target -= len;
        }
    }

    if !tris.is_empty() {
        let idx = ((tris.len() - 1) as f32 * t).round() as usize;
        let (a, b, c) = tris[idx];
        return Ok(point_value((a + b + c) / 3.0));
    }

    if !dots.is_empty() {
        let idx = ((dots.len() - 1) as f32 * t).round() as usize;
        return Ok(point_value(dots[idx]));
    }

    Err(ExecutorError::InvalidArgument {
        arg: "mesh",
        message: "mesh tree must contain at least one vertex",
    })
}

#[stdlib_func]
pub async fn mesh_normal(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "mesh").await?;
    let t = crate::read_float(executor, stack_idx, -1, "t")?.clamp(0.0, 1.0) as f32;
    let mut line_normals = Vec::new();
    let mut tri_normals = Vec::new();
    let mut dot_normals = Vec::new();
    for mesh in tree.iter() {
        dot_normals.extend(mesh.dots.iter().map(|dot| dot.norm));
        line_normals.extend(mesh.lins.iter().map(|lin| lin.norm));
        tri_normals.extend(
            mesh.tris
                .iter()
                .map(|tri| triangle_normal(tri.a.pos, tri.b.pos, tri.c.pos)),
        );
    }
    let normal = if !line_normals.is_empty() {
        line_normals[((line_normals.len() - 1) as f32 * t).round() as usize]
    } else if !tri_normals.is_empty() {
        tri_normals[((tri_normals.len() - 1) as f32 * t).round() as usize]
    } else if !dot_normals.is_empty() {
        dot_normals[((dot_normals.len() - 1) as f32 * t).round() as usize]
    } else {
        return Err(ExecutorError::InvalidArgument {
            arg: "mesh",
            message: "mesh tree must contain at least one vertex",
        });
    };
    Ok(point_value(normal))
}

#[stdlib_func]
pub async fn mesh_tangent(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "mesh").await?;
    let t = crate::read_float(executor, stack_idx, -1, "t")?.clamp(0.0, 1.0) as f32;
    let mut tangents = Vec::new();
    for mesh in tree.iter() {
        tangents.extend(
            mesh.lins
                .iter()
                .map(|lin| (lin.b.pos - lin.a.pos).normalize()),
        );
        tangents.extend(
            mesh.tris
                .iter()
                .map(|tri| (tri.b.pos - tri.a.pos).normalize()),
        );
    }
    if tangents.is_empty() {
        return Err(ExecutorError::InvalidArgument {
            arg: "mesh",
            message: "mesh tree must contain at least one edge",
        });
    }
    Ok(point_value(
        tangents[((tangents.len() - 1) as f32 * t).round() as usize],
    ))
}

#[stdlib_func]
pub async fn mesh_contains(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "mesh").await?;
    let point = read_float3(executor, stack_idx, -1, "point")?;
    let contains = tree.iter().any(|mesh| {
        mesh.tris.iter().any(|tri| {
            let normal = triangle_normal(tri.a.pos, tri.b.pos, tri.c.pos);
            let area = normal
                .dot((tri.b.pos - tri.a.pos).cross(tri.c.pos - tri.a.pos))
                .abs();
            let a = normal
                .dot((tri.c.pos - tri.b.pos).cross(point - tri.b.pos))
                .abs();
            let b = normal
                .dot((tri.a.pos - tri.c.pos).cross(point - tri.c.pos))
                .abs();
            let c = normal
                .dot((tri.b.pos - tri.a.pos).cross(point - tri.a.pos))
                .abs();
            (a + b + c - area).abs() < 1e-3
        }) || mesh
            .lins
            .iter()
            .any(|lin| segment_distance(lin.a.pos, lin.b.pos, point) < 1e-4)
            || mesh.dots.iter().any(|dot| (dot.pos - point).len() < 1e-4)
    });
    Ok(Value::Integer(contains as i64))
}

#[stdlib_func]
pub async fn mesh_dist(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn triangle_distance(a: Float3, b: Float3, c: Float3, point: Float3) -> f32 {
        let normal = triangle_normal(a, b, c);
        let plane_dist = normal.dot(point - a);
        let projected = point - normal * plane_dist;
        let area = normal.dot((b - a).cross(c - a)).abs();
        let aa = normal.dot((c - b).cross(projected - b)).abs();
        let bb = normal.dot((a - c).cross(projected - c)).abs();
        let cc = normal.dot((b - a).cross(projected - a)).abs();
        if (aa + bb + cc - area).abs() < 1e-3 {
            plane_dist.abs()
        } else {
            segment_distance(a, b, point)
                .min(segment_distance(b, c, point))
                .min(segment_distance(c, a, point))
        }
    }

    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "mesh").await?;
    let point = read_float3(executor, stack_idx, -1, "test_point")?;
    let mut dist = f32::INFINITY;
    for mesh in tree.iter() {
        for tri in &mesh.tris {
            dist = dist.min(triangle_distance(tri.a.pos, tri.b.pos, tri.c.pos, point));
        }
        for lin in &mesh.lins {
            dist = dist.min(segment_distance(lin.a.pos, lin.b.pos, point));
        }
        for dot in &mesh.dots {
            dist = dist.min((dot.pos - point).len());
        }
    }
    Ok(Value::Float(dist as f64))
}

#[stdlib_func]
pub async fn mesh_raycast(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -3, "mesh").await?;
    let src = read_float3(executor, stack_idx, -2, "src")?;
    let direction = read_float3(executor, stack_idx, -1, "direction")?;
    let len = direction.len();
    if len <= 1e-12 {
        return Ok(Value::Float(-1.0));
    }
    let dir = direction / len;
    let hit = tree
        .iter()
        .flat_map(|mesh| mesh.tris.iter())
        .filter_map(|tri| ray_triangle_intersection(src, dir, tri.a.pos, tri.b.pos, tri.c.pos))
        .min_by(|a, b| a.total_cmp(b));
    Ok(Value::Float(hit.map(|t| (t / len) as f64).unwrap_or(-1.0)))
}

#[stdlib_func]
pub async fn mesh_vertex_set(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let mut seen = HashSet::new();
    let mut vertices = Vec::new();
    for mesh in tree.iter() {
        for vertex in mesh_vertices(mesh) {
            if seen.insert(float3_key(vertex)) {
                vertices.push(point_value(vertex));
            }
        }
    }
    Ok(list_value(vertices))
}

#[stdlib_func]
pub async fn mesh_edge_set(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let mut seen = HashSet::new();
    let mut edges = Vec::new();
    for mesh in tree.iter() {
        for lin in &mesh.lins {
            let key = canonical_edge_key(lin.a.pos, lin.b.pos);
            if seen.insert(key) {
                edges.push(edge_value(lin.a.pos, lin.b.pos));
            }
        }
        for tri in &mesh.tris {
            for (a, b) in [
                (tri.a.pos, tri.b.pos),
                (tri.b.pos, tri.c.pos),
                (tri.c.pos, tri.a.pos),
            ] {
                let key = canonical_edge_key(a, b);
                if seen.insert(key) {
                    edges.push(edge_value(a, b));
                }
            }
        }
    }
    Ok(list_value(edges))
}

#[stdlib_func]
pub async fn mesh_triangle_set(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let mut seen = HashSet::new();
    let mut triangles = Vec::new();
    for mesh in tree.iter() {
        for tri in &mesh.tris {
            let key = triangle_key(tri.a.pos, tri.b.pos, tri.c.pos);
            if seen.insert(key) {
                triangles.push(triangle_value(tri.a.pos, tri.b.pos, tri.c.pos));
            }
        }
    }
    Ok(list_value(triangles))
}

#[stdlib_func]
pub async fn mesh_contour_count(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    Ok(Value::Integer(tree.iter().count() as i64))
}

#[stdlib_func]
pub async fn mesh_contour_separate(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    Ok(list_value(tree.flatten().into_iter().enumerate().map(
        |(index, mesh)| {
            let mut mesh = mesh.as_ref().clone();
            mesh.tag = vec![index as isize];
            Value::Mesh(Arc::new(mesh))
        },
    )))
}
