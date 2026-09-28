use std::collections::HashMap;

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{
    mesh::{Lin, Mesh},
    mesh_build::{BoundaryEdge, BoundaryEdges, IndexedSurface, SurfaceVertex},
    simd::Float3,
};
use stdlib_macros::stdlib_func;

use crate::mesh::helpers::*;

use super::*;

fn midpoint_vertex(
    vertices: &mut Vec<SurfaceVertex>,
    edge_midpoints: &mut HashMap<(usize, usize), usize>,
    a: usize,
    b: usize,
) -> usize {
    let key = if a <= b { (a, b) } else { (b, a) };
    if let Some(&idx) = edge_midpoints.get(&key) {
        return idx;
    }

    let idx = vertices.len();
    vertices.push(SurfaceVertex {
        pos: vertices[a].pos.lerp(vertices[b].pos, 0.5),
        col: vertices[a].col.lerp(vertices[b].col, 0.5),
        uv: vertices[a].uv.lerp(vertices[b].uv, 0.5),
    });
    edge_midpoints.insert(key, idx);
    idx
}

pub(super) fn subdivide_indexed_surface(surface: &IndexedSurface) -> IndexedSurface {
    let mut vertices = surface.vertices.clone();
    let mut edge_midpoints = HashMap::<(usize, usize), usize>::new();
    let mut faces = Vec::with_capacity(surface.faces.len() * 4);

    for &[a, b, c] in &surface.faces {
        let ab = midpoint_vertex(&mut vertices, &mut edge_midpoints, a, b);
        let bc = midpoint_vertex(&mut vertices, &mut edge_midpoints, b, c);
        let ca = midpoint_vertex(&mut vertices, &mut edge_midpoints, c, a);
        faces.push([a, ab, ca]);
        faces.push([ab, b, bc]);
        faces.push([ca, bc, c]);
        faces.push([ab, bc, ca]);
    }

    let mut boundary_edges = BoundaryEdges::with_capacity_and_hasher(
        surface.boundary_edges.len() * 2,
        Default::default(),
    );
    for (&(a, b), template) in &surface.boundary_edges {
        let mid = edge_midpoints[&if a <= b { (a, b) } else { (b, a) }];
        boundary_edges.insert(
            (a, mid),
            BoundaryEdge {
                a_col: template.a_col,
                b_col: template.a_col.lerp(template.b_col, 0.5),
                norm: template.norm,
            },
        );
        boundary_edges.insert(
            (mid, b),
            BoundaryEdge {
                a_col: template.a_col.lerp(template.b_col, 0.5),
                b_col: template.b_col,
                norm: template.norm,
            },
        );
    }

    IndexedSurface {
        vertices,
        faces,
        boundary_edges,
    }
}

pub(super) fn clear_surface_line_refs(mesh: &mut Mesh) {
    for tri in &mut mesh.tris {
        for edge in [&mut tri.ab, &mut tri.bc, &mut tri.ca] {
            if *edge < -1 {
                *edge = -1;
            }
        }
    }
}

fn linked_prev(lines: &[Lin], idx: usize) -> Option<usize> {
    let prev = lines[idx].prev;
    (prev >= 0)
        .then_some(prev as usize)
        .filter(|&prev| prev < lines.len() && lines[prev].next == idx as i32)
}

fn linked_next(lines: &[Lin], idx: usize) -> Option<usize> {
    let next = lines[idx].next;
    (next >= 0)
        .then_some(next as usize)
        .filter(|&next| next < lines.len() && lines[next].prev == idx as i32)
}

pub(super) fn line_paths(lines: &[Lin]) -> Vec<Vec<usize>> {
    let mut visited = vec![false; lines.len()];
    let mut out = Vec::new();

    let mut walk_path = |start: usize, visited: &mut [bool]| {
        let mut path = Vec::new();
        let mut cursor = start;
        loop {
            if visited[cursor] {
                break;
            }
            visited[cursor] = true;
            path.push(cursor);

            let Some(next) = linked_next(lines, cursor) else {
                break;
            };
            if next == start || visited[next] {
                break;
            }
            cursor = next;
        }

        if !path.is_empty() {
            out.push(path);
        }
    };

    for idx in 0..lines.len() {
        if visited[idx] || linked_prev(lines, idx).is_some() {
            continue;
        }
        walk_path(idx, &mut visited);
    }

    for idx in 0..lines.len() {
        if visited[idx] {
            continue;
        }
        walk_path(idx, &mut visited);
    }

    out
}

