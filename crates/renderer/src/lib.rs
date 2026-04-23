mod blade;
mod software;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use anyhow::Result;
use executor::scene_snapshot::{BackgroundSnapshot, CameraSnapshot, SceneSnapshot};
use geo::{
    mesh::{Dot, Lin, Mesh, Tri},
    simd::{Float2, Float3, Float4},
};
pub use image::RgbaImage;

use crate::{blade::BladeRenderer, software::SoftwareRenderer};

#[derive(Clone, Debug)]
pub struct SceneRenderData {
    pub background: BackgroundSnapshot,
    pub camera: CameraSnapshot,
    pub meshes: Vec<Arc<Mesh>>,
}

impl From<SceneSnapshot> for SceneRenderData {
    fn from(snapshot: SceneSnapshot) -> Self {
        Self {
            background: snapshot.background,
            camera: snapshot.camera,
            meshes: snapshot.meshes,
        }
    }
}

pub fn scene_fingerprint(scene: &SceneRenderData) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_background(&mut hasher, scene.background);
    hash_camera(&mut hasher, &scene.camera);
    for mesh in &scene.meshes {
        mesh_fingerprint_into(&mut hasher, mesh);
    }
    hasher.finish()
}

pub(crate) fn mesh_fingerprint(mesh: &Mesh) -> u64 {
    let mut hasher = DefaultHasher::new();
    mesh_fingerprint_into(&mut hasher, mesh);
    hasher.finish()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderSize {
    pub width: u32,
    pub height: u32,
}

impl RenderSize {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderStyle {
    pub line_width_px: f32,
    pub dot_radius_px: f32,
}

impl Default for RenderStyle {
    fn default() -> Self {
        Self {
            line_width_px: 1.0,
            dot_radius_px: 3.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    Auto,
    Blade,
    Software,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderOptions {
    pub backend: BackendKind,
    pub style: RenderStyle,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            backend: BackendKind::Auto,
            style: RenderStyle::default(),
        }
    }
}

pub struct Renderer {
    backend: Backend,
}

enum Backend {
    Blade(BladeRenderer),
    Software(SoftwareRenderer),
}

impl Backend {
    fn kind(&self) -> BackendKind {
        match self {
            Self::Blade(_) => BackendKind::Blade,
            Self::Software(_) => BackendKind::Software,
        }
    }

    fn render(&mut self, scene: &SceneRenderData, size: RenderSize) -> Result<RgbaImage> {
        match self {
            Self::Blade(renderer) => renderer.render(scene, size),
            Self::Software(renderer) => renderer.render(scene, size),
        }
    }
}

impl Renderer {
    pub fn new(options: RenderOptions) -> Self {
        match Self::try_new(options) {
            Ok(renderer) => renderer,
            Err(error) => {
                log::warn!(
                    "renderer backend initialization failed for {:?}: {error:#}; falling back to software renderer",
                    options.backend
                );
                Self {
                    backend: Backend::Software(SoftwareRenderer::new(options.style)),
                }
            }
        }
    }

    pub fn try_new(options: RenderOptions) -> Result<Self> {
        let backend = match options.backend {
            BackendKind::Auto => match BladeRenderer::new(options.style) {
                Ok(renderer) => Backend::Blade(renderer),
                Err(error) => {
                    log::warn!(
                        "blade renderer unavailable: {error:#}; using software renderer instead"
                    );
                    Backend::Software(SoftwareRenderer::new(options.style))
                }
            },
            BackendKind::Blade => Backend::Blade(BladeRenderer::new(options.style)?),
            BackendKind::Software => Backend::Software(SoftwareRenderer::new(options.style)),
        };
        Ok(Self { backend })
    }

    pub fn backend_kind(&self) -> BackendKind {
        self.backend.kind()
    }

    /// Returns a frame encoded in GPUI-compatible BGRA byte order.
    ///
    /// The return type is still `image::RgbaImage` because GPUI's `RenderImage`
    /// construction accepts that container, but callers should treat the raw bytes
    /// as BGRA rather than semantic RGBA.
    pub fn render(&mut self, scene: &SceneRenderData, size: RenderSize) -> Result<RgbaImage> {
        if size.is_empty() {
            return Ok(RgbaImage::new(size.width, size.height));
        }

        self.backend.render(scene, size)
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new(RenderOptions::default())
    }
}

fn hash_background(hasher: &mut impl Hasher, background: BackgroundSnapshot) {
    hash_float4(hasher, background.color);
}

fn hash_camera(hasher: &mut impl Hasher, camera: &CameraSnapshot) {
    hash_float3(hasher, camera.position);
    hash_float3(hasher, camera.look_at);
    hash_float3(hasher, camera.up);
    camera.near.to_bits().hash(hasher);
    camera.far.to_bits().hash(hasher);
}

fn mesh_fingerprint_into(hasher: &mut impl Hasher, mesh: &Mesh) {
    mesh.uniform.alpha.to_bits().hash(hasher);
    mesh.uniform
        .stroke_miter_radius_scale
        .to_bits()
        .hash(hasher);
    mesh.uniform.stroke_radius.to_bits().hash(hasher);
    mesh.uniform.dot_radius.to_bits().hash(hasher);
    mesh.uniform.dot_vertex_count.hash(hasher);
    mesh.uniform.smooth.hash(hasher);
    mesh.uniform.gloss.to_bits().hash(hasher);
    mesh.uniform.z_index.hash(hasher);
    mesh.uniform.img.hash(hasher);

    mesh.dots.len().hash(hasher);
    for dot in &mesh.dots {
        hash_dot(hasher, dot);
    }

    mesh.lins.len().hash(hasher);
    for line in &mesh.lins {
        hash_line(hasher, line);
    }

    mesh.tris.len().hash(hasher);
    for tri in &mesh.tris {
        hash_tri(hasher, tri);
    }
}

fn hash_dot(hasher: &mut impl Hasher, dot: &Dot) {
    hash_float3(hasher, dot.pos);
    hash_float3(hasher, dot.norm);
    hash_float4(hasher, float4_tuple(dot.col));
    dot.inv.hash(hasher);
    dot.anti.hash(hasher);
    dot.is_dom_sib.hash(hasher);
}

fn hash_line(hasher: &mut impl Hasher, line: &Lin) {
    hash_float3(hasher, line.a.pos);
    hash_float4(hasher, float4_tuple(line.a.col));
    hash_float3(hasher, line.b.pos);
    hash_float4(hasher, float4_tuple(line.b.col));
    hash_float3(hasher, line.norm);
    line.prev.hash(hasher);
    line.next.hash(hasher);
    line.inv.hash(hasher);
    line.anti.hash(hasher);
    line.is_dom_sib.hash(hasher);
}

fn hash_tri(hasher: &mut impl Hasher, tri: &Tri) {
    hash_float3(hasher, tri.a.pos);
    hash_float4(hasher, float4_tuple(tri.a.col));
    hash_float2(hasher, tri.a.uv);
    hash_float3(hasher, tri.b.pos);
    hash_float4(hasher, float4_tuple(tri.b.col));
    hash_float2(hasher, tri.b.uv);
    hash_float3(hasher, tri.c.pos);
    hash_float4(hasher, float4_tuple(tri.c.col));
    hash_float2(hasher, tri.c.uv);
    tri.ab.hash(hasher);
    tri.bc.hash(hasher);
    tri.ca.hash(hasher);
    tri.anti.hash(hasher);
    tri.is_dom_sib.hash(hasher);
}

fn hash_float2(hasher: &mut impl Hasher, value: Float2) {
    value.x.to_bits().hash(hasher);
    value.y.to_bits().hash(hasher);
}

fn hash_float3(hasher: &mut impl Hasher, value: Float3) {
    value.x.to_bits().hash(hasher);
    value.y.to_bits().hash(hasher);
    value.z.to_bits().hash(hasher);
}

fn hash_float4(hasher: &mut impl Hasher, value: (f32, f32, f32, f32)) {
    value.0.to_bits().hash(hasher);
    value.1.to_bits().hash(hasher);
    value.2.to_bits().hash(hasher);
    value.3.to_bits().hash(hasher);
}

fn float4_tuple(value: Float4) -> (f32, f32, f32, f32) {
    (value.x, value.y, value.z, value.w)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use executor::scene_snapshot::{BackgroundSnapshot, CameraSnapshot};
    use geo::{
        mesh::{Mesh, Tri, TriVertex, Uniforms},
        simd::{Float2, Float3, Float4},
    };

    use crate::{
        BackendKind, RenderOptions, RenderSize, Renderer, SceneRenderData, scene_fingerprint,
    };

    #[test]
    fn renders_background_when_scene_is_empty() {
        let mut renderer = Renderer::new(RenderOptions {
            backend: BackendKind::Software,
            ..RenderOptions::default()
        });
        let scene = SceneRenderData {
            background: BackgroundSnapshot {
                color: (0.25, 0.5, 0.75, 1.0),
            },
            camera: CameraSnapshot::default(),
            meshes: Vec::new(),
        };

        let image = renderer.render(&scene, RenderSize::new(2, 2)).unwrap();

        assert_eq!(
            image.as_raw(),
            &[
                191, 128, 64, 255, 191, 128, 64, 255, 191, 128, 64, 255, 191, 128, 64, 255
            ]
        );
    }

    #[test]
    fn higher_z_index_wins_for_overlapping_geometry() {
        let mut renderer = Renderer::new(RenderOptions {
            backend: BackendKind::Software,
            ..RenderOptions::default()
        });
        let scene = SceneRenderData {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: vec![
                Arc::new(flat_triangle_mesh(Float4::new(1.0, 0.0, 0.0, 1.0), 0)),
                Arc::new(flat_triangle_mesh(Float4::new(0.0, 1.0, 0.0, 1.0), 1)),
            ],
        };

        let image = renderer.render(&scene, RenderSize::new(32, 32)).unwrap();
        let center = image.get_pixel(16, 16).0;

        assert!(
            center[1] > center[2],
            "expected higher z-index triangle to be visible at the center, got {center:?}"
        );
    }

    #[test]
    fn scene_fingerprint_changes_when_mesh_geometry_changes() {
        let mut scene = SceneRenderData {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: vec![Arc::new(flat_triangle_mesh(
                Float4::new(1.0, 0.0, 0.0, 1.0),
                0,
            ))],
        };
        let before = scene_fingerprint(&scene);

        let mesh = Arc::make_mut(&mut scene.meshes[0]);
        mesh.tris[0].a.pos.x += 1.0;

        let after = scene_fingerprint(&scene);
        assert_ne!(before, after);
    }

    fn flat_triangle_mesh(color: Float4, z_index: i32) -> Mesh {
        Mesh {
            dots: Vec::new(),
            lins: Vec::new(),
            tris: vec![Tri {
                a: TriVertex {
                    pos: Float3::new(-1.0, -1.0, 0.0),
                    col: color,
                    uv: Float2::new(0.0, 0.0),
                },
                b: TriVertex {
                    pos: Float3::new(1.0, -1.0, 0.0),
                    col: color,
                    uv: Float2::new(1.0, 0.0),
                },
                c: TriVertex {
                    pos: Float3::new(0.0, 1.0, 0.0),
                    col: color,
                    uv: Float2::new(0.5, 1.0),
                },
                ab: -1,
                bc: -1,
                ca: -1,
                anti: -1,
                is_dom_sib: false,
            }],
            uniform: Uniforms {
                alpha: 1.0,
                stroke_miter_radius_scale: geo::mesh::DEFAULT_STROKE_MITER_RADIUS_SCALE,
                stroke_radius: geo::mesh::DEFAULT_STROKE_RADIUS,
                dot_radius: geo::mesh::DEFAULT_DOT_RADIUS,
                dot_vertex_count: geo::mesh::DEFAULT_DOT_VERTEX_COUNT,
                smooth: geo::mesh::DEFAULT_SMOOTH,
                gloss: geo::mesh::DEFAULT_GLOSS,
                img: None,
                z_index,
            },
            tag: Vec::new(),
        }
    }
}
