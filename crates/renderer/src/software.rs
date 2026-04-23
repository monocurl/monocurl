use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use executor::camera::CameraBasis;
use geo::{
    mesh::{DEFAULT_GLOSS, Dot, Lin, Mesh, Tri},
    simd::{Float2, Float3, Float4},
};
use image::{Rgba, RgbaImage};

use crate::{RenderSize, RenderStyle, SceneRenderData};

const TRI_LAYER: i32 = 0;
const LINE_LAYER: i32 = 1;
const DOT_LAYER: i32 = 2;
const MAX_ORDER_LAYERS: i32 = 8;
const REFERENCE_WIDTH: f32 = 1480.0;
const LIGHT_SRC: Float3 = Float3 {
    x: 1.0,
    y: 1.0,
    z: 0.0,
};
const GAMMA: f32 = 3.0;

pub(crate) struct SoftwareRenderer {
    style: RenderStyle,
    texture_cache: HashMap<PathBuf, Option<Arc<RgbaImage>>>,
}

impl SoftwareRenderer {
    pub(crate) fn new(style: RenderStyle) -> Self {
        Self {
            style,
            texture_cache: HashMap::new(),
        }
    }

    pub(crate) fn render(
        &mut self,
        scene: &SceneRenderData,
        size: RenderSize,
    ) -> Result<RgbaImage> {
        let mut surface = Surface::new(size, scene.background.color);
        let basis = scene.camera.basis();

        for mesh in &scene.meshes {
            self.draw_mesh(&mut surface, mesh, basis, size);
        }

        Ok(surface.into_image())
    }

    fn draw_mesh(
        &mut self,
        surface: &mut Surface,
        mesh: &Arc<Mesh>,
        basis: CameraBasis,
        size: RenderSize,
    ) {
        if mesh.uniform.alpha <= 0.0 {
            return;
        }

        let alpha = mesh.uniform.alpha as f32;
        let order_base = mesh.uniform.z_index.saturating_mul(MAX_ORDER_LAYERS);
        let line_width = mesh_line_width_px(mesh, size, self.style);
        let dot_radius = mesh_dot_radius_px(mesh, self.style);
        let dot_vertex_count = mesh.uniform.dot_vertex_count.max(3);
        let texture = mesh
            .uniform
            .img
            .as_deref()
            .and_then(|path| self.texture(path));

        for tri in &mesh.tris {
            draw_triangle(
                surface,
                tri,
                texture.as_deref(),
                alpha,
                mesh.uniform.gloss,
                order_base + TRI_LAYER,
                basis,
                size,
            );
        }
        if line_width > f32::EPSILON {
            for lin in &mesh.lins {
                draw_line(
                    surface,
                    lin,
                    line_width,
                    alpha,
                    order_base + LINE_LAYER,
                    basis,
                    size,
                );
            }
        }
        if dot_radius > f32::EPSILON {
            for dot in &mesh.dots {
                draw_dot(
                    surface,
                    dot,
                    dot_radius,
                    dot_vertex_count,
                    alpha,
                    order_base + DOT_LAYER,
                    basis,
                    size,
                );
            }
        }
    }

    fn texture(&mut self, path: &Path) -> Option<Arc<RgbaImage>> {
        if let Some(image) = self.texture_cache.get(path) {
            return image.clone();
        }

        let image = image::open(path)
            .ok()
            .map(|image| Arc::new(image.into_rgba8()));
        self.texture_cache.insert(path.to_path_buf(), image.clone());
        image
    }
}

#[derive(Clone, Copy)]
struct ProjectedPoint {
    screen: Float2,
    depth: f32,
    model: Float3,
}

struct Surface {
    size: RenderSize,
    rgba: Vec<u8>,
    order: Vec<i32>,
    depth: Vec<f32>,
}

impl Surface {
    fn new(size: RenderSize, background: (f32, f32, f32, f32)) -> Self {
        let pixel_count = size.width as usize * size.height as usize;
        let background = rgba8(Float4::new(
            background.0,
            background.1,
            background.2,
            background.3,
        ));
        let mut rgba = vec![0; pixel_count * 4];
        for chunk in rgba.chunks_exact_mut(4) {
            chunk.copy_from_slice(&background.0);
        }

        Self {
            size,
            rgba,
            order: vec![i32::MIN; pixel_count],
            depth: vec![f32::INFINITY; pixel_count],
        }
    }

