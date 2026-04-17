//! native implementations referenced from std.mesh. Most heavy geometry /
//! topology routines are still todo, but tree-aware mesh iteration,
//! lightweight styling/tagging operators, and simple mesh queries live here.

use std::{collections::HashSet, future::Future, pin::Pin, rc::Rc, sync::Arc};

use executor::{
    error::ExecutorError,
    executor::Executor,
    value::{Value, container::List, lambda::Lambda, rc_value},
};
use geo::{
    mesh::Mesh,
    simd::{Float3, Float4},
};
use stdlib_macros::stdlib_func;

use crate::read_float;

#[derive(Clone)]
enum MeshTree {
    Mesh(Arc<Mesh>),
    List(Vec<MeshTree>),
}

enum TagFilter {
    Exact(HashSet<isize>),
    Predicate(Rc<Lambda>),
}

impl MeshTree {
    fn iter(&self) -> MeshTreeIter<'_> {
        MeshTreeIter { stack: vec![self] }
    }

    fn for_each_mut(&mut self, f: &mut impl FnMut(&mut Mesh)) {
        match self {
            MeshTree::Mesh(arc) => f(Arc::make_mut(arc)),
            MeshTree::List(children) => {
                for child in children {
                    child.for_each_mut(f);
                }
            }
        }
    }

    fn into_value(self) -> Value {
        match self {
            MeshTree::Mesh(arc) => Value::Mesh(arc),
            MeshTree::List(children) => list_value(children.into_iter().map(MeshTree::into_value)),
        }
    }

    fn flatten(self) -> Vec<Arc<Mesh>> {
        let mut meshes = Vec::new();
        self.flatten_into(&mut meshes);
        meshes
    }

    fn flatten_into(self, out: &mut Vec<Arc<Mesh>>) {
        match self {
            MeshTree::Mesh(arc) => out.push(arc),
            MeshTree::List(children) => {
                for child in children {
                    child.flatten_into(out);
                }
            }
        }
    }
}

struct MeshTreeIter<'a> {
    stack: Vec<&'a MeshTree>,
}

impl<'a> Iterator for MeshTreeIter<'a> {
    type Item = &'a Mesh;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.stack.pop() {
            match node {
                MeshTree::Mesh(arc) => return Some(arc),
                MeshTree::List(children) => {
                    self.stack.extend(children.iter().rev());
                }
            }
        }
        None
    }
}

fn list_value(values: impl IntoIterator<Item = Value>) -> Value {
    Value::List(Rc::new(List {
        elements: values.into_iter().map(rc_value).collect(),
    }))
}

fn float_to_value(value: f64) -> Value {
    if value.fract() == 0.0 {
        Value::Integer(value as i64)
    } else {
        Value::Float(value)
    }
}

fn point_value(point: Float3) -> Value {
    list_value(
        point
            .to_array()
            .into_iter()
            .map(|v| float_to_value(v as f64)),
    )
}

fn edge_value(a: Float3, b: Float3) -> Value {
    list_value([point_value(a), point_value(b)])
}

fn triangle_value(a: Float3, b: Float3, c: Float3) -> Value {
    list_value([point_value(a), point_value(b), point_value(c)])
}

fn read_string(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<String, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
    {
        Value::String(value) => Ok(value),
        other => Err(ExecutorError::type_error_for(
            "string",
            other.type_name(),
            name,
        )),
    }
}

fn read_int(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<i64, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
    {
        Value::Integer(value) => Ok(value),
        Value::Float(value) if value.fract() == 0.0 => Ok(value as i64),
        other => Err(ExecutorError::type_error_for(
            "int",
            other.type_name(),
            name,
        )),
    }
}

fn read_flag(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<bool, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue()
    {
        Value::Integer(value) => Ok(value != 0),
        Value::Float(value) => Ok(value != 0.0),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            name,
        )),
    }
}

