use std::{collections::HashMap, ops::Range};

use crate::{
    mesh::{Lin, LinVertex, Tri, TriVertex},
    simd::{Float2, Float3, Float4},
};

#[derive(Clone, Copy, Debug)]
pub struct SurfaceVertex {
    pub pos: Float3,
    pub col: Float4,
    pub uv: Float2,
}

#[derive(Clone, Copy, Debug)]
pub struct BoundaryEdge {
    pub a_col: Float4,
    pub b_col: Float4,
    pub norm: Float3,
}

#[derive(Clone, Debug)]
pub struct IndexedSurface {
    pub vertices: Vec<SurfaceVertex>,
    pub faces: Vec<[usize; 3]>,
    pub boundary_edges: HashMap<(usize, usize), BoundaryEdge>,
}

#[derive(Clone, Debug)]
pub struct IndexedLineMesh {
    pub vertices: Vec<SurfaceVertex>,
    pub segments: Vec<[usize; 2]>,
}

pub fn mesh_ref(idx: usize) -> i32 {
    -2 - idx as i32
}

pub fn shift_line_refs(lines: &mut [Lin], delta: usize) {
    let delta = delta as i32;
    for line in lines {
        for value in [&mut line.prev, &mut line.next, &mut line.inv] {
            if *value >= 0 {
                *value += delta;
            }
        }
    }
}

pub fn line(a: Float3, b: Float3, norm: Float3, color: Float4) -> Lin {
    Lin {
        a: LinVertex { pos: a, col: color },
        b: LinVertex { pos: b, col: color },
        norm,
        prev: -1,
        next: -1,
        inv: -1,
        is_dom_sib: true,
    }
}

pub fn push_open_polyline(
    out: &mut Vec<Lin>,
    points: &[Float3],
    normal: Float3,
    color: Float4,
) -> Range<usize> {
    let start = out.len();
    if points.len() < 2 {
        return start..start;
    }

    let mut lines: Vec<_> = points
        .windows(2)
        .enumerate()
        .map(|(i, window)| {
            let mut lin = line(window[0], window[1], normal, color);
            lin.prev = if i == 0 { -1 } else { i as i32 - 1 };
            lin.next = if i + 1 == points.len() - 1 {
                -1
            } else {
                i as i32 + 1
            };
            lin
        })
        .collect();
    shift_line_refs(&mut lines, start);
    out.extend(lines);
    start..out.len()
}

pub fn push_closed_polyline(
    out: &mut Vec<Lin>,
    points: &[Float3],
    normal: Float3,
    color: Float4,
) -> Range<usize> {
    let start = out.len();
    if points.len() < 2 {
        return start..start;
    }

    let mut lines = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        let mut lin = line(points[i], points[(i + 1) % points.len()], normal, color);
        lin.prev = ((i + points.len() - 1) % points.len()) as i32;
        lin.next = ((i + 1) % points.len()) as i32;
        lines.push(lin);
    }
    shift_line_refs(&mut lines, start);
    out.extend(lines);
    start..out.len()
}

pub fn build_indexed_surface(
    vertices: &[SurfaceVertex],
    faces: &[[usize; 3]],
    boundary_edges: &HashMap<(usize, usize), BoundaryEdge>,
) -> (Vec<Lin>, Vec<Tri>) {
    let (mut tris, edge_map) = build_surface_tris(vertices, faces);

    let mut boundary_items = Vec::new();
    for ((a, b), edges) in edge_map {
        for (tri_idx, edge_idx) in edges {
            boundary_items.push((tri_idx, edge_idx, a, b));
        }
    }
    boundary_items.sort_unstable_by_key(|(tri_idx, edge_idx, _, _)| (*tri_idx, *edge_idx));

    let mut lins = Vec::with_capacity(boundary_items.len());
    let mut line_edges = Vec::with_capacity(boundary_items.len());
    for (tri_idx, edge_idx, a, b) in boundary_items {
        let template = boundary_edges
            .get(&(a, b))
            .copied()
            .unwrap_or(BoundaryEdge {
                a_col: vertices[a].col,
                b_col: vertices[b].col,
                norm: Float3::ZERO,
            });
        let line_idx = lins.len();
        let mut edge = line(
            vertices[a].pos,
            vertices[b].pos,
            template.norm,
            template.a_col,
        );
        edge.b.col = template.b_col;
        edge.inv = mesh_ref(tri_idx);
        set_tri_edge(&mut tris[tri_idx], edge_idx, mesh_ref(line_idx));
        lins.push(edge);
        line_edges.push((tri_idx, edge_idx));
    }

    link_boundary_loops(&mut lins, &tris, faces, &line_edges);

    (lins, tris)
}

