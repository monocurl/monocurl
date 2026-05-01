use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};
use wide::f32x4;

// scalar: 2-wide SIMD offers no real benefit

#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Float2 {
    pub x: f32,
    pub y: f32,
}

impl Float2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const ONE: Self = Self { x: 1.0, y: 1.0 };
    pub const X: Self = Self { x: 1.0, y: 0.0 };
    pub const Y: Self = Self { x: 0.0, y: 1.0 };

    #[inline]
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    #[inline]
    pub fn splat(v: f32) -> Self {
        Self { x: v, y: v }
    }
    #[inline]
    pub fn to_array(self) -> [f32; 2] {
        [self.x, self.y]
    }
    #[inline]
    pub fn from_array([x, y]: [f32; 2]) -> Self {
        Self { x, y }
    }
    #[inline]
    pub fn extend(self, z: f32) -> Float3 {
        Float3 {
            x: self.x,
            y: self.y,
            z,
        }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }
    #[inline]
    pub fn len_sq(self) -> f32 {
        self.dot(self)
    }
    #[inline]
    pub fn len(self) -> f32 {
        self.len_sq().sqrt()
    }
    #[inline]
    pub fn normalize(self) -> Self {
        self * (1.0 / self.len())
    }
    /// 90° ccw rotation
    #[inline]
    pub fn perp(self) -> Self {
        Self {
            x: -self.y,
            y: self.x,
        }
    }
    #[inline]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }
}

impl Add for Float2 {
    type Output = Self;
    #[inline]
    fn add(self, r: Self) -> Self {
        Self {
            x: self.x + r.x,
            y: self.y + r.y,
        }
    }
}
impl Sub for Float2 {
    type Output = Self;
    #[inline]
    fn sub(self, r: Self) -> Self {
        Self {
            x: self.x - r.x,
            y: self.y - r.y,
        }
    }
}
impl Mul for Float2 {
    type Output = Self;
    #[inline]
    fn mul(self, r: Self) -> Self {
        Self {
            x: self.x * r.x,
            y: self.y * r.y,
        }
    }
}
impl Div for Float2 {
    type Output = Self;
    #[inline]
    fn div(self, r: Self) -> Self {
        Self {
            x: self.x / r.x,
            y: self.y / r.y,
        }
    }
}
impl Neg for Float2 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}
impl Mul<f32> for Float2 {
    type Output = Self;
    #[inline]
    fn mul(self, s: f32) -> Self {
        Self {
            x: self.x * s,
            y: self.y * s,
        }
    }
}
impl Div<f32> for Float2 {
    type Output = Self;
    #[inline]
    fn div(self, s: f32) -> Self {
        Self {
            x: self.x / s,
            y: self.y / s,
        }
    }
}
impl AddAssign for Float2 {
    #[inline]
    fn add_assign(&mut self, r: Self) {
        *self = *self + r;
    }
}
impl SubAssign for Float2 {
    #[inline]
    fn sub_assign(&mut self, r: Self) {
        *self = *self - r;
    }
}
impl MulAssign<f32> for Float2 {
    #[inline]
    fn mul_assign(&mut self, s: f32) {
        *self = *self * s;
    }
}

// ops use f32x4 with w=0 padding; 3/4 lanes used

