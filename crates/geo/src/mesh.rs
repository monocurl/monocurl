use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use crate::simd::{Float2, Float3, Float4};

#[derive(Debug, Clone, Copy)]
pub struct Dot {
    pub pos: Float3,
    pub norm: Float3,
    pub col: Float4,

    pub inv: i32,
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

    pub is_dom_sib: bool,
}

#[derive(Debug, Clone)]
pub struct Uniforms {
    pub alpha: f64,
    pub stroke_miter_radius_scale: f32,
    pub stroke_radius: f32,
    pub dot_radius: f32,
    pub dot_vertex_count: u16,
    pub smooth: bool,
    pub gloss: f32,
    pub img: Option<PathBuf>,
    pub z_index: i32,
}

pub const DEFAULT_STROKE_MITER_RADIUS_SCALE: f32 = 4.0;
pub const DEFAULT_STROKE_RADIUS: f32 = 1.0;
pub const DEFAULT_DOT_RADIUS: f32 = 4.0;
pub const DEFAULT_DOT_VERTEX_COUNT: u16 = 8;
pub const DEFAULT_SMOOTH: bool = false;
pub const DEFAULT_GLOSS: f32 = 0.0;
pub const GLOSSY_GLOSS: f32 = 0.5;

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            stroke_miter_radius_scale: DEFAULT_STROKE_MITER_RADIUS_SCALE,
            stroke_radius: DEFAULT_STROKE_RADIUS,
            dot_radius: DEFAULT_DOT_RADIUS,
            dot_vertex_count: DEFAULT_DOT_VERTEX_COUNT,
            smooth: DEFAULT_SMOOTH,
            gloss: DEFAULT_GLOSS,
            img: None,
            z_index: 0,
        }
    }
}

static NEXT_MESH_VERSION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct Mesh {
    pub dots: Vec<Dot>,
    pub lins: Vec<Lin>,
    pub tris: Vec<Tri>,

    pub uniform: Uniforms,

    pub tag: Vec<isize>,
    pub version: u64,
}

