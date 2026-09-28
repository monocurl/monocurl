use executor::{error::ExecutorError, executor::Executor, kernel::KVal, value::Value};
use geo::{
    mesh::Mesh,
    mesh_build::{BoundaryEdges, SurfaceVertex},
    simd::{Float2, Float3, Float4},
};
use smallvec::{SmallVec, smallvec};
use stdlib_macros::stdlib_func;

use crate::mesh::{helpers::*, implicit2d};

use super::*;

/// Read a scalar `y = f(x)` sample. `nil` or a non-finite number is a domain
/// gap (`Ok(None)`); a wrong type is still an error.
/// `explicit_sample_y` read straight off a kernel result
fn explicit_sample_y_from_kernel(value: &KVal) -> Option<Option<f32>> {
    match value {
        KVal::Nil => Some(None),
        KVal::Int(v) => Some(Some(*v as f32)),
        KVal::Float(v) => Some(v.is_finite().then_some(*v as f32)),
        _ => None,
    }
}

fn explicit_sample_y(value: Value, name: &'static str) -> Result<Option<f32>, ExecutorError> {
    match value.elide_cached_wrappers_rec() {
        Value::Nil => Ok(None),
        Value::Integer(v) => Ok(Some(v as f32)),
        Value::Float(v) => Ok(v.is_finite().then_some(v as f32)),
        other => Err(ExecutorError::type_error_for(
            "float or nil",
            other.type_name(),
            name,
        )),
    }
}

/// Read a 3-D parametric sample. `nil` or any non-finite component is a domain
/// gap (`Ok(None)`); a non-3-list is still an error.
fn parametric_sample_point(
    value: Value,
    name: &'static str,
) -> Result<Option<Float3>, ExecutorError> {
    match value.elide_cached_wrappers_rec() {
        Value::Nil => Ok(None),
        other => {
            let p = float3_from_value(other, name)?;
            Ok((p.x.is_finite() && p.y.is_finite() && p.z.is_finite()).then_some(p))
        }
    }
}

#[stdlib_func]
pub async fn mk_field(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x0 = crate::read_float(executor, stack_idx, -8, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -7, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -6, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -5, "y1")? as f32;
    let x_samples = point_samples(read_int(executor, stack_idx, -4, "x_samples")?);
    let y_samples = point_samples(read_int(executor, stack_idx, -3, "y_samples")?);
    let mask = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_lvalue();
    let mesh_at = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();

    let sample_count = checked_product("field samples", x_samples, y_samples, MAX_FIELD_SAMPLES)?;
    let mut out = Vec::with_capacity(sample_count);
    let nx = x_samples.saturating_sub(1).max(1);
    let ny = y_samples.saturating_sub(1).max(1);

    let mut samples = Vec::with_capacity(sample_count);
    let mut mask_args = Vec::<SmallVec<[Value; 2]>>::with_capacity(sample_count);
    for ix in 0..x_samples {
        for iy in 0..y_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            let pos = Float3::new(x, y, 0.0);
            samples.push((pos, ix, iy));
            mask_args.push(smallvec![point_value(pos)]);
        }
    }
    let mask_values = invoke_callable_many(executor, &mask, &mask_args, "mask").await?;
    let mut mesh_args = Vec::<SmallVec<[Value; 2]>>::new();
    for ((pos, ix, iy), mask_value) in samples.into_iter().zip(mask_values) {
        if mask_value.check_truthy()? {
            mesh_args.push(smallvec![point_value(pos), sample_index_value(ix, iy)]);
        }
    }
    out.extend(invoke_callable_many(executor, &mesh_at, &mesh_args, "mesh_at").await?);

    Ok(list_value(out))
}