#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Float3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Float3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };
    pub const X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
    };
    pub const Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
    };

    #[inline]
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    #[inline]
    pub fn splat(v: f32) -> Self {
        Self { x: v, y: v, z: v }
    }
    #[inline]
    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
    #[inline]
    pub fn from_array([x, y, z]: [f32; 3]) -> Self {
        Self { x, y, z }
    }
    #[inline]
    pub fn extend(self, w: f32) -> Float4 {
        Float4 {
            x: self.x,
            y: self.y,
            z: self.z,
            w,
        }
    }
    #[inline]
    pub fn truncate(self) -> Float2 {
        Float2 {
            x: self.x,
            y: self.y,
        }
    }

    #[inline]
    fn to_simd(self) -> f32x4 {
        f32x4::from([self.x, self.y, self.z, 0.0])
    }
    #[inline]
    fn from_simd(v: f32x4) -> Self {
        let a = v.to_array();
        Self {
            x: a[0],
            y: a[1],
            z: a[2],
        }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }
    #[inline]
    pub fn cross(self, rhs: Self) -> Self {
        Self {
            x: self.y * rhs.z - self.z * rhs.y,
            y: self.z * rhs.x - self.x * rhs.z,
            z: self.x * rhs.y - self.y * rhs.x,
        }
    }
    #[inline]
    pub fn len_sq(self) -> f32 {
        self.dot(self)
    }
    #[inline]
    pub fn len(self) -> f32 {
        self.len_sq().sqrt()
    }
    #[inline]
    pub fn normalize(self) -> Self {
        self * (1.0 / self.len())
    }
    #[inline]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }
}

impl Add for Float3 {
    type Output = Self;
    #[inline]
    fn add(self, r: Self) -> Self {
        Self::from_simd(self.to_simd() + r.to_simd())
    }
}
impl Sub for Float3 {
    type Output = Self;
    #[inline]
    fn sub(self, r: Self) -> Self {
        Self::from_simd(self.to_simd() - r.to_simd())
    }
}
impl Mul for Float3 {
    type Output = Self;
    #[inline]
    fn mul(self, r: Self) -> Self {
        Self::from_simd(self.to_simd() * r.to_simd())
    }
}
impl Div for Float3 {
    type Output = Self;
    #[inline]
    fn div(self, r: Self) -> Self {
        Self::from_simd(self.to_simd() / r.to_simd())
    }
}
impl Neg for Float3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
        }
    }
}
impl Mul<f32> for Float3 {
    type Output = Self;
    #[inline]
    fn mul(self, s: f32) -> Self {
        Self::from_simd(self.to_simd() * f32x4::splat(s))
    }
}
impl Div<f32> for Float3 {
    type Output = Self;
    #[inline]
    fn div(self, s: f32) -> Self {
        Self::from_simd(self.to_simd() / f32x4::splat(s))
    }
}
impl AddAssign for Float3 {
    #[inline]
    fn add_assign(&mut self, r: Self) {
        *self = *self + r;
    }
}
impl SubAssign for Float3 {
    #[inline]
    fn sub_assign(&mut self, r: Self) {
        *self = *self - r;
    }
}
impl MulAssign<f32> for Float3 {
    #[inline]
    fn mul_assign(&mut self, s: f32) {
        *self = *self * s;
    }
}

// ops map directly to f32x4; all 4 lanes used

#[repr(C, align(16))]
#[derive(Copy, Clone, Debug, PartialEq, Default)]
pub struct Float4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Float4 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };
    pub const ONE: Self = Self {
        x: 1.0,
        y: 1.0,
        z: 1.0,
        w: 1.0,
    };
    pub const X: Self = Self {
        x: 1.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };
    pub const Y: Self = Self {
        x: 0.0,
        y: 1.0,
        z: 0.0,
        w: 0.0,
    };
    pub const Z: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 1.0,
        w: 0.0,
    };
    pub const W: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
    };

    #[inline]
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
    #[inline]
    pub fn splat(v: f32) -> Self {
        Self {
            x: v,
            y: v,
            z: v,
            w: v,
        }
    }
    #[inline]
    pub fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }
    #[inline]
    pub fn from_array([x, y, z, w]: [f32; 4]) -> Self {
        Self { x, y, z, w }
    }
    #[inline]
    pub fn truncate(self) -> Float3 {
        Float3 {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }

    #[inline]
    fn to_simd(self) -> f32x4 {
        f32x4::from([self.x, self.y, self.z, self.w])
    }
    #[inline]
    fn from_simd(v: f32x4) -> Self {
        let [x, y, z, w] = v.to_array();
        Self { x, y, z, w }
    }

    #[inline]
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z + self.w * rhs.w
    }
    #[inline]
    pub fn len_sq(self) -> f32 {
        self.dot(self)
    }
    #[inline]
    pub fn len(self) -> f32 {
        self.len_sq().sqrt()
    }
    #[inline]
    pub fn normalize(self) -> Self {
        self * (1.0 / self.len())
    }
    #[inline]
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }
}

