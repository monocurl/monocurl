use std::{future::Future, pin::Pin};

use executor::{
    error::ExecutorError, executor::Executor, heap::with_heap, kernel::BatchInput, value::Value,
};
use geo::mesh::make_mesh_mut;
use stdlib_macros::stdlib_func;

use crate::mesh::helpers::*;

use super::*;

#[stdlib_func]
pub async fn op_point_map(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let (key, hit) = crate::mesh::result_cache::lookup(
        executor,
        stack_idx,
        4,
        crate::mesh::constructors::CACHE_TAG_POINT_MAP,
    );
    if let Some(value) = hit {
        return Ok(value);
    }
    let value = op_point_map_uncached(executor, stack_idx).await?;
    crate::mesh::result_cache::remember(key, &value);
    Ok(value)
}

async fn op_point_map_uncached(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
        level: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if !keep {
                        return Ok(());
                    }
                    let positions = {
                        let mesh = arc.as_ref();
                        let mut positions = Vec::with_capacity(
                            mesh.dots.len() + mesh.lins.len() * 2 + mesh.tris.len() * 3,
                        );
                        positions.extend(mesh.dots.iter().map(|dot| dot.pos));
                        for lin in &mesh.lins {
                            positions.push(lin.a.pos);
                            positions.push(lin.b.pos);
                        }
                        for tri in &mesh.tris {
                            positions.push(tri.a.pos);
                            positions.push(tri.b.pos);
                            positions.push(tri.c.pos);
                        }
                        positions
                    };
                    let mapped = invoke_callable_many_input(
                        executor,
                        func,
                        BatchInput::Points(&positions),
                        "f",
                        float3_from_kernel,
                        |value| float3_from_value(value, "f"),
                    )
                    .await?;
                    let mesh = make_mesh_mut(arc);
                    let mut mapped_iter = mapped.into_iter();
                    for dot in &mut mesh.dots {
                        let original = dot.pos;
                        dot.pos = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    for lin in &mut mesh.lins {
                        let original = lin.a.pos;
                        lin.a.pos = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = lin.b.pos;
                        lin.b.pos = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    for tri in &mut mesh.tris {
                        let original = tri.a.pos;
                        tri.a.pos = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.b.pos;
                        tri.b.pos = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.c.pos;
                        tri.c.pos = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func, filter, level).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-3)
        .clone()
        .elide_lvalue();
    recurse(executor, &mut tree, &func, filter.as_ref(), level).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_color_map(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let (key, hit) = crate::mesh::result_cache::lookup(
        executor,
        stack_idx,
        4,
        crate::mesh::constructors::CACHE_TAG_COLOR_MAP,
    );
    if let Some(value) = hit {
        return Ok(value);
    }
    let value = op_color_map_uncached(executor, stack_idx).await?;
    crate::mesh::result_cache::remember(key, &value);
    Ok(value)
}

async fn op_color_map_uncached(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
        level: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if !keep {
                        return Ok(());
                    }
                    let positions = {
                        let mesh = arc.as_ref();
                        let mut positions = Vec::with_capacity(
                            mesh.dots.len() + mesh.lins.len() * 2 + mesh.tris.len() * 3,
                        );
                        positions.extend(mesh.dots.iter().map(|dot| dot.pos));
                        for lin in &mesh.lins {
                            positions.push(lin.a.pos);
                            positions.push(lin.b.pos);
                        }
                        for tri in &mesh.tris {
                            positions.push(tri.a.pos);
                            positions.push(tri.b.pos);
                            positions.push(tri.c.pos);
                        }
                        positions
                    };
                    let mapped = invoke_callable_many_input(
                        executor,
                        func,
                        BatchInput::Points(&positions),
                        "f",
                        float4_from_kernel,
                        |value| float4_from_value(value, "f"),
                    )
                    .await?;
                    let mesh = make_mesh_mut(arc);
                    let mut mapped_iter = mapped.into_iter();
                    for dot in &mut mesh.dots {
                        let original = dot.col;
                        dot.col = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    for lin in &mut mesh.lins {
                        let original = lin.a.col;
                        lin.a.col = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = lin.b.col;
                        lin.b.col = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    for tri in &mut mesh.tris {
                        let original = tri.a.col;
                        tri.a.col = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.b.col;
                        tri.b.col = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.c.col;
                        tri.c.col = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func, filter, level).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-3)
        .clone()
        .elide_lvalue();
    recurse(executor, &mut tree, &func, filter.as_ref(), level).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_uv_map(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
        level: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if !keep {
                        return Ok(());
                    }
                    let positions = {
                        let mesh = arc.as_ref();
                        let mut positions = Vec::with_capacity(mesh.tris.len() * 3);
                        for tri in &mesh.tris {
                            positions.push(tri.a.pos);
                            positions.push(tri.b.pos);
                            positions.push(tri.c.pos);
                        }
                        positions
                    };
                    let mapped = invoke_callable_many_input(
                        executor,
                        func,
                        BatchInput::Points(&positions),
                        "f",
                        float2_from_kernel,
                        |value| float2_from_value(value, "f"),
                    )
                    .await?;
                    let mesh = make_mesh_mut(arc);
                    let mut mapped_iter = mapped.into_iter();
                    for tri in &mut mesh.tris {
                        let original = tri.a.uv;
                        tri.a.uv = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.b.uv;
                        tri.b.uv = original.lerp(mapped_iter.next().unwrap(), level);

                        let original = tri.c.uv;
                        tri.c.uv = original.lerp(mapped_iter.next().unwrap(), level);
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func, filter, level).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -4, "target").await?;
    let level = read_level(executor, stack_idx, -1, "level")?;
    if level <= 0.0 {
        return Ok(tree.into_value());
    }
    let filter = read_optional_tag_filter(executor, stack_idx, -2, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-3)
        .clone()
        .elide_lvalue();
    recurse(executor, &mut tree, &func, filter.as_ref(), level).await?;
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_retagged(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    fn parse_tags(value: Value) -> Result<Vec<isize>, ExecutorError> {
        match value.elide_lvalue_leader_rec() {
            Value::Integer(tag) => Ok(vec![tag as isize]),
            Value::Float(tag) if tag.fract() == 0.0 => Ok(vec![tag as isize]),
            Value::List(list) => list
                .elements()
                .iter()
                .map(|key| {
                    int_from_value(with_heap(|h| h.get(key.key()).clone()), "f")
                        .map(|tag| tag as isize)
                })
                .collect(),
            other => Err(ExecutorError::type_error_for(
                "int / list",
                other.type_name(),
                "f",
            )),
        }
    }

    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: &'a mut MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if !keep {
                        return Ok(());
                    }
                    let mesh = make_mesh_mut(arc);
                    let tags = list_value(
                        mesh.tag
                            .iter()
                            .copied()
                            .map(|tag| Value::Integer(tag as i64)),
                    );
                    mesh.tag = parse_tags(invoke_callable(executor, func, vec![tags], "f").await?)?;
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        recurse(executor, child, func, filter).await?;
                    }
                    Ok(())
                }
            }
        })
    }

    let mut tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let filter = read_optional_tag_filter(executor, stack_idx, -1, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-2)
        .clone()
        .elide_lvalue();
    recurse(executor, &mut tree, &func, filter.as_ref()).await?;
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
                    let tags = list_value(
                        mesh.tag
                            .iter()
                            .copied()
                            .map(|tag| Value::Integer(tag as i64)),
                    );
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
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();
    recurse(executor, tree, &func).await
}

