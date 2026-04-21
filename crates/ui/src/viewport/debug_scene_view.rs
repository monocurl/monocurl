use std::{cmp::Ordering, sync::Arc};

use executor::{camera::CameraBasis, scene_snapshot::CameraSnapshot};
use geo::{
    mesh::{Dot, Lin, Mesh, Tri},
    simd::{Float3, Float4},
};
use gpui::*;

use crate::{state::execution_state::ExecutionState, theme::ThemeSettings};

const DOT_RADIUS: f32 = 3.5;
const EDGE_WIDTH: f32 = 1.0;
const TRANSPARENT: Rgba = Rgba {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

pub struct DebugSceneView {
    execution_state: Entity<ExecutionState>,
}

#[derive(Clone)]
struct SceneRenderData {
    background_color: Rgba,
    camera: CameraSnapshot,
    meshes: Vec<Arc<Mesh>>,
}

#[derive(Clone, Copy)]
struct ProjectedPoint {
    point: Point<Pixels>,
    depth: f32,
}

enum DrawPrimitive {
    Triangle {
        points: [Point<Pixels>; 3],
        color: Rgba,
    },
    Line {
        points: [Point<Pixels>; 2],
        color: Rgba,
        width: Pixels,
    },
    Dot {
        center: Point<Pixels>,
        color: Rgba,
    },
}

struct DrawItem {
    z_index: i32,
    depth: f32,
    primitive: DrawPrimitive,
}

impl DebugSceneView {
    pub fn new(execution_state: Entity<ExecutionState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&execution_state, |_this, _, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self { execution_state }
    }
}

impl Render for DebugSceneView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scene = {
            let execution_state = self.execution_state.read(cx);
            SceneRenderData {
                background_color: rgba_from_tuple(execution_state.background.color),
                camera: execution_state.camera.clone(),
                meshes: execution_state.meshes.clone(),
            }
        };

        div().size_full().child(
            canvas(
                move |bounds, _, _| bounds,
                move |_, bounds, window, _| {
                    paint_scene(&scene, bounds, window);
                },
            )
            .size_full(),
        )
    }
}

fn paint_scene(scene: &SceneRenderData, bounds: Bounds<Pixels>, window: &mut Window) {
    window.paint_quad(fill(bounds, scene.background_color));

    let basis = scene.camera.basis();
    let mut items = Vec::new();
    for mesh in &scene.meshes {
        collect_mesh_draw_items(mesh, basis, bounds, &mut items);
    }

    items.sort_by(|a, b| {
        a.z_index
            .cmp(&b.z_index)
            .then_with(|| b.depth.partial_cmp(&a.depth).unwrap_or(Ordering::Equal))
    });

    for item in items {
        paint_draw_item(item, window);
    }
}

fn collect_mesh_draw_items(
    mesh: &Mesh,
    basis: CameraBasis,
    bounds: Bounds<Pixels>,
    items: &mut Vec<DrawItem>,
) {
    if mesh.uniform.alpha <= 0.0 {
        return;
    }

    let alpha = mesh.uniform.alpha as f32;
    for tri in &mesh.tris {
        collect_triangle_draw_items(tri, alpha, mesh.uniform.z_index, basis, bounds, items);
    }
    for lin in &mesh.lins {
        collect_line_draw_item(lin, alpha, mesh.uniform.z_index, basis, bounds, items);
    }
    for dot in &mesh.dots {
        collect_dot_draw_item(dot, alpha, mesh.uniform.z_index, basis, bounds, items);
    }
}

fn collect_triangle_draw_items(
    tri: &Tri,
    alpha: f32,
    z_index: i32,
    basis: CameraBasis,
    bounds: Bounds<Pixels>,
    items: &mut Vec<DrawItem>,
) {
    let Some(a) = project_point(tri.a.pos, basis, bounds) else {
        return;
    };
    let Some(b) = project_point(tri.b.pos, basis, bounds) else {
        return;
    };
    let Some(c) = project_point(tri.c.pos, basis, bounds) else {
        return;
    };

    let depth = (a.depth + b.depth + c.depth) / 3.0;
    let fill_color = rgba_from_color((tri.a.col + tri.b.col + tri.c.col) / 3.0, alpha);
    items.push(DrawItem {
        z_index,
        depth,
        primitive: DrawPrimitive::Triangle {
            points: [a.point, b.point, c.point],
            color: fill_color,
        },
    });
}

