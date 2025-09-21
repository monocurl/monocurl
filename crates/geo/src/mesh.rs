#[derive(Debug, Clone, Copy)]
struct Dot {
    anti: usize,
}

#[derive(Debug, Clone, Copy)]
struct LinVertex {}

#[derive(Debug, Clone, Copy)]
struct Lin {
    a: LinVertex,
    b: LinVertex,

    prev: i32,
    next: i32,
    anti_norm: i32,
}

#[derive(Debug, Clone, Copy)]
struct TriVertex {}

#[derive(Debug, Clone, Copy)]
struct Tri {
    a: TriVertex,
    b: TriVertex,
    c: TriVertex,

    ab: i32,
    bc: i32,
    ca: i32,

    anti: i32,
}

#[derive(Debug, Clone, Copy)]
struct Uniforms {}

#[derive(Debug, Clone)]
pub struct Mesh {
    dots: Vec<Dot>,
    lins: Vec<Lin>,
    tris: Vec<Tri>,

    uniform: Uniforms,

    tags: Vec<isize>,
}

impl Mesh {
    pub fn rank() -> usize {
        return 0;
    }
}

impl Mesh {}