#[stdlib_func]
pub async fn op_subset_map(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    fn recurse<'a>(
        executor: &'a mut Executor,
        tree: MeshTree,
        func: &'a Value,
        filter: Option<&'a TagFilter>,
    ) -> Pin<Box<dyn Future<Output = Result<MeshTree, ExecutorError>> + 'a>> {
        Box::pin(async move {
            match tree {
                MeshTree::Mesh(mesh) => {
                    let keep = match filter {
                        Some(filter) => mesh_matches_tag_filter(executor, filter, &mesh).await?,
                        None => true,
                    };
                    if !keep {
                        return Ok(MeshTree::Mesh(mesh));
                    }

                    let mapped =
                        invoke_callable(executor, func, vec![Value::Mesh(mesh.clone())], "f")
                            .await?;
                    read_mesh_tree(executor, mapped, "f").await
                }
                MeshTree::List(children) => {
                    let mut mapped = Vec::with_capacity(children.len());
                    for child in children {
                        mapped.push(recurse(executor, child, func, filter).await?);
                    }
                    Ok(MeshTree::List(mapped))
                }
            }
        })
    }

    let tree = read_mesh_tree_arg(executor, stack_idx, -3, "target").await?;
    let filter = read_tag_filter(executor, stack_idx, -2, "filter")?;
    let func = executor
        .state
        .stack(stack_idx)
        .read_at(-1)
        .clone()
        .elide_lvalue();
    recurse(executor, tree, &func, Some(&filter))
        .await
        .map(MeshTree::into_value)
}
