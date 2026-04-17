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
