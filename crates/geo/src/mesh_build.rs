use std::ops::Range;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    mesh::{Lin, LinVertex, Tri, TriVertex},
    simd::{Float2, Float3, Float4},
};

// keys are vertex-index pairs from our own meshes, so a fast non-cryptographic
// hash is appropriate; the default SipHash showed up clearly when tessellating
/// unmatched directed edges by endpoints; one or two per edge on any
/// sensible surface, so they stay inline rather than costing an allocation
/// per edge of every surface built per frame
type BoundaryEdgeMap = FxHashMap<(usize, usize), SmallVec<[(usize, usize); 2]>>;

/// per-edge templates for the boundary lines a surface produces, keyed by the
/// vertex indices of the edge. `BoundaryEdges::default()` means "no templates"
pub type BoundaryEdges = FxHashMap<(usize, usize), BoundaryEdge>;

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
    pub boundary_edges: BoundaryEdges,
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
    boundary_edges: &BoundaryEdges,
) -> (Vec<Lin>, Vec<Tri>) {
    build_indexed_surface_with(vertices, faces, |a, b| boundary_edges.get(&(a, b)).copied())
}

/// `build_indexed_surface` with the boundary edge of `(a, b)` answered by a
/// function, for callers that would otherwise build a map saying the same
/// thing for every edge
pub fn build_indexed_surface_with(
    vertices: &[SurfaceVertex],
    faces: &[[usize; 3]],
    edge_for: impl Fn(usize, usize) -> Option<BoundaryEdge>,
) -> (Vec<Lin>, Vec<Tri>) {
    let mut tris = unconnected_tris(vertices, faces);
    let lines = connect_faces(faces, tris.as_mut_slice());
    (boundary_lins(vertices, &lines, edge_for), tris)
}

/// the part of `build_indexed_surface` that depends only on the faces: which
/// triangles meet across each edge and the boundary lines between them, so a
/// surface whose vertices move but whose faces stay put skips matching edges
#[derive(Clone, Debug)]
pub struct SurfaceTopology {
    faces: Vec<[usize; 3]>,
    adjacency: Vec<[i32; 3]>,
    lines: Vec<BoundaryLine>,
}

impl SurfaceTopology {
    pub fn new(faces: Vec<[usize; 3]>) -> Self {
        let mut adjacency = vec![[-1; 3]; faces.len()];
        let lines = connect_faces(&faces, adjacency.as_mut_slice());
        Self {
            faces,
            adjacency,
            lines,
        }
    }

    pub fn faces(&self) -> &[[usize; 3]] {
        &self.faces
    }

    /// `build_indexed_surface(vertices, self.faces(), boundary_edges)`
    pub fn build(
        &self,
        vertices: &[SurfaceVertex],
        boundary_edges: &BoundaryEdges,
    ) -> (Vec<Lin>, Vec<Tri>) {
        let tris = self
            .faces
            .iter()
            .zip(&self.adjacency)
            .map(|(&face, &[ab, bc, ca])| Tri {
                ab,
                bc,
                ca,
                ..unconnected_tri(vertices, face)
            })
            .collect();
        let lins = boundary_lins(vertices, &self.lines, |a, b| {
            boundary_edges.get(&(a, b)).copied()
        });
        (lins, tris)
    }
}

/// where a surface's triangles keep their `[ab, bc, ca]`: a neighbouring
/// triangle, `mesh_ref` of a boundary line, or -1
trait TriEdges {
    fn edge(&self, tri: usize, edge_idx: usize) -> i32;
    fn set_edge(&mut self, tri: usize, edge_idx: usize, value: i32);
}

impl TriEdges for [Tri] {
    fn edge(&self, tri: usize, edge_idx: usize) -> i32 {
        let tri = &self[tri];
        [tri.ab, tri.bc, tri.ca][edge_idx]
    }

    fn set_edge(&mut self, tri: usize, edge_idx: usize, value: i32) {
        let tri = &mut self[tri];
        *[&mut tri.ab, &mut tri.bc, &mut tri.ca][edge_idx] = value;
    }
}