pub fn build_indexed_tris_with_open_boundaries(
    vertices: &[Float3],
    faces: &[[usize; 3]],
    color: Float4,
) -> Vec<Tri> {
    let vertices: Vec<_> = vertices
        .iter()
        .copied()
        .map(|pos| SurfaceVertex {
            pos,
            col: color,
            uv: Float2::ZERO,
        })
        .collect();
    build_surface_tris(&vertices, faces).0
}

pub fn build_indexed_tris(vertices: &[Float3], faces: &[[usize; 3]], color: Float4) -> Vec<Tri> {
    let vertices: Vec<_> = vertices
        .iter()
        .copied()
        .map(|pos| SurfaceVertex {
            pos,
            col: color,
            uv: Float2::ZERO,
        })
        .collect();
    let (lins, tris) = build_indexed_surface(&vertices, faces, &HashMap::new());
    assert!(
        lins.is_empty(),
        "build_indexed_tris requires a closed surface; open triangle boundaries must remain explicit lines",
    );
    tris
}

fn build_surface_tris(
    vertices: &[SurfaceVertex],
    faces: &[[usize; 3]],
) -> (Vec<Tri>, HashMap<(usize, usize), Vec<(usize, usize)>>) {
    let mut tris: Vec<_> = faces
        .iter()
        .map(|face| Tri {
            a: TriVertex {
                pos: vertices[face[0]].pos,
                col: vertices[face[0]].col,
                uv: vertices[face[0]].uv,
            },
            b: TriVertex {
                pos: vertices[face[1]].pos,
                col: vertices[face[1]].col,
                uv: vertices[face[1]].uv,
            },
            c: TriVertex {
                pos: vertices[face[2]].pos,
                col: vertices[face[2]].col,
                uv: vertices[face[2]].uv,
            },
            ab: -1,
            bc: -1,
            ca: -1,
            is_dom_sib: false,
        })
        .collect();

    let mut edge_map = HashMap::<(usize, usize), Vec<(usize, usize)>>::new();
    for (tri_idx, face) in faces.iter().enumerate() {
        for (edge_idx, (a, b)) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])]
            .into_iter()
            .enumerate()
        {
            if let Some(other_edges) = edge_map.get_mut(&(b, a)) {
                if let Some((other_tri, other_edge)) = other_edges.pop() {
                    if other_edges.is_empty() {
                        edge_map.remove(&(b, a));
                    }
                    set_tri_edge(&mut tris[tri_idx], edge_idx, other_tri as i32);
                    set_tri_edge(&mut tris[other_tri], other_edge, tri_idx as i32);
                    continue;
                }
            }

            edge_map
                .entry((a, b))
                .or_default()
                .push((tri_idx, edge_idx));
        }
    }

    (tris, edge_map)
}

fn set_tri_edge(tri: &mut Tri, edge_idx: usize, value: i32) {
    match edge_idx {
        0 => tri.ab = value,
        1 => tri.bc = value,
        2 => tri.ca = value,
        _ => unreachable!(),
    }
}

fn link_boundary_loops(
    lins: &mut [Lin],
    tris: &[Tri],
    faces: &[[usize; 3]],
    line_edges: &[(usize, usize)],
) {
    for (line_idx, &(tri_idx, edge_idx)) in line_edges.iter().enumerate() {
        let Some(next_idx) = next_boundary_line(tris, faces, tri_idx, edge_idx) else {
            continue;
        };
        lins[line_idx].next = next_idx as i32;
        lins[next_idx].prev = line_idx as i32;
    }
}