    fn blend_pixel(&mut self, x: u32, y: u32, order: i32, depth: f32, color: Rgba<u8>) {
        let index = y as usize * self.size.width as usize + x as usize;
        let current_order = self.order[index];
        let current_depth = self.depth[index];
        if order < current_order || (order == current_order && depth > current_depth) {
            return;
        }

        let offset = index * 4;
        let src = color.0;
        let src_alpha = src[3] as f32 / 255.0;
        if src_alpha <= f32::EPSILON {
            return;
        }

        let dst = &mut self.rgba[offset..offset + 4];
        let inv_alpha = 1.0 - src_alpha;

        for channel in 0..3 {
            let value = src[channel] as f32 * src_alpha + dst[channel] as f32 * inv_alpha;
            dst[channel] = value.round().clamp(0.0, 255.0) as u8;
        }
        let alpha = src[3] as f32 + dst[3] as f32 * inv_alpha;
        dst[3] = alpha.round().clamp(0.0, 255.0) as u8;

        self.order[index] = order;
        self.depth[index] = depth;
    }

    fn into_image(self) -> RgbaImage {
        RgbaImage::from_raw(self.size.width, self.size.height, self.rgba)
            .expect("image dimensions should match buffer")
    }
}

fn draw_triangle(
    surface: &mut Surface,
    tri: &Tri,
    texture: Option<&RgbaImage>,
    alpha: f32,
    gloss: f32,
    order: i32,
    basis: CameraBasis,
    size: RenderSize,
) {
    let Some(a) = project_point(tri.a.pos, basis, size) else {
        return;
    };
    let Some(b) = project_point(tri.b.pos, basis, size) else {
        return;
    };
    let Some(c) = project_point(tri.c.pos, basis, size) else {
        return;
    };

    let area = edge(a.screen, b.screen, c.screen);
    if area.abs() <= 1e-6 {
        return;
    }

    let min_x = a.screen.x.min(b.screen.x).min(c.screen.x).floor().max(0.0) as i32;
    let max_x = a
        .screen
        .x
        .max(b.screen.x)
        .max(c.screen.x)
        .ceil()
        .min(size.width.saturating_sub(1) as f32) as i32;
    let min_y = a.screen.y.min(b.screen.y).min(c.screen.y).floor().max(0.0) as i32;
    let max_y = a
        .screen
        .y
        .max(b.screen.y)
        .max(c.screen.y)
        .ceil()
        .min(size.height.saturating_sub(1) as f32) as i32;
    if min_x > max_x || min_y > max_y {
        return;
    }

    let inverse_area = 1.0 / area;
    let face_normal = triangle_face_normal(a.model, b.model, c.model);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let pixel = Float2::new(x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge(b.screen, c.screen, pixel);
            let w1 = edge(c.screen, a.screen, pixel);
            let w2 = edge(a.screen, b.screen, pixel);
            if !same_sign(area, w0) || !same_sign(area, w1) || !same_sign(area, w2) {
                continue;
            }

            let barycentric = [w0 * inverse_area, w1 * inverse_area, w2 * inverse_area];
            let depth =
                barycentric[0] * a.depth + barycentric[1] * b.depth + barycentric[2] * c.depth;
            let model =
                a.model * barycentric[0] + b.model * barycentric[1] + c.model * barycentric[2];
            let color = triangle_color(tri, barycentric, texture, alpha, gloss, face_normal, model);
            surface.blend_pixel(x as u32, y as u32, order, depth, color);
        }
    }
}

fn draw_line(
    surface: &mut Surface,
    line: &Lin,
    line_width: f32,
    alpha: f32,
    order: i32,
    basis: CameraBasis,
    size: RenderSize,
) {
    let Some(a) = project_point(line.a.pos, basis, size) else {
        return;
    };
    let Some(b) = project_point(line.b.pos, basis, size) else {
        return;
    };

    let half_width = line_width * 0.5;
    let min_x = a.screen.x.min(b.screen.x).floor() - half_width;
    let max_x = a.screen.x.max(b.screen.x).ceil() + half_width;
    let min_y = a.screen.y.min(b.screen.y).floor() - half_width;
    let max_y = a.screen.y.max(b.screen.y).ceil() + half_width;
    let min_x = min_x.max(0.0) as i32;
    let max_x = max_x.min(size.width.saturating_sub(1) as f32) as i32;
    let min_y = min_y.max(0.0) as i32;
    let max_y = max_y.min(size.height.saturating_sub(1) as f32) as i32;
    if min_x > max_x || min_y > max_y {
        return;
    }

    let delta = b.screen - a.screen;
    let length_sq = delta.len_sq().max(1e-6);
    let radius_sq = half_width * half_width;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let pixel = Float2::new(x as f32 + 0.5, y as f32 + 0.5);
            let t = ((pixel - a.screen).dot(delta) / length_sq).clamp(0.0, 1.0);
            let closest = a.screen + delta * t;
            if (pixel - closest).len_sq() > radius_sq {
                continue;
            }

            let color = rgba8(lerp4(line.a.col, line.b.col, t) * Float4::new(1.0, 1.0, 1.0, alpha));
            let depth = a.depth + (b.depth - a.depth) * t;
            surface.blend_pixel(x as u32, y as u32, order, depth, color);
        }
    }
}

