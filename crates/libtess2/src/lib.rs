#![doc = include_str!("../README.md")]

use std::{
    collections::HashSet, error::Error, f32::consts::PI, ffi::c_int, fmt, mem::size_of,
    ptr::NonNull, slice,
};

pub use geo::simd::Float3;

mod raw {
    use std::ffi::{c_int, c_void};

    pub type TESSindex = c_int;
    pub type TESSreal = f32;
    pub type TESStesselator = c_void;

    pub const TESS_UNDEF: TESSindex = !0;

    pub const TESS_WINDING_ODD: c_int = 0;
    pub const TESS_WINDING_NONZERO: c_int = 1;
    pub const TESS_WINDING_POSITIVE: c_int = 2;
    pub const TESS_WINDING_NEGATIVE: c_int = 3;
    pub const TESS_WINDING_ABS_GEQ_TWO: c_int = 4;

    pub const TESS_POLYGONS: c_int = 0;

    pub const TESS_CONSTRAINED_DELAUNAY_TRIANGULATION: c_int = 0;
    pub const TESS_REVERSE_CONTOURS: c_int = 1;

    pub const TESS_STATUS_OK: c_int = 0;
    pub const TESS_STATUS_OUT_OF_MEMORY: c_int = 1;
    pub const TESS_STATUS_INVALID_INPUT: c_int = 2;

