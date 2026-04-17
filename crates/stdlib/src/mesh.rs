//! native implementations referenced from std.mesh. Most are todo!() — the
//! actual mesh construction, CSG, and sampling routines live in the `geo`
//! crate and will be wired up here.

use executor::{error::ExecutorError, executor::Executor, value::Value};
use stdlib_macros::stdlib_func;

// ── primitive constructors (2d / line) ──────────────────────────────────────

#[stdlib_func]
pub async fn mk_dot(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("build a single-point mesh")
}

#[stdlib_func]
pub async fn mk_circle(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("sampled circle mesh")
}

#[stdlib_func]
pub async fn mk_annulus(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("annulus mesh")
}

#[stdlib_func]
pub async fn mk_square(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("axis-aligned square")
}

#[stdlib_func]
pub async fn mk_rect(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("axis-aligned rectangle")
}

#[stdlib_func]
pub async fn mk_regular_polygon(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("regular-polygon mesh (n sides)")
}

#[stdlib_func]
pub async fn mk_polygon(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("arbitrary closed polygon from a vertex list")
}

#[stdlib_func]
pub async fn mk_polyline(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("open polyline from a vertex list")
}

#[stdlib_func]
pub async fn mk_line(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("line segment mesh")
}

#[stdlib_func]
pub async fn mk_arrow(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("straight / curved arrow")
}

#[stdlib_func]
pub async fn mk_arc(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("arc of a circle between two angles")
}

#[stdlib_func]
pub async fn mk_capsule(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("capsule (stadium) mesh")
}

#[stdlib_func]
pub async fn mk_triangle(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("triangle from three points")
}

// ── 3d primitives ───────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn mk_sphere(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("icosphere-subdivided sphere surface")
}

#[stdlib_func]
pub async fn mk_rect_prism(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("axis-aligned rectangular prism")
}

#[stdlib_func]
pub async fn mk_cylinder(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("finite cylinder surface")
}

#[stdlib_func]
pub async fn mk_cone(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("cone surface with apex and circular base")
}

#[stdlib_func]
pub async fn mk_torus(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("torus surface with major / minor radii")
}

#[stdlib_func]
pub async fn mk_plane(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("finite planar patch with a normal")
}

// ── visual / graphical ─────────────────────────────────────────────────────

#[stdlib_func]
pub async fn mk_bezier(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("bezier curve from control points")
}

#[stdlib_func]
pub async fn mk_vector(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("arrow-with-tail oriented vector mesh")
}

#[stdlib_func]
pub async fn mk_half_vector(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("ray-style half-vector mesh (no base tick)")
}

#[stdlib_func]
pub async fn mk_image(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("image-textured quad")
}

#[stdlib_func]
pub async fn mk_color_grid(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("scalar-field heatmap sampled over a rectangle")
}

#[stdlib_func]
pub async fn mk_field(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("arbitrary mesh-generating function sampled over a rectangle")
}

// ── text / labels ───────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn mk_text(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("text glyphs via the embedded font")
}

#[stdlib_func]
pub async fn mk_tex(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("tex / mathjax rendered glyphs")
}

#[stdlib_func]
pub async fn mk_brace(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("curly brace spanning a mesh along a direction")
}

#[stdlib_func]
pub async fn mk_measure(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("dimension / measurement annotation")
}

#[stdlib_func]
pub async fn mk_label(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("text label attached to a mesh in a direction")
}

#[stdlib_func]
pub async fn mk_number(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("number rendered with fixed precision, usable for counters")
}

// ── graphing ────────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn mk_axis1d(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("single-axis number line")
}

#[stdlib_func]
pub async fn mk_axis2d(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("2d cartesian axes with optional grid")
}

#[stdlib_func]
pub async fn mk_axis3d(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("3d cartesian axes")
}

#[stdlib_func]
pub async fn mk_polar_axis(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("polar / circular axis grid")
}

#[stdlib_func]
pub async fn mk_parametric(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("parametric curve f(t)")
}

#[stdlib_func]
pub async fn mk_explicit(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("explicit function y = f(x)")
}

#[stdlib_func]
pub async fn mk_explicit2d(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("explicit surface z = f(x, y)")
}

#[stdlib_func]
pub async fn mk_implicit2d(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("level set f(x, y) = 0 via marching squares")
}

#[stdlib_func]
pub async fn mk_explicit_diff(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("filled region between two explicit curves")
}

// ── layout ──────────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn mk_stack(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("linearly arrange meshes along a direction with alignment")
}

#[stdlib_func]
pub async fn mk_grid(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("arrange a rectangular list-of-lists of meshes as a grid")
}

#[stdlib_func]
pub async fn mk_table(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("grid + cell borders")
}

#[stdlib_func]
pub async fn mk_bounding_box(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("axis-aligned bounding rectangle of a mesh")
}

// ── transform operators ─────────────────────────────────────────────────────

#[stdlib_func]
pub async fn op_shift(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("translate every vertex by a delta")
}

#[stdlib_func]
pub async fn op_scale(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("uniform scale around the mesh centroid")
}

#[stdlib_func]
pub async fn op_scale_xyz(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("per-axis scale around the mesh centroid")
}

#[stdlib_func]
pub async fn op_rotate(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("euler-angle rotation around the mesh centroid")
}

#[stdlib_func]
pub async fn op_rotate_around(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("rotation around an arbitrary pivot")
}