fn draw_dot(
    surface: &mut Surface,
    dot: &Dot,
    radius: f32,
    vertex_count: u16,
    alpha: f32,
    order: i32,
    basis: CameraBasis,
    size: RenderSize,
) {
    let Some(projected) = project_point(dot.pos, basis, size) else {
        return;
    };

    let color = rgba8(dot.col * Float4::new(1.0, 1.0, 1.0, alpha));
    let vertices = regular_polygon(projected, radius, vertex_count);
    for i in 1..vertices.len().saturating_sub(1) {
        draw_flat_triangle(
            surface,
            vertices[0],
            vertices[i],
            vertices[i + 1],
            color,
            order,
            size,
        );
    }
}

fn triangle_color(
    tri: &Tri,
    barycentric: [f32; 3],
    texture: Option<&RgbaImage>,
    alpha: f32,
    gloss: f32,
    normal: Float3,
    model: Float3,
) -> Rgba<u8> {
    let vertex_color =
        tri.a.col * barycentric[0] + tri.b.col * barycentric[1] + tri.c.col * barycentric[2];
    let specular = lighting_specular(normal, model, gloss);
    let lit_color = Float4::new(
        vertex_color.x + (1.0 - vertex_color.x) * specular,
        vertex_color.y + (1.0 - vertex_color.y) * specular,
        vertex_color.z + (1.0 - vertex_color.z) * specular,
        vertex_color.w,
    );
    let mut color = lit_color;
    if let Some(texture) = texture {
        let uv = tri.a.uv * barycentric[0] + tri.b.uv * barycentric[1] + tri.c.uv * barycentric[2];
        color = color * sample_texture(texture, uv);
    }
    color.w *= alpha;
    rgba8(color)
}

fn sample_texture(texture: &RgbaImage, uv: Float2) -> Float4 {
    if texture.width() == 0 || texture.height() == 0 {
        return Float4::ONE;
    }

    let u = uv.x.clamp(0.0, 1.0);
    let v = uv.y.clamp(0.0, 1.0);
    let x = u * (texture.width().saturating_sub(1) as f32);
    let y = v * (texture.height().saturating_sub(1) as f32);

    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(texture.width().saturating_sub(1));
    let y1 = (y0 + 1).min(texture.height().saturating_sub(1));
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let c00 = float4_from_rgba(*texture.get_pixel(x0, y0));
    let c10 = float4_from_rgba(*texture.get_pixel(x1, y0));
    let c01 = float4_from_rgba(*texture.get_pixel(x0, y1));
    let c11 = float4_from_rgba(*texture.get_pixel(x1, y1));

    let top = c00 * (1.0 - tx) + c10 * tx;
    let bottom = c01 * (1.0 - tx) + c11 * tx;
    top * (1.0 - ty) + bottom * ty
}

fn project_point(world: Float3, basis: CameraBasis, size: RenderSize) -> Option<ProjectedPoint> {
    let relative = world - basis.position;
    let camera_x = relative.dot(basis.right);
    let camera_y = relative.dot(basis.up);
    let camera_z = relative.dot(basis.forward);

    if camera_z < basis.near || camera_z > basis.far {
        return None;
    }

    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let aspect = width / height.max(1.0);
    let tan_half_fov = (basis.fov * 0.5).tan().max(0.05);
    let ndc_x = camera_x / (camera_z * tan_half_fov * aspect.max(0.1));
    let ndc_y = camera_y / (camera_z * tan_half_fov);
    if !ndc_x.is_finite() || !ndc_y.is_finite() {
        return None;
    }

    Some(ProjectedPoint {
        screen: Float2::new(width * (0.5 + 0.5 * ndc_x), height * (0.5 - 0.5 * ndc_y)),
        depth: camera_z,
        model: Float3::new(camera_x, camera_y, camera_z),
    })
}