impl TriEdges for [[i32; 3]] {
    fn edge(&self, tri: usize, edge_idx: usize) -> i32 {
        self[tri][edge_idx]
    }

    fn set_edge(&mut self, tri: usize, edge_idx: usize, value: i32) {
        self[tri][edge_idx] = value;
    }
}

/// a boundary edge `a -> b` of triangle `tri` and its neighbours in the loop
#[derive(Clone, Copy, Debug)]
struct BoundaryLine {
    a: usize,
    b: usize,
    tri: usize,
    prev: i32,
    next: i32,
}

/// links the faces' triangles in `edges` and gives the boundary lines in
/// triangle and edge order, each set as its triangle's edge
fn connect_faces(faces: &[[usize; 3]], edges: &mut (impl TriEdges + ?Sized)) -> Vec<BoundaryLine> {
    let edge_map = match_edges(faces, edges);

    let mut boundary_items = Vec::new();
    for ((a, b), unmatched) in edge_map {
        for (tri_idx, edge_idx) in unmatched {
            boundary_items.push((tri_idx, edge_idx, a, b));
        }
    }
    boundary_items.sort_unstable_by_key(|(tri_idx, edge_idx, _, _)| (*tri_idx, *edge_idx));

    let mut lines = Vec::with_capacity(boundary_items.len());
    for (line_idx, &(tri, edge_idx, a, b)) in boundary_items.iter().enumerate() {
        edges.set_edge(tri, edge_idx, mesh_ref(line_idx));
        lines.push(BoundaryLine {
            a,
            b,
            tri,
            prev: -1,
            next: -1,
        });
    }

    for (line_idx, &(tri_idx, edge_idx, _, _)) in boundary_items.iter().enumerate() {
        let Some(next_idx) = next_boundary_line(edges, faces, tri_idx, edge_idx) else {
            continue;
        };
        lines[line_idx].next = next_idx as i32;
        lines[next_idx].prev = line_idx as i32;
    }

    lines
}

fn boundary_lins(
    vertices: &[SurfaceVertex],
    lines: &[BoundaryLine],
    edge_for: impl Fn(usize, usize) -> Option<BoundaryEdge>,
) -> Vec<Lin> {
    lines
        .iter()
        .map(
            |&BoundaryLine {
                 a,
                 b,
                 tri,
                 prev,
                 next,
             }| {
                let template = edge_for(a, b).unwrap_or(BoundaryEdge {
                    a_col: vertices[a].col,
                    b_col: vertices[b].col,
                    norm: Float3::ZERO,
                });
                let mut edge = line(
                    vertices[a].pos,
                    vertices[b].pos,
                    template.norm,
                    template.a_col,
                );
                edge.b.col = template.b_col;
                edge.inv = mesh_ref(tri);
                edge.prev = prev;
                edge.next = next;
                edge
            },
        )
        .collect()
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
    let mut tris = unconnected_tris(&vertices, faces);
    match_edges(faces, tris.as_mut_slice());
    tris
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
    let (lins, tris) = build_indexed_surface(&vertices, faces, &BoundaryEdges::default());
    assert!(
        lins.is_empty(),
        "build_indexed_tris requires a closed surface; open triangle boundaries must remain explicit lines",
    );
    tris
}

fn unconnected_tri(vertices: &[SurfaceVertex], face: [usize; 3]) -> Tri {
    let corner = |idx: usize| {
        let SurfaceVertex { pos, col, uv } = vertices[idx];
        TriVertex { pos, col, uv }
    };
    Tri {
        a: corner(face[0]),
        b: corner(face[1]),
        c: corner(face[2]),
        ab: -1,
        bc: -1,
        ca: -1,
        is_dom_sib: false,
    }
}

fn unconnected_tris(vertices: &[SurfaceVertex], faces: &[[usize; 3]]) -> Vec<Tri> {
    faces
        .iter()
        .map(|&face| unconnected_tri(vertices, face))
        .collect()
}