#[stdlib_func]
pub async fn op_uprank(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let mut tessellation_error = None;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        match uprank_mesh(mesh) {
            Ok(Some(upranked)) => *mesh = upranked,
            Ok(None) => {}
            Err(err) => tessellation_error = Some(err),
        }
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    if let Some(err) = tessellation_error {
        return Err(err);
    }
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_downrank(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if !mesh.tris.is_empty() {
            if !mesh.lins.is_empty() {
                mesh.lins = mesh
                    .lins
                    .iter()
                    .map(|lin| {
                        let mut out = *lin;
                        out.inv = -1;
                        out
                    })
                    .collect();
            } else {
                let surface = mesh_to_indexed_surface(mesh);
                mesh.lins = build_indexed_surface(
                    &surface.vertices,
                    &surface.faces,
                    &surface.boundary_edges,
                )
                .0
                .into_iter()
                .map(|lin| {
                    let mut out = lin;
                    out.inv = -1;
                    out
                })
                .collect();
            }
            mesh.tris.clear();
        } else if !mesh.lins.is_empty() {
            mesh.dots = mesh
                .lins
                .iter()
                .flat_map(|lin| {
                    [
                        default_dot(lin.a.pos, lin.norm),
                        default_dot(lin.b.pos, lin.norm),
                    ]
                })
                .collect();
            mesh.lins.clear();
        }
        mesh.normalize_line_dot_topology();
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_wireframe(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    op_downrank(executor, stack_idx).await
}

pub(super) fn subdivide_line_mesh(mesh: &mut Mesh, factor: usize) {
    let original_lins = mesh.lins.clone();
    let mut lins = Vec::with_capacity(original_lins.len() * factor);

    for (lin_idx, lin) in original_lins.iter().enumerate() {
        let base = lin_idx * factor;
        for i in 0..factor {
            let u = i as f32 / factor as f32;
            let v = (i + 1) as f32 / factor as f32;
            let a = if i == 0 {
                lin.a.pos
            } else {
                lin.a.pos.lerp(lin.b.pos, u)
            };
            let b = if i + 1 == factor {
                lin.b.pos
            } else {
                lin.a.pos.lerp(lin.b.pos, v)
            };
            let mut out = default_lin(a, b, lin.norm);
            out.a.col = lin.a.col.lerp(lin.b.col, u);
            out.b.col = lin.a.col.lerp(lin.b.col, v);
            out.prev = if i == 0 {
                if lin.prev >= 0 {
                    (lin.prev as usize * factor + (factor - 1)) as i32
                } else {
                    lin.prev
                }
            } else {
                (base + i - 1) as i32
            };
            out.next = if i + 1 == factor {
                if lin.next >= 0 {
                    (lin.next as usize * factor) as i32
                } else {
                    lin.next
                }
            } else {
                (base + i + 1) as i32
            };
            out.inv = if lin.inv >= 0 {
                (lin.inv as usize * factor + (factor - 1 - i)) as i32
            } else {
                lin.inv
            };
            out.is_dom_sib = lin.is_dom_sib;
            lins.push(out);
        }
    }

    for line_idx in 0..original_lins.len() {
        let inv_idx = original_lins[line_idx].inv;
        if inv_idx < 0 {
            continue;
        }
        let inv_idx = inv_idx as usize;
        if inv_idx >= original_lins.len() || line_idx >= inv_idx {
            continue;
        }

        for i in 0..factor {
            let line_piece_idx = line_idx * factor + i;
            let inv_piece_idx = inv_idx * factor + (factor - 1 - i);
            let line_piece = lins[line_piece_idx];
            lins[inv_piece_idx].a.pos = line_piece.b.pos;
            lins[inv_piece_idx].b.pos = line_piece.a.pos;
        }
    }

    for (dot_idx, dot) in mesh.dots.iter_mut().enumerate() {
        let Some(line_idx) = decode_mesh_ref(dot.inv) else {
            continue;
        };
        let Some(line) = original_lins.get(line_idx) else {
            continue;
        };
        let dot_ref = mesh_ref(dot_idx);
        if line.prev == dot_ref {
            dot.inv = mesh_ref(line_idx * factor);
        } else if line.next == dot_ref {
            dot.inv = mesh_ref(line_idx * factor + factor - 1);
        }
    }

    mesh.lins = lins;
}

#[stdlib_func]
pub async fn op_subdivide(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let factor = read_int(executor, stack_idx, -2, "factor")?.max(1) as usize;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if !mesh.tris.is_empty() {
            let mut surface = mesh_to_indexed_surface(mesh);
            for _ in 1..factor {
                surface = subdivide_indexed_surface(&surface);
            }
            let (lins, tris) =
                build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
            mesh.lins = lins;
            mesh.tris = tris;
        } else if !mesh.lins.is_empty() && factor > 1 {
            subdivide_line_mesh(mesh, factor);
        }
        mesh.debug_assert_consistent_topology();
    })
    .await?;

    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_tesselated(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let depth = read_int(executor, stack_idx, -2, "depth")?.max(0) as usize;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if !mesh.tris.is_empty() {
            let mut surface = mesh_to_indexed_surface(mesh);
            for _ in 0..depth {
                surface = subdivide_indexed_surface(&surface);
            }
            let (lins, tris) =
                build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges);
            mesh.lins = lins;
            mesh.tris = tris;
        }
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_extrude(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let delta = read_float3(executor, stack_idx, -2, "delta")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let mut extrude_error = None;

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if extrude_error.is_some() {
            return;
        }
        if mesh.tris.is_empty() {
            extrude_error = Some(ExecutorError::invalid_operation(
                "can only extrude meshes that have triangles",
            ));
            return;
        }
        if mesh.lins.iter().any(|lin| lin.is_dom_sib && lin.inv >= 0) {
            extrude_error = Some(ExecutorError::invalid_operation(
                "cannot extrude meshes that have standalone line loops; try upranking first",
            ));
            return;
        }

        let surface = mesh_to_indexed_surface(mesh);
        let base_vertex_count = surface.vertices.len();
        let mut vertices = surface.vertices.clone();
        vertices.extend(surface.vertices.iter().map(|vertex| SurfaceVertex {
            pos: vertex.pos + delta,
            col: vertex.col,
            uv: vertex.uv,
        }));

        let mut faces =
            Vec::with_capacity(surface.faces.len() * 2 + surface.boundary_edges.len() * 2);
        faces.extend(surface.faces.iter().copied());
        faces.extend(surface.faces.iter().map(|&[a, b, c]| {
            [
                c + base_vertex_count,
                b + base_vertex_count,
                a + base_vertex_count,
            ]
        }));
        for &(a, b) in surface.boundary_edges.keys() {
            faces.push([b, a, a + base_vertex_count]);
            faces.push([b, a + base_vertex_count, b + base_vertex_count]);
        }

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &BoundaryEdges::default());
        mesh.lins = lins;
        mesh.tris = tris;
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    if let Some(err) = extrude_error {
        return Err(err);
    }

    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_revolve(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let rotation = read_float3(executor, stack_idx, -2, "rotation")?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let angle = rotation.len();
    let axis = if angle <= 1e-6 {
        Float3::Y
    } else {
        rotation / angle
    };
    let full_turn = angle >= std::f32::consts::TAU - 1e-3;
    let steps = ((angle.abs() / (std::f32::consts::TAU / 24.0)).ceil() as usize).max(1);
    let mut revolve_error = None;

    tree.for_each_filtered(executor, filter.as_ref(), &mut |mesh| {
        if revolve_error.is_some() {
            return;
        }
        if mesh.lins.is_empty() || !mesh.tris.is_empty() {
            revolve_error = Some(ExecutorError::invalid_operation(
                "can only revolve meshes that are line meshes",
            ));
            return;
        }

        let profile = mesh_to_indexed_lines(mesh);
        let ring_count = if full_turn { steps } else { steps + 1 };
        let mut vertices = Vec::with_capacity(profile.vertices.len() * ring_count);
        for step in 0..ring_count {
            let theta = angle * step as f32 / steps as f32;
            for vertex in &profile.vertices {
                vertices.push(SurfaceVertex {
                    pos: rotate_about_axis(vertex.pos, axis, theta),
                    col: vertex.col,
                    uv: vertex.uv,
                });
            }
        }

        let mut faces = Vec::with_capacity(profile.segments.len() * steps * 2);
        let ring_stride = profile.vertices.len();
        let ring_vertex = |step: usize, vertex: usize| step * ring_stride + vertex;
        for &[a, b] in &profile.segments {
            for step in 0..steps {
                let next = if full_turn {
                    (step + 1) % ring_count
                } else {
                    step + 1
                };
                let a0 = ring_vertex(step, a);
                let b0 = ring_vertex(step, b);
                let a1 = ring_vertex(next, a);
                let b1 = ring_vertex(next, b);
                faces.push([a0, b0, b1]);
                faces.push([a0, b1, a1]);
            }
        }

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &BoundaryEdges::default());
        mesh.lins = lins;
        mesh.tris = tris;
        mesh.debug_assert_consistent_topology();
    })
    .await?;
    if let Some(err) = revolve_error {
        return Err(err);
    }

    Ok(tree.into_value())
}