impl Add for Float4 {
    type Output = Self;
    #[inline]
    fn add(self, r: Self) -> Self {
        Self::from_simd(self.to_simd() + r.to_simd())
    }
}
impl Sub for Float4 {
    type Output = Self;
    #[inline]
    fn sub(self, r: Self) -> Self {
        Self::from_simd(self.to_simd() - r.to_simd())
    }
}
impl Mul for Float4 {
    type Output = Self;
    #[inline]
    fn mul(self, r: Self) -> Self {
        Self::from_simd(self.to_simd() * r.to_simd())
    }
}
impl Div for Float4 {
    type Output = Self;
    #[inline]
    fn div(self, r: Self) -> Self {
        Self::from_simd(self.to_simd() / r.to_simd())
    }
}
impl Neg for Float4 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z,
            w: -self.w,
        }
    }
}
impl Mul<f32> for Float4 {
    type Output = Self;
    #[inline]
    fn mul(self, s: f32) -> Self {
        Self::from_simd(self.to_simd() * f32x4::splat(s))
    }
}
impl Div<f32> for Float4 {
    type Output = Self;
    #[inline]
    fn div(self, s: f32) -> Self {
        Self::from_simd(self.to_simd() / f32x4::splat(s))
    }
}
impl AddAssign for Float4 {
    #[inline]
    fn add_assign(&mut self, r: Self) {
        *self = *self + r;
    }
}
impl SubAssign for Float4 {
    #[inline]
    fn sub_assign(&mut self, r: Self) {
        *self = *self - r;
    }
}
impl MulAssign<f32> for Float4 {
    #[inline]
    fn mul_assign(&mut self, s: f32) {
        *self = *self * s;
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Mat2 {
    pub cols: [Float2; 2],
}

impl Mat2 {
    #[inline]
    pub fn from_cols(c0: Float2, c1: Float2) -> Self {
        Self { cols: [c0, c1] }
    }

    pub fn identity() -> Self {
        Self::from_cols(Float2::X, Float2::Y)
    }

    pub fn transpose(self) -> Self {
        let [c0, c1] = self.cols;
        Self::from_cols(Float2::new(c0.x, c1.x), Float2::new(c0.y, c1.y))
    }

    pub fn det(self) -> f32 {
        let [c0, c1] = self.cols;
        c0.x * c1.y - c1.x * c0.y
    }

    pub fn inverse(self) -> Self {
        let inv_d = 1.0 / self.det();
        let [c0, c1] = self.cols;
        Self::from_cols(
            Float2::new(c1.y * inv_d, -c0.y * inv_d),
            Float2::new(-c1.x * inv_d, c0.x * inv_d),
        )
    }

    #[inline]
    pub fn mul_vec(self, v: Float2) -> Float2 {
        self.cols[0] * v.x + self.cols[1] * v.y
    }
}

impl Mul for Mat2 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            cols: rhs.cols.map(|c| self.mul_vec(c)),
        }
    }
}