fn draw_flat_triangle(
    surface: &mut Surface,
    a: ProjectedPoint,
    b: ProjectedPoint,
    c: ProjectedPoint,
    color: Rgba<u8>,
    order: i32,
    size: RenderSize,
) {
    let area = edge(a.screen, b.screen, c.screen);
    if area.abs() <= 1e-6 {
        return;
    }

    let min_x = a.screen.x.min(b.screen.x).min(c.screen.x).floor().max(0.0) as i32;
    let max_x = a
        .screen
        .x
        .max(b.screen.x)
        .max(c.screen.x)
        .ceil()
        .min(size.width.saturating_sub(1) as f32) as i32;
    let min_y = a.screen.y.min(b.screen.y).min(c.screen.y).floor().max(0.0) as i32;
    let max_y = a
        .screen
        .y
        .max(b.screen.y)
        .max(c.screen.y)
        .ceil()
        .min(size.height.saturating_sub(1) as f32) as i32;
    if min_x > max_x || min_y > max_y {
        return;
    }

    let inverse_area = 1.0 / area;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let pixel = Float2::new(x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge(b.screen, c.screen, pixel);
            let w1 = edge(c.screen, a.screen, pixel);
            let w2 = edge(a.screen, b.screen, pixel);
            if !same_sign(area, w0) || !same_sign(area, w1) || !same_sign(area, w2) {
                continue;
            }

            let barycentric = [w0 * inverse_area, w1 * inverse_area, w2 * inverse_area];
            let depth =
                barycentric[0] * a.depth + barycentric[1] * b.depth + barycentric[2] * c.depth;
            surface.blend_pixel(x as u32, y as u32, order, depth, color);
        }
    }
}

fn regular_polygon(center: ProjectedPoint, radius: f32, vertex_count: u16) -> Vec<ProjectedPoint> {
    (0..vertex_count.max(3))
        .map(|i| {
            let theta = std::f32::consts::TAU * i as f32 / vertex_count.max(3) as f32;
            ProjectedPoint {
                screen: center.screen + Float2::new(radius * theta.cos(), radius * theta.sin()),
                depth: center.depth,
                model: center.model,
            }
        })
        .collect()
}

fn triangle_face_normal(a: Float3, b: Float3, c: Float3) -> Float3 {
    let normal = (b - a).cross(c - a);
    if normal.len_sq() <= 1e-12 {
        Float3::Z
    } else {
        normal.normalize()
    }
}

fn lighting_specular(normal: Float3, model: Float3, gloss: f32) -> f32 {
    if gloss <= DEFAULT_GLOSS {
        return 0.0;
    }
    let normal = if normal.len_sq() <= 1e-12 {
        Float3::Z
    } else {
        normal.normalize()
    };
    let light = LIGHT_SRC - model;
    if light.len_sq() <= 1e-12 {
        return 0.0;
    }
    gloss * normal.dot(light.normalize()).max(0.0).powf(GAMMA)
}

fn mesh_line_width_px(mesh: &Mesh, size: RenderSize, style: RenderStyle) -> f32 {
    2.0 * mesh_line_radius_px(mesh, size, style)
}

fn mesh_line_radius_px(mesh: &Mesh, size: RenderSize, style: RenderStyle) -> f32 {
    let radius = if mesh.uniform.stroke_radius.is_finite() {
        mesh.uniform.stroke_radius.max(0.0)
    } else {
        (style.line_width_px.max(0.0) * 0.5).max(0.0)
    };
    radius * size.width.max(1) as f32 / REFERENCE_WIDTH
}

fn mesh_dot_radius_px(mesh: &Mesh, style: RenderStyle) -> f32 {
    if mesh.uniform.dot_radius.is_finite() {
        mesh.uniform.dot_radius.max(0.0)
    } else {
        style.dot_radius_px.max(0.0)
    }
}

fn rgba8(color: Float4) -> Rgba<u8> {
    Rgba([
        channel_to_u8(color.z),
        channel_to_u8(color.y),
        channel_to_u8(color.x),
        channel_to_u8(color.w),
    ])
}

fn channel_to_u8(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn float4_from_rgba(color: Rgba<u8>) -> Float4 {
    Float4::new(
        color.0[0] as f32 / 255.0,
        color.0[1] as f32 / 255.0,
        color.0[2] as f32 / 255.0,
        color.0[3] as f32 / 255.0,
    )
}

fn edge(a: Float2, b: Float2, c: Float2) -> f32 {
    let ab = b - a;
    let ac = c - a;
    ab.x * ac.y - ab.y * ac.x
}

fn same_sign(area: f32, edge_value: f32) -> bool {
    if area >= 0.0 {
        edge_value >= 0.0
    } else {
        edge_value <= 0.0
    }
}

fn lerp4(a: Float4, b: Float4, t: f32) -> Float4 {
    a + (b - a) * t
}
