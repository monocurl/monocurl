//! planar tessellation with two memos in front of libtess2. scenes rebuild the
//! same shapes every frame (a lerp of one parameter re-runs every constructor),
//! and libtess2 plus the boundary bookkeeping dominated those frames. contours
//! that repeat exactly hit the geometry memo; the parametrised primitives
//! (circle, regular polygon, square, rect) are tessellated once at unit size
//! and scaled per instance, so a lerp of a radius never tessellates at all.
//!
//! entries are pure functions of their key, so nothing is ever stale; memory is
//! bounded by clearing the memo when it outgrows its budget

use std::{cell::RefCell, collections::VecDeque, mem::size_of};

use executor::error::ExecutorError;
use geo::{
    mesh::{Lin, Tri},
    mesh_build::{self, BoundaryEdge, SurfaceVertex},
    simd::{Float2, Float3},
};
use libtess2::{TessellationOptions, WindingRule};
use rustc_hash::FxHashMap;

use super::helpers::{contour_area_normal, default_ink, normalize_nonzero, polygon_basis};

const MAX_ENTRIES: usize = 1 << 10;
const MAX_BYTES: usize = 32 << 20;

/// single loops up to this size go through the in-crate triangulator, whose
/// simplicity check is quadratic
const FAST_PATH_MAX_POINTS: usize = 16;

/// libtess2 results kept for reuse on later frames of a morphing outline
const REUSE_ENTRIES: usize = 64;

#[derive(Clone)]
pub(super) struct Surface {
    pub lins: Vec<Lin>,
    pub tris: Vec<Tri>,
}

impl Surface {
    fn bytes(&self) -> usize {
        self.lins.len() * size_of::<Lin>() + self.tris.len() * size_of::<Tri>()
    }

    fn parts(&self) -> (Vec<Lin>, Vec<Tri>) {
        (self.lins.clone(), self.tris.clone())
    }

    fn scaled(&self, scale: Float3) -> (Vec<Lin>, Vec<Tri>) {
        let lins = self
            .lins
            .iter()
            .map(|lin| {
                let mut lin = *lin;
                lin.a.pos = lin.a.pos * scale;
                lin.b.pos = lin.b.pos * scale;
                lin
            })
            .collect();
        let tris = self
            .tris
            .iter()
            .map(|tri| {
                let mut tri = *tri;
                tri.a.pos = tri.a.pos * scale;
                tri.b.pos = tri.b.pos * scale;
                tri.c.pos = tri.c.pos * scale;
                tri
            })
            .collect();
        (lins, tris)
    }
}

#[derive(Hash, PartialEq, Eq)]
enum Key {
    /// bit patterns of the normal, then each contour as its length followed
    /// by its points
    Exact { words: Vec<u32>, normalize: bool },
    /// unit circle / regular polygon with this many vertices
    UnitPolygon(usize),
    /// unit square centred on the origin
    UnitSquare,
}

impl Key {
    fn exact(contours: &[Vec<Float3>], normal: Float3, normalize: bool) -> Self {
        let mut words =
            Vec::with_capacity(3 + contours.iter().map(|c| 1 + 3 * c.len()).sum::<usize>());
        words.extend(normal.to_array().map(f32::to_bits));
        for contour in contours {
            words.push(contour.len() as u32);
            words.extend(contour.iter().flat_map(|p| p.to_array().map(f32::to_bits)));
        }
        Key::Exact { words, normalize }
    }
}

#[derive(Default)]
struct Memo {
    entries: FxHashMap<Key, Surface>,
    bytes: usize,
}

impl Memo {
    fn with<R>(
        &mut self,
        key: Key,
        build: impl FnOnce() -> Result<Surface, ExecutorError>,
        read: impl FnOnce(&Surface) -> R,
    ) -> Result<R, ExecutorError> {
        if let Some(surface) = self.entries.get(&key) {
            return Ok(read(surface));
        }
        let surface = build()?;
        let out = read(&surface);
        let bytes = surface.bytes();
        if bytes > MAX_BYTES / 8 {
            return Ok(out);
        }
        if self.entries.len() >= MAX_ENTRIES || self.bytes + bytes > MAX_BYTES {
            self.entries.clear();
            self.bytes = 0;
        }
        self.bytes += bytes;
        self.entries.insert(key, surface);
        Ok(out)
    }
}

thread_local! {
    static MEMO: RefCell<Memo> = RefCell::default();
}

fn memoized<R>(
    key: Key,
    build: impl FnOnce() -> Result<Surface, ExecutorError>,
    read: impl FnOnce(&Surface) -> R,
) -> Result<R, ExecutorError> {
    MEMO.with(|memo| memo.borrow_mut().with(key, build, read))
}