fn read_float3(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Float3, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_rec()
    {
        Value::List(list) if list.elements.len() == 3 => {
            let mut components = [0.0; 3];
            for (slot, value) in components.iter_mut().zip(&list.elements) {
                *slot = match &*value.borrow() {
                    Value::Integer(n) => *n as f32,
                    Value::Float(f) => *f as f32,
                    other => {
                        return Err(ExecutorError::type_error_for(
                            "number",
                            other.type_name(),
                            name,
                        ));
                    }
                };
            }
            Ok(Float3::from_array(components))
        }
        Value::List(list) => Err(ExecutorError::Other(format!(
            "{}: expected 3-vector, got list of length {}",
            name,
            list.elements.len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "3-vector",
            other.type_name(),
            name,
        )),
    }
}

fn read_float4(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Float4, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_rec()
    {
        Value::List(list) if list.elements.len() == 4 => {
            let mut components = [0.0; 4];
            for (slot, value) in components.iter_mut().zip(&list.elements) {
                *slot = match &*value.borrow() {
                    Value::Integer(n) => *n as f32,
                    Value::Float(f) => *f as f32,
                    other => {
                        return Err(ExecutorError::type_error_for(
                            "number",
                            other.type_name(),
                            name,
                        ));
                    }
                };
            }
            Ok(Float4::from_array(components))
        }
        Value::List(list) => Err(ExecutorError::Other(format!(
            "{}: expected 4-vector, got list of length {}",
            name,
            list.elements.len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "4-vector",
            other.type_name(),
            name,
        )),
    }
}

fn read_mesh_tree<'a>(
    executor: &'a mut Executor,
    value: Value,
    name: &'static str,
) -> Pin<Box<dyn Future<Output = Result<MeshTree, ExecutorError>> + 'a>> {
    Box::pin(async move {
        let value = value.elide_wrappers(executor).await?;
        match value {
            Value::Mesh(arc) => Ok(MeshTree::Mesh(arc)),
            Value::List(list) => {
                let mut children = Vec::with_capacity(list.elements.len());
                for child in &list.elements {
                    children.push(read_mesh_tree(executor, child.borrow().clone(), name).await?);
                }
                Ok(MeshTree::List(children))
            }
            other => Err(ExecutorError::type_error_for(
                "mesh / list of meshes",
                other.type_name(),
                name,
            )),
        }
    })
}

