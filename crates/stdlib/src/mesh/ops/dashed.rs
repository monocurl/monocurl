use std::{future::Future, pin::Pin, sync::Arc};

use executor::{error::ExecutorError, executor::Executor, value::Value};
use geo::{
    mesh::{Lin, Mesh},
    simd::{Float3, Float4},
};
use stdlib_macros::stdlib_func;

use crate::mesh::helpers::*;

use super::topology::{clear_surface_line_refs, line_paths};

fn snap_line_t(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 1e-5 {
        0.0
    } else if (1.0 - t).abs() <= 1e-5 {
        1.0
    } else {
        t
    }
}

pub(super) fn push_dashed_segment(
    out: &mut Vec<Lin>,
    current_piece_last: &mut Option<usize>,
    template: &Lin,
    t0: f32,
    t1: f32,
) {
    let t0 = snap_line_t(t0);
    let t1 = snap_line_t(t1);
    if (t1 - t0).abs() <= 1e-6 {
        return;
    }

    let point_at = |start: Float3, end: Float3, t: f32| match t {
        0.0 => start,
        1.0 => end,
        _ => start.lerp(end, t),
    };
    let color_at = |start: Float4, end: Float4, t: f32| match t {
        0.0 => start,
        1.0 => end,
        _ => start.lerp(end, t),
    };

    let mut segment = default_lin(
        point_at(template.a.pos, template.b.pos, t0),
        point_at(template.a.pos, template.b.pos, t1),
        template.norm,
    );
    segment.a.col = color_at(template.a.col, template.b.col, t0);
    segment.b.col = color_at(template.a.col, template.b.col, t1);

    if let Some(prev) = *current_piece_last
        && float3_key(out[prev].b.pos) == float3_key(segment.a.pos)
        && float3_key(out[prev].norm) == float3_key(segment.norm)
    {
        segment.prev = prev as i32;
        out[prev].next = out.len() as i32;
    }

    *current_piece_last = Some(out.len());
    out.push(segment);
}

pub(super) fn dashed_lines(source_lines: &[Lin], dash_length: f32, gap_length: f32, offset: f32) -> Vec<Lin> {
    let period = dash_length + gap_length;
    let mut out = Vec::new();

    for path in line_paths(source_lines) {
        let mut current_piece_last = None;
        let mut distance = 0.0f32;

        for &line_idx in &path {
            let line = &source_lines[line_idx];
            let length = (line.b.pos - line.a.pos).len();
            if length <= 1e-6 {
                continue;
            }

            let mut local = 0.0f32;
            while local < length - 1e-6 {
                let global = distance + local;
                let phase = (global + offset).rem_euclid(period);

                if phase < dash_length - 1e-6 || gap_length <= 1e-6 {
                    let visible = (dash_length - phase).max(0.0).min(length - local);
                    if visible <= 1e-6 {
                        break;
                    }
                    push_dashed_segment(
                        &mut out,
                        &mut current_piece_last,
                        line,
                        local / length,
                        (local + visible) / length,
                    );
                    local += visible;
                } else {
                    current_piece_last = None;
                    let hidden = (period - phase).max(1e-6).min(length - local);
                    local += hidden;
                }
            }

            distance += length;
        }
    }

    out
}

fn stroke_source_lines(mesh: &Mesh) -> Vec<Lin> {
    if !mesh.lins.is_empty() {
        return mesh
            .lins
            .iter()
            .copied()
            .filter(|line| line.is_dom_sib)
            .collect();
    }
    if mesh.tris.is_empty() {
        return Vec::new();
    }

    let surface = mesh_to_indexed_surface(mesh);
    build_indexed_surface(&surface.vertices, &surface.faces, &surface.boundary_edges)
        .0
        .into_iter()
        .map(|mut line| {
            line.inv = -1;
            line
        })
        .collect()
}

pub(super) fn dashed_mesh(mesh: &Mesh, dash_length: f32, gap_length: f32, offset: f32) -> MeshTree {
    let dashed_lins = dashed_lines(&stroke_source_lines(mesh), dash_length, gap_length, offset);
    let has_base_geometry =
        mesh.dots.iter().any(|dot| dot.col.w > f32::EPSILON) || !mesh.tris.is_empty();

    if !has_base_geometry {
        if dashed_lins.is_empty() {
            return MeshTree::Mesh(Arc::new(mesh.clone()));
        }

        let dashed = Mesh {
            dots: Default::default(),
            lins: dashed_lins.into(),
            tris: Default::default(),
            uniform: mesh.uniform.clone(),
            tag: mesh.tag.clone(),
            version: Mesh::fresh_version(),
        };
        let mut dashed = dashed;
        dashed.normalize_line_dot_topology();
        dashed.debug_assert_consistent_topology();
        return MeshTree::Mesh(Arc::new(dashed));
    }

    let mut children = Vec::with_capacity(2);

    let mut base = mesh.clone();
    if !base.lins.is_empty() {
        base.lins.clear();
    }
    if !base.tris.is_empty() {
        clear_surface_line_refs(&mut base);
    }
    base.debug_assert_consistent_topology();
    children.push(MeshTree::Mesh(Arc::new(base)));

    if !dashed_lins.is_empty() {
        let mut dashed = Mesh {
            dots: Default::default(),
            lins: dashed_lins.into(),
            tris: Default::default(),
            uniform: mesh.uniform.clone(),
            tag: mesh.tag.clone(),
            version: Mesh::fresh_version(),
        };
        dashed.normalize_line_dot_topology();
        dashed.debug_assert_consistent_topology();
        children.push(MeshTree::Mesh(Arc::new(dashed)));
    }

    MeshTree::List(children)
}

fn dashed_tree<'a>(
    executor: &'a mut Executor,
    tree: MeshTree,
    dash_length: f32,
    gap_length: f32,
    offset: f32,
    filter: Option<&'a TagFilter>,
) -> Pin<Box<dyn Future<Output = Result<MeshTree, ExecutorError>> + 'a>> {
    Box::pin(async move {
        match tree {
            MeshTree::Mesh(mesh) => {
                let keep = match filter {
                    Some(filter) => mesh_matches_tag_filter(executor, filter, &mesh).await?,
                    None => true,
                };
                if keep {
                    Ok(dashed_mesh(&mesh, dash_length, gap_length, offset))
                } else {
                    Ok(MeshTree::Mesh(mesh))
                }
            }
            MeshTree::List(children) => {
                let mut out = Vec::with_capacity(children.len());
                for child in children {
                    out.push(
                        dashed_tree(executor, child, dash_length, gap_length, offset, filter)
                            .await?,
                    );
                }
                Ok(MeshTree::List(out))
            }
        }
    })
}

#[stdlib_func]
pub async fn op_dashed(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let dash_length = crate::read_float(executor, stack_idx, -4, "dash_length")? as f32;
    let gap_length = crate::read_float(executor, stack_idx, -3, "gap_length")? as f32;
    let offset = crate::read_float(executor, stack_idx, -2, "offset")? as f32;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;

    if dash_length <= 1e-6 {
        return Err(ExecutorError::InvalidArgument {
            arg: "dash_length",
            message: "dash length must be positive",
        });
    }
    if gap_length < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "gap_length",
            message: "gap length must be non-negative",
        });
    }

    dashed_tree(
        executor,
        tree,
        dash_length,
        gap_length,
        offset,
        filter.as_ref(),
    )
    .await
    .map(MeshTree::into_value)
}
