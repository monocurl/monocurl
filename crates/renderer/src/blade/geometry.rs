use std::collections::HashMap;

use geo::{
    mesh::Mesh,
    simd::{Float3, Float4},
};

use crate::{RenderSize, RenderStyle};

use super::{
    DEFAULT_LINE_MITER_SCALE, LINE_VERTICES_PER_INSTANCE, REFERENCE_WIDTH,
    types::{
        DotInstancePod, LineVertexPod, PositionKey, TriVertexPod, float4_from_float4,
        float4_from_xyz,
    },
};

pub(super) fn build_triangle_vertices(mesh: &Mesh) -> Vec<TriVertexPod> {
    let visible_triangles = mesh
        .tris
        .iter()
        .filter(|tri| {
            !(tri.a.col.w <= f32::EPSILON
                && tri.b.col.w <= f32::EPSILON
                && tri.c.col.w <= f32::EPSILON)
        })
        .count();
    let smooth_normals = mesh.uniform.smooth.then(|| averaged_triangle_normals(mesh));
    let mut vertices = Vec::with_capacity(visible_triangles * 3);
    for tri in &mesh.tris {
        if tri.a.col.w <= f32::EPSILON && tri.b.col.w <= f32::EPSILON && tri.c.col.w <= f32::EPSILON
        {
            continue;
        }
        let face_normal = triangle_face_normal(tri.a.pos, tri.b.pos, tri.c.pos);
        let a_normal = triangle_vertex_normal(smooth_normals.as_ref(), tri.a.pos, face_normal);
        let b_normal = triangle_vertex_normal(smooth_normals.as_ref(), tri.b.pos, face_normal);
        let c_normal = triangle_vertex_normal(smooth_normals.as_ref(), tri.c.pos, face_normal);

        vertices.push(tri_vertex(tri.a, a_normal));
        vertices.push(tri_vertex(tri.b, b_normal));
        vertices.push(tri_vertex(tri.c, c_normal));
    }
    vertices
}

fn averaged_triangle_normals(mesh: &Mesh) -> HashMap<PositionKey, Float3> {
    let mut normals = HashMap::new();

    for tri in &mesh.tris {
        if tri.a.col.w <= f32::EPSILON && tri.b.col.w <= f32::EPSILON && tri.c.col.w <= f32::EPSILON
        {
            continue;
        }

        let area_normal = triangle_normal_sum(tri.a.pos, tri.b.pos, tri.c.pos);
        if area_normal.len_sq() <= 1e-12 {
            continue;
        }

        for pos in [tri.a.pos, tri.b.pos, tri.c.pos] {
            normals
                .entry(PositionKey::new(pos))
                .and_modify(|normal| *normal += area_normal)
                .or_insert(area_normal);
        }
    }

    normals
}

fn triangle_vertex_normal(
    smooth_normals: Option<&HashMap<PositionKey, Float3>>,
    pos: Float3,
    fallback: Float3,
) -> Float3 {
    smooth_normals
        .and_then(|normals| normals.get(&PositionKey::new(pos)).copied())
        .filter(|normal| normal.len_sq() > 1e-12)
        .map(Float3::normalize)
        .unwrap_or(fallback)
}

pub(super) fn build_line_vertices(mesh: &Mesh) -> Vec<LineVertexPod> {
    let mut lines = Vec::with_capacity(mesh.lins.len() * LINE_VERTICES_PER_INSTANCE as usize);
    for line_idx in 0..mesh.lins.len() {
        let Some((source_idx, source)) = rendered_line_source(mesh, line_idx) else {
            continue;
        };

        let prev = resolved_line_neighbor(mesh, source_idx, source.prev, NeighborDirection::Prev)
            .unwrap_or(source);
        let next = resolved_line_neighbor(mesh, source_idx, source.next, NeighborDirection::Next)
            .unwrap_or(source);
        let tangent = source.b.pos - source.a.pos;
        let prev_tangent = source.a.pos - prev.a.pos;
        let next_tangent = next.b.pos - source.b.pos;

        lines.extend([
            line_vertex(source.a.pos, source.a.col, tangent, prev_tangent, 1.0),
            line_vertex(source.a.pos, source.a.col, tangent, tangent, 0.0),
            line_vertex(source.a.pos, source.a.col, tangent, tangent, 1.0),
            line_vertex(source.b.pos, source.b.col, tangent, tangent, 0.0),
            line_vertex(source.b.pos, source.b.col, tangent, tangent, 1.0),
            line_vertex(source.b.pos, source.b.col, tangent, next_tangent, 1.0),
        ]);
    }
    lines
}

fn rendered_line_source(mesh: &Mesh, line_idx: usize) -> Option<(usize, &geo::mesh::Lin)> {
    let line = &mesh.lins[line_idx];
    let Some(inv_idx) = line_inverse_index(mesh, line.inv) else {
        return line_visible(line).then_some((line_idx, line));
    };

    let inverse = &mesh.lins[inv_idx];

    if line.is_dom_sib {
        return line_visible(line).then_some((line_idx, line));
    }
    if inverse.is_dom_sib {
        return None;
    }

    if line_idx > inv_idx {
        return None;
    }
    if !line_visible(line) && line_visible(inverse) {
        return Some((inv_idx, inverse));
    }

    line_visible(line).then_some((line_idx, line))
}

