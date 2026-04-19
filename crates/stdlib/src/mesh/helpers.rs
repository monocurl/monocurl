use std::{collections::HashSet, future::Future, pin::Pin, rc::Rc, sync::Arc};

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{VRc, with_heap},
    value::{Value, container::List, lambda::Lambda},
};
use geo::{
    mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
    simd::{Float2, Float3, Float4},
};

#[derive(Clone)]
pub(super) enum MeshTree {
    Mesh(Arc<Mesh>),
    List(Vec<MeshTree>),
}

pub(super) enum TagFilter {
    Exact(HashSet<isize>),
    Predicate(Rc<Lambda>),
}

impl MeshTree {
    pub(super) fn iter(&self) -> MeshTreeIter<'_> {
        MeshTreeIter { stack: vec![self] }
    }

    pub(super) fn for_each_mut(&mut self, f: &mut impl FnMut(&mut Mesh)) {
        match self {
            MeshTree::Mesh(arc) => f(Arc::make_mut(arc)),
            MeshTree::List(children) => {
                for child in children {
                    child.for_each_mut(f);
                }
            }
        }
    }

    pub(super) fn into_value(self) -> Value {
        match self {
            MeshTree::Mesh(arc) => Value::Mesh(arc),
            MeshTree::List(children) => list_value(children.into_iter().map(MeshTree::into_value)),
        }
    }

    pub(super) fn flatten(self) -> Vec<Arc<Mesh>> {
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

pub(super) struct MeshTreeIter<'a> {
    stack: Vec<&'a MeshTree>,
}

impl<'a> Iterator for MeshTreeIter<'a> {
    type Item = &'a Mesh;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(node) = self.stack.pop() {
            match node {
                MeshTree::Mesh(arc) => return Some(arc),
                MeshTree::List(children) => self.stack.extend(children.iter().rev()),
            }
        }
        None
    }
}

pub(super) fn list_value(values: impl IntoIterator<Item = Value>) -> Value {
    Value::List(Rc::new(List::new_with(
        values.into_iter().map(VRc::new).collect(),
    )))
}

pub(super) fn float_to_value(value: f64) -> Value {
    if value.fract() == 0.0 {
        Value::Integer(value as i64)
    } else {
        Value::Float(value)
    }
}

pub(super) fn point_value(point: Float3) -> Value {
    list_value(
        point
            .to_array()
            .into_iter()
            .map(|v| float_to_value(v as f64)),
    )
}

pub(super) fn edge_value(a: Float3, b: Float3) -> Value {
    list_value([point_value(a), point_value(b)])
}

pub(super) fn triangle_value(a: Float3, b: Float3, c: Float3) -> Value {
    list_value([point_value(a), point_value(b), point_value(c)])
}