#[stdlib_func]
pub async fn mk_parametric(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-4)
        .clone()
        .elide_lvalue();
    let t0 = crate::read_float(executor, stack_idx, -3, "t0")?;
    let t1 = crate::read_float(executor, stack_idx, -2, "t1")?;
    let samples = read_int(executor, stack_idx, -1, "samples")?.max(2) as usize;
    ensure_limit("parametric samples", samples, MAX_CURVE_SAMPLES)?;
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(samples);
    for i in 0..samples {
        let t = if samples == 1 {
            t0
        } else {
            t0 + (t1 - t0) * i as f64 / (samples - 1) as f64
        };
        args.push(smallvec![Value::Float(t)]);
    }
    let values = invoke_callable_many(executor, &f, &args, "f").await?;
    let points = values
        .into_iter()
        .map(|value| parametric_sample_point(value, "f"))
        .collect::<Result<Vec<_>, _>>()?;
    let valid: Vec<Float3> = points.iter().filter_map(|p| *p).collect();
    let normal = valid
        .windows(3)
        .find_map(|w| {
            let cross = (w[1] - w[0]).cross(w[2] - w[1]);
            (cross.len_sq() > 1e-6).then(|| cross.normalize())
        })
        .unwrap_or(Float3::Z);
    Ok(mesh_from_parts(
        vec![],
        segmented_open_polyline(&points, normal),
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_explicit(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-4)
        .clone()
        .elide_lvalue();
    let x0 = crate::read_float(executor, stack_idx, -3, "x0")?;
    let x1 = crate::read_float(executor, stack_idx, -2, "x1")?;
    let samples = read_int(executor, stack_idx, -1, "samples")?.max(2) as usize;
    ensure_limit("explicit samples", samples, MAX_CURVE_SAMPLES)?;
    let mut xs = Vec::with_capacity(samples);
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(samples);
    for i in 0..samples {
        let x = x0 + (x1 - x0) * i as f64 / (samples - 1) as f64;
        xs.push(x);
        args.push(smallvec![Value::Float(x)]);
    }
    let ys = invoke_callable_many_mapped(
        executor,
        &f,
        &args,
        "f",
        explicit_sample_y_from_kernel,
        |value| explicit_sample_y(value, "f"),
    )
    .await?;
    let points = xs
        .into_iter()
        .zip(ys)
        .map(|(x, y)| y.map(|y| Float3::new(x as f32, y, 0.0)))
        .collect::<Vec<_>>();
    Ok(mesh_from_parts(
        vec![],
        segmented_open_polyline(&points, Float3::Z),
        vec![],
    ))
}

#[stdlib_func]
pub async fn mk_explicit2d(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-8)
        .clone()
        .elide_lvalue();
    let x0 = crate::read_float(executor, stack_idx, -7, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -6, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -5, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -4, "y1")? as f32;
    let x_samples = grid_axis_samples(read_int(executor, stack_idx, -3, "x_samples")?);
    let y_samples = grid_axis_samples(read_int(executor, stack_idx, -2, "y_samples")?);
    let color_at = {
        let raw = executor
            .state
            .stack(stack_idx)
            .read_at(-1)
            .clone()
            .elide_lvalue();
        (!matches!(raw.clone().elide_cached_wrappers_rec(), Value::Nil)).then_some(raw)
    };
    let nx = x_samples - 1;
    let ny = y_samples - 1;
    let cell_count = ensure_grid_cells("explicit surface cells", nx, ny)?;
    ensure_surface_triangles("explicit surface triangles", cell_count.saturating_mul(2))?;
    let mut grid = vec![vec![Float3::ZERO; y_samples]; x_samples];
    let mut coords = Vec::with_capacity(x_samples * y_samples);
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(x_samples * y_samples);
    for ix in 0..x_samples {
        for iy in 0..y_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            coords.push((ix, iy, x, y));
            args.push(smallvec![Value::Float(x as f64), Value::Float(y as f64)]);
        }
    }
    let values =
        invoke_callable_many_mapped(
            executor,
            &f,
            &args,
            "f",
            f32_from_kernel,
            |value| match value {
                Value::Float(v) => Ok(v as f32),
                Value::Integer(v) => Ok(v as f32),
                other => Err(ExecutorError::type_error_for(
                    "float",
                    other.type_name(),
                    "f",
                )),
            },
        )
        .await?;
    for ((ix, iy, x, y), z) in coords.into_iter().zip(values) {
        grid[ix][iy] = Float3::new(x, y, z);
    }
    let index = |ix: usize, iy: usize| ix * (ny + 1) + iy;
    let vertices: Vec<_> = grid.iter().flat_map(|col| col.iter().copied()).collect();
    let mut faces = Vec::with_capacity(nx * ny * 2);
    for ix in 0..nx {
        for iy in 0..ny {
            faces.push([index(ix, iy), index(ix + 1, iy), index(ix + 1, iy + 1)]);
            faces.push([index(ix, iy), index(ix + 1, iy + 1), index(ix, iy + 1)]);
        }
    }
    let colors: Vec<Float4> = if let Some(cb) = &color_at {
        let color_args: Vec<SmallVec<[Value; 2]>> = vertices
            .iter()
            .map(|p| {
                smallvec![
                    Value::Float(p.x as f64),
                    Value::Float(p.y as f64),
                    Value::Float(p.z as f64),
                ]
            })
            .collect();
        invoke_callable_many_mapped(
            executor,
            cb,
            &color_args,
            "color_at",
            float4_from_kernel,
            |value| float4_from_value(value, "color_at"),
        )
        .await?
    } else {
        vec![Float4::new(0.0, 0.0, 0.0, 1.0); vertices.len()]
    };
    let surface_vertices: Vec<_> = vertices
        .into_iter()
        .zip(colors)
        .map(|(pos, col)| SurfaceVertex {
            pos,
            col,
            uv: Float2::ZERO,
        })
        .collect();
    let (lins, tris) = build_indexed_surface(&surface_vertices, &faces, &BoundaryEdges::default());
    Ok(mesh_from_parts(vec![], lins, tris))
}