#[derive(Clone, Copy)]
enum NeighborDirection {
    Prev,
    Next,
}

fn resolved_line_neighbor(
    mesh: &Mesh,
    line_idx: usize,
    explicit: i32,
    direction: NeighborDirection,
) -> Option<&geo::mesh::Lin> {
    if let Some(neighbor) = line_neighbor(mesh, explicit)
        .filter(|neighbor| line_neighbor_matches(&mesh.lins[line_idx], neighbor, direction))
    {
        return Some(neighbor);
    }

    let current = &mesh.lins[line_idx];
    let inverse_idx = line_inverse_index(mesh, current.inv);
    let candidates = mesh
        .lins
        .iter()
        .enumerate()
        .filter(|(candidate_idx, candidate)| {
            *candidate_idx != line_idx
                && Some(*candidate_idx) != inverse_idx
                && line_visible(candidate)
                && line_neighbor_matches(current, candidate, direction)
        })
        .map(|(_, candidate)| candidate)
        .collect::<Vec<_>>();

    if candidates.len() == 1 {
        Some(candidates[0])
    } else {
        None
    }
}

fn line_neighbor_matches(
    current: &geo::mesh::Lin,
    candidate: &geo::mesh::Lin,
    direction: NeighborDirection,
) -> bool {
    match direction {
        NeighborDirection::Prev => {
            same_position(candidate.b.pos, current.a.pos)
                && !same_position(candidate.a.pos, current.b.pos)
        }
        NeighborDirection::Next => {
            same_position(candidate.a.pos, current.b.pos)
                && !same_position(candidate.b.pos, current.a.pos)
        }
    }
}

fn line_neighbor(mesh: &Mesh, index: i32) -> Option<&geo::mesh::Lin> {
    (index >= 0)
        .then_some(index as usize)
        .and_then(|index| mesh.lins.get(index))
}

fn line_inverse_index(mesh: &Mesh, index: i32) -> Option<usize> {
    (index >= 0)
        .then_some(index as usize)
        .filter(|&index| index < mesh.lins.len())
}

fn line_visible(line: &geo::mesh::Lin) -> bool {
    line.a.col.w > f32::EPSILON || line.b.col.w > f32::EPSILON
}

fn same_position(a: Float3, b: Float3) -> bool {
    (a - b).len_sq() <= 1e-12
}

pub(super) fn build_dot_instances(mesh: &Mesh) -> Vec<DotInstancePod> {
    let mut dots = Vec::with_capacity(mesh.dots.len());
    for dot in &mesh.dots {
        if dot.col.w <= f32::EPSILON {
            continue;
        }
        dots.push(DotInstancePod {
            pos: float4_from_xyz(dot.pos.x, dot.pos.y, dot.pos.z, 0.0),
            col: float4_from_float4(dot.col),
        });
    }
    dots
}

fn line_vertex(
    pos: Float3,
    col: Float4,
    tangent: Float3,
    prev_tangent: Float3,
    extrude: f32,
) -> LineVertexPod {
    LineVertexPod {
        pos: float4_from_xyz(pos.x, pos.y, pos.z, 0.0),
        col: float4_from_float4(col),
        tangent: float4_from_xyz(tangent.x, tangent.y, tangent.z, 0.0),
        prev_tangent: float4_from_xyz(prev_tangent.x, prev_tangent.y, prev_tangent.z, 0.0),
        extrude: [extrude, 0.0, 0.0, 0.0],
    }
}

fn tri_vertex(vertex: geo::mesh::TriVertex, normal: Float3) -> TriVertexPod {
    TriVertexPod {
        pos: float4_from_xyz(vertex.pos.x, vertex.pos.y, vertex.pos.z, 0.0),
        norm: float4_from_xyz(normal.x, normal.y, normal.z, 0.0),
        col: float4_from_float4(vertex.col),
        uv: [vertex.uv.x, vertex.uv.y, 0.0, 0.0],
    }
}

fn triangle_normal_sum(a: Float3, b: Float3, c: Float3) -> Float3 {
    (b - a).cross(c - a)
}

fn triangle_face_normal(a: Float3, b: Float3, c: Float3) -> Float3 {
    let normal = triangle_normal_sum(a, b, c);
    if normal.len_sq() <= 1e-12 {
        Float3::Z
    } else {
        normal.normalize()
    }
}

pub(super) fn build_dot_indices(vertex_count: u16) -> Vec<u16> {
    let vertex_count = vertex_count.max(3);
    let mut indices = Vec::with_capacity((vertex_count as usize - 2) * 3);
    for i in 1..vertex_count - 1 {
        indices.push(0);
        indices.push(i);
        indices.push(i + 1);
    }
    indices
}

pub(super) fn build_line_indices() -> [u16; 12] {
    [0, 2, 1, 1, 2, 4, 1, 4, 3, 3, 4, 5]
}

