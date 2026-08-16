//! Minimal numeric/geometry payload types.
//!
//! Per `OPERATORS.md` / `API_DIRECTION.md`: "Do not wire into executor, geo, or
//! runtime in this pass — `MeshValue`'s concrete mesh payload can remain
//! simple." These types exist only so the operator/lerp machinery has
//! something concrete to move around; they are not `crates/geo` types.

use crate::{error::Result, keyed::Keyed};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

impl Keyed for Vec2 {
    fn lerp(&self, other: &Self, t: f64) -> Result<Self> {
        Ok(Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
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

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn scale_by(self, factor: Self) -> Self {
        Self::new(self.x * factor.x, self.y * factor.y, self.z * factor.z)
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl Keyed for Vec3 {
    fn lerp(&self, other: &Self, t: f64) -> Result<Self> {
        Ok(Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl Vec4 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 0.0,
    };

    pub const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }
}

impl Keyed for Vec4 {
    fn lerp(&self, other: &Self, t: f64) -> Result<Self> {
        Ok(Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
            self.w + (other.w - self.w) * t,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub const BLUE: Self = Self::rgb(0.176, 0.451, 0.961);
    pub const ORANGE: Self = Self::rgb(0.961, 0.529, 0.176);

    pub const fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }
}

impl Keyed for Color {
    fn lerp(&self, other: &Self, t: f64) -> Result<Self> {
        Ok(Self::rgba(
            self.r + (other.r - self.r) * t,
            self.g + (other.g - self.g) * t,
            self.b + (other.b - self.b) * t,
            self.a + (other.a - self.a) * t,
        ))
    }
}