#[stdlib_func]
pub async fn mk_implicit2d(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-7)
        .clone()
        .elide_lvalue();
    let x0 = crate::read_float(executor, stack_idx, -6, "x0")? as f32;
    let x1 = crate::read_float(executor, stack_idx, -5, "x1")? as f32;
    let y0 = crate::read_float(executor, stack_idx, -4, "y0")? as f32;
    let y1 = crate::read_float(executor, stack_idx, -3, "y1")? as f32;
    let x_samples = grid_axis_samples(read_int(executor, stack_idx, -2, "x_samples")?);
    let y_samples = grid_axis_samples(read_int(executor, stack_idx, -1, "y_samples")?);
    let nx = x_samples - 1;
    let ny = y_samples - 1;
    ensure_grid_cells("implicit surface cells", nx, ny)?;
    let sign_stride = x_samples + 2;
    let mut sign = vec![false; (y_samples + 2) * sign_stride];
    let mut coords = Vec::with_capacity(x_samples * y_samples);
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(x_samples * y_samples);
    for iy in 0..y_samples {
        for ix in 0..x_samples {
            let x = x0 + (x1 - x0) * ix as f32 / nx as f32;
            let y = y0 + (y1 - y0) * iy as f32 / ny as f32;
            coords.push((ix, iy));
            args.push(smallvec![Value::Float(x as f64), Value::Float(y as f64)]);
        }
    }
    let values = invoke_callable_many(executor, &f, &args, "f").await?;
    for ((ix, iy), value) in coords.into_iter().zip(values) {
        let value = match value {
            Value::Float(v) => v as f32,
            Value::Integer(v) => v as f32,
            other => {
                return Err(ExecutorError::type_error_for(
                    "float",
                    other.type_name(),
                    "f",
                ));
            }
        };
        sign[(iy + 1) * sign_stride + ix + 1] = value <= 0.0;
    }

    let lins = implicit2d::contour_lins(
        &sign,
        y_samples,
        x_samples,
        x0,
        y0,
        (x1 - x0) / nx as f32,
        (y1 - y0) / ny as f32,
    );
    Ok(mesh_from_parts(vec![], lins, vec![]))
}

