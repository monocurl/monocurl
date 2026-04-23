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
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            img: None,
            z_index: 0,
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
        self.first_topology_mismatch().is_none()
    }

    pub fn topology_mismatch_report(&self) -> Option<String> {
        self.first_topology_mismatch().map(|mismatch| {
            format!(
                "mesh topology mismatch: {mismatch}\nmesh counts: dots={} lins={} tris={} tag={:?}",
                self.dots.len(),
                self.lins.len(),
                self.tris.len(),
                self.tag,
            )
        })
    }

    #[track_caller]
    pub fn debug_assert_consistent_topology(&self) {
        debug_assert!(
            self.has_consistent_topology(),
            "{}",
            self.topology_mismatch_report()
                .unwrap_or_else(|| "mesh topology mismatch: no specific mismatch found".into()),
        );
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

    fn first_topology_mismatch(&self) -> Option<String> {
        for (dot_idx, dot) in self.dots.iter().enumerate() {
            if let Some(mismatch) = dot_pair_mismatch(
                self.dots.len(),
                dot.inv,
                &format!(
                    "dot[{dot_idx}].inv = {} should reference a dot inverse",
                    dot.inv
                ),
            ) {
                return Some(mismatch);
            }
            if dot.inv >= 0 {
                let inv_idx = dot.inv as usize;
                let inv = self.dots[inv_idx];
                if inv.inv != dot_idx as i32 {
                    return Some(format!(
                        "dot[{dot_idx}].inv -> dot[{inv_idx}] does not point back: dot[{inv_idx}].inv = {}",
                        inv.inv
                    ));
                }
                if inv.is_dom_sib == dot.is_dom_sib {
                    return Some(format!(
                        "dot[{dot_idx}].inv -> dot[{inv_idx}] has matching is_dom_sib = {}, expected opposite",
                        inv.is_dom_sib
                    ));
                }
                if !same_point(dot.pos, inv.pos) {
                    return Some(format!(
                        "dot[{dot_idx}].inv -> dot[{inv_idx}] has mismatched position: {} vs {}",
                        fmt_point(dot.pos),
                        fmt_point(inv.pos),
                    ));
                }
                if !same_vec(dot.norm, inv.norm) {
                    return Some(format!(
                        "dot[{dot_idx}].inv -> dot[{inv_idx}] has mismatched normal: {} vs {}",
                        fmt_point(dot.norm),
                        fmt_point(inv.norm),
                    ));
                }
            }

            if let Some(mismatch) = dot_pair_mismatch(
                self.dots.len(),
                dot.anti,
                &format!(
                    "dot[{dot_idx}].anti = {} should reference a dot anti",
                    dot.anti
                ),
            ) {
                return Some(mismatch);
            }
            if dot.anti >= 0 {
                let anti_idx = dot.anti as usize;
                let anti = self.dots[anti_idx];
                if anti.anti != dot_idx as i32 {
                    return Some(format!(
                        "dot[{dot_idx}].anti -> dot[{anti_idx}] does not point back: dot[{anti_idx}].anti = {}",
                        anti.anti
                    ));
                }
                if anti.is_dom_sib != dot.is_dom_sib {
                    return Some(format!(
                        "dot[{dot_idx}].anti -> dot[{anti_idx}] has mismatched is_dom_sib: {} vs {}",
                        dot.is_dom_sib, anti.is_dom_sib,
                    ));
                }
                if !same_point(dot.pos, anti.pos) {
                    return Some(format!(
                        "dot[{dot_idx}].anti -> dot[{anti_idx}] has mismatched position: {} vs {}",
                        fmt_point(dot.pos),
                        fmt_point(anti.pos),
                    ));
                }
                if !opposite_vec(dot.norm, anti.norm) {
                    return Some(format!(
                        "dot[{dot_idx}].anti -> dot[{anti_idx}] normal is not opposite: {} vs {}",
                        fmt_point(dot.norm),
                        fmt_point(anti.norm),
                    ));
                }
            }
        }

        for line_idx in 0..self.lins.len() {
            if let Some(mismatch) = line_neighbor_mismatch(
                self,
                line_idx,
                self.lins[line_idx].prev,
                NeighborDirection::Prev,
            ) {
                return Some(mismatch);
            }
            if let Some(mismatch) = line_neighbor_mismatch(
                self,
                line_idx,
                self.lins[line_idx].next,
                NeighborDirection::Next,
            ) {
                return Some(mismatch);
            }
            if let Some(mismatch) = line_inverse_mismatch(self, line_idx) {
                return Some(mismatch);
            }

            let line = self.lins[line_idx];
            if let Some(mismatch) = dot_pair_mismatch(
                self.lins.len(),
                line.anti,
                &format!(
                    "line[{line_idx}].anti = {} should reference a line anti",
                    line.anti
                ),
            ) {
                return Some(mismatch);
            }
            if line.anti >= 0 {
                let anti_idx = line.anti as usize;
                let anti = self.lins[anti_idx];
                if anti.anti != line_idx as i32 {
                    return Some(format!(
                        "line[{line_idx}].anti -> line[{anti_idx}] does not point back: line[{anti_idx}].anti = {}",
                        anti.anti
                    ));
                }
                if anti.is_dom_sib != line.is_dom_sib {
                    return Some(format!(
                        "line[{line_idx}].anti -> line[{anti_idx}] has mismatched is_dom_sib: {} vs {}",
                        line.is_dom_sib, anti.is_dom_sib,
                    ));
                }
                if !same_point(line.a.pos, anti.a.pos) || !same_point(line.b.pos, anti.b.pos) {
                    return Some(format!(
                        "line[{line_idx}].anti -> line[{anti_idx}] has mismatched endpoints: {} vs {}",
                        fmt_line(&line),
                        fmt_line(&anti),
                    ));
                }
                if !opposite_vec(line.norm, anti.norm) {
                    return Some(format!(
                        "line[{line_idx}].anti -> line[{anti_idx}] normal is not opposite: {} vs {}",
                        fmt_point(line.norm),
                        fmt_point(anti.norm),
                    ));
                }
            }
        }

        for tri_idx in 0..self.tris.len() {
            let tri = self.tris[tri_idx];
            if let Some(mismatch) = dot_pair_mismatch(
                self.tris.len(),
                tri.anti,
                &format!(
                    "tri[{tri_idx}].anti = {} should reference a triangle anti",
                    tri.anti
                ),
            ) {
                return Some(mismatch);
            }
            if tri.anti >= 0 {
                let anti_idx = tri.anti as usize;
                let anti = self.tris[anti_idx];
                if anti.anti != tri_idx as i32 {
                    return Some(format!(
                        "tri[{tri_idx}].anti -> tri[{anti_idx}] does not point back: tri[{anti_idx}].anti = {}",
                        anti.anti
                    ));
                }
                if anti.is_dom_sib == tri.is_dom_sib {
                    return Some(format!(
                        "tri[{tri_idx}].anti -> tri[{anti_idx}] has matching is_dom_sib = {}, expected opposite",
                        anti.is_dom_sib
                    ));
                }
                if !same_triangle_positions(&tri, &anti) {
                    return Some(format!(
                        "tri[{tri_idx}].anti -> tri[{anti_idx}] has mismatched vertices: {} vs {}",
                        fmt_tri(&tri),
                        fmt_tri(&anti),
                    ));
                }
            }

            if let Some(mismatch) = tri_edge_mismatch(self, tri_idx) {
                return Some(mismatch);
            }
        }

        None
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

fn fmt_point(point: Float3) -> String {
    format!("[{:.6}, {:.6}, {:.6}]", point.x, point.y, point.z)
}

fn fmt_line(line: &Lin) -> String {
    format!("{} -> {}", fmt_point(line.a.pos), fmt_point(line.b.pos))
}

fn fmt_tri(tri: &Tri) -> String {
    format!(
        "{} / {} / {}",
        fmt_point(tri.a.pos),
        fmt_point(tri.b.pos),
        fmt_point(tri.c.pos),
    )
}

fn dot_pair_mismatch(len: usize, value: i32, prefix: &str) -> Option<String> {
    if value >= 0 && value as usize >= len {
        Some(format!(
            "{prefix}, but index {value} is out of bounds for len {len}"
        ))
    } else {
        None
    }
}

fn line_neighbor_mismatch(
    mesh: &Mesh,
    line_idx: usize,
    value: i32,
    direction: NeighborDirection,
) -> Option<String> {
    let line = mesh.lins[line_idx];
    let field = match direction {
        NeighborDirection::Prev => "prev",
        NeighborDirection::Next => "next",
    };

    if value >= 0 {
        let neighbor = value as usize;
        if neighbor >= mesh.lins.len() {
            return Some(format!(
                "line[{line_idx}].{field} = {value} is out of bounds for {} lines",
                mesh.lins.len()
            ));
        }

        let other = mesh.lins[neighbor];
        return match direction {
            NeighborDirection::Prev => {
                if other.next != line_idx as i32 {
                    Some(format!(
                        "line[{line_idx}].prev -> line[{neighbor}] does not point back: line[{neighbor}].next = {}",
                        other.next
                    ))
                } else if !same_point(other.b.pos, line.a.pos) {
                    Some(format!(
                        "line[{line_idx}].prev -> line[{neighbor}] has mismatched endpoint: {} does not end at {}",
                        fmt_line(&other),
                        fmt_point(line.a.pos),
                    ))
                } else if other.is_dom_sib != line.is_dom_sib {
                    Some(format!(
                        "line[{line_idx}].prev -> line[{neighbor}] has mismatched is_dom_sib: {} vs {}",
                        line.is_dom_sib, other.is_dom_sib,
                    ))
                } else if !same_vec(other.norm, line.norm) {
                    Some(format!(
                        "line[{line_idx}].prev -> line[{neighbor}] has mismatched normal: {} vs {}",
                        fmt_point(line.norm),
                        fmt_point(other.norm),
                    ))
                } else {
                    None
                }
            }
            NeighborDirection::Next => {
                if other.prev != line_idx as i32 {
                    Some(format!(
                        "line[{line_idx}].next -> line[{neighbor}] does not point back: line[{neighbor}].prev = {}",
                        other.prev
                    ))
                } else if !same_point(line.b.pos, other.a.pos) {
                    Some(format!(
                        "line[{line_idx}].next -> line[{neighbor}] has mismatched endpoint: {} does not start at {}",
                        fmt_line(&other),
                        fmt_point(line.b.pos),
                    ))
                } else if other.is_dom_sib != line.is_dom_sib {
                    Some(format!(
                        "line[{line_idx}].next -> line[{neighbor}] has mismatched is_dom_sib: {} vs {}",
                        line.is_dom_sib, other.is_dom_sib,
                    ))
                } else if !same_vec(other.norm, line.norm) {
                    Some(format!(
                        "line[{line_idx}].next -> line[{neighbor}] has mismatched normal: {} vs {}",
                        fmt_point(line.norm),
                        fmt_point(other.norm),
                    ))
                } else {
                    None
                }
            }
        };
    }

    let Some(dot_idx) = decode_ref(value) else {
        return (value != -1).then_some(format!(
            "line[{line_idx}].{field} = {value} is neither -1 nor a valid dot reference"
        ));
    };
    let Some(dot) = mesh.dots.get(dot_idx) else {
        return Some(format!(
            "line[{line_idx}].{field} references dot[{dot_idx}], but mesh only has {} dots",
            mesh.dots.len()
        ));
    };
    let expected = match direction {
        NeighborDirection::Prev => line.a.pos,
        NeighborDirection::Next => line.b.pos,
    };
    if !same_point(dot.pos, expected) {
        return Some(format!(
            "line[{line_idx}].{field} references dot[{dot_idx}] at {}, expected {}",
            fmt_point(dot.pos),
            fmt_point(expected),
        ));
    }
    None
}

fn line_inverse_mismatch(mesh: &Mesh, line_idx: usize) -> Option<String> {
    let line = mesh.lins[line_idx];
    if line.inv >= 0 {
        let inv_idx = line.inv as usize;
        let Some(inv) = mesh.lins.get(inv_idx).copied() else {
            return Some(format!(
                "line[{line_idx}].inv = {} is out of bounds for {} lines",
                line.inv,
                mesh.lins.len()
            ));
        };
        if inv.inv != line_idx as i32 {
            return Some(format!(
                "line[{line_idx}].inv -> line[{inv_idx}] does not point back: line[{inv_idx}].inv = {}",
                inv.inv
            ));
        }
        if inv.is_dom_sib == line.is_dom_sib {
            return Some(format!(
                "line[{line_idx}].inv -> line[{inv_idx}] has matching is_dom_sib = {}, expected opposite",
                inv.is_dom_sib
            ));
        }
        if !same_point(line.a.pos, inv.b.pos) || !same_point(line.b.pos, inv.a.pos) {
            return Some(format!(
                "line[{line_idx}].inv -> line[{inv_idx}] is not reversed: {} vs {}",
                fmt_line(&line),
                fmt_line(&inv),
            ));
        }
        if !same_vec(line.norm, inv.norm) {
            return Some(format!(
                "line[{line_idx}].inv -> line[{inv_idx}] has mismatched normal: {} vs {}",
                fmt_point(line.norm),
                fmt_point(inv.norm),
            ));
        }
        return None;
    }

    let Some(tri_idx) = decode_ref(line.inv) else {
        return (line.inv != -1).then_some(format!(
            "line[{line_idx}].inv = {} is neither -1 nor a valid triangle reference",
            line.inv
        ));
    };
    if tri_idx >= mesh.tris.len() {
        return Some(format!(
            "line[{line_idx}].inv references tri[{tri_idx}], but mesh only has {} triangles",
            mesh.tris.len()
        ));
    }
    if !tri_references_line(mesh, tri_idx, line_idx) {
        return Some(format!(
            "line[{line_idx}].inv references tri[{tri_idx}], but tri[{tri_idx}] does not reference line[{line_idx}]"
        ));
    }
    None
}

fn tri_edge_name(edge_idx: usize) -> &'static str {
    match edge_idx {
        0 => "ab",
        1 => "bc",
        2 => "ca",
        _ => unreachable!(),
    }
}

fn tri_edge_mismatch(mesh: &Mesh, tri_idx: usize) -> Option<String> {
    let tri = &mesh.tris[tri_idx];
    [tri.ab, tri.bc, tri.ca]
        .into_iter()
        .enumerate()
        .find_map(|(edge_idx, value)| {
            let (a, b) = tri_edge_positions(tri)[edge_idx];
            let edge_name = tri_edge_name(edge_idx);
            if value >= 0 {
                let other_idx = value as usize;
                let Some(other) = mesh.tris.get(other_idx) else {
                    return Some(format!(
                        "tri[{tri_idx}].{edge_name} = {value} is out of bounds for {} triangles",
                        mesh.tris.len()
                    ));
                };
                if !tri_edge_positions(other)
                    .into_iter()
                    .any(|(c, d)| same_point(c, b) && same_point(d, a))
                {
                    return Some(format!(
                        "tri[{tri_idx}].{edge_name} -> tri[{other_idx}] does not contain the reversed edge {} -> {}",
                        fmt_point(b),
                        fmt_point(a),
                    ));
                }
                return None;
            }

            let Some(line_idx) = decode_ref(value) else {
                return (value != -1).then_some(format!(
                    "tri[{tri_idx}].{edge_name} = {value} is neither -1 nor a valid line reference"
                ));
            };
            let Some(line) = mesh.lins.get(line_idx).copied() else {
                return Some(format!(
                    "tri[{tri_idx}].{edge_name} references line[{line_idx}], but mesh only has {} lines",
                    mesh.lins.len()
                ));
            };
            if !same_point(line.a.pos, a) || !same_point(line.b.pos, b) {
                return Some(format!(
                    "tri[{tri_idx}].{edge_name} references line[{line_idx}] with endpoints {}, expected {} -> {}",
                    fmt_line(&line),
                    fmt_point(a),
                    fmt_point(b),
                ));
            }
            if let Some(owner_idx) = decode_ref(line.inv) {
                if owner_idx != tri_idx {
                    return Some(format!(
                        "tri[{tri_idx}].{edge_name} references line[{line_idx}], but line[{line_idx}].inv points to tri[{owner_idx}]"
                    ));
                }
            } else if line.inv < -1 {
                return Some(format!(
                    "tri[{tri_idx}].{edge_name} references line[{line_idx}], but line[{line_idx}].inv = {} is not a valid triangle ref",
                    line.inv
                ));
            }
            None
        })
}

fn tri_references_line(mesh: &Mesh, tri_idx: usize, line_idx: usize) -> bool {
    let tri = &mesh.tris[tri_idx];
    [tri.ab, tri.bc, tri.ca]
        .into_iter()
        .any(|value| decode_ref(value) == Some(line_idx))
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