fn collect_line_draw_item(
    lin: &Lin,
    alpha: f32,
    z_index: i32,
    basis: CameraBasis,
    bounds: Bounds<Pixels>,
    items: &mut Vec<DrawItem>,
) {
    let Some(a) = project_point(lin.a.pos, basis, bounds) else {
        return;
    };
    let Some(b) = project_point(lin.b.pos, basis, bounds) else {
        return;
    };

    items.push(DrawItem {
        z_index,
        depth: (a.depth + b.depth) / 2.0,
        primitive: DrawPrimitive::Line {
            points: [a.point, b.point],
            color: rgba_from_color((lin.a.col + lin.b.col) / 2.0, alpha),
            width: px(EDGE_WIDTH),
        },
    });
}

fn collect_dot_draw_item(
    dot: &Dot,
    alpha: f32,
    z_index: i32,
    basis: CameraBasis,
    bounds: Bounds<Pixels>,
    items: &mut Vec<DrawItem>,
) {
    let Some(projected) = project_point(dot.pos, basis, bounds) else {
        return;
    };

    items.push(DrawItem {
        z_index,
        depth: projected.depth,
        primitive: DrawPrimitive::Dot {
            center: projected.point,
            color: rgba_from_color(dot.col, alpha),
        },
    });
}

fn project_point(
    world: Float3,
    basis: CameraBasis,
    bounds: Bounds<Pixels>,
) -> Option<ProjectedPoint> {
    let relative = world - basis.position;
    let camera_x = relative.dot(basis.right);
    let camera_y = relative.dot(basis.up);
    let camera_z = relative.dot(basis.forward);

    if camera_z < basis.near || camera_z > basis.far {
        return None;
    }

    let aspect = f32::from(bounds.size.width) / f32::from(bounds.size.height).max(1.0);
    let (ndc_x, ndc_y) = {
        let tan_half_fov = (basis.fov * 0.5).tan().max(0.05);
        (
            camera_x / (camera_z * tan_half_fov * aspect.max(0.1)),
            camera_y / (camera_z * tan_half_fov),
        )
    };

    if !ndc_x.is_finite() || !ndc_y.is_finite() {
        return None;
    }

    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let screen_x = width * (0.5 + 0.5 * ndc_x);
    let screen_y = height * (0.5 - 0.5 * ndc_y);

    Some(ProjectedPoint {
        point: point(
            bounds.origin.x + px(screen_x),
            bounds.origin.y + px(screen_y),
        ),
        depth: camera_z,
    })
}

fn paint_draw_item(item: DrawItem, window: &mut Window) {
    match item.primitive {
        DrawPrimitive::Triangle { points, color } => {
            let mut builder = PathBuilder::fill();
            builder.move_to(points[0]);
            builder.line_to(points[1]);
            builder.line_to(points[2]);
            builder.line_to(points[0]);
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
        DrawPrimitive::Line {
            points,
            color,
            width,
        } => {
            let mut builder = PathBuilder::stroke(width);
            builder.move_to(points[0]);
            builder.line_to(points[1]);
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
        DrawPrimitive::Dot { center, color } => {
            window.paint_quad(quad(
                Bounds::new(
                    point(center.x - px(DOT_RADIUS), center.y - px(DOT_RADIUS)),
                    size(px(DOT_RADIUS * 2.0), px(DOT_RADIUS * 2.0)),
                ),
                px(DOT_RADIUS),
                color,
                px(0.0),
                TRANSPARENT,
                BorderStyle::Solid,
            ));
        }
    }
}

fn rgba_from_color(color: Float4, alpha_scale: f32) -> Rgba {
    Rgba {
        r: color.x.clamp(0.0, 1.0),
        g: color.y.clamp(0.0, 1.0),
        b: color.z.clamp(0.0, 1.0),
        a: (color.w * alpha_scale).clamp(0.0, 1.0),
    }
}

fn rgba_from_tuple(color: (f32, f32, f32, f32)) -> Rgba {
    Rgba {
        r: color.0.clamp(0.0, 1.0),
        g: color.1.clamp(0.0, 1.0),
        b: color.2.clamp(0.0, 1.0),
        a: color.3.clamp(0.0, 1.0),
    }
}