#[cfg(test)]
pub(super) fn clear_memo() {
    MEMO.with(|memo| *memo.borrow_mut() = Memo::default());
}

pub(crate) fn tessellate_planar_loops(
    contours: &[Vec<Float3>],
    normal: Float3,
) -> Result<(Vec<Lin>, Vec<Tri>), ExecutorError> {
    tessellate_planar_loops_with_options(contours, normal, false)
}

pub(super) fn tessellate_planar_loops_with_options(
    contours: &[Vec<Float3>],
    normal: Float3,
    normalize_input: bool,
) -> Result<(Vec<Lin>, Vec<Tri>), ExecutorError> {
    memoized(
        Key::exact(contours, normal, normalize_input),
        || tessellate_uncached(contours, normal, normalize_input),
        Surface::parts,
    )
}

/// circle of `radius` with `samples` vertices; a regular polygon is the same
/// shape with fewer samples
pub(super) fn polygon_surface(
    radius: f32,
    samples: usize,
) -> Result<(Vec<Lin>, Vec<Tri>), ExecutorError> {
    memoized(
        Key::UnitPolygon(samples),
        || tessellate_uncached(&[unit_polygon(samples)], Float3::Z, false),
        |surface| surface.scaled(Float3::new(radius, radius, 1.0)),
    )
}

/// axis-aligned rectangle centred on the origin in the xy plane
pub(super) fn rect_surface(width: f32, height: f32) -> Result<(Vec<Lin>, Vec<Tri>), ExecutorError> {
    memoized(
        Key::UnitSquare,
        || tessellate_uncached(&[unit_square()], Float3::Z, false),
        |surface| surface.scaled(Float3::new(width, height, 1.0)),
    )
}

fn unit_polygon(samples: usize) -> Vec<Float3> {
    (0..samples)
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / samples as f32;
            Float3::new(theta.cos(), theta.sin(), 0.0)
        })
        .collect()
}

fn unit_square() -> Vec<Float3> {
    vec![
        Float3::new(-0.5, -0.5, 0.0),
        Float3::new(0.5, -0.5, 0.0),
        Float3::new(0.5, 0.5, 0.0),
        Float3::new(-0.5, 0.5, 0.0),
    ]
}

fn resolve_planar_normal(contours: &[Vec<Float3>], requested: Float3) -> Float3 {
    normalize_nonzero(requested)
        .or_else(|| contour_area_normal(contours))
        .unwrap_or(Float3::Z)
}

fn tessellate_uncached(
    contours: &[Vec<Float3>],
    normal: Float3,
    normalize_input: bool,
) -> Result<Surface, ExecutorError> {
    let contours: Vec<_> = contours
        .iter()
        .filter(|contour| contour.len() >= 3)
        .cloned()
        .collect();
    if contours.is_empty() {
        return Ok(Surface {
            lins: Vec::new(),
            tris: Vec::new(),
        });
    }
    let normal = resolve_planar_normal(&contours, normal);

    let (vertices, faces) = triangulate_loops(&contours, normal, normalize_input)?;

    let edge = BoundaryEdge {
        a_col: default_ink(),
        b_col: default_ink(),
        norm: normal,
    };
    let surface_vertices: Vec<_> = vertices
        .into_iter()
        .map(|pos| SurfaceVertex {
            pos,
            col: default_ink(),
            uv: Float2::ZERO,
        })
        .collect();
    let (lins, tris) =
        mesh_build::build_indexed_surface_with(&surface_vertices, &faces, |_, _| Some(edge));
    Ok(Surface { lins, tris })
}

/// faces of a libtess2 run, in source vertex order, kept because a `Trans`
/// re-tessellates a morphing outline every frame: while every face keeps its
/// orientation, last frame's faces are still a valid triangulation of the
/// same vertices, so libtess2 is skipped
struct ReusableFaces {
    loop_lens: Vec<usize>,
    /// the first points of the outline it was made for; a morphing outline
    /// stays near them between frames, another outline with the same loop
    /// sizes does not, so the orientation check runs on one candidate
    sample: [[f64; 2]; SAMPLE_POINTS],
    faces: Vec<[usize; 3]>,
}

const SAMPLE_POINTS: usize = 8;

fn sample_points(points: &[[f64; 2]]) -> [[f64; 2]; SAMPLE_POINTS] {
    let mut sample = [[0.0; 2]; SAMPLE_POINTS];
    for (slot, point) in sample.iter_mut().zip(points) {
        *slot = *point;
    }
    sample
}

fn sample_distance(a: &[[f64; 2]; SAMPLE_POINTS], b: &[[f64; 2]; SAMPLE_POINTS]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(p, q)| (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2))
        .sum()
}