pub(super) fn mesh_line_radius_px(mesh: &Mesh, size: RenderSize, style: RenderStyle) -> f32 {
    let radius = if mesh.uniform.stroke_radius.is_finite() {
        mesh.uniform.stroke_radius.max(0.0)
    } else {
        style.line_width_px.max(0.0) * 0.5
    };
    radius * size.width.max(1) as f32 / REFERENCE_WIDTH
}

pub(super) fn mesh_line_miter_scale(mesh: &Mesh) -> f32 {
    if mesh.uniform.stroke_miter_radius_scale.is_finite() {
        mesh.uniform.stroke_miter_radius_scale.max(0.0)
    } else {
        DEFAULT_LINE_MITER_SCALE
    }
}

pub(super) fn mesh_dot_radius_px(mesh: &Mesh, style: RenderStyle) -> f32 {
    if mesh.uniform.dot_radius.is_finite() {
        mesh.uniform.dot_radius.max(0.0)
    } else {
        style.dot_radius_px.max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use geo::{
        mesh::{Lin, LinVertex, Mesh, Uniforms},
        simd::{Float3, Float4},
    };

    use super::{LINE_VERTICES_PER_INSTANCE, build_line_vertices};

    #[test]
    fn line_vertices_prefer_dominant_sibling_orientation() {
        let mesh = Mesh {
            dots: Vec::new(),
            tris: Vec::new(),
            lins: vec![
                test_line(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(1.0, 0.0, 0.0),
                    1,
                    -1,
                    -1,
                    false,
                    1.0,
                ),
                test_line(
                    Float3::new(1.0, 0.0, 0.0),
                    Float3::new(0.0, 0.0, 0.0),
                    0,
                    -1,
                    -1,
                    true,
                    1.0,
                ),
            ],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        };

        let vertices = build_line_vertices(&mesh);
        assert_eq!(vertices.len(), LINE_VERTICES_PER_INSTANCE as usize);
        assert_eq!(vertices[0].pos, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[5].pos, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn line_vertices_render_inverse_pairs_once_without_dominant_metadata() {
        let mesh = Mesh {
            dots: Vec::new(),
            tris: Vec::new(),
            lins: vec![
                test_line(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(1.0, 0.0, 0.0),
                    1,
                    -1,
                    -1,
                    false,
                    1.0,
                ),
                test_line(
                    Float3::new(1.0, 0.0, 0.0),
                    Float3::new(0.0, 0.0, 0.0),
                    0,
                    -1,
                    -1,
                    false,
                    1.0,
                ),
            ],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        };

        let vertices = build_line_vertices(&mesh);
        assert_eq!(vertices.len(), LINE_VERTICES_PER_INSTANCE as usize);
    }

    #[test]
    fn line_vertices_fall_back_to_visible_inverse_sibling() {
        let mesh = Mesh {
            dots: Vec::new(),
            tris: Vec::new(),
            lins: vec![
                test_line(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(1.0, 0.0, 0.0),
                    1,
                    -1,
                    -1,
                    false,
                    0.0,
                ),
                test_line(
                    Float3::new(1.0, 0.0, 0.0),
                    Float3::new(0.0, 0.0, 0.0),
                    0,
                    -1,
                    -1,
                    false,
                    1.0,
                ),
            ],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        };

        let vertices = build_line_vertices(&mesh);
        assert_eq!(vertices.len(), LINE_VERTICES_PER_INSTANCE as usize);
        assert_eq!(vertices[0].pos, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[5].pos, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[0].col[3], 1.0);
    }

    #[test]
    fn line_vertices_infer_missing_neighbors_from_shared_endpoints() {
        let mesh = Mesh {
            dots: Vec::new(),
            tris: Vec::new(),
            lins: vec![
                test_line(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(0.0, 1.0, 0.0),
                    -1,
                    -1,
                    -1,
                    false,
                    1.0,
                ),
                test_line(
                    Float3::new(0.0, 1.0, 0.0),
                    Float3::new(1.0, 1.0, 0.0),
                    -1,
                    -1,
                    -1,
                    false,
                    1.0,
                ),
            ],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        };

        let vertices = build_line_vertices(&mesh);
        assert_eq!(vertices.len(), (LINE_VERTICES_PER_INSTANCE * 2) as usize);
        assert_eq!(vertices[5].prev_tangent, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[6].prev_tangent, [0.0, 1.0, 0.0, 0.0]);
    }

    fn test_line(
        a: Float3,
        b: Float3,
        inv: i32,
        prev: i32,
        next: i32,
        is_dom_sib: bool,
        alpha: f32,
    ) -> Lin {
        Lin {
            a: LinVertex {
                pos: a,
                col: Float4::new(1.0, 1.0, 1.0, alpha),
            },
            b: LinVertex {
                pos: b,
                col: Float4::new(1.0, 1.0, 1.0, alpha),
            },
            norm: Float3::Z,
            prev,
            next,
            inv,
            is_dom_sib,
        }
    }
}
