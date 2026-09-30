use executor::{error::ExecutorError, executor::Executor, kernel::BatchInput, value::Value};
use geo::{
    mesh_build::{BoundaryEdges, SurfaceVertex},
    simd::{Float2, Float3, Float4},
};
use stdlib_macros::stdlib_func;

use crate::mesh::helpers::*;

use super::*;

#[stdlib_func]
pub async fn mk_color_grid(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let x0 = crate::read_float(executor, stack_idx, -9, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -8, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -7, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -6, "y1")? as f32;
    let x_samples = grid_axis_samples(read_int(executor, stack_idx, -5, "x_samples")?);
    let y_samples = grid_axis_samples(read_int(executor, stack_idx, -4, "y_samples")?);
    let smooth = read_flag(executor, stack_idx, -3, "smooth")?;
    let mask = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_lvalue();
    let color_at = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();

    let nx = x_samples - 1;
    let ny = y_samples - 1;
    let cell_count = ensure_grid_cells("color grid cells", nx, ny)?;
    ensure_surface_triangles("color grid triangles", cell_count.saturating_mul(2))?;

    let grid_vertex = |ix: usize, iy: usize| ix * y_samples + iy;
    let mut cells = Vec::with_capacity(cell_count);
    let mut centers = Vec::with_capacity(cell_count);
    for ix in 0..nx {
        for iy in 0..ny {
            cells.push((ix, iy));
            centers.push(grid_cell_center(x0, x1, y0, y1, nx, ny, ix, iy));
        }
    }
    let mask_values =
        invoke_callable_many_values(executor, &mask, BatchInput::Points(&centers), "mask").await?;
    let mut enabled_cells = Vec::new();
    for (cell, mask_value) in cells.into_iter().zip(mask_values) {
        if mask_value.check_truthy()? {
            enabled_cells.push(cell);
        }
    }

    let mut vertices = Vec::<SurfaceVertex>::with_capacity(x_samples * y_samples);
    let mut faces = Vec::<[usize; 3]>::with_capacity(enabled_cells.len() * 2);
    if smooth {
        let mut samples = Vec::with_capacity(x_samples * y_samples);
        for ix in 0..x_samples {
            for iy in 0..y_samples {
                samples.push((grid_point(x0, x1, y0, y1, nx, ny, ix, iy), [ix, iy]));
            }
        }
        let colors = sample_colors(executor, &color_at, &samples).await?;
        vertices.extend(
            samples
                .into_iter()
                .zip(colors)
                .map(|((pos, _), col)| SurfaceVertex {
                    pos,
                    col,
                    uv: Float2::ZERO,
                }),
        );

        for (ix, iy) in enabled_cells {
            let a = grid_vertex(ix, iy);
            let b = grid_vertex(ix + 1, iy);
            let c = grid_vertex(ix + 1, iy + 1);
            let d = grid_vertex(ix, iy + 1);
            faces.push([a, b, c]);
            faces.push([a, c, d]);
        }

        let (lins, tris) = build_indexed_surface(&vertices, &faces, &BoundaryEdges::default());
        return Ok(mesh_from_parts(vec![], lins, tris));
    }

    for ix in 0..x_samples {
        for iy in 0..y_samples {
            vertices.push(SurfaceVertex {
                pos: grid_point(x0, x1, y0, y1, nx, ny, ix, iy),
                col: Float4::ONE,
                uv: Float2::ZERO,
            });
        }
    }

    let samples: Vec<_> = enabled_cells
        .iter()
        .map(|&(ix, iy)| (grid_point(x0, x1, y0, y1, nx, ny, ix, iy), [ix, iy]))
        .collect();
    let colors = sample_colors(executor, &color_at, &samples).await?;

    for (ix, iy) in enabled_cells {
        let a = grid_vertex(ix, iy);
        let b = grid_vertex(ix + 1, iy);
        let c = grid_vertex(ix + 1, iy + 1);
        let d = grid_vertex(ix, iy + 1);
        faces.push([a, b, c]);
        faces.push([a, c, d]);
    }

    let (mut lins, mut tris) = build_indexed_surface(&vertices, &faces, &BoundaryEdges::default());
    for (tri_pair, color) in tris.chunks_mut(2).zip(colors) {
        for tri in tri_pair {
            tri.a.col = color;
            tri.b.col = color;
            tri.c.col = color;
        }
    }

    for lin in &mut lins {
        if lin.inv <= -2 {
            let tri_idx = (-lin.inv - 2) as usize;
            if let Some(tri) = tris.get(tri_idx) {
                lin.a.col = tri.a.col;
                lin.b.col = tri.a.col;
            }
        }
    }

    Ok(mesh_from_parts(vec![], lins, tris))
}

/// `color_at` at every `(pos, [ix, iy])`
async fn sample_colors(
    executor: &mut Executor,
    color_at: &Value,
    samples: &[(Float3, [usize; 2])],
) -> Result<Vec<Float4>, ExecutorError> {
    invoke_callable_many_flat(
        executor,
        color_at,
        BatchInput::IndexedPoints(samples),
        "color_at",
        |value| float4_from_value(value, "color_at"),
    )
    .await
}

#[stdlib_func]
pub fn mk_line_grid(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x0 = crate::read_float(executor, stack_idx, -7, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -6, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -5, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -4, "y1")? as f32;
    let x_samples = point_samples(read_int(executor, stack_idx, -3, "x_samples")?);
    let y_samples = point_samples(read_int(executor, stack_idx, -2, "y_samples")?);
    let subdivision = read_int(executor, stack_idx, -1, "subdivision")?.max(1) as usize;

    let line_count = checked_sum(
        "line grid points",
        x_samples,
        y_samples,
        MAX_LINE_GRID_POINTS,
    )?;
    let points_per_line = subdivision
        .checked_add(1)
        .ok_or_else(|| mesh_limit_error("line grid points", usize::MAX, MAX_LINE_GRID_POINTS))?;
    let point_count = checked_product(
        "line grid points",
        line_count,
        points_per_line,
        MAX_LINE_GRID_POINTS,
    )?;

    let x_den = x_samples.saturating_sub(1).max(1);
    let y_den = y_samples.saturating_sub(1).max(1);
    let mut lins = Vec::with_capacity(point_count - line_count);
    for ix in 0..x_samples {
        let x = x0 + (x1 - x0) * ix as f32 / x_den as f32;
        push_subdivided_line(
            &mut lins,
            Float3::new(x, y0, 0.0),
            Float3::new(x, y1, 0.0),
            subdivision,
        );
    }
    for iy in 0..y_samples {
        let y = y0 + (y1 - y0) * iy as f32 / y_den as f32;
        push_subdivided_line(
            &mut lins,
            Float3::new(x0, y, 0.0),
            Float3::new(x1, y, 0.0),
            subdivision,
        );
    }

    Ok(mesh_from_parts(vec![], lins, vec![]))
}