thread_local! {
    static REUSABLE: RefCell<VecDeque<ReusableFaces>> = RefCell::default();
}

/// faces over the returned vertices: the fast path and reused faces index the
/// input points themselves, libtess2 supplies its own vertex list (it adds
/// intersections) unless every vertex maps back to a source point
fn triangulate_loops(
    contours: &[Vec<Float3>],
    normal: Float3,
    normalize_input: bool,
) -> Result<(Vec<Float3>, Vec<[usize; 3]>), ExecutorError> {
    let (x, y, _) = polygon_basis(normal);
    let project = |p: &Float3| [p.dot(x) as f64, p.dot(y) as f64];
    let projected: Vec<[f64; 2]> = contours.iter().flatten().map(project).collect();
    let points = || contours.iter().flatten().copied().collect::<Vec<_>>();

    // normalising only nudges loops apart, so one loop needs none of it
    if let [contour] = contours
        && contour.len() <= FAST_PATH_MAX_POINTS
        && let Some(faces) = geo::polygon::triangulate_simple_polygon(&projected)
    {
        return Ok((points(), faces));
    }

    let loop_lens: Vec<usize> = contours.iter().map(Vec::len).collect();
    let sample = sample_points(&projected);
    let reused = REUSABLE.with(|reusable| {
        let mut reusable = reusable.borrow_mut();
        // the nearest outline of the same loop sizes is the one to try;
        // checking every candidate made a text morph twice as slow
        let (index, _) = reusable
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.loop_lens == loop_lens)
            .map(|(index, entry)| (index, sample_distance(&entry.sample, &sample)))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        if !faces_keep_orientation(&projected, &reusable[index].faces) {
            return None;
        }
        // most recently useful goes to the front, tracking the outline
        let mut entry = reusable.remove(index)?;
        entry.sample = sample;
        let faces = entry.faces.clone();
        reusable.push_front(entry);
        Some(faces)
    });
    if let Some(faces) = reused {
        return Ok((points(), faces));
    }

    let tess = tessellate_with_libtess(contours, normal, normalize_input)?;
    let Some(faces) = faces_in_source_order(&tess, projected.len()) else {
        return Ok((tess.vertices, tess.triangles));
    };
    REUSABLE.with(|reusable| {
        let mut reusable = reusable.borrow_mut();
        reusable.push_front(ReusableFaces {
            loop_lens,
            sample,
            faces: faces.clone(),
        });
        reusable.truncate(REUSE_ENTRIES);
    });
    Ok((points(), faces))
}

/// whether every face is still strictly counter-clockwise over `points`,
/// which is what keeps a triangulation valid as its vertices move
fn faces_keep_orientation(points: &[[f64; 2]], faces: &[[usize; 3]]) -> bool {
    let extent = points
        .iter()
        .fold(0.0f64, |acc, p| acc.max(p[0].abs()).max(p[1].abs()));
    let eps = extent * extent * 1e-9;
    faces.iter().all(|face| {
        let [a, b, c] = face.map(|i| points[i]);
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > eps
    })
}

/// libtess2's faces re-indexed onto the input points, when it neither added
/// nor dropped any vertex
fn faces_in_source_order(
    tess: &libtess2::Tessellation,
    point_count: usize,
) -> Option<Vec<[usize; 3]>> {
    if tess.vertices.len() != point_count {
        return None;
    }
    let mut seen = vec![false; point_count];
    let sources: Vec<usize> = tess
        .source_vertex_indices
        .iter()
        .map(|source| {
            let source = (*source)?;
            (!std::mem::replace(seen.get_mut(source)?, true)).then_some(source)
        })
        .collect::<Option<_>>()?;
    Some(
        tess.triangles
            .iter()
            .map(|face| face.map(|i| sources[i]))
            .collect(),
    )
}