impl Mesh {
    pub fn fresh_version() -> u64 {
        NEXT_MESH_VERSION.fetch_add(1, Ordering::Relaxed)
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn bump_version(&mut self) {
        self.version = Self::fresh_version();
    }

    pub fn normalize_line_dot_topology(&mut self) {
        let original_line_count = self.lins.len();
        let mut created_inverses = vec![None; original_line_count];
        for line_idx in 0..original_line_count {
            if self.lins[line_idx].inv != -1 {
                continue;
            }

            self.lins[line_idx].is_dom_sib = true;
            let source = self.lins[line_idx];
            let inverse_idx = self.lins.len();
            self.lins[line_idx].inv = inverse_idx as i32;
            self.lins.push(Lin {
                a: source.b,
                b: source.a,
                norm: source.norm,
                prev: -1,
                next: -1,
                inv: line_idx as i32,
                is_dom_sib: false,
            });
            created_inverses[line_idx] = Some(inverse_idx);
        }

        for line_idx in 0..original_line_count {
            let Some(inverse_idx) = created_inverses[line_idx] else {
                continue;
            };
            let source = self.lins[line_idx];
            self.lins[inverse_idx].prev = mirrored_line_ref(&self.lins, source.next);
            self.lins[inverse_idx].next = mirrored_line_ref(&self.lins, source.prev);
        }

        let mut endpoint_dots = Vec::new();
        for line_idx in 0..self.lins.len() {
            let line = self.lins[line_idx];
            if line.prev == -1 {
                let dot_idx = self.dots.len() + endpoint_dots.len();
                endpoint_dots.push(endpoint_dot(
                    line.a.pos,
                    line.norm,
                    line_idx,
                    line.is_dom_sib,
                ));
                self.lins[line_idx].prev = encode_ref(dot_idx);
            }
            if line.next == -1 {
                let dot_idx = self.dots.len() + endpoint_dots.len();
                endpoint_dots.push(endpoint_dot(
                    line.b.pos,
                    line.norm,
                    line_idx,
                    line.is_dom_sib,
                ));
                self.lins[line_idx].next = encode_ref(dot_idx);
            }
        }
        self.dots.extend(endpoint_dots);

        let original_dot_count = self.dots.len();
        for dot_idx in 0..original_dot_count {
            if self.dots[dot_idx].inv != -1 {
                continue;
            }

            self.dots[dot_idx].is_dom_sib = true;
            let source = self.dots[dot_idx];
            let inverse_idx = self.dots.len();
            self.dots[dot_idx].inv = inverse_idx as i32;
            self.dots.push(Dot {
                pos: source.pos,
                norm: source.norm,
                col: source.col,
                inv: dot_idx as i32,
                is_dom_sib: false,
            });
        }
    }

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
        for dot_idx in 0..self.dots.len() {
            if let Some(mismatch) = dot_inverse_mismatch(self, dot_idx) {
                return Some(mismatch);
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
            if let Some(mismatch) = line_triangle_boundary_mismatch(self, line_idx) {
                return Some(mismatch);
            }
        }

        for tri_idx in 0..self.tris.len() {
            if let Some(mismatch) = tri_edge_mismatch(self, tri_idx) {
                return Some(mismatch);
            }
        }

        None
    }
}

impl Clone for Mesh {
    fn clone(&self) -> Self {
        Self {
            dots: self.dots.clone(),
            lins: self.lins.clone(),
            tris: self.tris.clone(),
            uniform: self.uniform.clone(),
            tag: self.tag.clone(),
            version: Self::fresh_version(),
        }
    }
}

pub fn make_mesh_mut(mesh: &mut Arc<Mesh>) -> &mut Mesh {
    let mesh = Arc::make_mut(mesh);
    mesh.bump_version();
    mesh
}

#[derive(Clone, Copy)]
enum NeighborDirection {
    Prev,
    Next,
}

fn encode_ref(idx: usize) -> i32 {
    -2 - idx as i32
}

fn mirrored_line_ref(lines: &[Lin], value: i32) -> i32 {
    if value < 0 {
        return value;
    }

    lines
        .get(value as usize)
        .map(|line| line.inv)
        .filter(|inv| *inv >= 0)
        .unwrap_or(value)
}

fn endpoint_dot(pos: Float3, norm: Float3, line_idx: usize, is_dom_sib: bool) -> Dot {
    Dot {
        pos,
        norm,
        col: Float4::ZERO,
        inv: encode_ref(line_idx),
        is_dom_sib,
    }
}

fn tri_edge_positions(tri: &Tri) -> [(Float3, Float3); 3] {
    [
        (tri.a.pos, tri.b.pos),
        (tri.b.pos, tri.c.pos),
        (tri.c.pos, tri.a.pos),
    ]
}

fn fmt_point(point: Float3) -> String {
    format!("[{:.6}, {:.6}, {:.6}]", point.x, point.y, point.z)
}

fn fmt_line(line: &Lin) -> String {
    format!("{} -> {}", fmt_point(line.a.pos), fmt_point(line.b.pos))
}

fn dot_inverse_mismatch(mesh: &Mesh, dot_idx: usize) -> Option<String> {
    let dot = mesh.dots[dot_idx];
    if dot.inv >= 0 {
        let inv_idx = dot.inv as usize;
        let Some(inv) = mesh.dots.get(inv_idx).copied() else {
            return Some(format!(
                "dot[{dot_idx}].inv = {} is out of bounds for {} dots",
                dot.inv,
                mesh.dots.len()
            ));
        };
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
        return None;
    }

    if dot.inv == -1 {
        return None;
    }

    let Some(line_idx) = decode_ref(dot.inv) else {
        return Some(format!(
            "dot[{dot_idx}].inv = {} is neither -1 nor a valid line reference",
            dot.inv
        ));
    };
    let Some(line) = mesh.lins.get(line_idx).copied() else {
        return Some(format!(
            "dot[{dot_idx}].inv references line[{line_idx}], but mesh only has {} lines",
            mesh.lins.len()
        ));
    };

    let prev_matches = decode_ref(line.prev) == Some(dot_idx);
    let next_matches = decode_ref(line.next) == Some(dot_idx);
    if !prev_matches && !next_matches {
        return Some(format!(
            "dot[{dot_idx}].inv references line[{line_idx}], but line[{line_idx}] does not point back through prev or next"
        ));
    }
    if prev_matches && !same_point(line.a.pos, dot.pos) {
        return Some(format!(
            "dot[{dot_idx}].inv references line[{line_idx}] through prev, but line[{line_idx}].a = {} instead of {}",
            fmt_point(line.a.pos),
            fmt_point(dot.pos),
        ));
    }
    if next_matches && !same_point(line.b.pos, dot.pos) {
        return Some(format!(
            "dot[{dot_idx}].inv references line[{line_idx}] through next, but line[{line_idx}].b = {} instead of {}",
            fmt_point(line.b.pos),
            fmt_point(dot.pos),
        ));
    }
    None
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
    if decode_ref(dot.inv) != Some(line_idx) {
        return Some(format!(
            "line[{line_idx}].{field} -> dot[{dot_idx}] does not point back: dot[{dot_idx}].inv = {}",
            dot.inv
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

    if line.inv == -1 {
        return (!line.is_dom_sib).then_some(format!(
            "line[{line_idx}] has no line inverse and must be dominant"
        ));
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
    if !line.is_dom_sib {
        return Some(format!(
            "line[{line_idx}] is owned by tri[{tri_idx}] and must be dominant"
        ));
    }
    None
}

fn line_triangle_boundary_mismatch(mesh: &Mesh, line_idx: usize) -> Option<String> {
    let line = mesh.lins[line_idx];
    let Some(tri_idx) = decode_ref(line.inv) else {
        return None;
    };
    if tri_idx >= mesh.tris.len() {
        return None;
    }
    if line.prev < 0 {
        return Some(format!(
            "line[{line_idx}] is owned by tri[{tri_idx}] but prev = {} does not reference a boundary line",
            line.prev
        ));
    }
    if line.next < 0 {
        return Some(format!(
            "line[{line_idx}] is owned by tri[{tri_idx}] but next = {} does not reference a boundary line",
            line.next
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

#[cfg(test)]
mod tests {
    use crate::mesh_build::mesh_ref;

    use super::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms};
    use crate::simd::{Float2, Float3, Float4};

    fn dot(pos: Float3, inv: i32) -> Dot {
        Dot {
            pos,
            norm: Float3::Z,
            col: Float4::ONE,
            inv,
            is_dom_sib: false,
        }
    }

    fn line(a: Float3, b: Float3, prev: i32, next: i32, inv: i32) -> Lin {
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
            inv,
            is_dom_sib: true,
        }
    }

    fn inverse_line(a: Float3, b: Float3, prev: i32, next: i32, inv: i32) -> Lin {
        Lin {
            is_dom_sib: false,
            ..line(a, b, prev, next, inv)
        }
    }

    fn tri(a: Float3, b: Float3, c: Float3, ab: i32, bc: i32, ca: i32) -> Tri {
        Tri {
            a: TriVertex {
                pos: a,
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            b: TriVertex {
                pos: b,
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            c: TriVertex {
                pos: c,
                col: Float4::ONE,
                uv: Float2::ZERO,
            },
            ab,
            bc,
            ca,
            is_dom_sib: false,
        }
    }

    #[test]
    fn standalone_open_line_can_omit_neighbors() {
        let mesh = Mesh {
            dots: vec![],
            lins: vec![line(Float3::ZERO, Float3::X, -1, -1, -1)],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn triangle_boundary_lines_require_prev_and_next_links() {
        let p = Float3::ZERO;
        let q = Float3::X;
        let r = Float3::Y;
        let mesh = Mesh {
            dots: vec![],
            lins: vec![
                line(p, q, -1, 1, mesh_ref(0)),
                line(q, r, 0, 2, mesh_ref(0)),
                line(r, p, 1, -1, mesh_ref(0)),
            ],
            tris: vec![tri(p, q, r, mesh_ref(0), mesh_ref(1), mesh_ref(2))],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        let report = mesh
            .topology_mismatch_report()
            .expect("surface boundary gap should be reported");
        assert!(report.contains("does not reference a boundary line"));
    }

    #[test]
    fn triangle_boundary_lines_pass_when_closed() {
        let p = Float3::ZERO;
        let q = Float3::X;
        let r = Float3::Y;
        let mesh = Mesh {
            dots: vec![],
            lins: vec![
                line(p, q, 2, 1, mesh_ref(0)),
                line(q, r, 0, 2, mesh_ref(0)),
                line(r, p, 1, 0, mesh_ref(0)),
            ],
            tris: vec![tri(p, q, r, mesh_ref(0), mesh_ref(1), mesh_ref(2))],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn standalone_lines_must_be_dominant() {
        let mut mesh = Mesh {
            dots: vec![],
            lins: vec![line(Float3::ZERO, Float3::X, -1, -1, -1)],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        mesh.lins[0].is_dom_sib = false;

        let report = mesh
            .topology_mismatch_report()
            .expect("non-dominant standalone line should be rejected");
        assert!(report.contains("must be dominant"));
    }

    #[test]
    fn inverse_line_pairs_require_exactly_one_dominant_sibling() {
        let mesh = Mesh {
            dots: vec![],
            lins: vec![
                line(Float3::ZERO, Float3::X, -1, -1, 1),
                inverse_line(Float3::X, Float3::ZERO, -1, -1, 0),
            ],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn normalize_line_dot_topology_pairs_open_lines_and_endpoints() {
        let mut mesh = Mesh {
            dots: vec![],
            lins: vec![line(Float3::ZERO, Float3::X, -1, -1, -1)],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        mesh.normalize_line_dot_topology();

        assert_eq!(mesh.lins.len(), 2);
        assert_eq!(mesh.lins[0].inv, 1);
        assert_eq!(mesh.lins[1].inv, 0);
        assert!(mesh.lins[0].is_dom_sib);
        assert!(!mesh.lins[1].is_dom_sib);
        assert_eq!(mesh.dots.len(), 4);
        assert!(mesh.dots.iter().all(|dot| dot.inv < -1));
        assert!(mesh.dots.iter().all(|dot| dot.col.w == 0.0));
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn normalize_line_dot_topology_pairs_standalone_dots() {
        let mut mesh = Mesh {
            dots: vec![dot(Float3::ZERO, -1)],
            lins: vec![],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        mesh.normalize_line_dot_topology();

        assert_eq!(mesh.dots.len(), 2);
        assert_eq!(mesh.dots[0].inv, 1);
        assert_eq!(mesh.dots[1].inv, 0);
        assert!(mesh.dots[0].is_dom_sib);
        assert!(!mesh.dots[1].is_dom_sib);
        assert!(mesh.has_consistent_topology());
    }

    #[test]
    fn line_endpoint_dots_must_point_back() {
        let mesh = Mesh {
            dots: vec![dot(Float3::ZERO, -1)],
            lins: vec![line(Float3::ZERO, Float3::X, mesh_ref(0), -1, -1)],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        let report = mesh
            .topology_mismatch_report()
            .expect("line endpoint dot without backreference should fail");
        assert!(report.contains("does not point back"));
    }

    #[test]
    fn dot_line_inverse_passes_when_line_points_back() {
        let mesh = Mesh {
            dots: vec![dot(Float3::ZERO, mesh_ref(0))],
            lins: vec![line(Float3::ZERO, Float3::X, mesh_ref(0), -1, -1)],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: vec![],
            version: Mesh::fresh_version(),
        };
        assert!(mesh.has_consistent_topology());
    }
}