fn next_boundary_line(
    tris: &[Tri],
    faces: &[[usize; 3]],
    start_tri_idx: usize,
    start_edge_idx: usize,
) -> Option<usize> {
    let mut tri_idx = start_tri_idx;
    let mut edge_idx = start_edge_idx;
    for _ in 0..tris.len().saturating_mul(3) {
        let next_edge_idx = (edge_idx + 1) % 3;
        let edge_ref = tri_edge(&tris[tri_idx], next_edge_idx);
        if let Some(line_idx) = decode_mesh_ref(edge_ref) {
            return Some(line_idx);
        }

        let next_tri_idx = (edge_ref >= 0).then_some(edge_ref as usize)?;
        let (a, b) = face_edge(faces[tri_idx], next_edge_idx);
        edge_idx = find_directed_edge(faces[next_tri_idx], b, a)?;
        tri_idx = next_tri_idx;
    }
    None
}

fn tri_edge(tri: &Tri, edge_idx: usize) -> i32 {
    match edge_idx {
        0 => tri.ab,
        1 => tri.bc,
        2 => tri.ca,
        _ => unreachable!(),
    }
}

fn face_edge(face: [usize; 3], edge_idx: usize) -> (usize, usize) {
    match edge_idx {
        0 => (face[0], face[1]),
        1 => (face[1], face[2]),
        2 => (face[2], face[0]),
        _ => unreachable!(),
    }
}

fn find_directed_edge(face: [usize; 3], a: usize, b: usize) -> Option<usize> {
    (0..3).find(|&edge_idx| face_edge(face, edge_idx) == (a, b))
}

fn decode_mesh_ref(value: i32) -> Option<usize> {
    (value < -1).then_some((-value - 2) as usize)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{
        mesh::Mesh,
        simd::{Float2, Float3, Float4},
    };

    use super::{
        SurfaceVertex, build_indexed_surface, build_indexed_tris,
        build_indexed_tris_with_open_boundaries,
    };

    #[test]
    fn build_indexed_surface_keeps_same_direction_duplicate_edges_on_boundary() {
        let white = Float4::ONE;
        let vertices = vec![
            SurfaceVertex {
                pos: Float3::new(0.0, 0.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 0.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 1.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(0.0, 1.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(0.0, 0.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 0.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, -1.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
        ];
        let faces = vec![[0, 1, 2], [4, 5, 6]];

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &HashMap::new());
        let mesh = Mesh {
            dots: Vec::new(),
            lins,
            tris,
            uniform: Default::default(),
            tag: Vec::new(),
        };

        assert!(mesh.has_consistent_topology());
        assert_eq!(mesh.tris[0].ab, -2);
        assert_eq!(mesh.tris[1].ab, -5);
    }

    #[test]
    fn build_indexed_surface_links_boundary_loops_through_repeated_vertices() {
        let white = Float4::ONE;
        let vertices = vec![
            SurfaceVertex {
                pos: Float3::new(0.0, 0.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(1.0, 0.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(0.0, 1.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(-1.0, 0.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
            SurfaceVertex {
                pos: Float3::new(0.0, -1.0, 0.0),
                col: white,
                uv: Float2::ZERO,
            },
        ];
        let faces = vec![[0, 1, 2], [0, 3, 4]];

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &HashMap::new());
        let mesh = Mesh {
            dots: Vec::new(),
            lins,
            tris,
            uniform: Default::default(),
            tag: Vec::new(),
        };

        assert!(mesh.has_consistent_topology());
        assert!(
            mesh.lins
                .iter()
                .all(|line| line.prev >= 0 && line.next >= 0)
        );
    }

    #[test]
    #[should_panic(expected = "open triangle boundaries must remain explicit lines")]
    fn build_indexed_tris_rejects_open_boundaries() {
        let _ = build_indexed_tris(
            &[
                Float3::new(0.0, 0.0, 0.0),
                Float3::new(1.0, 0.0, 0.0),
                Float3::new(0.0, 1.0, 0.0),
            ],
            &[[0, 1, 2]],
            Float4::ONE,
        );
    }

    #[test]
    fn build_indexed_tris_with_open_boundaries_leaves_edges_unset() {
        let tris = build_indexed_tris_with_open_boundaries(
            &[
                Float3::new(0.0, 0.0, 0.0),
                Float3::new(1.0, 0.0, 0.0),
                Float3::new(0.0, 1.0, 0.0),
            ],
            &[[0, 1, 2]],
            Float4::ONE,
        );

        assert_eq!(tris.len(), 1);
        assert_eq!(tris[0].ab, -1);
        assert_eq!(tris[0].bc, -1);
        assert_eq!(tris[0].ca, -1);
    }
}