/// the general tessellator: any number of loops, holes, self-intersections;
/// its output vertices are its own (intersections add some), so the faces
/// index into the returned vertex list
fn tessellate_with_libtess(
    contours: &[Vec<Float3>],
    normal: Float3,
    normalize_input: bool,
) -> Result<libtess2::Tessellation, ExecutorError> {
    libtess2::triangulate(
        contours.iter().map(Vec::as_slice),
        TessellationOptions {
            winding_rule: WindingRule::NonZero,
            normal: Some(normal),
            constrained_delaunay: true,
            reverse_contours: false,
            normalize_input,
        },
    )
    .map_err(|error| {
        ExecutorError::invalid_operation(format!("failed to tessellate polygon: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions((lins, tris): &(Vec<Lin>, Vec<Tri>)) -> Vec<Float3> {
        lins.iter()
            .flat_map(|lin| [lin.a.pos, lin.b.pos])
            .chain(
                tris.iter()
                    .flat_map(|tri| [tri.a.pos, tri.b.pos, tri.c.pos]),
            )
            .collect()
    }

    #[test]
    fn scaled_unit_polygon_matches_direct_tessellation_up_to_triangulation() {
        clear_memo();
        let direct = tessellate_uncached(
            &[unit_polygon(16)
                .into_iter()
                .map(|p| p * Float3::new(2.5, 2.5, 1.0))
                .collect()],
            Float3::Z,
            false,
        )
        .unwrap();
        let scaled = polygon_surface(2.5, 16).unwrap();
        assert_eq!(scaled.0.len(), direct.lins.len());
        assert_eq!(scaled.1.len(), direct.tris.len());
        let mut expected: Vec<_> = direct.lins.iter().map(|lin| lin.a.pos.to_array()).collect();
        let mut actual: Vec<_> = scaled.0.iter().map(|lin| lin.a.pos.to_array()).collect();
        expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
        actual.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_small_triangulator_matches_libtess_orientation() {
        let square = vec![
            Float3::new(0.0, 0.0, 0.0),
            Float3::new(1.0, 0.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(0.0, 1.0, 0.0),
        ];
        let mut reversed = square.clone();
        reversed.reverse();
        for contour in [square, reversed] {
            let (_, fast) = triangulate_loops(&[contour.clone()], Float3::Z, false).unwrap();
            let tess = tessellate_with_libtess(&[contour.clone()], Float3::Z, false).unwrap();
            let (vertices, faces) = (tess.vertices, tess.triangles);
            let signed = |points: &[Float3], face: &[usize; 3]| {
                let (a, b, c) = (points[face[0]], points[face[1]], points[face[2]]);
                (b - a).cross(c - a).z
            };
            assert!(fast.iter().all(|face| signed(&contour, face) > 0.0));
            assert!(faces.iter().all(|face| signed(&vertices, face) > 0.0));
        }
    }

    fn ring(inner: f32, outer: f32, samples: usize) -> Vec<Vec<Float3>> {
        let circle = |radius: f32, reverse: bool| {
            let mut points: Vec<_> = (0..samples)
                .map(|i| {
                    let theta = std::f32::consts::TAU * i as f32 / samples as f32;
                    Float3::new(radius * theta.cos(), radius * theta.sin(), 0.0)
                })
                .collect();
            if reverse {
                points.reverse();
            }
            points
        };
        vec![circle(outer, false), circle(inner, true)]
    }

    #[test]
    fn a_morphing_outline_reuses_last_frames_faces() {
        REUSABLE.with(|reusable| reusable.borrow_mut().clear());
        let (first_points, first) =
            triangulate_loops(&ring(0.4, 1.0, 40), Float3::Z, true).unwrap();
        assert_eq!(first_points.len(), 80);
        let (_, second) = triangulate_loops(&ring(0.45, 1.05, 40), Float3::Z, true).unwrap();
        assert_eq!(first, second);
        // a different topology never matches
        let (_, other) = triangulate_loops(&ring(0.4, 1.0, 41), Float3::Z, true).unwrap();
        assert_ne!(first.len(), other.len());
    }

    #[test]
    fn repeated_contours_come_from_the_memo() {
        clear_memo();
        let contour = vec![
            Float3::new(0.0, 0.0, 0.0),
            Float3::new(1.0, 0.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(0.0, 1.0, 0.0),
        ];
        let first = tessellate_planar_loops(&[contour.clone()], Float3::Z).unwrap();
        let entries = MEMO.with(|memo| memo.borrow().entries.len());
        let second = tessellate_planar_loops(&[contour], Float3::Z).unwrap();
        assert_eq!(entries, MEMO.with(|memo| memo.borrow().entries.len()));
        assert_eq!(positions(&first).len(), positions(&second).len());
        assert!(
            positions(&first)
                .iter()
                .zip(positions(&second).iter())
                .all(|(a, b)| a == b)
        );
    }

    #[test]
    fn memo_clears_instead_of_growing_past_its_budget() {
        clear_memo();
        for i in 0..(MAX_ENTRIES + 8) {
            let x = i as f32;
            let contour = vec![
                Float3::new(x, 0.0, 0.0),
                Float3::new(x + 1.0, 0.0, 0.0),
                Float3::new(x + 1.0, 1.0, 0.0),
            ];
            tessellate_planar_loops(&[contour], Float3::Z).unwrap();
        }
        let (entries, bytes) = MEMO.with(|memo| (memo.borrow().entries.len(), memo.borrow().bytes));
        assert!(entries <= MAX_ENTRIES);
        assert!(bytes <= MAX_BYTES);
    }
}