async fn read_mesh_tree_arg(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<MeshTree, ExecutorError> {
    let value = executor.state.stack(stack_idx).read_at(index).clone();
    read_mesh_tree(executor, value, name).await
}

fn read_tag_filter(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<TagFilter, ExecutorError> {
    let value = executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue();

    match value {
        Value::Lambda(lambda) => Ok(TagFilter::Predicate(lambda)),
        value => Ok(TagFilter::Exact(match value.elide_lvalue_rec() {
            Value::List(list) => list
                .elements
                .iter()
                .map(|value| match &*value.borrow() {
                    Value::Integer(tag) => Ok(*tag as isize),
                    Value::Float(tag) if tag.fract() == 0.0 => Ok(*tag as isize),
                    other => Err(ExecutorError::type_error_for(
                        "int",
                        other.type_name(),
                        name,
                    )),
                })
                .collect::<Result<HashSet<_>, _>>()?,
            Value::Integer(tag) => HashSet::from([tag as isize]),
            Value::Float(tag) if tag.fract() == 0.0 => HashSet::from([tag as isize]),
            other => {
                return Err(ExecutorError::type_error_for(
                    "int / list / lambda",
                    other.type_name(),
                    name,
                ));
            }
        })),
    }
}

fn axis_aligned_rank(mesh: &Mesh) -> i64 {
    if !mesh.tris.is_empty() {
        2
    } else if !mesh.lins.is_empty() {
        1
    } else if !mesh.dots.is_empty() {
        0
    } else {
        -1
    }
}

fn mesh_vertices(mesh: &Mesh) -> impl Iterator<Item = Float3> + '_ {
    mesh.dots
        .iter()
        .map(|dot| dot.pos)
        .chain(mesh.lins.iter().flat_map(|lin| [lin.a.pos, lin.b.pos]))
        .chain(
            mesh.tris
                .iter()
                .flat_map(|tri| [tri.a.pos, tri.b.pos, tri.c.pos]),
        )
}

fn float3_key(point: Float3) -> [u32; 3] {
    [point.x.to_bits(), point.y.to_bits(), point.z.to_bits()]
}

fn canonical_edge_key(a: Float3, b: Float3) -> ([u32; 3], [u32; 3]) {
    let a_key = float3_key(a);
    let b_key = float3_key(b);
    if a_key <= b_key {
        (a_key, b_key)
    } else {
        (b_key, a_key)
    }
}

fn triangle_key(a: Float3, b: Float3, c: Float3) -> [[u32; 3]; 3] {
    let mut key = [float3_key(a), float3_key(b), float3_key(c)];
    key.sort_unstable();
    key
}

fn bounds_of(tree: &MeshTree) -> Option<(Float3, Float3)> {
    let mut vertices = tree.iter().flat_map(mesh_vertices);
    let first = vertices.next()?;
    Some(vertices.fold((first, first), |(mut min, mut max), point| {
        min.x = min.x.min(point.x);
        min.y = min.y.min(point.y);
        min.z = min.z.min(point.z);
        max.x = max.x.max(point.x);
        max.y = max.y.max(point.y);
        max.z = max.z.max(point.z);
        (min, max)
    }))
}

fn extremal_point(tree: &MeshTree, direction: Float3) -> Option<Float3> {
    let mut vertices = tree.iter().flat_map(mesh_vertices);
    let first = vertices.next()?;
    Some(vertices.fold(first, |best: Float3, point: Float3| {
        if point.dot(direction) > best.dot(direction) {
            point
        } else {
            best
        }
    }))
}

fn require_point(point: Option<Float3>, name: &'static str) -> Result<Value, ExecutorError> {
    point
        .map(point_value)
        .ok_or_else(|| ExecutorError::InvalidArgument {
            arg: name,
            message: "mesh tree must contain at least one vertex",
        })
}

fn filter_tree_by_tag(tree: MeshTree, tags: &HashSet<isize>) -> Option<MeshTree> {
    match tree {
        MeshTree::Mesh(mesh) => mesh
            .tag
            .iter()
            .any(|tag| tags.contains(tag))
            .then_some(MeshTree::Mesh(mesh)),
        MeshTree::List(children) => {
            let filtered: Vec<_> = children
                .into_iter()
                .filter_map(|child| filter_tree_by_tag(child, tags))
                .collect();
            (!filtered.is_empty()).then_some(MeshTree::List(filtered))
        }
    }
}

async fn mesh_matches_tag_filter(
    executor: &mut Executor,
    filter: &TagFilter,
    mesh: &Mesh,
) -> Result<bool, ExecutorError> {
    match filter {
        TagFilter::Exact(tags) => Ok(mesh.tag.iter().any(|tag| tags.contains(tag))),
        TagFilter::Predicate(lambda) => {
            for &tag in &mesh.tag {
                let value = executor
                    .invoke_lambda(lambda, vec![Value::Integer(tag as i64)])
                    .await?;
                let value = value.elide_wrappers(executor).await?;
                if value.check_truthy()? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

fn filter_tree_by_tag_filter<'a>(
    executor: &'a mut Executor,
    tree: MeshTree,
    filter: &'a TagFilter,
) -> Pin<Box<dyn Future<Output = Result<Option<MeshTree>, ExecutorError>> + 'a>> {
    Box::pin(async move {
        match tree {
            tree @ MeshTree::Mesh(_) => match filter {
                TagFilter::Exact(tags) => Ok(filter_tree_by_tag(tree, tags)),
                TagFilter::Predicate(_) => {
                    let MeshTree::Mesh(mesh) = tree else {
                        unreachable!()
                    };
                    let keep = mesh_matches_tag_filter(executor, filter, &mesh).await?;
                    Ok(keep.then_some(MeshTree::Mesh(mesh)))
                }
            },
            MeshTree::List(children) => {
                if let TagFilter::Exact(tags) = filter {
                    return Ok(filter_tree_by_tag(MeshTree::List(children), tags));
                }

                let mut filtered = Vec::new();
                for child in children {
                    if let Some(child) = filter_tree_by_tag_filter(executor, child, filter).await? {
                        filtered.push(child);
                    }
                }
                Ok((!filtered.is_empty()).then_some(MeshTree::List(filtered)))
            }
        }
    })
}

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
pub async fn op_fade(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let alpha = read_float(executor, stack_idx, -1, "opacity")?;
    tree.for_each_mut(&mut |mesh| {
        mesh.uniform.alpha *= alpha;
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_restroke(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
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
pub async fn op_normal_hint(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let normal = read_float3(executor, stack_idx, -1, "normal")?;
    tree.for_each_mut(&mut |mesh| {
        for lin in &mut mesh.lins {
            lin.norm = normal;
        }
    });
    Ok(tree.into_value())
}

#[stdlib_func]
pub async fn op_retextured(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
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
pub async fn op_fixed_in_frame(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let mut tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let fixed = read_flag(executor, stack_idx, -1, "fixed")?;
    tree.for_each_mut(&mut |mesh| {
        mesh.uniform.fixed_in_frame = fixed;
    });
    Ok(tree.into_value())
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
pub async fn tag_filter(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "target").await?;
    let filter = read_tag_filter(executor, stack_idx, -1, "filter")?;
    Ok(filter_tree_by_tag_filter(executor, tree, &filter)
        .await?
        .map(MeshTree::into_value)
        .unwrap_or_else(|| list_value([])))
}

#[stdlib_func]
pub async fn mesh_left(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(-1.0, 0.0, 0.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_right(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(1.0, 0.0, 0.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_up(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(0.0, 1.0, 0.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_down(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(0.0, -1.0, 0.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_forward(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(0.0, 0.0, 1.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_backward(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    require_point(extremal_point(&tree, Float3::new(0.0, 0.0, -1.0)), "mesh")
}

#[stdlib_func]
pub async fn mesh_direc(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -2, "mesh").await?;
    let direction = read_float3(executor, stack_idx, -1, "direc")?;
    require_point(extremal_point(&tree, direction), "mesh")
}

#[stdlib_func]
pub async fn mesh_width(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let (min, max) = bounds_of(&tree).ok_or(ExecutorError::InvalidArgument {
        arg: "mesh",
        message: "mesh tree must contain at least one vertex",
    })?;
    Ok(Value::Float((max.x - min.x) as f64))
}

#[stdlib_func]
pub async fn mesh_height(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let (min, max) = bounds_of(&tree).ok_or(ExecutorError::InvalidArgument {
        arg: "mesh",
        message: "mesh tree must contain at least one vertex",
    })?;
    Ok(Value::Float((max.y - min.y) as f64))
}

#[stdlib_func]
pub async fn mesh_center(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let (min, max) = bounds_of(&tree).ok_or(ExecutorError::InvalidArgument {
        arg: "mesh",
        message: "mesh tree must contain at least one vertex",
    })?;
    Ok(point_value((min + max) / 2.0))
}

#[stdlib_func]
pub async fn mesh_rank(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    Ok(Value::Integer(
        tree.iter().map(axis_aligned_rank).max().unwrap_or(-1),
    ))
}

#[stdlib_func]
pub async fn mesh_tags(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for mesh in tree.iter() {
        for &tag in &mesh.tag {
            if seen.insert(tag) {
                tags.push(Value::Integer(tag as i64));
            }
        }
    }
    Ok(list_value(tags))
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
pub async fn mesh_vertex_set(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let mut seen = HashSet::new();
    let mut vertices = Vec::new();
    for mesh in tree.iter() {
        for vertex in mesh_vertices(mesh) {
            if seen.insert(float3_key(vertex)) {
                vertices.push(point_value(vertex));
            }
        }
    }
    Ok(list_value(vertices))
}

#[stdlib_func]
pub async fn mesh_edge_set(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let mut seen = HashSet::new();
    let mut edges = Vec::new();
    for mesh in tree.iter() {
        for lin in &mesh.lins {
            let key = canonical_edge_key(lin.a.pos, lin.b.pos);
            if seen.insert(key) {
                edges.push(edge_value(lin.a.pos, lin.b.pos));
            }
        }
        for tri in &mesh.tris {
            for (a, b) in [
                (tri.a.pos, tri.b.pos),
                (tri.b.pos, tri.c.pos),
                (tri.c.pos, tri.a.pos),
            ] {
                let key = canonical_edge_key(a, b);
                if seen.insert(key) {
                    edges.push(edge_value(a, b));
                }
            }
        }
    }
    Ok(list_value(edges))
}

#[stdlib_func]
pub async fn mesh_triangle_set(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    let mut seen = HashSet::new();
    let mut triangles = Vec::new();
    for mesh in tree.iter() {
        for tri in &mesh.tris {
            let key = triangle_key(tri.a.pos, tri.b.pos, tri.c.pos);
            if seen.insert(key) {
                triangles.push(triangle_value(tri.a.pos, tri.b.pos, tri.c.pos));
            }
        }
    }
    Ok(list_value(triangles))
}

#[stdlib_func]
pub async fn mesh_contour_count(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    Ok(Value::Integer(tree.iter().count() as i64))
}

#[stdlib_func]
pub async fn mesh_contour_separate(
    executor: &mut Executor,
    stack_idx: usize,
) -> Result<Value, ExecutorError> {
    let tree = read_mesh_tree_arg(executor, stack_idx, -1, "mesh").await?;
    Ok(list_value(tree.flatten().into_iter().map(Value::Mesh)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::mesh::{Dot, Uniforms};

    fn dot_mesh(pos: [f32; 3], tag: &[isize]) -> Mesh {
        Mesh {
            dots: vec![Dot {
                pos: Float3::from_array(pos),
                norm: Float3::Z,
                col: Float4::new(1.0, 1.0, 1.0, 1.0),
                inv: -1,
                anti: -1,
                is_dom_sib: false,
            }],
            lins: vec![],
            tris: vec![],
            uniform: Uniforms::default(),
            tag: tag.to_vec(),
        }
    }

    #[test]
    fn mesh_tree_iter_walks_nested_lists() {
        let tree = MeshTree::List(vec![
            MeshTree::Mesh(Arc::new(dot_mesh([0.0, 0.0, 0.0], &[1]))),
            MeshTree::List(vec![
                MeshTree::Mesh(Arc::new(dot_mesh([1.0, 0.0, 0.0], &[2]))),
                MeshTree::Mesh(Arc::new(dot_mesh([2.0, 0.0, 0.0], &[3]))),
            ]),
        ]);

        let positions: Vec<_> = tree.iter().map(|mesh| mesh.dots[0].pos.x).collect();
        assert_eq!(positions, vec![0.0, 1.0, 2.0]);
    }

    #[test]
    fn mesh_tree_iter_mut_updates_all_leaves() {
        let mut tree = MeshTree::List(vec![
            MeshTree::Mesh(Arc::new(dot_mesh([0.0, 0.0, 0.0], &[1]))),
            MeshTree::List(vec![MeshTree::Mesh(Arc::new(dot_mesh([1.0, 0.0, 0.0], &[2])))]),
        ]);

        tree.for_each_mut(&mut |mesh| {
            mesh.uniform.alpha = 0.25;
        });

        let alphas: Vec<_> = tree.iter().map(|mesh| mesh.uniform.alpha).collect();
        assert_eq!(alphas, vec![0.25, 0.25]);
    }

    #[test]
    fn tag_filter_keeps_only_matching_meshes() {
        let tree = MeshTree::List(vec![
            MeshTree::Mesh(Arc::new(dot_mesh([0.0, 0.0, 0.0], &[1]))),
            MeshTree::List(vec![
                MeshTree::Mesh(Arc::new(dot_mesh([1.0, 0.0, 0.0], &[2]))),
                MeshTree::Mesh(Arc::new(dot_mesh([2.0, 0.0, 0.0], &[3, 4]))),
            ]),
        ]);

        let filtered = filter_tree_by_tag(tree, &HashSet::from([2, 4])).unwrap();
        let positions: Vec<_> = filtered.iter().map(|mesh| mesh.dots[0].pos.x).collect();
        assert_eq!(positions, vec![1.0, 2.0]);
    }
}