impl Mul<Float2> for Mat2 {
    type Output = Float2;
    #[inline]
    fn mul(self, v: Float2) -> Float2 {
        self.mul_vec(v)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Mat3 {
    pub cols: [Float3; 3],
}

impl Mat3 {
    #[inline]
    pub fn from_cols(c0: Float3, c1: Float3, c2: Float3) -> Self {
        Self { cols: [c0, c1, c2] }
    }

    pub fn identity() -> Self {
        Self::from_cols(Float3::X, Float3::Y, Float3::Z)
    }

    pub fn transpose(self) -> Self {
        let [c0, c1, c2] = self.cols.map(|c| [c.x, c.y, c.z]);
        Self::from_cols(
            Float3::new(c0[0], c1[0], c2[0]),
            Float3::new(c0[1], c1[1], c2[1]),
            Float3::new(c0[2], c1[2], c2[2]),
        )
    }

    // column-major: c0 = (a00, a10, a20), c1 = (a01, a11, a21), c2 = (a02, a12, a22)
    pub fn det(self) -> f32 {
        let [c0, c1, c2] = self.cols;
        c0.x * (c1.y * c2.z - c2.y * c1.z) - c1.x * (c0.y * c2.z - c2.y * c0.z)
            + c2.x * (c0.y * c1.z - c1.y * c0.z)
    }

    pub fn inverse(self) -> Self {
        let [c0, c1, c2] = self.cols;
        let inv_d = 1.0 / self.det();
        // columns of the adjugate (transposed cofactor matrix), divided by det
        Self::from_cols(
            Float3::new(
                (c1.y * c2.z - c2.y * c1.z) * inv_d,
                -(c1.x * c2.z - c2.x * c1.z) * inv_d,
                (c1.x * c2.y - c2.x * c1.y) * inv_d,
            ),
            Float3::new(
                -(c0.y * c2.z - c2.y * c0.z) * inv_d,
                (c0.x * c2.z - c2.x * c0.z) * inv_d,
                -(c0.x * c2.y - c2.x * c0.y) * inv_d,
            ),
            Float3::new(
                (c0.y * c1.z - c1.y * c0.z) * inv_d,
                -(c0.x * c1.z - c1.x * c0.z) * inv_d,
                (c0.x * c1.y - c1.x * c0.y) * inv_d,
            ),
        )
    }

    #[inline]
    pub fn mul_vec(self, v: Float3) -> Float3 {
        self.cols[0] * v.x + self.cols[1] * v.y + self.cols[2] * v.z
    }
}

impl Mul for Mat3 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            cols: rhs.cols.map(|c| self.mul_vec(c)),
        }
    }
}

impl Mul<Float3> for Mat3 {
    type Output = Float3;
    #[inline]
    fn mul(self, v: Float3) -> Float3 {
        self.mul_vec(v)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Mat4 {
    pub cols: [Float4; 4],
}

impl Mat4 {
    #[inline]
    pub fn from_cols(c0: Float4, c1: Float4, c2: Float4, c3: Float4) -> Self {
        Self {
            cols: [c0, c1, c2, c3],
        }
    }

    pub fn identity() -> Self {
        Self::from_cols(Float4::X, Float4::Y, Float4::Z, Float4::W)
    }

    /// embed Mat3 in top-left, filling the 4th row/col with the identity
    pub fn from_mat3(m: Mat3) -> Self {
        Self::from_cols(
            m.cols[0].extend(0.0),
            m.cols[1].extend(0.0),
            m.cols[2].extend(0.0),
            Float4::W,
        )
    }

    pub fn transpose(self) -> Self {
        let [c0, c1, c2, c3] = self.cols.map(|c| [c.x, c.y, c.z, c.w]);
        Self::from_cols(
            Float4::new(c0[0], c1[0], c2[0], c3[0]),
            Float4::new(c0[1], c1[1], c2[1], c3[1]),
            Float4::new(c0[2], c1[2], c2[2], c3[2]),
            Float4::new(c0[3], c1[3], c2[3], c3[3]),
        )
    }

    #[inline]
    pub fn mul_vec(self, v: Float4) -> Float4 {
        self.cols[0] * v.x + self.cols[1] * v.y + self.cols[2] * v.z + self.cols[3] * v.w
    }
}

impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self {
            cols: rhs.cols.map(|c| self.mul_vec(c)),
        }
    }
}

impl Mul<Float4> for Mat4 {
    type Output = Float4;
    #[inline]
    fn mul(self, v: Float4) -> Float4 {
        self.mul_vec(v)
    }
}
