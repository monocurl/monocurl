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
    for source in &mesh.lins {
        if !line_visible(source) || !source.is_dom_sib {
            continue;
        }

        let prev = if source.prev >= 0 {
            mesh.lins[source.prev as usize]
        } else {
            *source
        };
        let next = if source.next >= 0 {
            mesh.lins[source.next as usize]
        } else {
            *source
        };

        let tangent = source.b.pos - source.a.pos;
        let prev_tangent = source.a.pos - prev.a.pos;
        let next_tangent = next.b.pos - source.b.pos;
        let reverse_tangent = -tangent;
        let reverse_prev_tangent = -next_tangent;
        let reverse_next_tangent = -prev_tangent;

        lines.extend([
            line_vertex(source.a.pos, source.a.col, tangent, prev_tangent, 1.0),
            line_vertex(source.a.pos, source.a.col, tangent, tangent, 0.0),
            line_vertex(source.a.pos, source.a.col, tangent, tangent, 1.0),
            line_vertex(source.b.pos, source.b.col, tangent, tangent, 0.0),
            line_vertex(source.b.pos, source.b.col, tangent, tangent, 1.0),
            line_vertex(source.b.pos, source.b.col, tangent, next_tangent, 1.0),
            line_vertex(
                source.b.pos,
                source.b.col,
                reverse_tangent,
                reverse_prev_tangent,
                1.0,
            ),
            line_vertex(
                source.b.pos,
                source.b.col,
                reverse_tangent,
                reverse_tangent,
                1.0,
            ),
            line_vertex(
                source.a.pos,
                source.a.col,
                reverse_tangent,
                reverse_tangent,
                1.0,
            ),
            line_vertex(
                source.a.pos,
                source.a.col,
                reverse_tangent,
                reverse_next_tangent,
                1.0,
            ),
        ]);
    }
    lines
}

fn line_visible(line: &geo::mesh::Lin) -> bool {
    line.a.col.w > f32::EPSILON || line.b.col.w > f32::EPSILON
}

pub(super) fn build_dot_instances(mesh: &Mesh) -> Vec<DotInstancePod> {
    let mut dots = Vec::with_capacity(mesh.dots.len());
    for dot in &mesh.dots {
        if !dot.is_dom_sib || dot.col.w <= f32::EPSILON {
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

#[cfg(test)]
pub(super) fn build_line_indices() -> [u16; 24] {
    [
        0, 2, 1, 1, 2, 4, 1, 4, 3, 3, 4, 5, 6, 7, 3, 3, 7, 8, 3, 8, 1, 1, 8, 9,
    ]
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

    use super::{LINE_VERTICES_PER_INSTANCE, build_line_indices, build_line_vertices};

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
        assert_eq!(vertices[6].tangent, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[9].tangent, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[1].extrude[0], 0.0);
        assert_eq!(vertices[3].extrude[0], 0.0);
    }

    #[test]
    fn line_vertices_skip_non_dominant_inverse_pairs() {
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
        assert!(vertices.is_empty());
    }

    #[test]
    fn line_vertices_do_not_fall_back_to_non_dominant_visible_inverse() {
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
        assert!(vertices.is_empty());
    }

    #[test]
    fn line_vertices_keep_butt_caps_without_explicit_neighbors() {
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
                    true,
                    1.0,
                ),
                test_line(
                    Float3::new(0.0, 1.0, 0.0),
                    Float3::new(1.0, 1.0, 0.0),
                    -1,
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
        assert_eq!(vertices.len(), (LINE_VERTICES_PER_INSTANCE * 2) as usize);
        assert_eq!(vertices[5].prev_tangent, [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[6].prev_tangent, [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn line_vertices_reverse_second_half_tangents() {
        let mesh = Mesh {
            dots: Vec::new(),
            tris: Vec::new(),
            lins: vec![
                test_line(
                    Float3::new(0.0, 0.0, 0.0),
                    Float3::new(1.0, 0.0, 0.0),
                    -1,
                    -1,
                    1,
                    true,
                    1.0,
                ),
                test_line(
                    Float3::new(1.0, 0.0, 0.0),
                    Float3::new(1.0, 1.0, 0.0),
                    -1,
                    0,
                    -1,
                    true,
                    1.0,
                ),
            ],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        };

        let vertices = build_line_vertices(&mesh);
        assert_eq!(vertices[0].tangent, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(vertices[6].tangent, [-1.0, -0.0, -0.0, 0.0]);
        assert_eq!(vertices[6].prev_tangent, [-0.0, -1.0, -0.0, 0.0]);
        assert_eq!(vertices[9].prev_tangent, [-0.0, -0.0, -0.0, 0.0]);
    }

    #[test]
    fn line_indices_fit_shared_endpoint_layout() {
        assert!(
            build_line_indices()
                .into_iter()
                .all(|idx| idx < LINE_VERTICES_PER_INSTANCE as u16)
        );
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