pub(super) fn read_string(
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

pub(super) fn read_int(
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

pub(super) fn read_flag(
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

pub(super) fn read_float3(
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
        .elide_lvalue_leader_rec()
    {
        Value::List(list) if list.elements().len() == 3 => {
            let mut components = [0.0; 3];
            for (slot, key) in components.iter_mut().zip(list.elements().iter()) {
                *slot = match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => n as f32,
                    Value::Float(f) => f as f32,
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
            "{}: expected list of length 3, got list of length {}",
            name,
            list.elements().len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "3-vector",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn float2_from_value(value: Value, name: &'static str) -> Result<Float2, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::List(list) if list.elements().len() == 2 => {
            let mut components = [0.0; 2];
            for (slot, key) in components.iter_mut().zip(list.elements().iter()) {
                *slot = match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => n as f32,
                    Value::Float(f) => f as f32,
                    other => {
                        return Err(ExecutorError::type_error_for(
                            "number",
                            other.type_name(),
                            name,
                        ));
                    }
                };
            }
            Ok(Float2::from_array(components))
        }
        other => Err(ExecutorError::type_error_for(
            "2-vector",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn float3_from_value(value: Value, name: &'static str) -> Result<Float3, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::List(list) if list.elements().len() == 3 => {
            let mut components = [0.0; 3];
            for (slot, key) in components.iter_mut().zip(list.elements().iter()) {
                *slot = match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => n as f32,
                    Value::Float(f) => f as f32,
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
        other => Err(ExecutorError::type_error_for(
            "3-vector",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn float4_from_value(value: Value, name: &'static str) -> Result<Float4, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::List(list) if list.elements().len() == 4 => {
            let mut components = [0.0; 4];
            for (slot, key) in components.iter_mut().zip(list.elements().iter()) {
                *slot = match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => n as f32,
                    Value::Float(f) => f as f32,
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
        other => Err(ExecutorError::type_error_for(
            "4-vector",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn int_from_value(value: Value, name: &'static str) -> Result<i64, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::Integer(n) => Ok(n),
        Value::Float(f) if f.fract() == 0.0 => Ok(f as i64),
        other => Err(ExecutorError::type_error_for(
            "int",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn read_float3_list(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Vec<Float3>, ExecutorError> {
    let Value::List(list) = executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    else {
        let value = executor
            .state
            .stack(stack_idx)
            .read_at(index)
            .clone()
            .elide_lvalue_leader_rec();
        return Err(ExecutorError::type_error_for(
            "list",
            value.type_name(),
            name,
        ));
    };

    list.elements()
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let value = with_heap(|h| h.get(key.key()).clone());
            match value.elide_lvalue_leader_rec() {
                Value::List(inner) if inner.elements().len() == 3 => {
                    let mut components = [0.0; 3];
                    for (slot, key) in components.iter_mut().zip(inner.elements().iter()) {
                        *slot = match with_heap(|h| h.get(key.key()).clone()) {
                            Value::Integer(n) => n as f32,
                            Value::Float(f) => f as f32,
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
                other => Err(ExecutorError::Other(format!(
                    "{}[{}]: expected list of length 3, got {}",
                    name,
                    i,
                    other.type_name()
                ))),
            }
        })
        .collect()
}

pub(super) fn read_float4(
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
        .elide_lvalue_leader_rec()
    {
        Value::List(list) if list.elements().len() == 4 => {
            let mut components = [0.0; 4];
            for (slot, key) in components.iter_mut().zip(list.elements().iter()) {
                *slot = match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => n as f32,
                    Value::Float(f) => f as f32,
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
            "{}: expected list of length 4, got list of length {}",
            name,
            list.elements().len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "4-vector",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn default_dot(pos: Float3, norm: Float3) -> Dot {
    Dot {
        pos,
        norm,
        col: Float4::ONE,
        inv: -1,
        anti: -1,
        is_dom_sib: false,
    }
}

pub(super) fn default_lin(a: Float3, b: Float3, norm: Float3) -> Lin {
    Lin {
        a: LinVertex {
            pos: a,
            col: Float4::ONE,
        },
        b: LinVertex {
            pos: b,
            col: Float4::ONE,
        },
        norm,
        prev: -1,
        next: -1,
        inv: -1,
        anti: -1,
        is_dom_sib: false,
    }
}

pub(super) fn default_tri(a: Float3, b: Float3, c: Float3) -> Tri {
    let uv_a = Float2::new(0.0, 0.0);
    let uv_b = Float2::new(1.0, 0.0);
    let uv_c = Float2::new(0.0, 1.0);
    Tri {
        a: TriVertex {
            pos: a,
            col: Float4::ONE,
            uv: uv_a,
        },
        b: TriVertex {
            pos: b,
            col: Float4::ONE,
            uv: uv_b,
        },
        c: TriVertex {
            pos: c,
            col: Float4::ONE,
            uv: uv_c,
        },
        ab: -1,
        bc: -1,
        ca: -1,
        anti: -1,
        is_dom_sib: false,
    }
}

pub(super) fn mesh_from_parts(dots: Vec<Dot>, lins: Vec<Lin>, tris: Vec<Tri>) -> Value {
    Value::Mesh(Arc::new(Mesh {
        dots,
        lins,
        tris,
        uniform: Uniforms::default(),
        tag: vec![],
    }))
}

pub(super) fn polygon_basis(normal: Float3) -> (Float3, Float3, Float3) {
    let normal = if normal.len_sq() == 0.0 {
        Float3::Z
    } else {
        normal.normalize()
    };
    let seed = if normal.z.abs() < 0.9 {
        Float3::Z
    } else {
        Float3::X
    };
    let x = normal.cross(seed).normalize();
    let y = normal.cross(x).normalize();
    (x, y, normal)
}

pub(super) fn mesh_vertices(mesh: &Mesh) -> impl Iterator<Item = Float3> + '_ {
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

pub(super) fn read_mesh_tree<'a>(
    executor: &'a mut Executor,
    value: Value,
    name: &'static str,
) -> Pin<Box<dyn Future<Output = Result<MeshTree, ExecutorError>> + 'a>> {
    Box::pin(async move {
        let value = value.elide_wrappers(executor).await?;
        match value {
            Value::Mesh(arc) => Ok(MeshTree::Mesh(arc)),
            Value::List(list) => {
                let mut children = Vec::with_capacity(list.elements().len());
                for key in list.elements() {
                    let val = with_heap(|h| h.get(key.key()).clone());
                    children.push(read_mesh_tree(executor, val, name).await?);
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

pub(super) async fn read_mesh_tree_arg(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<MeshTree, ExecutorError> {
    let value = executor.state.stack(stack_idx).read_at(index).clone();
    read_mesh_tree(executor, value, name).await
}

pub(super) async fn read_mesh_tree_list_arg(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Vec<MeshTree>, ExecutorError> {
    let value = executor.state.stack(stack_idx).read_at(index).clone();
    let value = value.elide_wrappers(executor).await?;
    let Value::List(list) = value else {
        return Err(ExecutorError::type_error_for(
            "list",
            value.type_name(),
            name,
        ));
    };
    let mut out = Vec::with_capacity(list.len());
    for key in list.elements() {
        let value = with_heap(|h| h.get(key.key()).clone());
        out.push(read_mesh_tree(executor, value, name).await?);
    }
    Ok(out)
}

pub(super) fn read_tag_filter(
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
        value => Ok(TagFilter::Exact(match value.elide_lvalue_leader_rec() {
            Value::List(list) => list
                .elements()
                .iter()
                .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(tag) => Ok(tag as isize),
                    Value::Float(tag) if tag.fract() == 0.0 => Ok(tag as isize),
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

pub(super) fn axis_aligned_rank(mesh: &Mesh) -> i64 {
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

pub(super) fn float3_key(point: Float3) -> [u32; 3] {
    [point.x.to_bits(), point.y.to_bits(), point.z.to_bits()]
}

pub(super) fn canonical_edge_key(a: Float3, b: Float3) -> ([u32; 3], [u32; 3]) {
    let a_key = float3_key(a);
    let b_key = float3_key(b);
    if a_key <= b_key {
        (a_key, b_key)
    } else {
        (b_key, a_key)
    }
}

pub(super) fn triangle_key(a: Float3, b: Float3, c: Float3) -> [[u32; 3]; 3] {
    let mut key = [float3_key(a), float3_key(b), float3_key(c)];
    key.sort_unstable();
    key
}

pub(super) fn bounds_of(tree: &MeshTree) -> Option<(Float3, Float3)> {
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

pub(super) fn extremal_point(tree: &MeshTree, direction: Float3) -> Option<Float3> {
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

pub(super) fn tree_center(tree: &MeshTree) -> Option<Float3> {
    bounds_of(tree).map(|(min, max)| (min + max) / 2.0)
}

pub(super) fn require_point(
    point: Option<Float3>,
    name: &'static str,
) -> Result<Value, ExecutorError> {
    point
        .map(point_value)
        .ok_or_else(|| ExecutorError::InvalidArgument {
            arg: name,
            message: "mesh tree must contain at least one vertex",
        })
}

pub(super) fn filter_tree_by_tag(tree: MeshTree, tags: &HashSet<isize>) -> Option<MeshTree> {
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

pub(super) async fn mesh_matches_tag_filter(
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

pub(super) async fn invoke_callable(
    executor: &mut Executor,
    callable: &Value,
    args: Vec<Value>,
    name: &'static str,
) -> Result<Value, ExecutorError> {
    let raw = match callable.clone().elide_lvalue() {
        Value::Lambda(lambda) => executor.invoke_lambda(&lambda, args).await?,
        Value::Operator(operator) => executor.invoke_lambda(&operator.0, args).await?,
        other => {
            return Err(ExecutorError::type_error_for(
                "lambda / operator",
                other.type_name(),
                name,
            ));
        }
    };
    raw.elide_wrappers(executor).await
}

pub(super) fn filter_tree_by_tag_filter<'a>(
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

pub(super) fn rotate_point(point: Float3, pivot: Float3, rotation: Float3) -> Float3 {
    let mut p = point - pivot;

    let (sx, cx) = rotation.x.sin_cos();
    p = Float3::new(p.x, cx * p.y - sx * p.z, sx * p.y + cx * p.z);

    let (sy, cy) = rotation.y.sin_cos();
    p = Float3::new(cy * p.x + sy * p.z, p.y, -sy * p.x + cy * p.z);

    let (sz, cz) = rotation.z.sin_cos();
    p = Float3::new(cz * p.x - sz * p.y, sz * p.x + cz * p.y, p.z);

    p + pivot
}

pub(super) fn rotate_about_axis(point: Float3, axis: Float3, angle: f32) -> Float3 {
    if axis.len_sq() <= 1e-12 || angle.abs() <= 1e-12 {
        return point;
    }
    let axis = axis.normalize();
    let (s, c) = angle.sin_cos();
    point * c + axis.cross(point) * s + axis * axis.dot(point) * (1.0 - c)
}

pub(super) fn transform_mesh_positions(mesh: &mut Mesh, mut f: impl FnMut(Float3) -> Float3) {
    for dot in &mut mesh.dots {
        dot.pos = f(dot.pos);
    }
    for lin in &mut mesh.lins {
        lin.a.pos = f(lin.a.pos);
        lin.b.pos = f(lin.b.pos);
    }
    for tri in &mut mesh.tris {
        tri.a.pos = f(tri.a.pos);
        tri.b.pos = f(tri.b.pos);
        tri.c.pos = f(tri.c.pos);
    }
}

pub(super) fn mesh_center(mesh: &Mesh) -> Option<Float3> {
    let first = mesh_vertices(mesh).next()?;
    let (sum, count) =
        mesh_vertices(mesh).fold((first, 1usize), |(sum, count), p| (sum + p, count + 1));
    Some(sum / count as f32)
}

pub(super) fn triangle_normal(a: Float3, b: Float3, c: Float3) -> Float3 {
    let cross = (b - a).cross(c - a);
    if cross.len_sq() <= 1e-12 {
        Float3::Z
    } else {
        cross.normalize()
    }
}

pub(super) fn segment_distance(a: Float3, b: Float3, point: Float3) -> f32 {
    let delta = b - a;
    let len_sq = delta.len_sq();
    if len_sq <= 1e-12 {
        return (point - a).len();
    }
    let t = ((point - a).dot(delta) / len_sq).clamp(0.0, 1.0);
    (a + delta * t - point).len()
}

pub(super) fn ray_triangle_intersection(
    origin: Float3,
    direction: Float3,
    a: Float3,
    b: Float3,
    c: Float3,
) -> Option<f32> {
    let ab = b - a;
    let ac = c - a;
    let pvec = direction.cross(ac);
    let det = ab.dot(pvec);
    if det.abs() <= 1e-6 {
        return None;
    }
    let inv_det = 1.0 / det;
    let tvec = origin - a;
    let u = tvec.dot(pvec) * inv_det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let qvec = tvec.cross(ab);
    let v = direction.dot(qvec) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = ac.dot(qvec) * inv_det;
    (t >= 0.0).then_some(t)
}

pub(super) fn set_triangle_uv_rect(
    mesh: &mut Mesh,
    min: Float3,
    max: Float3,
    basis_x: Float3,
    basis_y: Float3,
) {
    let dx = (max - min).dot(basis_x).abs().max(1e-6);
    let dy = (max - min).dot(basis_y).abs().max(1e-6);
    for tri in &mut mesh.tris {
        for vertex in [&mut tri.a, &mut tri.b, &mut tri.c] {
            let rel = vertex.pos - min;
            vertex.uv = Float2::new(rel.dot(basis_x) / dx, rel.dot(basis_y) / dy);
        }
    }
}
