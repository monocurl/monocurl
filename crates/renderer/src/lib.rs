mod blade;

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use anyhow::{Result, anyhow};
use executor::scene_snapshot::{BackgroundSnapshot, CameraSnapshot, SceneSnapshot};
use geo::{
    mesh::{Dot, Lin, Mesh, Tri},
    simd::{Float2, Float3, Float4},
};
pub use image::RgbaImage;

use crate::blade::BladeRenderer;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderView {
    pub output_size: RenderSize,
    pub projection_size: RenderSize,
}

impl RenderView {
    pub const fn new(output_size: RenderSize, projection_size: RenderSize) -> Self {
        Self {
            output_size,
            projection_size,
        }
    }

    pub const fn full(size: RenderSize) -> Self {
        Self::new(size, size)
    }

    pub const fn is_empty(self) -> bool {
        self.output_size.is_empty() || self.projection_size.is_empty()
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderOptions {
    pub style: RenderStyle,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            style: RenderStyle::default(),
        }
    }
}

pub struct Renderer {
    blade: Result<BladeRenderer, String>,
}

impl Renderer {
    pub fn new(options: RenderOptions) -> Self {
        let blade = BladeRenderer::new(options.style).map_err(|error| {
            log::error!("blade renderer initialization failed: {error:#}");
            format!("{error:#}")
        });
        Self { blade }
    }

    pub fn try_new(options: RenderOptions) -> Result<Self> {
        Ok(Self {
            blade: Ok(BladeRenderer::new(options.style)?),
        })
    }

    /// Returns a frame encoded in GPUI-compatible BGRA byte order.
    ///
    /// The return type is still `image::RgbaImage` because GPUI's `RenderImage`
    /// construction accepts that container, but callers should treat the raw bytes
    /// as BGRA rather than semantic RGBA.
    pub fn render(&mut self, scene: &SceneRenderData, size: RenderSize) -> Result<RgbaImage> {
        self.render_view(scene, RenderView::full(size))
    }