/// sets the neighbours of every pair of faces meeting on opposite directed
/// edges and gives the directed edges left unmatched
fn match_edges(faces: &[[usize; 3]], edges: &mut (impl TriEdges + ?Sized)) -> BoundaryEdgeMap {
    let mut edge_map =
        BoundaryEdgeMap::with_capacity_and_hasher(faces.len() * 3, Default::default());
    for (tri_idx, face) in faces.iter().enumerate() {
        for (edge_idx, (a, b)) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])]
            .into_iter()
            .enumerate()
        {
            if let Some(other_edges) = edge_map.get_mut(&(b, a))
                && let Some((other_tri, other_edge)) = other_edges.pop()
            {
                if other_edges.is_empty() {
                    edge_map.remove(&(b, a));
                }
                edges.set_edge(tri_idx, edge_idx, other_tri as i32);
                edges.set_edge(other_tri, other_edge, tri_idx as i32);
                continue;
            }

            edge_map
                .entry((a, b))
                .or_default()
                .push((tri_idx, edge_idx));
        }
    }
    edge_map
}

fn next_boundary_line(
    edges: &(impl TriEdges + ?Sized),
    faces: &[[usize; 3]],
    start_tri_idx: usize,
    start_edge_idx: usize,
) -> Option<usize> {
    let mut tri_idx = start_tri_idx;
    let mut edge_idx = start_edge_idx;
    for _ in 0..faces.len().saturating_mul(3) {
        let next_edge_idx = (edge_idx + 1) % 3;
        let edge_ref = edges.edge(tri_idx, next_edge_idx);
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

    use crate::{
        mesh::Mesh,
        mesh_build::BoundaryEdges,
        simd::{Float2, Float3, Float4},
    };

    use super::{
        SurfaceTopology, SurfaceVertex, build_indexed_surface, build_indexed_tris,
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

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &BoundaryEdges::default());
        let mesh = Mesh {
            dots: Default::default(),
            lins: lins.into(),
            tris: tris.into(),
            uniform: Default::default(),
            tag: Vec::new(),
            version: Mesh::fresh_version(),
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

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &BoundaryEdges::default());
        let mesh = Mesh {
            dots: Default::default(),
            lins: lins.into(),
            tris: tris.into(),
            uniform: Default::default(),
            tag: Vec::new(),
            version: Mesh::fresh_version(),
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

    #[test]
    fn surface_topology_builds_what_build_indexed_surface_does() {
        // a 4 by 3 grid with its middle cell missing: an outer and an inner loop
        let (nx, ny) = (4, 3);
        let vertices: Vec<_> = (0..=nx)
            .flat_map(|ix| {
                (0..=ny).map(move |iy| SurfaceVertex {
                    pos: Float3::new(ix as f32, iy as f32, (ix * iy) as f32),
                    col: Float4::new(ix as f32, iy as f32, 0.5, 1.0),
                    uv: Float2::ZERO,
                })
            })
            .collect();
        let vertex = |ix: usize, iy: usize| ix * (ny + 1) + iy;
        let faces: Vec<_> = (0..nx)
            .flat_map(|ix| (0..ny).map(move |iy| (ix, iy)))
            .filter(|&cell| cell != (1, 1))
            .flat_map(|(ix, iy)| {
                let (a, b, c, d) = (
                    vertex(ix, iy),
                    vertex(ix + 1, iy),
                    vertex(ix + 1, iy + 1),
                    vertex(ix, iy + 1),
                );
                [[a, b, c], [a, c, d]]
            })
            .collect();

        let expected = build_indexed_surface(&vertices, &faces, &BoundaryEdges::default());
        let built = SurfaceTopology::new(faces).build(&vertices, &BoundaryEdges::default());
        assert!(!expected.0.is_empty());
        assert_eq!(format!("{built:?}"), format!("{expected:?}"));
    }
}
