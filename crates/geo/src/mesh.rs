use std::path::PathBuf;

use crate::simd::{Float2, Float3, Float4};

#[derive(Debug, Clone, Copy)]
pub struct Dot {
    pub pos: Float3,
    pub norm: Float3,
    pub col: Float4,

    pub inv: i32,
    pub anti: i32,
    pub is_dom_sib: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct LinVertex {
    pub pos: Float3,
    pub col: Float4,
}

#[derive(Debug, Clone, Copy)]
pub struct Lin {
    pub a: LinVertex,
    pub b: LinVertex,

    pub norm: Float3,

    pub prev: i32,
    pub next: i32,
    pub inv: i32,
    pub anti: i32,
    pub is_dom_sib: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct TriVertex {
    pub pos: Float3,
    pub col: Float4,
    pub uv: Float2,
}

#[derive(Debug, Clone, Copy)]
pub struct Tri {
    pub a: TriVertex,
    pub b: TriVertex,
    pub c: TriVertex,

    pub ab: i32,
    pub bc: i32,
    pub ca: i32,

    pub anti: i32,
    pub is_dom_sib: bool,
}

#[derive(Debug, Clone)]
pub struct Uniforms {
    pub alpha: f64,
    pub img: Option<PathBuf>,
    pub z_index: i32,
    pub fixed_in_frame: bool,
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            img: None,
            z_index: 0,
            fixed_in_frame: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub dots: Vec<Dot>,
    pub lins: Vec<Lin>,
    pub tris: Vec<Tri>,

    pub uniform: Uniforms,

    pub tag: Vec<isize>,
}

impl Mesh {
    pub fn has_consistent_topology(&self) -> bool {
        self.dots.iter().enumerate().all(|(i, dot)| {
            ref_matches(dot.inv, self.dots.len(), |j| {
                let inv = self.dots[j];
                inv.inv == i as i32
                    && inv.is_dom_sib != dot.is_dom_sib
                    && same_point(dot.pos, inv.pos)
                    && same_vec(dot.norm, inv.norm)
            }) && ref_matches(dot.anti, self.dots.len(), |j| {
                let anti = self.dots[j];
                anti.anti == i as i32
                    && anti.is_dom_sib == dot.is_dom_sib
                    && same_point(dot.pos, anti.pos)
                    && opposite_vec(dot.norm, anti.norm)
            })
        }) && self.lins.iter().enumerate().all(|(i, lin)| {
            line_neighbor_matches(self, lin.prev, i, NeighborDirection::Prev)
                && line_neighbor_matches(self, lin.next, i, NeighborDirection::Next)
                && line_inverse_matches(self, i)
                && ref_matches(lin.anti, self.lins.len(), |j| {
                    let anti = self.lins[j];
                    anti.anti == i as i32
                        && anti.is_dom_sib == lin.is_dom_sib
                        && same_point(lin.a.pos, anti.a.pos)
                        && same_point(lin.b.pos, anti.b.pos)
                        && opposite_vec(lin.norm, anti.norm)
                })
        }) && self.tris.iter().enumerate().all(|(i, tri)| {
            ref_matches(tri.anti, self.tris.len(), |j| {
                let anti = self.tris[j];
                anti.anti == i as i32
                    && anti.is_dom_sib != tri.is_dom_sib
                    && same_triangle_positions(tri, &anti)
            }) && tri_edges_are_consistent(self, i)
        })
    }

