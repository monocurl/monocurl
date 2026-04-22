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
        for value in [
            &mut line.prev,
            &mut line.next,
            &mut line.inv,
            &mut line.anti,
        ] {
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
        anti: -1,
        is_dom_sib: false,
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
            anti: -1,
            is_dom_sib: false,
        })
        .collect();

    let mut edge_map = HashMap::<(usize, usize), (usize, usize, usize, usize)>::new();
    for (tri_idx, face) in faces.iter().enumerate() {
        for (edge_idx, (a, b)) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])]
            .into_iter()
            .enumerate()
        {
            let key = canonical_index_edge_key(a, b);
            if let Some((other_tri, other_edge, _, _)) = edge_map.remove(&key) {
                set_tri_edge(&mut tris[tri_idx], edge_idx, other_tri as i32);
                set_tri_edge(&mut tris[other_tri], other_edge, tri_idx as i32);
            } else {
                edge_map.insert(key, (tri_idx, edge_idx, a, b));
            }
        }
    }

    let mut boundary_items: Vec<_> = edge_map.into_values().collect();
    boundary_items.sort_unstable_by_key(|(tri_idx, edge_idx, _, _)| (*tri_idx, *edge_idx));

    let mut lins = Vec::with_capacity(boundary_items.len());
    let mut edge_to_line = HashMap::<(usize, usize), usize>::with_capacity(boundary_items.len());
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
        edge_to_line.insert((a, b), line_idx);
    }

    let mut incoming = HashMap::<usize, Vec<usize>>::new();
    let mut outgoing = HashMap::<usize, Vec<usize>>::new();
    for (&(a, b), &line_idx) in &edge_to_line {
        outgoing.entry(a).or_default().push(line_idx);
        incoming.entry(b).or_default().push(line_idx);
    }

    for (&(a, b), &line_idx) in &edge_to_line {
        lins[line_idx].prev = incoming
            .get(&a)
            .and_then(|candidates| (candidates.len() == 1).then_some(candidates[0] as i32))
            .unwrap_or(-1);
        lins[line_idx].next = outgoing
            .get(&b)
            .and_then(|candidates| (candidates.len() == 1).then_some(candidates[0] as i32))
            .unwrap_or(-1);
    }

    (lins, tris)
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
    build_indexed_surface(&vertices, faces, &HashMap::new()).1
}

fn canonical_index_edge_key(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

fn set_tri_edge(tri: &mut Tri, edge_idx: usize, value: i32) {
    match edge_idx {
        0 => tri.ab = value,
        1 => tri.bc = value,
        2 => tri.ca = value,
        _ => unreachable!(),
    }
}
