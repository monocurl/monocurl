use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};

use executor::{error::ExecutorError, executor::Executor, heap::with_heap, value::Value};
use geo::{mesh::Mesh, simd::Float3};
use stdlib_macros::stdlib_func;

use super::helpers::*;

#[stdlib_func]
pub async fn op_shift(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let delta = read_float3(executor, stack_idx, -1, "delta")?;
    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_scale(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let factor = crate::read_float(executor, stack_idx, -1, "factor")? as f32;
    tree.for_each_mut(&mut |mesh| {
        let center = mesh_center(mesh).unwrap_or(Float3::ZERO);
        transform_mesh_positions(mesh, |p| center + (p - center) * factor);
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_scale_xyz(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let factor = read_float3(executor, stack_idx, -1, "factors")?;
    tree.for_each_mut(&mut |mesh| {
        let center = mesh_center(mesh).unwrap_or(Float3::ZERO);
        transform_mesh_positions(mesh, |p| center + (p - center) * factor);
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_rotate(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let rotation = read_float3(executor, stack_idx, -1, "rotation")?;
    tree.for_each_mut(&mut |mesh| {
        let center = mesh_center(mesh).unwrap_or(Float3::ZERO);
        transform_mesh_positions(mesh, |p| rotate_point(p, center, rotation));
        for dot in &mut mesh.dots {
            dot.norm = rotate_point(dot.norm, Float3::ZERO, rotation);
        }
        for lin in &mut mesh.lins {
            lin.norm = rotate_point(lin.norm, Float3::ZERO, rotation);
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_rotate_around(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let pivot = read_float3(executor, stack_idx, -2, "pivot")?;
    let rotation = read_float3(executor, stack_idx, -1, "rotation")?;
    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| rotate_point(p, pivot, rotation)));
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_fade(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let alpha = crate::read_float(executor, stack_idx, -1, "opacity")?;
    tree.for_each_mut(&mut |mesh| {
        mesh.uniform.alpha *= alpha;
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_restroke(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let color = read_float4(executor, stack_idx, -1, "color")?;
    tree.for_each_mut(&mut |mesh| {
        for lin in &mut mesh.lins {
            lin.a.col = color;
            lin.b.col = color;
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_refill(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let color = read_float4(executor, stack_idx, -1, "color")?;
    tree.for_each_mut(&mut |mesh| {
        for tri in &mut mesh.tris {
            tri.a.col = color;
            tri.b.col = color;
            tri.c.col = color;
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_redot(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let color = read_float4(executor, stack_idx, -1, "color")?;
    tree.for_each_mut(&mut |mesh| {
        for dot in &mut mesh.dots {
            dot.col = color;
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_normal_hint(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let normal = read_float3(executor, stack_idx, -1, "normal")?;
    tree.for_each_mut(&mut |mesh| {
        for dot in &mut mesh.dots {
            dot.norm = normal;
        }
        for lin in &mut mesh.lins {
            lin.norm = normal;
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_retextured(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let image = read_string(executor, stack_idx, -1, "image")?;
    tree.for_each_mut(&mut |mesh| {
        mesh.uniform.img = Some(image.clone().into());
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_with_z(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let z_index = read_int(executor, stack_idx, -1, "index")?;
    tree.for_each_mut(&mut |mesh| {
        mesh.uniform.z_index = z_index as i32;
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_fixed_in_frame(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let fixed = read_flag(executor, stack_idx, -1, "fixed")?;
    tree.for_each_mut(&mut |mesh| {
        mesh.uniform.fixed_in_frame = fixed;
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_gloss(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    read_mesh_tree_arg(executor, stack_idx, -1, "target").await.map(MeshTree::into_value)
}

#[stdlib_func]
pub async fn op_point_map(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let mesh = Arc::make_mut(arc);
                    for dot in &mut mesh.dots {
                        dot.pos = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(dot.pos)], "f").await?,
                            "f",
                        )?;
                    }
                    for lin in &mut mesh.lins {
                        lin.a.pos = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(lin.a.pos)], "f").await?,
                            "f",
                        )?;
                        lin.b.pos = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(lin.b.pos)], "f").await?,
                            "f",
                        )?;
                    }
                    for tri in &mut mesh.tris {
                        tri.a.pos = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.a.pos)], "f").await?,
                            "f",
                        )?;
                        tri.b.pos = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.b.pos)], "f").await?,
                            "f",
                        )?;
                        tri.c.pos = float3_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.c.pos)], "f").await?,
                            "f",
                        )?;
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let func = executor.state.stack(stack_idx).read_at(-1).clone().elide_lvalue();
    recurse(executor, &mut tree, &func).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_color_map(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let mesh = Arc::make_mut(arc);
                    for dot in &mut mesh.dots {
                        dot.col = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(dot.pos)], "f").await?,
                            "f",
                        )?;
                    }
                    for lin in &mut mesh.lins {
                        lin.a.col = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(lin.a.pos)], "f").await?,
                            "f",
                        )?;
                        lin.b.col = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(lin.b.pos)], "f").await?,
                            "f",
                        )?;
                    }
                    for tri in &mut mesh.tris {
                        tri.a.col = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.a.pos)], "f").await?,
                            "f",
                        )?;
                        tri.b.col = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.b.pos)], "f").await?,
                            "f",
                        )?;
                        tri.c.col = float4_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.c.pos)], "f").await?,
                            "f",
                        )?;
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let func = executor.state.stack(stack_idx).read_at(-1).clone().elide_lvalue();
    recurse(executor, &mut tree, &func).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_uv_map(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let mesh = Arc::make_mut(arc);
                    for tri in &mut mesh.tris {
                        tri.a.uv = float2_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.a.pos)], "f").await?,
                            "f",
                        )?;
                        tri.b.uv = float2_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.b.pos)], "f").await?,
                            "f",
                        )?;
                        tri.c.uv = float2_from_value(
                            invoke_callable(executor, func, vec![point_value(tri.c.pos)], "f").await?,
                            "f",
                        )?;
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let func = executor.state.stack(stack_idx).read_at(-1).clone().elide_lvalue();
    recurse(executor, &mut tree, &func).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_retagged(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn parse_tags(value: Value) -> Result<Vec<isize>, ExecutorError> {
        match value.elide_lvalue_leader_rec() {
            Value::Integer(tag) => Ok(vec![tag as isize]),
            Value::Float(tag) if tag.fract() == 0.0 => Ok(vec![tag as isize]),
            Value::List(list) => list
                .elements()
                .iter()
                .map(|key| int_from_value(with_heap(|h| h.get(key.key()).clone()), "f").map(|tag| tag as isize))
                .collect(),
            other => Err(ExecutorError::type_error_for("int / list", other.type_name(), "f")),
        }
    }

    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let mesh = Arc::make_mut(arc);
                    let tags = list_value(mesh.tag.iter().copied().map(|tag| Value::Integer(tag as i64)));
                    mesh.tag = parse_tags(invoke_callable(executor, func, vec![tags], "f").await?)?;
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let func = executor.state.stack(stack_idx).read_at(-1).clone().elide_lvalue();
    recurse(executor, &mut tree, &func).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_tag_map(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: MeshTree,
        func: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(mesh) => {
                    let tags = list_value(mesh.tag.iter().copied().map(|tag| Value::Integer(tag as i64)));
                    invoke_callable(executor, func, vec![tags, Value::Mesh(mesh)], "f").await
                }
                MeshTree::List(children) => {
                    let mut out = Vec::with_capacity(children.len());
                    for child in children {
                        out.push(recurse(executor, child, func).await?);
                    }
                    Ok(list_value(out))
                }
            }
        })
    }

    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let func = executor.state.stack(stack_idx).read_at(-1).clone().elide_lvalue();
    recurse(executor, tree, &func).await
}