#[stdlib_func]
pub async fn mk_explicit_diff(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let f = executor
        .state
        .stack(stack_idx)
        .read_at(-9)
        .clone()
        .elide_lvalue();
    let g = executor
        .state
        .stack(stack_idx)
        .read_at(-8)
        .clone()
        .elide_lvalue();
    let x0 = crate::read_float(executor, stack_idx, -7, "x0")?;
    let x1 = crate::read_float(executor, stack_idx, -6, "x1")?;
    let samples = read_int(executor, stack_idx, -5, "samples")?.max(2) as usize;
    ensure_limit("explicit diff samples", samples, MAX_CURVE_SAMPLES)?;
    ensure_surface_triangles(
        "explicit diff triangles",
        samples.saturating_sub(1).saturating_mul(2),
    )?;
    let fill0 = read_float4(executor, stack_idx, -4, "fill0").await?;
    let fill1 = read_float4(executor, stack_idx, -3, "fill1").await?;
    let tag0 = read_tags(executor, stack_idx, -2, "tag0")?;
    let tag1 = read_tags(executor, stack_idx, -1, "tag1")?;

    let mut upper = Vec::with_capacity(samples);
    let mut lower = Vec::with_capacity(samples);
    let mut xs = Vec::with_capacity(samples);
    let mut args = Vec::<SmallVec<[Value; 2]>>::with_capacity(samples);
    for i in 0..samples {
        let x = x0 + (x1 - x0) * i as f64 / (samples - 1) as f64;
        xs.push(x);
        args.push(smallvec![Value::Float(x)]);
    }
    let upper_values = invoke_callable_many(executor, &f, &args, "f").await?;
    let lower_values = invoke_callable_many(executor, &g, &args, "g").await?;
    // a column is valid only if both f and g are finite there; `nil` / non-finite
    // marks a domain gap and the fill / outline is split around it.
    let mut valid = Vec::with_capacity(samples);
    for ((x, upper_value), lower_value) in xs.into_iter().zip(upper_values).zip(lower_values) {
        let yf = explicit_sample_y(upper_value, "f")?;
        let yg = explicit_sample_y(lower_value, "g")?;
        match (yf, yg) {
            (Some(a), Some(b)) => {
                upper.push(Float3::new(x as f32, a, 0.0));
                lower.push(Float3::new(x as f32, b, 0.0));
                valid.push(true);
            }
            _ => {
                upper.push(Float3::new(x as f32, 0.0, 0.0));
                lower.push(Float3::new(x as f32, 0.0, 0.0));
                valid.push(false);
            }
        }
    }

    // split contiguous same-sign columns into shared strips so interior columns
    // do not leave stroked seams between identical regions
    let mut pos_verts: Vec<Float3> = Vec::new();
    let mut pos_faces: Vec<[usize; 3]> = Vec::new();
    let mut neg_verts: Vec<Float3> = Vec::new();
    let mut neg_faces: Vec<[usize; 3]> = Vec::new();

    let interval_is_pos =
        |i: usize| (upper[i].y + upper[i + 1].y) * 0.5 >= (lower[i].y + lower[i + 1].y) * 0.5;
    let append_strip =
        |verts: &mut Vec<Float3>, faces: &mut Vec<[usize; 3]>, start: usize, end: usize| {
            let base = verts.len();
            for idx in start..=end {
                verts.push(lower[idx]);
                verts.push(upper[idx]);
            }
            for idx in 0..end - start {
                let col = base + idx * 2;
                faces.push([col, col + 1, col + 3]);
                faces.push([col, col + 3, col + 2]);
            }
        };

    // for each maximal run of valid columns, tile it with same-sign strips
    let mut i = 0usize;
    while i < samples {
        if !valid[i] {
            i += 1;
            continue;
        }
        let seg_start = i;
        while i + 1 < samples && valid[i + 1] {
            i += 1;
        }
        let seg_end = i;
        i += 1;
        if seg_end == seg_start {
            continue;
        }

        let mut run_start = seg_start;
        let mut is_pos = interval_is_pos(seg_start);
        for k in (seg_start + 1)..seg_end {
            let next_is_pos = interval_is_pos(k);
            if next_is_pos != is_pos {
                if is_pos {
                    append_strip(&mut pos_verts, &mut pos_faces, run_start, k);
                } else {
                    append_strip(&mut neg_verts, &mut neg_faces, run_start, k);
                }
                run_start = k;
                is_pos = next_is_pos;
            }
        }
        if is_pos {
            append_strip(&mut pos_verts, &mut pos_faces, run_start, seg_end);
        } else {
            append_strip(&mut neg_verts, &mut neg_faces, run_start, seg_end);
        }
    }

    let build_region = |verts: Vec<Float3>, faces: Vec<[usize; 3]>, fill: Float4| {
        let vertices: Vec<_> = verts
            .into_iter()
            .map(|pos| SurfaceVertex {
                pos,
                col: fill,
                uv: Float2::ZERO,
            })
            .collect();
        build_indexed_surface(&vertices, &faces, &BoundaryEdges::default())
    };

    let (pos_lins, pos_tris) = build_region(pos_verts, pos_faces, fill0);
    let (neg_lins, neg_tris) = build_region(neg_verts, neg_faces, fill1);

    let make_tagged_mesh = |lins, tris, tag: Vec<isize>| {
        let mesh = geo::mesh::Mesh {
            dots: vec![],
            lins,
            tris,
            uniform: geo::mesh::Uniforms::default(),
            tag,
            version: Mesh::fresh_version(),
        };
        mesh.debug_assert_consistent_topology();
        Value::Mesh(std::sync::Arc::new(mesh))
    };

    let pos_val = make_tagged_mesh(pos_lins, pos_tris, tag0);
    let neg_val = make_tagged_mesh(neg_lins, neg_tris, tag1);

    let upper_opt: Vec<Option<Float3>> = upper
        .iter()
        .zip(&valid)
        .map(|(p, &ok)| ok.then_some(*p))
        .collect();
    let lower_opt: Vec<Option<Float3>> = lower
        .iter()
        .zip(&valid)
        .map(|(p, &ok)| ok.then_some(*p))
        .collect();
    let mut lins = Vec::new();
    push_segmented_open_polyline(&mut lins, &upper_opt, Float3::Z);
    push_segmented_open_polyline(&mut lins, &lower_opt, Float3::Z);
    let outline_val = mesh_from_parts(vec![], lins, vec![]);

    Ok(list_value([pos_val, neg_val, outline_val]))
}