    pub fn has_consistent_line_links(&self) -> bool {
        self.lins.iter().enumerate().all(|(i, lin)| {
            let prev_ok = if lin.prev < 0 {
                true
            } else {
                let prev = lin.prev as usize;
                prev < self.lins.len() && self.lins[prev].next == i as i32
            };
            let next_ok = if lin.next < 0 {
                true
            } else {
                let next = lin.next as usize;
                next < self.lins.len() && self.lins[next].prev == i as i32
            };
            prev_ok && next_ok
        })
    }
}

#[derive(Clone, Copy)]
enum NeighborDirection {
    Prev,
    Next,
}

fn tri_edge_positions(tri: &Tri) -> [(Float3, Float3); 3] {
    [
        (tri.a.pos, tri.b.pos),
        (tri.b.pos, tri.c.pos),
        (tri.c.pos, tri.a.pos),
    ]
}

fn same_triangle_positions(a: &Tri, b: &Tri) -> bool {
    let mut lhs = [point_key(a.a.pos), point_key(a.b.pos), point_key(a.c.pos)];
    let mut rhs = [point_key(b.a.pos), point_key(b.b.pos), point_key(b.c.pos)];
    lhs.sort_unstable();
    rhs.sort_unstable();
    lhs == rhs
}

fn line_neighbor_matches(
    mesh: &Mesh,
    value: i32,
    line_idx: usize,
    direction: NeighborDirection,
) -> bool {
    if value >= 0 {
        let neighbor = value as usize;
        if neighbor >= mesh.lins.len() {
            return false;
        }

        let line = mesh.lins[line_idx];
        let other = mesh.lins[neighbor];
        return match direction {
            NeighborDirection::Prev => {
                other.next == line_idx as i32
                    && same_point(other.b.pos, line.a.pos)
                    && other.is_dom_sib == line.is_dom_sib
                    && same_vec(other.norm, line.norm)
            }
            NeighborDirection::Next => {
                other.prev == line_idx as i32
                    && same_point(line.b.pos, other.a.pos)
                    && other.is_dom_sib == line.is_dom_sib
                    && same_vec(other.norm, line.norm)
            }
        };
    }

    let Some(dot_idx) = decode_ref(value) else {
        return value == -1;
    };
    mesh.dots.get(dot_idx).is_some_and(|dot| match direction {
        NeighborDirection::Prev => same_point(dot.pos, mesh.lins[line_idx].a.pos),
        NeighborDirection::Next => same_point(dot.pos, mesh.lins[line_idx].b.pos),
    })
}

fn line_inverse_matches(mesh: &Mesh, line_idx: usize) -> bool {
    let line = mesh.lins[line_idx];
    if line.inv >= 0 {
        let inv_idx = line.inv as usize;
        return mesh.lins.get(inv_idx).is_some_and(|inv| {
            inv.inv == line_idx as i32
                && inv.is_dom_sib != line.is_dom_sib
                && same_point(line.a.pos, inv.b.pos)
                && same_point(line.b.pos, inv.a.pos)
                && same_vec(line.norm, inv.norm)
        });
    }

    let Some(tri_idx) = decode_ref(line.inv) else {
        return line.inv == -1;
    };
    mesh.tris
        .get(tri_idx)
        .is_some_and(|_| tri_references_line(mesh, tri_idx, line_idx))
}

fn tri_edges_are_consistent(mesh: &Mesh, tri_idx: usize) -> bool {
    let tri = &mesh.tris[tri_idx];
    [tri.ab, tri.bc, tri.ca]
        .into_iter()
        .enumerate()
        .all(|(edge_idx, value)| {
            let (a, b) = tri_edge_positions(tri)[edge_idx];
            if value >= 0 {
                let other_idx = value as usize;
                return mesh.tris.get(other_idx).is_some_and(|other| {
                    tri_edge_positions(other)
                        .into_iter()
                        .any(|(c, d)| same_point(c, b) && same_point(d, a))
                });
            }

            let Some(line_idx) = decode_ref(value) else {
                return value == -1;
            };
            mesh.lins.get(line_idx).is_some_and(|line| {
                same_point(line.a.pos, a)
                    && same_point(line.b.pos, b)
                    && decode_ref(line.inv).map_or(line.inv >= -1, |owner| owner == tri_idx)
            })
        })
}

fn tri_references_line(mesh: &Mesh, tri_idx: usize, line_idx: usize) -> bool {
    let tri = &mesh.tris[tri_idx];
    [tri.ab, tri.bc, tri.ca]
        .into_iter()
        .any(|value| decode_ref(value) == Some(line_idx))
}

fn ref_matches(value: i32, len: usize, check: impl FnOnce(usize) -> bool) -> bool {
    if value < 0 {
        return true;
    }

    let idx = value as usize;
    idx < len && check(idx)
}

fn decode_ref(value: i32) -> Option<usize> {
    (value < -1).then_some((-value - 2) as usize)
}

fn same_point(a: Float3, b: Float3) -> bool {
    point_key(a) == point_key(b)
}

fn same_vec(a: Float3, b: Float3) -> bool {
    point_key(a) == point_key(b)
}

fn opposite_vec(a: Float3, b: Float3) -> bool {
    point_key(a) == point_key(-b)
}

fn point_key(point: Float3) -> [u32; 3] {
    [
        canonical_bits(point.x),
        canonical_bits(point.y),
        canonical_bits(point.z),
    ]
}

fn canonical_bits(value: f32) -> u32 {
    if value == 0.0 {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}