#[stdlib_func]
pub async fn op_fade(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("multiply alpha of every vertex")
}

#[stdlib_func]
pub async fn op_restroke(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("replace stroke color")
}

#[stdlib_func]
pub async fn op_refill(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("replace fill color")
}

#[stdlib_func]
pub async fn op_redot(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("replace dot color")
}

#[stdlib_func]
pub async fn op_retextured(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("apply an image texture")
}

#[stdlib_func]
pub async fn op_with_z(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("override z-order")
}

#[stdlib_func]
pub async fn op_gloss(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("enable specular gloss for 3d surfaces")
}

#[stdlib_func]
pub async fn op_point_map(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("remap each vertex position through a lambda")
}

#[stdlib_func]
pub async fn op_color_map(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("remap each vertex color through a function of position")
}

#[stdlib_func]
pub async fn op_uv_map(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("assign uv coordinates per vertex")
}

#[stdlib_func]
pub async fn op_retagged(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("rename tags through a pure function")
}

#[stdlib_func]
pub async fn op_tag_map(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("apply a per-sub-mesh transform keyed by tag")
}

#[stdlib_func]
pub async fn op_uprank(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("promote rank (e.g. line → triangle mesh)")
}

#[stdlib_func]
pub async fn op_downrank(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("demote rank (e.g. surface → wireframe)")
}

#[stdlib_func]
pub async fn op_wireframe(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("render only the edges of the mesh")
}

#[stdlib_func]
pub async fn op_subdivide(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("split each line into n segments")
}

#[stdlib_func]
pub async fn op_tesselated(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("recursively tesselate faces")
}

#[stdlib_func]
pub async fn op_extrude(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("extrude mesh along a delta vector")
}

#[stdlib_func]
pub async fn op_revolve(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("revolve a line mesh around an axis to form a surface")
}

#[stdlib_func]
pub async fn op_centered(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("translate mesh so its centroid sits at the given point")
}

#[stdlib_func]
pub async fn op_moved_to_side(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("move mesh flush against a given viewport edge")
}

#[stdlib_func]
pub async fn op_matched_edge(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("align an edge of the target with an edge of a reference mesh")
}

#[stdlib_func]
pub async fn op_next_to(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("place mesh next to another in a given direction")
}

#[stdlib_func]
pub async fn op_projected(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("project each vertex onto another mesh along a ray")
}

#[stdlib_func]
pub async fn op_in_space(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("embed an axes-space mesh into world space")
}

// ── boolean mesh ops ────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn op_masked(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("keep only the part of the target inside a mask mesh")
}

#[stdlib_func]
pub async fn op_joined(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("union two meshes")
}

#[stdlib_func]
pub async fn op_set_diff(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("subtract one mesh from another")
}

#[stdlib_func]
pub async fn op_sym_diff(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("symmetric difference of two meshes")
}

#[stdlib_func]
pub async fn op_minkowski_sum(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("minkowski sum of two meshes")
}

// ── queries ─────────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn mesh_left(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("leftmost point of the mesh")
}

#[stdlib_func]
pub async fn mesh_right(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("rightmost point of the mesh")
}

#[stdlib_func]
pub async fn mesh_up(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("topmost point of the mesh")
}

#[stdlib_func]
pub async fn mesh_down(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("bottommost point of the mesh")
}

#[stdlib_func]
pub async fn mesh_forward(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("most-forward point of the mesh")
}

#[stdlib_func]
pub async fn mesh_backward(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("most-backward point of the mesh")
}

#[stdlib_func]
pub async fn mesh_direc(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("farthest point along an arbitrary direction")
}

#[stdlib_func]
pub async fn mesh_width(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("axis-aligned width of the mesh")
}

#[stdlib_func]
pub async fn mesh_height(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("axis-aligned height of the mesh")
}

#[stdlib_func]
pub async fn mesh_center(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("centroid of the mesh")
}

#[stdlib_func]
pub async fn mesh_rank(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("rank of the mesh: -1 empty, 0 dots, 1 lines, 2 triangles")
}

#[stdlib_func]
pub async fn mesh_sample(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("sample point at parameter t in [0, 1]")
}

#[stdlib_func]
pub async fn mesh_normal(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("sample normal at parameter t")
}

#[stdlib_func]
pub async fn mesh_tangent(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("sample tangent at parameter t")
}

#[stdlib_func]
pub async fn mesh_contains(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("inside-test for a point against the mesh")
}

#[stdlib_func]
pub async fn mesh_dist(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("signed distance from a point to the mesh surface")
}

#[stdlib_func]
pub async fn mesh_raycast(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("raycast against the mesh")
}

#[stdlib_func]
pub async fn mesh_vertex_set(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("mesh vertex list")
}

#[stdlib_func]
pub async fn mesh_edge_set(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("mesh edge list")
}

#[stdlib_func]
pub async fn mesh_triangle_set(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("mesh triangle list")
}

#[stdlib_func]
pub async fn mesh_contour_count(_e: &mut Executor, _s: usize) -> Result<Value, ExecutorError> {
    todo!("number of distinct contours in the mesh")
}

#[stdlib_func]
pub async fn mesh_contour_separate(
    _e: &mut Executor,
    _s: usize,
) -> Result<Value, ExecutorError> {
    todo!("split the mesh into one mesh per contour")
}