    pub fn render_view(&mut self, scene: &SceneRenderData, view: RenderView) -> Result<RgbaImage> {
        if view.is_empty() {
            return Ok(RgbaImage::new(
                view.output_size.width,
                view.output_size.height,
            ));
        }
        self.blade
            .as_mut()
            .map_err(|error| anyhow!("blade renderer unavailable: {error}"))?
            .render(scene, view)
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
        mesh::{Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
        simd::{Float2, Float3, Float4},
    };

    use crate::{RenderOptions, RenderSize, Renderer, SceneRenderData, scene_fingerprint};

    #[test]
    fn renders_background_when_scene_is_empty() {
        let Ok(mut renderer) = Renderer::try_new(RenderOptions::default()) else {
            return;
        };
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
        let Ok(mut renderer) = Renderer::try_new(RenderOptions::default()) else {
            return;
        };
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

    #[test]
    fn renders_standalone_line_pixels() {
        let Ok(mut renderer) = Renderer::try_new(RenderOptions::default()) else {
            return;
        };
        let scene = SceneRenderData {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: vec![Arc::new(line_mesh(Float4::new(1.0, 0.0, 0.0, 1.0)))],
        };

        let image = renderer.render(&scene, RenderSize::new(128, 128)).unwrap();
        let red_pixels = image
            .pixels()
            .filter(|pixel| {
                let [b, g, r, a] = pixel.0;
                a > 0 && r > 48 && r > b.saturating_add(24) && r > g.saturating_add(24)
            })
            .count();

        assert!(
            red_pixels > 0,
            "expected standalone line render to produce visible red pixels"
        );
    }

    #[test]
    fn renders_stroked_triangle_pixels() {
        let Ok(mut renderer) = Renderer::try_new(RenderOptions::default()) else {
            return;
        };
        let scene = SceneRenderData {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: vec![Arc::new(stroked_triangle_mesh(
                Float4::new(0.0, 0.0, 1.0, 1.0),
                Float4::new(1.0, 0.0, 0.0, 1.0),
            ))],
        };

        let image = renderer.render(&scene, RenderSize::new(128, 128)).unwrap();
        let red_pixels = image
            .pixels()
            .filter(|pixel| {
                let [b, g, r, a] = pixel.0;
                a > 0 && r > 48 && r > b.saturating_add(24) && r > g.saturating_add(24)
            })
            .count();
        let blue_pixels = image
            .pixels()
            .filter(|pixel| {
                let [b, g, r, a] = pixel.0;
                a > 0 && b > 48 && b > r.saturating_add(24) && b > g.saturating_add(24)
            })
            .count();

        assert!(
            blue_pixels > 0,
            "expected stroked triangle render to produce visible blue fill pixels"
        );
        assert!(
            red_pixels > 0,
            "expected stroked triangle render to produce visible red stroke pixels"
        );
    }

    #[test]
    fn renders_clockwise_triangle_pixels() {
        let Ok(mut renderer) = Renderer::try_new(RenderOptions::default()) else {
            return;
        };
        let mut triangle = flat_triangle_mesh(Float4::new(0.0, 1.0, 0.0, 1.0), 0);
        let tri = &mut triangle.tris[0];
        std::mem::swap(&mut tri.b, &mut tri.c);

        let scene = SceneRenderData {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: vec![Arc::new(triangle)],
        };

        let image = renderer.render(&scene, RenderSize::new(128, 128)).unwrap();
        let green_pixels = image
            .pixels()
            .filter(|pixel| {
                let [b, g, r, a] = pixel.0;
                a > 0 && g > 48 && g > r.saturating_add(24) && g > b.saturating_add(24)
            })
            .count();

        assert!(
            green_pixels > 0,
            "expected clockwise triangle winding to remain visible"
        );
    }

    #[test]
    fn linked_polyline_adds_corner_pixels_over_disconnected_segments() {
        let Ok(mut renderer) = Renderer::try_new(RenderOptions::default()) else {
            return;
        };
        let joined = SceneRenderData {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: vec![Arc::new(elbow_line_mesh(
                true,
                Float4::new(1.0, 0.0, 0.0, 1.0),
            ))],
        };
        let disconnected = SceneRenderData {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: vec![Arc::new(elbow_line_mesh(
                false,
                Float4::new(1.0, 0.0, 0.0, 1.0),
            ))],
        };

        let joined_image = renderer.render(&joined, RenderSize::new(128, 128)).unwrap();
        let disconnected_image = renderer
            .render(&disconnected, RenderSize::new(128, 128))
            .unwrap();

        let joined_red = count_red_pixels(&joined_image);
        let disconnected_red = count_red_pixels(&disconnected_image);

        assert!(
            joined_red > disconnected_red,
            "expected linked polyline join to add visible corner pixels, got joined={joined_red} disconnected={disconnected_red}"
        );
    }

    #[test]
    fn linked_polyline_populates_outer_corner_miter_region() {
        let Ok(mut renderer) = Renderer::try_new(RenderOptions::default()) else {
            return;
        };
        let size = RenderSize::new(1024, 1024);
        let scene = SceneRenderData {
            background: BackgroundSnapshot::default(),
            camera: CameraSnapshot::default(),
            meshes: vec![Arc::new(elbow_line_mesh(
                true,
                Float4::new(1.0, 0.0, 0.0, 1.0),
            ))],
        };

        let image = renderer.render(&scene, size).unwrap();
        let joint = screen_point(Float3::new(-0.35, -0.35, 0.0), size);
        let outer_corner_red = image
            .enumerate_pixels()
            .filter(|(x, y, pixel)| {
                let dx = *x as i32 - joint.0;
                let dy = *y as i32 - joint.1;
                let [b, g, r, a] = pixel.0;
                dx <= -8
                    && dx >= -14
                    && dy >= 8
                    && dy <= 14
                    && a > 0
                    && r > 48
                    && r > b.saturating_add(24)
                    && r > g.saturating_add(24)
            })
            .count();

        assert!(
            outer_corner_red > 0,
            "expected linked polyline to populate the miter-only outer corner region"
        );
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

    fn count_red_pixels(image: &image::RgbaImage) -> usize {
        image
            .pixels()
            .filter(|pixel| {
                let [b, g, r, a] = pixel.0;
                a > 0 && r > 48 && r > b.saturating_add(24) && r > g.saturating_add(24)
            })
            .count()
    }

    fn screen_point(world: Float3, size: RenderSize) -> (i32, i32) {
        let camera = CameraSnapshot::default();
        let basis = camera.basis();
        let relative = world - basis.position;
        let model_x = relative.dot(basis.right);
        let model_y = relative.dot(basis.up);
        let model_z = relative.dot(basis.forward);
        let tan_half_fov = (basis.fov * 0.5).tan().max(0.05);
        let aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        let ndc_x = model_x / (model_z * tan_half_fov * aspect);
        let ndc_y = model_y / (model_z * tan_half_fov);
        let x = ((ndc_x + 1.0) * 0.5 * size.width as f32).round() as i32;
        let y = ((1.0 - (ndc_y + 1.0) * 0.5) * size.height as f32).round() as i32;
        (x, y)
    }

    fn line_mesh(color: Float4) -> Mesh {
        Mesh {
            dots: Vec::new(),
            lins: vec![Lin {
                a: LinVertex {
                    pos: Float3::new(-0.5, 0.0, 0.0),
                    col: color,
                },
                b: LinVertex {
                    pos: Float3::new(0.5, 0.0, 0.0),
                    col: color,
                },
                norm: Float3::Z,
                prev: -1,
                next: -1,
                inv: -1,
                is_dom_sib: false,
            }],
            tris: Vec::new(),
            uniform: Uniforms::default(),
            tag: Vec::new(),
        }
    }

    fn elbow_line_mesh(linked: bool, color: Float4) -> Mesh {
        let (first_next, second_prev) = if linked { (1, 0) } else { (-1, -1) };
        Mesh {
            dots: Vec::new(),
            lins: vec![
                Lin {
                    a: LinVertex {
                        pos: Float3::new(-0.35, 0.35, 0.0),
                        col: color,
                    },
                    b: LinVertex {
                        pos: Float3::new(-0.35, -0.35, 0.0),
                        col: color,
                    },
                    norm: Float3::Z,
                    prev: -1,
                    next: first_next,
                    inv: -1,
                    is_dom_sib: false,
                },
                Lin {
                    a: LinVertex {
                        pos: Float3::new(-0.35, -0.35, 0.0),
                        col: color,
                    },
                    b: LinVertex {
                        pos: Float3::new(0.35, -0.35, 0.0),
                        col: color,
                    },
                    norm: Float3::Z,
                    prev: second_prev,
                    next: -1,
                    inv: -1,
                    is_dom_sib: false,
                },
            ],
            tris: Vec::new(),
            uniform: Uniforms::default(),
            tag: Vec::new(),
        }
    }

    fn stroked_triangle_mesh(fill: Float4, stroke: Float4) -> Mesh {
        Mesh {
            dots: Vec::new(),
            lins: vec![
                Lin {
                    a: LinVertex {
                        pos: Float3::new(-0.5, -0.5, 0.0),
                        col: stroke,
                    },
                    b: LinVertex {
                        pos: Float3::new(0.5, -0.5, 0.0),
                        col: stroke,
                    },
                    norm: Float3::Z,
                    prev: 2,
                    next: 1,
                    inv: -1,
                    is_dom_sib: false,
                },
                Lin {
                    a: LinVertex {
                        pos: Float3::new(0.5, -0.5, 0.0),
                        col: stroke,
                    },
                    b: LinVertex {
                        pos: Float3::new(-0.5, 0.5, 0.0),
                        col: stroke,
                    },
                    norm: Float3::Z,
                    prev: 0,
                    next: 2,
                    inv: -1,
                    is_dom_sib: false,
                },
                Lin {
                    a: LinVertex {
                        pos: Float3::new(-0.5, 0.5, 0.0),
                        col: stroke,
                    },
                    b: LinVertex {
                        pos: Float3::new(-0.5, -0.5, 0.0),
                        col: stroke,
                    },
                    norm: Float3::Z,
                    prev: 1,
                    next: 0,
                    inv: -1,
                    is_dom_sib: false,
                },
            ],
            tris: vec![Tri {
                a: TriVertex {
                    pos: Float3::new(-0.5, -0.5, 0.0),
                    col: fill,
                    uv: Float2::new(0.0, 0.0),
                },
                b: TriVertex {
                    pos: Float3::new(0.5, -0.5, 0.0),
                    col: fill,
                    uv: Float2::new(1.0, 0.0),
                },
                c: TriVertex {
                    pos: Float3::new(-0.5, 0.5, 0.0),
                    col: fill,
                    uv: Float2::new(0.0, 1.0),
                },
                ab: -2,
                bc: -3,
                ca: -4,
                is_dom_sib: false,
            }],
            uniform: Uniforms::default(),
            tag: Vec::new(),
        }
    }
}
