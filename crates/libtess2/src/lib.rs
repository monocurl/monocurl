#![doc = include_str!("../README.md")]

use std::{error::Error, ffi::c_int, fmt, mem::size_of, ptr::NonNull, slice};

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
}

impl Default for TessellationOptions {
    fn default() -> Self {
        Self {
            winding_rule: WindingRule::Odd,
            normal: None,
            constrained_delaunay: false,
            reverse_contours: false,
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

pub fn triangulate<I, C>(
    contours: I,
    options: TessellationOptions,
) -> Result<Tessellation, TessError>
where
    I: IntoIterator<Item = C>,
    C: AsRef<[Float3]>,
{
    let mut tessellator = Tessellator::new()?;
    for contour in contours {
        tessellator.add_contour(contour.as_ref())?;
    }
    tessellator.tessellate(options)
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
}