    #[link(name = "tess2_upstream", kind = "static")]
    unsafe extern "C" {
        pub fn tessNewTess(alloc: *mut c_void) -> *mut TESStesselator;
        pub fn tessDeleteTess(tess: *mut TESStesselator);
        pub fn tessAddContour(
            tess: *mut TESStesselator,
            size: c_int,
            pointer: *const c_void,
            stride: c_int,
            count: c_int,
        );
        pub fn tessSetOption(tess: *mut TESStesselator, option: c_int, value: c_int);
        pub fn tessTesselate(
            tess: *mut TESStesselator,
            winding_rule: c_int,
            element_type: c_int,
            poly_size: c_int,
            vertex_size: c_int,
            normal: *const TESSreal,
        ) -> c_int;
        pub fn tessGetVertexCount(tess: *mut TESStesselator) -> c_int;
        pub fn tessGetVertices(tess: *mut TESStesselator) -> *const TESSreal;
        pub fn tessGetVertexIndices(tess: *mut TESStesselator) -> *const TESSindex;
        pub fn tessGetElementCount(tess: *mut TESStesselator) -> c_int;
        pub fn tessGetElements(tess: *mut TESStesselator) -> *const TESSindex;
        pub fn tessGetStatus(tess: *mut TESStesselator) -> c_int;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TessStatus {
    Ok,
    OutOfMemory,
    InvalidInput,
}

impl TessStatus {
    fn from_raw(status: c_int) -> Self {
        match status {
            raw::TESS_STATUS_OK => Self::Ok,
            raw::TESS_STATUS_OUT_OF_MEMORY => Self::OutOfMemory,
            raw::TESS_STATUS_INVALID_INPUT => Self::InvalidInput,
            _ => Self::InvalidInput,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindingRule {
    #[default]
    Odd,
    NonZero,
    Positive,
    Negative,
    AbsGeqTwo,
}

impl WindingRule {
    fn as_raw(self) -> c_int {
        match self {
            Self::Odd => raw::TESS_WINDING_ODD,
            Self::NonZero => raw::TESS_WINDING_NONZERO,
            Self::Positive => raw::TESS_WINDING_POSITIVE,
            Self::Negative => raw::TESS_WINDING_NEGATIVE,
            Self::AbsGeqTwo => raw::TESS_WINDING_ABS_GEQ_TWO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TessellationOptions {
    pub winding_rule: WindingRule,
    pub normal: Option<Float3>,
    pub constrained_delaunay: bool,
    pub reverse_contours: bool,
    pub normalize_input: bool,
}

impl Default for TessellationOptions {
    fn default() -> Self {
        Self {
            winding_rule: WindingRule::Odd,
            normal: None,
            constrained_delaunay: false,
            reverse_contours: false,
            normalize_input: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tessellation {
    pub vertices: Vec<Float3>,
    pub source_vertex_indices: Vec<Option<usize>>,
    pub triangles: Vec<[usize; 3]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TessError {
    CreateFailed,
    ContourTooShort,
    TooManyVertices,
    UnexpectedTriangleIndex(raw::TESSindex),
    Failed(TessStatus),
}

impl fmt::Display for TessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateFailed => f.write_str("failed to allocate libtess2 tesselator"),
            Self::ContourTooShort => f.write_str("libtess2 contours need at least 3 vertices"),
            Self::TooManyVertices => f.write_str("contour vertex count exceeds libtess2 limits"),
            Self::UnexpectedTriangleIndex(index) => {
                write!(f, "libtess2 produced unexpected triangle index {index}")
            }
            Self::Failed(TessStatus::Ok) => f.write_str("libtess2 reported an unknown failure"),
            Self::Failed(TessStatus::OutOfMemory) => f.write_str("libtess2 ran out of memory"),
            Self::Failed(TessStatus::InvalidInput) => {
                f.write_str("libtess2 rejected the input contour data")
            }
        }
    }
}

impl Error for TessError {}

pub struct Tessellator {
    raw: NonNull<raw::TESStesselator>,
}

impl Tessellator {
    pub fn new() -> Result<Self, TessError> {
        let raw = unsafe { raw::tessNewTess(std::ptr::null_mut()) };
        let raw = NonNull::new(raw).ok_or(TessError::CreateFailed)?;
        Ok(Self { raw })
    }

    pub fn add_contour(&mut self, contour: &[Float3]) -> Result<(), TessError> {
        if contour.len() < 3 {
            return Err(TessError::ContourTooShort);
        }

        let count = c_int::try_from(contour.len()).map_err(|_| TessError::TooManyVertices)?;

        unsafe {
            raw::tessAddContour(
                self.raw.as_ptr(),
                3,
                contour.as_ptr().cast(),
                size_of::<Float3>() as c_int,
                count,
            );
        }

        self.check_status()
    }

    pub fn set_constrained_delaunay(&mut self, enabled: bool) {
        unsafe {
            raw::tessSetOption(
                self.raw.as_ptr(),
                raw::TESS_CONSTRAINED_DELAUNAY_TRIANGULATION,
                enabled as c_int,
            );
        }
    }

    pub fn set_reverse_contours(&mut self, enabled: bool) {
        unsafe {
            raw::tessSetOption(
                self.raw.as_ptr(),
                raw::TESS_REVERSE_CONTOURS,
                enabled as c_int,
            );
        }
    }

    pub fn tessellate(mut self, options: TessellationOptions) -> Result<Tessellation, TessError> {
        self.set_constrained_delaunay(options.constrained_delaunay);
        self.set_reverse_contours(options.reverse_contours);

        let normal = options.normal.map(Float3::to_array);
        let normal_ptr = normal
            .as_ref()
            .map_or(std::ptr::null(), |normal| normal.as_ptr());

        let ok = unsafe {
            raw::tessTesselate(
                self.raw.as_ptr(),
                options.winding_rule.as_raw(),
                raw::TESS_POLYGONS,
                3,
                3,
                normal_ptr,
            )
        };

        if ok == 0 {
            return Err(TessError::Failed(self.status()));
        }

        self.extract_tessellation()
    }

    fn extract_tessellation(&mut self) -> Result<Tessellation, TessError> {
        let vertex_count = self.vertex_count();
        let element_count = self.element_count();

        let vertices = if vertex_count == 0 {
            Vec::new()
        } else {
            let raw_vertices = unsafe {
                slice::from_raw_parts(raw::tessGetVertices(self.raw.as_ptr()), vertex_count * 3)
            };
            raw_vertices
                .chunks_exact(3)
                .map(|coords| Float3::new(coords[0], coords[1], coords[2]))
                .collect()
        };

        let source_vertex_indices = if vertex_count == 0 {
            Vec::new()
        } else {
            let indices = unsafe {
                slice::from_raw_parts(raw::tessGetVertexIndices(self.raw.as_ptr()), vertex_count)
            };
            indices
                .iter()
                .map(|&index| source_vertex_index(index))
                .collect()
        };

        let triangles = if element_count == 0 {
            Vec::new()
        } else {
            let elements = unsafe {
                slice::from_raw_parts(raw::tessGetElements(self.raw.as_ptr()), element_count * 3)
            };
            let mut triangles = Vec::with_capacity(element_count);
            for triangle in elements.chunks_exact(3) {
                triangles.push([
                    triangle_index(triangle[0])?,
                    triangle_index(triangle[1])?,
                    triangle_index(triangle[2])?,
                ]);
            }
            triangles
        };

        Ok(Tessellation {
            vertices,
            source_vertex_indices,
            triangles,
        })
    }

    fn vertex_count(&self) -> usize {
        unsafe { raw::tessGetVertexCount(self.raw.as_ptr()) as usize }
    }

    fn element_count(&self) -> usize {
        unsafe { raw::tessGetElementCount(self.raw.as_ptr()) as usize }
    }

    fn status(&self) -> TessStatus {
        TessStatus::from_raw(unsafe { raw::tessGetStatus(self.raw.as_ptr()) })
    }

    fn check_status(&self) -> Result<(), TessError> {
        match self.status() {
            TessStatus::Ok => Ok(()),
            status => Err(TessError::Failed(status)),
        }
    }
}

impl Drop for Tessellator {
    fn drop(&mut self) {
        unsafe {
            raw::tessDeleteTess(self.raw.as_ptr());
        }
    }
}

fn float3_key(point: Float3) -> [u32; 3] {
    [point.x.to_bits(), point.y.to_bits(), point.z.to_bits()]
}

fn rotated_contour_cmp(
    keys: &[[u32; 3]],
    lhs_start: usize,
    rhs_start: usize,
) -> std::cmp::Ordering {
    for offset in 0..keys.len() {
        let lhs = keys[(lhs_start + offset) % keys.len()];
        let rhs = keys[(rhs_start + offset) % keys.len()];
        match lhs.cmp(&rhs) {
            std::cmp::Ordering::Equal => {}
            order => return order,
        }
    }
    std::cmp::Ordering::Equal
}

fn min_rotated_contour_key(keys: &[[u32; 3]]) -> Vec<[u32; 3]> {
    if keys.is_empty() {
        return Vec::new();
    }

    let mut best_start = 0usize;
    for start in 1..keys.len() {
        if rotated_contour_cmp(keys, start, best_start).is_lt() {
            best_start = start;
        }
    }

    (0..keys.len())
        .map(|offset| keys[(best_start + offset) % keys.len()])
        .collect()
}

fn canonical_contour_key(contour: &[Float3]) -> Vec<[u32; 3]> {
    let keys: Vec<_> = contour.iter().copied().map(float3_key).collect();
    min_rotated_contour_key(&keys)
}

pub fn normalize_contours(contours: &[Vec<Float3>]) -> Vec<Vec<Float3>> {
    let mut seen = HashSet::<Vec<[u32; 3]>>::new();
    let mut normalized: Vec<Vec<Float3>> = Vec::new();

    'contours: for contour in contours {
        if !seen.insert(canonical_contour_key(contour)) {
            continue;
        }

        for existing in &normalized {
            if contours_nearly_duplicate(existing, contour) {
                continue 'contours;
            }
        }

        normalized.push(contour.clone());
    }

    normalized
}

fn contour_bounds_diag(contour: &[Float3]) -> f32 {
    let Some((&first, rest)) = contour.split_first() else {
        return 0.0;
    };

    let (min, max) = rest
        .iter()
        .copied()
        .fold((first, first), |(mut min, mut max), point| {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            min.z = min.z.min(point.z);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
            max.z = max.z.max(point.z);
            (min, max)
        });
    (max - min).len()
}

fn aligned_contour_max_dist_sq(
    lhs: &[Float3],
    rhs: &[Float3],
    reverse_rhs: bool,
    shift: usize,
) -> f32 {
    let len = lhs.len();
    let mut worst = 0.0f32;
    for (idx, lhs_point) in lhs.iter().enumerate() {
        let rhs_idx = if reverse_rhs {
            (len + shift - idx % len) % len
        } else {
            (idx + shift) % len
        };
        worst = worst.max((*lhs_point - rhs[rhs_idx]).len_sq());
    }
    worst
}

fn contours_nearly_duplicate(lhs: &[Float3], rhs: &[Float3]) -> bool {
    if lhs.len() != rhs.len() || lhs.is_empty() {
        return false;
    }

    let scale = contour_bounds_diag(lhs).max(contour_bounds_diag(rhs));
    let tolerance = 1e-4 + scale * 1e-3;
    let tolerance_sq = tolerance * tolerance;

    (0..lhs.len()).any(|shift| {
        aligned_contour_max_dist_sq(lhs, rhs, false, shift) <= tolerance_sq
            || aligned_contour_max_dist_sq(lhs, rhs, true, shift) <= tolerance_sq
    })
}

fn polygon_basis(normal: Float3) -> (Float3, Float3, Float3) {
    let normal = if normal.len_sq() == 0.0 {
        Float3::Z
    } else {
        normal.normalize()
    };
    let seed = if normal.z.abs() < 0.9 {
        Float3::Z
    } else {
        Float3::X
    };
    let x = normal.cross(seed).normalize();
    let y = normal.cross(x).normalize();
    (x, y, normal)
}

fn inferred_batch_normal(contours: &[Vec<Float3>], preferred: Option<Float3>) -> Float3 {
    if let Some(normal) = preferred.filter(|normal| normal.len_sq() > 0.0) {
        return normal.normalize();
    }

    contours
        .iter()
        .find_map(|contour| {
            let area_normal = contour
                .iter()
                .copied()
                .zip(contour.iter().copied().cycle().skip(1))
                .take(contour.len())
                .fold(Float3::ZERO, |acc, (a, b)| acc + a.cross(b));
            (area_normal.len_sq() > 1e-8).then_some(area_normal.normalize())
        })
        .unwrap_or(Float3::Z)
}

#[cfg(test)]
fn signed_area_2d(points: &[(f32, f32)]) -> f32 {
    points
        .iter()
        .copied()
        .zip(points.iter().copied().cycle().skip(1))
        .take(points.len())
        .fold(0.0, |area, ((ax, ay), (bx, by))| area + ax * by - ay * bx)
        * 0.5
}

fn separate_contours_with_sources(
    contours: &[(usize, Vec<Float3>)],
    normal: Option<Float3>,
) -> Vec<(usize, Vec<Float3>)> {
    if contours.len() <= 1 {
        return contours.to_vec();
    }

    let contour_points: Vec<_> = contours
        .iter()
        .map(|(_, contour)| contour.clone())
        .collect();
    let (basis_x, basis_y, _) = polygon_basis(inferred_batch_normal(&contour_points, normal));
    let contour_count = contours.len() as f32;

    contours
        .iter()
        .enumerate()
        .map(|(q, (contour_idx, contour))| {
            if q == 0 {
                return (*contour_idx, contour.clone());
            }

            let theta = 2.0 * PI * q as f32 * (contour_count - 1.0) / contour_count;
            let delta = basis_x * (theta.cos() * 3e-3) + basis_y * (theta.sin() * 3e-3);
            (
                *contour_idx,
                contour.iter().map(|point| *point + delta).collect(),
            )
        })
        .collect()
}

fn triangulate_batch(
    contours: &[(usize, Vec<Float3>)],
    source_offsets: &[usize],
    mut options: TessellationOptions,
) -> Result<Tessellation, TessError> {
    let original_vertices = contours
        .iter()
        .flat_map(|(_, contour)| contour.iter().copied())
        .collect::<Vec<_>>();
    let contours = if options.normalize_input {
        separate_contours_with_sources(contours, options.normal)
    } else {
        contours.to_vec()
    };

    let mut tessellator = Tessellator::new()?;
    let mut local_to_global_source = Vec::new();
    for (contour_idx, contour) in &contours {
        tessellator.add_contour(contour)?;
        let offset = source_offsets[*contour_idx];
        local_to_global_source.extend((0..contour.len()).map(|vertex_idx| offset + vertex_idx));
    }

    options.normalize_input = false;
    let mut tessellation = tessellator.tessellate(options)?;
    for source in &mut tessellation.source_vertex_indices {
        *source = source.and_then(|local_idx| local_to_global_source.get(local_idx).copied());
    }
    for (vertex, source) in tessellation
        .vertices
        .iter_mut()
        .zip(tessellation.source_vertex_indices.iter().copied())
    {
        if let Some(source) = source {
            *vertex = original_vertices[source];
        }
    }
    Ok(tessellation)
}

pub fn triangulate<I, C>(
    contours: I,
    options: TessellationOptions,
) -> Result<Tessellation, TessError>
where
    I: IntoIterator<Item = C>,
    C: AsRef<[Float3]>,
{
    let contours: Vec<_> = contours
        .into_iter()
        .map(|contour| contour.as_ref().to_vec())
        .collect();
    if contours.is_empty() {
        return Ok(Tessellation {
            vertices: Vec::new(),
            source_vertex_indices: Vec::new(),
            triangles: Vec::new(),
        });
    }

    let source_offsets: Vec<_> = contours
        .iter()
        .scan(0usize, |offset, contour| {
            let current = *offset;
            *offset += contour.len();
            Some(current)
        })
        .collect();

    let indexed_contours = contours.into_iter().enumerate().collect::<Vec<_>>();
    triangulate_batch(&indexed_contours, &source_offsets, options)
}

fn triangle_index(index: raw::TESSindex) -> Result<usize, TessError> {
    if index == raw::TESS_UNDEF || index < 0 {
        return Err(TessError::UnexpectedTriangleIndex(index));
    }

    Ok(index as usize)
}

fn source_vertex_index(index: raw::TESSindex) -> Option<usize> {
    if index == raw::TESS_UNDEF || index < 0 {
        None
    } else {
        Some(index as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangulates_a_square() {
        let contour = [
            Float3::new(0.0, 0.0, 0.0),
            Float3::new(1.0, 0.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(0.0, 1.0, 0.0),
        ];

        let tessellation =
            triangulate([contour.as_slice()], TessellationOptions::default()).unwrap();

        assert_eq!(tessellation.triangles.len(), 2);
        assert_eq!(tessellation.vertices.len(), 4);
        let mut source_indices = tessellation
            .source_vertex_indices
            .iter()
            .copied()
            .flatten()
            .collect::<Vec<_>>();
        source_indices.sort_unstable();
        assert_eq!(source_indices, vec![0, 1, 2, 3]);
    }

    #[test]
    fn triangulates_a_tilted_square_in_3d() {
        let contour = [
            Float3::new(0.0, 0.0, 0.0),
            Float3::new(1.0, 0.0, 1.0),
            Float3::new(1.0, 1.0, 2.0),
            Float3::new(0.0, 1.0, 1.0),
        ];

        let tessellation = triangulate(
            [contour.as_slice()],
            TessellationOptions {
                normal: Some(Float3::new(-1.0, -1.0, 1.0)),
                ..TessellationOptions::default()
            },
        )
        .unwrap();

        assert_eq!(tessellation.triangles.len(), 2);
        assert_eq!(tessellation.vertices.len(), 4);
        assert!(
            tessellation
                .vertices
                .iter()
                .all(|vertex| (vertex.z - (vertex.x + vertex.y)).abs() < 1e-5)
        );
    }

    #[test]
    fn normalize_contours_separates_duplicate_loops() {
        let square = vec![
            Float3::new(-1.0, -1.0, 0.0),
            Float3::new(1.0, -1.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(-1.0, 1.0, 0.0),
        ];
        let contours = vec![square.clone(), square];

        let normalized = normalize_contours(&contours);

        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0], contours[0]);
    }

    #[test]
    fn triangulates_duplicate_squares_when_normalized() {
        let contour = vec![
            Float3::new(-1.0, -1.0, 0.0),
            Float3::new(1.0, -1.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(-1.0, 1.0, 0.0),
        ];

        let tessellation = triangulate(
            [contour.as_slice(), contour.as_slice()],
            TessellationOptions {
                winding_rule: WindingRule::NonZero,
                constrained_delaunay: true,
                normalize_input: true,
                ..TessellationOptions::default()
            },
        )
        .unwrap();

        assert!(!tessellation.triangles.is_empty());
    }

    #[test]
    fn normalize_contours_dedupes_nearly_identical_rotations() {
        let base = vec![
            Float3::new(0.0, 0.0, 0.0),
            Float3::new(1.0, 0.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(0.0, 1.0, 0.0),
        ];
        let near = vec![
            Float3::new(1.0 + 2e-5, 1.0 - 2e-5, 0.0),
            Float3::new(-1e-5, 1.0 + 1e-5, 0.0),
            Float3::new(1e-5, -1e-5, 0.0),
            Float3::new(1.0 - 2e-5, 2e-5, 0.0),
        ];

        let normalized = normalize_contours(&[base, near]);

        assert_eq!(normalized.len(), 1);
    }

    #[test]
    fn separate_contours_preserves_authored_winding() {
        let outer = vec![
            Float3::new(-2.0, -2.0, 0.0),
            Float3::new(2.0, -2.0, 0.0),
            Float3::new(2.0, 2.0, 0.0),
            Float3::new(-2.0, 2.0, 0.0),
        ];
        let hole = vec![
            Float3::new(-1.0, -1.0, 0.0),
            Float3::new(-1.0, 1.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(1.0, -1.0, 0.0),
        ];

        let separated = separate_contours_with_sources(
            &[(0, outer.clone()), (1, hole.clone())],
            Some(Float3::Z),
        );
        let outer_area = signed_area_2d(
            &separated[0]
                .1
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        );
        let hole_area = signed_area_2d(
            &separated[1]
                .1
                .iter()
                .map(|point| (point.x, point.y))
                .collect::<Vec<_>>(),
        );

        assert!(outer_area * hole_area < 0.0);
    }

    #[test]
    fn triangulates_nearly_identical_squares_in_separate_batches_when_normalized() {
        let base = vec![
            Float3::new(-1.0, -1.0, 0.0),
            Float3::new(1.0, -1.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(-1.0, 1.0, 0.0),
        ];
        let near = vec![
            Float3::new(-0.99, -1.0, 0.0),
            Float3::new(1.01, -1.0, 0.0),
            Float3::new(1.01, 1.0, 0.0),
            Float3::new(-0.99, 1.0, 0.0),
        ];

        let tessellation = triangulate(
            [base.as_slice(), near.as_slice()],
            TessellationOptions {
                winding_rule: WindingRule::NonZero,
                constrained_delaunay: true,
                normalize_input: true,
                ..TessellationOptions::default()
            },
        )
        .unwrap();

        let mut source_indices = tessellation
            .source_vertex_indices
            .iter()
            .copied()
            .flatten()
            .collect::<Vec<_>>();
        source_indices.sort_unstable();
        source_indices.dedup();

        assert!(tessellation.triangles.len() >= 4);
        assert_eq!(source_indices, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    fn point_in_triangle_2d(point: (f32, f32), a: Float3, b: Float3, c: Float3) -> bool {
        let cross = |p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)| {
            (p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0)
        };
        let a2 = (a.x, a.y);
        let b2 = (b.x, b.y);
        let c2 = (c.x, c.y);
        let s1 = cross(a2, b2, point);
        let s2 = cross(b2, c2, point);
        let s3 = cross(c2, a2, point);
        (s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0) || (s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0)
    }

    #[test]
    fn triangulate_preserves_hole_winding_for_nested_opposite_winding_contours() {
        let outer = vec![
            Float3::new(-2.0, -2.0, 0.0),
            Float3::new(2.0, -2.0, 0.0),
            Float3::new(2.0, 2.0, 0.0),
            Float3::new(-2.0, 2.0, 0.0),
        ];
        let mut hole = vec![
            Float3::new(-1.0, -1.0, 0.0),
            Float3::new(1.0, -1.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(-1.0, 1.0, 0.0),
        ];
        hole.reverse();

        let tessellation = triangulate(
            [outer.as_slice(), hole.as_slice()],
            TessellationOptions {
                winding_rule: WindingRule::NonZero,
                constrained_delaunay: true,
                normalize_input: true,
                ..TessellationOptions::default()
            },
        )
        .unwrap();

        let center_is_filled = tessellation.triangles.iter().any(|face| {
            point_in_triangle_2d(
                (0.0, 0.0),
                tessellation.vertices[face[0]],
                tessellation.vertices[face[1]],
                tessellation.vertices[face[2]],
            )
        });

        assert!(
            !center_is_filled,
            "nested inner contour should stay excluded when authored as a hole"
        );
    }
}