#[stdlib_func]
pub async fn op_uprank(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn ordered_loop_points(mesh: &Mesh) -> Option<Vec<Float3>> {
        if mesh.lins.is_empty() {
            return None;
        }
        let mut points = Vec::with_capacity(mesh.lins.len() + 1);
        points.push(mesh.lins[0].a.pos);
        points.push(mesh.lins[0].b.pos);
        for lin in mesh.lins.iter().skip(1) {
            if lin.a.pos == *points.last()? {
                points.push(lin.b.pos);
            } else if lin.b.pos == *points.last()? {
                points.push(lin.a.pos);
            } else {
                return None;
            }
        }
        if points.first() == points.last() {
            points.pop();
        }
        Some(points)
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -1, "target").await?;
    tree.for_each_mut(&mut |mesh| {
        if !mesh.tris.is_empty() {
            return;
        }
        if mesh.lins.is_empty() && mesh.dots.len() >= 2 {
            mesh.lins = mesh
                .dots
                .windows(2)
                .map(|pair| default_lin(pair[0].pos, pair[1].pos, pair[0].norm))
                .collect();
        }
        if let Some(points) = ordered_loop_points(mesh) {
            if points.len() >= 3 {
                mesh.tris = (1..points.len() - 1)
                    .map(|i| default_tri(points[0], points[i], points[i + 1]))
                    .collect();
            }
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_downrank(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -1, "target").await?;
    tree.for_each_mut(&mut |mesh| {
        if !mesh.tris.is_empty() {
            let mut seen = std::collections::HashSet::new();
            let mut lins = mesh.lins.clone();
            for tri in &mesh.tris {
                for (a, b) in [(tri.a.pos, tri.b.pos), (tri.b.pos, tri.c.pos), (tri.c.pos, tri.a.pos)] {
                    let key = canonical_edge_key(a, b);
                    if seen.insert(key) {
                        lins.push(default_lin(a, b, Float3::Z));
                    }
                }
            }
            mesh.tris.clear();
            mesh.lins = lins;
        } else if !mesh.lins.is_empty() {
            mesh.dots.extend(mesh.lins.iter().flat_map(|lin| [default_dot(lin.a.pos, lin.norm), default_dot(lin.b.pos, lin.norm)]));
            mesh.lins.clear();
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_wireframe(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    op_downrank(executor, stack_idx).await
}

#[stdlib_func]
pub async fn op_subdivide(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let factor = read_int(executor, stack_idx, -1, "factor")?.max(1) as usize;

    tree.for_each_mut(&mut |mesh| {
        if !mesh.tris.is_empty() {
            for _ in 1..factor {
                let mut tris = Vec::with_capacity(mesh.tris.len() * 4);
                for tri in &mesh.tris {
                    let ab = (tri.a.pos + tri.b.pos) / 2.0;
                    let bc = (tri.b.pos + tri.c.pos) / 2.0;
                    let ca = (tri.c.pos + tri.a.pos) / 2.0;
                    tris.push(default_tri(tri.a.pos, ab, ca));
                    tris.push(default_tri(ab, tri.b.pos, bc));
                    tris.push(default_tri(ca, bc, tri.c.pos));
                    tris.push(default_tri(ab, bc, ca));
                }
                mesh.tris = tris;
            }
        } else if !mesh.lins.is_empty() && factor > 1 {
            let mut lins = Vec::with_capacity(mesh.lins.len() * factor);
            for lin in &mesh.lins {
                for i in 0..factor {
                    let u = i as f32 / factor as f32;
                    let v = (i + 1) as f32 / factor as f32;
                    let a = lin.a.pos.lerp(lin.b.pos, u);
                    let b = lin.a.pos.lerp(lin.b.pos, v);
                    lins.push(default_lin(a, b, lin.norm));
                }
            }
            mesh.lins = lins;
        }
    });

    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_tesselated(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let depth = read_int(executor, stack_idx, -1, "depth")?.max(0) as usize;
    tree.for_each_mut(&mut |mesh| {
        for _ in 0..depth {
            if mesh.tris.is_empty() {
                break;
            }
            let mut tris = Vec::with_capacity(mesh.tris.len() * 4);
            for tri in &mesh.tris {
                let ab = (tri.a.pos + tri.b.pos) / 2.0;
                let bc = (tri.b.pos + tri.c.pos) / 2.0;
                let ca = (tri.c.pos + tri.a.pos) / 2.0;
                tris.push(default_tri(tri.a.pos, ab, ca));
                tris.push(default_tri(ab, tri.b.pos, bc));
                tris.push(default_tri(ca, bc, tri.c.pos));
                tris.push(default_tri(ab, bc, ca));
            }
            mesh.tris = tris;
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_extrude(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let delta = read_float3(executor, stack_idx, -1, "delta")?;

    tree.for_each_mut(&mut |mesh| {
        if mesh.tris.is_empty() && !mesh.lins.is_empty() {
            let points: Vec<_> = mesh.lins.iter().map(|lin| lin.a.pos).collect();
            if points.len() >= 3 {
                mesh.tris = (1..points.len() - 1)
                    .map(|i| default_tri(points[0], points[i], points[i + 1]))
                    .collect();
            }
        }
        if mesh.tris.is_empty() {
            return;
        }

        let base_tris = mesh.tris.clone();
        let mut edge_counts = HashMap::<_, usize>::new();
        let mut edge_dirs = HashMap::<_, (Float3, Float3)>::new();
        for tri in &base_tris {
            for (a, b) in [(tri.a.pos, tri.b.pos), (tri.b.pos, tri.c.pos), (tri.c.pos, tri.a.pos)] {
                *edge_counts.entry(canonical_edge_key(a, b)).or_default() += 1;
                edge_dirs.entry(canonical_edge_key(a, b)).or_insert((a, b));
            }
        }

        let mut tris = Vec::with_capacity(base_tris.len() * 4);
        tris.extend(base_tris.iter().copied());
        tris.extend(base_tris.iter().map(|tri| default_tri(tri.c.pos + delta, tri.b.pos + delta, tri.a.pos + delta)));

        for (key, count) in edge_counts {
            if count != 1 {
                continue;
            }
            let (a, b) = edge_dirs[&key];
            tris.push(default_tri(a, b, b + delta));
            tris.push(default_tri(a, b + delta, a + delta));
        }
        mesh.tris = tris;
    });

    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_revolve(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let rotation = read_float3(executor, stack_idx, -1, "rotation")?;
    let angle = rotation.len();
    let axis = if angle <= 1e-6 { Float3::Y } else { rotation / angle };
    let full_turn = angle >= std::f32::consts::TAU - 1e-3;
    let steps = ((angle.abs() / (std::f32::consts::TAU / 24.0)).ceil() as usize).max(1);

    tree.for_each_mut(&mut |mesh| {
        if mesh.lins.is_empty() {
            return;
        }
        let source = mesh.lins.clone();
        let mut tris = Vec::with_capacity(source.len() * steps * 2);
        for lin in &source {
            for step in 0..steps {
                let t0 = angle * step as f32 / steps as f32;
                let t1 = angle * (step + 1) as f32 / steps as f32;
                let a0 = rotate_about_axis(lin.a.pos, axis, t0);
                let b0 = rotate_about_axis(lin.b.pos, axis, t0);
                let a1 = if full_turn && step + 1 == steps {
                    lin.a.pos
                } else {
                    rotate_about_axis(lin.a.pos, axis, t1)
                };
                let b1 = if full_turn && step + 1 == steps {
                    lin.b.pos
                } else {
                    rotate_about_axis(lin.b.pos, axis, t1)
                };
                tris.push(default_tri(a0, b0, b1));
                tris.push(default_tri(a0, b1, a1));
            }
        }
        mesh.tris = tris;
        mesh.lins.clear();
    });

    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_centered(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let at = read_float3(executor, stack_idx, -1, "at")?;
    let center = tree_center(&tree).unwrap_or(Float3::ZERO);
    let delta = at - center;
    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_moved_to_side(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let side = read_float3(executor, stack_idx, -1, "dir")?;
    let center = tree_center(&tree).unwrap_or(Float3::ZERO);
    let right = extremal_point(&tree, Float3::X).unwrap_or(center).x;
    let left = extremal_point(&tree, -Float3::X).unwrap_or(center).x;
    let up = extremal_point(&tree, Float3::Y).unwrap_or(center).y;
    let down = extremal_point(&tree, -Float3::Y).unwrap_or(center).y;

    let mut delta = Float3::new(-center.x, -center.y, -center.z);
    delta.x = if side.x < 0.0 {
        -3.8 - left
    } else if side.x > 0.0 {
        3.8 - right
    } else {
        -center.x
    };
    delta.y = if side.y < 0.0 {
        -2.05 - down
    } else if side.y > 0.0 {
        2.05 - up
    } else {
        -center.y
    };
    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_matched_edge(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let reference = read_mesh_tree_arg(executor, stack_idx, -2, "ref").await?;
    let dir = read_float3(executor, stack_idx, -1, "dir")?.normalize();
    let our = extremal_point(&tree, dir).unwrap_or(Float3::ZERO).dot(dir);
    let their = extremal_point(&reference, dir).unwrap_or(Float3::ZERO).dot(dir);
    let delta = dir * (their - our);
    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_next_to(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let reference = read_mesh_tree_arg(executor, stack_idx, -2, "ref").await?;
    let dir = read_float3(executor, stack_idx, -1, "dir")?.normalize();
    let our_center = tree_center(&tree).unwrap_or(Float3::ZERO);
    let ref_center = tree_center(&reference).unwrap_or(Float3::ZERO);
    let our_face = extremal_point(&tree, -dir).unwrap_or(our_center).dot(dir);
    let ref_face = extremal_point(&reference, dir).unwrap_or(ref_center).dot(dir);
    let orth = (ref_center - our_center) - dir * (ref_center - our_center).dot(dir);
    let delta = dir * (ref_face - our_face + 0.1) + orth;
    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_projected(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let screen = read_mesh_tree_arg(executor, stack_idx, -2, "screen").await?;
    let ray = read_float3(executor, stack_idx, -1, "ray")?;
    let ray = if ray.len_sq() <= 1e-12 { Float3::Z } else { ray.normalize() };
    let screen_tris: Vec<_> = screen
        .iter()
        .flat_map(|mesh| mesh.tris.iter().map(|tri| (tri.a.pos, tri.b.pos, tri.c.pos)))
        .collect();

    let cast = |point: Float3| {
        screen_tris
            .iter()
            .filter_map(|&(a, b, c)| ray_triangle_intersection(point, ray, a, b, c).map(|t| (t, point + ray * t)))
            .min_by(|(ta, _), (tb, _)| ta.total_cmp(tb))
            .map(|(_, hit)| hit)
            .unwrap_or(point)
    };

    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, cast));
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_in_space(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -5, "target").await?;
    let axis_center = read_float3(executor, stack_idx, -4, "axis_center")?;
    let x_unit = read_float3(executor, stack_idx, -3, "x_unit")?;
    let y_unit = read_float3(executor, stack_idx, -2, "y_unit")?;
    let z_unit = read_float3(executor, stack_idx, -1, "z_unit")?;
    tree.for_each_mut(&mut |mesh| {
        transform_mesh_positions(mesh, |p| axis_center + x_unit * p.x + y_unit * p.y + z_unit * p.z)
    });
    Ok(tree.into_value())
}

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
