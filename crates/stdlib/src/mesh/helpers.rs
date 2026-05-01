use std::{
    collections::{HashMap, HashSet},
    future::Future,
    ops::Range,
    path::{Path, PathBuf},
    pin::Pin,
    rc::Rc,
    sync::Arc,
};

use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{VRc, with_heap},
    value::{Value, container::List, lambda::Lambda},
};
use geo::{
    mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms, make_mesh_mut},
    mesh_build::{self, BoundaryEdge, IndexedLineMesh, IndexedSurface, SurfaceVertex},
    simd::{Float2, Float3, Float4},
};
use libtess2::{TessellationOptions, WindingRule};

const NORMAL_EPSILON: f32 = 1e-6;

fn default_ink() -> Float4 {
    Float4::new(0.0, 0.0, 0.0, 1.0)
}

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
            MeshTree::Mesh(arc) => f(make_mesh_mut(arc)),
            MeshTree::List(children) => {
                for child in children {
                    child.for_each_mut(f);
                }
            }
        }
    }

    pub(super) fn for_each_filtered<'a, F>(
        &'a mut self,
        executor: &'a mut Executor,
        filter: Option<&'a TagFilter>,
        f: &'a mut F,
    ) -> Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>>
    where
        F: FnMut(&mut Mesh) + 'a,
    {
        Box::pin(async move {
            match self {
                MeshTree::Mesh(arc) => {
                    let keep = match filter {
                        Some(filter) => {
                            mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?
                        }
                        None => true,
                    };
                    if keep {
                        f(make_mesh_mut(arc));
                    }
                    Ok(())
                }
                MeshTree::List(children) => {
                    for child in children {
                        child.for_each_filtered(executor, filter, f).await?;
                    }
                    Ok(())
                }
            }
        })
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
    Value::List(List::new_with(values.into_iter().map(VRc::new).collect()))
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

pub(super) async fn read_string(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<String, ExecutorError> {
    crate::stringify_value(
        executor,
        executor.state.stack(stack_idx).read_at(index).clone(),
    )
    .await
    .map_err(|error| match error {
        ExecutorError::TypeError { got, .. } => {
            ExecutorError::type_error_for(crate::STRING_COMPATIBLE_DESC, got, name)
        }
        other => other,
    })
}

fn image_source_path<'a>(executor: &'a Executor, stack_idx: usize) -> Option<&'a Path> {
    let mut fallback = None;
    let mut cursor = Some(stack_idx);

    while let Some(idx) = cursor {
        let current_section = executor.section_bytecode(executor.state.stack_ip(idx).0 as usize);
        if let Some(path) = current_section.source_file_path.as_deref() {
            if !current_section.flags.is_stdlib {
                return Some(path);
            }
            fallback.get_or_insert(path);
        }

        for ip in executor.state.stack_call_stack(idx).iter().rev() {
            let caller_section = executor.section_bytecode(ip.0 as usize);
            if let Some(path) = caller_section.source_file_path.as_deref() {
                if !caller_section.flags.is_stdlib {
                    return Some(path);
                }
                fallback.get_or_insert(path);
            }
        }

        cursor = executor
            .state
            .stack_trace_parent_idx(idx)
            .or_else(|| executor.state.stack_parent_idx(idx));
    }

    fallback
}

pub(super) fn resolve_image_path(
    executor: &Executor,
    stack_idx: usize,
    raw_path: &str,
) -> Result<PathBuf, ExecutorError> {
    let image_path = PathBuf::from(raw_path);
    if image_path.is_absolute() {
        return Ok(image_path);
    }

    let Some(source_path) = image_source_path(executor, stack_idx) else {
        return Err(ExecutorError::invalid_operation(format!(
            "relative image path '{}' requires a scene file path",
            image_path.display()
        )));
    };
    if source_path.as_os_str().is_empty() {
        return Err(ExecutorError::invalid_operation(format!(
            "relative image path '{}' requires a scene file path",
            image_path.display()
        )));
    }

    let base_dir = source_path.parent().unwrap_or(Path::new(""));
    Ok(base_dir.join(image_path))
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
        .elide_cached_wrappers_rec()
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
        .elide_cached_wrappers_rec()
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
        .elide_cached_wrappers_rec()
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
        Value::List(list) => Err(ExecutorError::invalid_operation(format!(
            "{}: expected list of length 3, got list of length {}",
            name,
            list.elements().len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "list of length 3",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn float2_from_value(value: Value, name: &'static str) -> Result<Float2, ExecutorError> {
    match value.elide_cached_wrappers_rec() {
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
            "list of length 2",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn float3_from_value(value: Value, name: &'static str) -> Result<Float3, ExecutorError> {
    match value.elide_cached_wrappers_rec() {
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
            "list of length 3",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn float4_from_value(value: Value, name: &'static str) -> Result<Float4, ExecutorError> {
    match value.elide_cached_wrappers_rec() {
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
            "list of length 4",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn int_from_value(value: Value, name: &'static str) -> Result<i64, ExecutorError> {
    match value.elide_cached_wrappers_rec() {
        Value::Integer(n) => Ok(n),
        Value::Float(f) if f.fract() == 0.0 => Ok(f as i64),
        other => Err(ExecutorError::type_error_for(
            "int",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn read_tags(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Vec<isize>, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_cached_wrappers_rec()
    {
        Value::Integer(tag) => Ok(vec![tag as isize]),
        Value::Float(tag) if tag.fract() == 0.0 => Ok(vec![tag as isize]),
        Value::List(list) => list
            .elements()
            .iter()
            .map(|key| {
                int_from_value(with_heap(|h| h.get(key.key()).clone()), name)
                    .map(|tag| tag as isize)
            })
            .collect(),
        other => Err(ExecutorError::type_error_for(
            "int / list",
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
        .elide_cached_wrappers_rec()
    else {
        let value = executor
            .state
            .stack(stack_idx)
            .read_at(index)
            .clone()
            .elide_cached_wrappers_rec();
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
            match value.elide_cached_wrappers_rec() {
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
                other => Err(ExecutorError::invalid_operation(format!(
                    "{}[{}]: expected list of length 3, got {}",
                    name,
                    i,
                    other.type_name()
                ))),
            }
        })
        .collect()
}

pub(super) async fn read_float4(
    executor: &mut Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Float4, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_wrappers_rec(executor)
        .await?
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
        Value::List(list) => Err(ExecutorError::invalid_operation(format!(
            "{}: expected list of length 4, got list of length {}",
            name,
            list.elements().len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "list of length 4",
            other.type_name(),
            name,
        )),
    }
}

pub(super) fn default_dot(pos: Float3, norm: Float3) -> Dot {
    Dot {
        pos,
        norm,
        col: default_ink(),
        inv: -1,
        is_dom_sib: false,
    }
}

pub(super) fn default_lin(a: Float3, b: Float3, norm: Float3) -> Lin {
    Lin {
        a: LinVertex {
            pos: a,
            col: default_ink(),
        },
        b: LinVertex {
            pos: b,
            col: default_ink(),
        },
        norm,
        prev: -1,
        next: -1,
        inv: -1,
        is_dom_sib: true,
    }
}

pub(super) fn default_tri(a: Float3, b: Float3, c: Float3) -> Tri {
    let uv_a = Float2::new(0.0, 0.0);
    let uv_b = Float2::new(1.0, 0.0);
    let uv_c = Float2::new(0.0, 1.0);
    Tri {
        a: TriVertex {
            pos: a,
            col: default_ink(),
            uv: uv_a,
        },
        b: TriVertex {
            pos: b,
            col: default_ink(),
            uv: uv_b,
        },
        c: TriVertex {
            pos: c,
            col: default_ink(),
            uv: uv_c,
        },
        ab: -1,
        bc: -1,
        ca: -1,
        is_dom_sib: false,
    }
}

#[derive(Debug)]
struct SlotDsu {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl SlotDsu {
    fn new(len: usize) -> Self {
        Self {
            parent: (0..len).collect(),
            rank: vec![0; len],
        }
    }

    fn find(&mut self, idx: usize) -> usize {
        let parent = self.parent[idx];
        if parent == idx {
            idx
        } else {
            let root = self.find(parent);
            self.parent[idx] = root;
            root
        }
    }

    fn union(&mut self, a: usize, b: usize) {
        let mut ra = self.find(a);
        let mut rb = self.find(b);
        if ra == rb {
            return;
        }
        if self.rank[ra] < self.rank[rb] {
            std::mem::swap(&mut ra, &mut rb);
        }
        self.parent[rb] = ra;
        if self.rank[ra] == self.rank[rb] {
            self.rank[ra] += 1;
        }
    }
}

fn decode_mesh_ref(value: i32) -> Option<usize> {
    (value < -1).then_some((-value - 2) as usize)
}

fn dot_slot(idx: usize) -> usize {
    idx
}

fn line_a_slot(mesh: &Mesh, idx: usize) -> usize {
    mesh.dots.len() + idx * 2
}

fn line_b_slot(mesh: &Mesh, idx: usize) -> usize {
    line_a_slot(mesh, idx) + 1
}

fn tri_a_slot(mesh: &Mesh, idx: usize) -> usize {
    mesh.dots.len() + mesh.lins.len() * 2 + idx * 3
}

fn tri_b_slot(mesh: &Mesh, idx: usize) -> usize {
    tri_a_slot(mesh, idx) + 1
}

fn tri_c_slot(mesh: &Mesh, idx: usize) -> usize {
    tri_a_slot(mesh, idx) + 2
}

fn tri_edge_slots(mesh: &Mesh, tri_idx: usize, edge_idx: usize) -> (usize, usize) {
    match edge_idx {
        0 => (tri_a_slot(mesh, tri_idx), tri_b_slot(mesh, tri_idx)),
        1 => (tri_b_slot(mesh, tri_idx), tri_c_slot(mesh, tri_idx)),
        _ => (tri_c_slot(mesh, tri_idx), tri_a_slot(mesh, tri_idx)),
    }
}

fn tri_edge_for(tri: &Tri, value: i32) -> Option<usize> {
    [tri.ab, tri.bc, tri.ca]
        .iter()
        .position(|edge| *edge == value)
}

pub(crate) fn mesh_position_groups(mesh: &Mesh) -> Vec<usize> {
    let slot_count = mesh.dots.len() + mesh.lins.len() * 2 + mesh.tris.len() * 3;
    let mut dsu = SlotDsu::new(slot_count);

    for (idx, dot) in mesh.dots.iter().enumerate() {
        if dot.inv >= 0 {
            let inv = dot.inv as usize;
            if inv < mesh.dots.len() {
                dsu.union(dot_slot(idx), dot_slot(inv));
            }
        } else if let Some(line_idx) = decode_mesh_ref(dot.inv).filter(|&i| i < mesh.lins.len()) {
            let line = mesh.lins[line_idx];
            if line.a.pos == dot.pos {
                dsu.union(dot_slot(idx), line_a_slot(mesh, line_idx));
            }
            if line.b.pos == dot.pos {
                dsu.union(dot_slot(idx), line_b_slot(mesh, line_idx));
            }
        }
    }

    for (idx, lin) in mesh.lins.iter().enumerate() {
        if lin.prev >= 0 {
            let prev = lin.prev as usize;
            if prev < mesh.lins.len() {
                dsu.union(line_a_slot(mesh, idx), line_b_slot(mesh, prev));
            }
        } else if let Some(dot_idx) = decode_mesh_ref(lin.prev).filter(|&i| i < mesh.dots.len()) {
            dsu.union(line_a_slot(mesh, idx), dot_slot(dot_idx));
        }

        if lin.next >= 0 {
            let next = lin.next as usize;
            if next < mesh.lins.len() {
                dsu.union(line_b_slot(mesh, idx), line_a_slot(mesh, next));
            }
        } else if let Some(dot_idx) = decode_mesh_ref(lin.next).filter(|&i| i < mesh.dots.len()) {
            dsu.union(line_b_slot(mesh, idx), dot_slot(dot_idx));
        }

        if lin.inv >= 0 {
            let inv = lin.inv as usize;
            if inv < mesh.lins.len() {
                dsu.union(line_a_slot(mesh, idx), line_b_slot(mesh, inv));
                dsu.union(line_b_slot(mesh, idx), line_a_slot(mesh, inv));
            }
        }
    }

    for (tri_idx, tri) in mesh.tris.iter().enumerate() {
        for (edge_idx, value) in [tri.ab, tri.bc, tri.ca].into_iter().enumerate() {
            let (lhs, rhs) = tri_edge_slots(mesh, tri_idx, edge_idx);
            if value >= 0 {
                let neighbor = value as usize;
                if neighbor < mesh.tris.len() {
                    if let Some(other_edge) = tri_edge_for(&mesh.tris[neighbor], tri_idx as i32) {
                        let (na, nb) = tri_edge_slots(mesh, neighbor, other_edge);
                        dsu.union(lhs, nb);
                        dsu.union(rhs, na);
                    }
                }
            } else if let Some(line_idx) = decode_mesh_ref(value).filter(|&i| i < mesh.lins.len()) {
                dsu.union(lhs, line_a_slot(mesh, line_idx));
                dsu.union(rhs, line_b_slot(mesh, line_idx));
            }
        }
    }

    let mut groups = Vec::with_capacity(slot_count);
    let mut root_to_group = HashMap::<usize, usize>::new();
    for slot in 0..slot_count {
        let root = dsu.find(slot);
        let next_group = root_to_group.len();
        groups.push(*root_to_group.entry(root).or_insert(next_group));
    }
    groups
}

pub(super) fn mesh_ref(idx: usize) -> i32 {
    mesh_build::mesh_ref(idx)
}

pub(super) fn push_open_polyline(
    out: &mut Vec<Lin>,
    points: &[Float3],
    normal: Float3,
) -> Range<usize> {
    mesh_build::push_open_polyline(out, points, normal, default_ink())
}

pub(crate) fn push_closed_polyline(
    out: &mut Vec<Lin>,
    points: &[Float3],
    normal: Float3,
) -> Range<usize> {
    mesh_build::push_closed_polyline(out, points, normal, default_ink())
}

pub(crate) fn build_indexed_surface(
    vertices: &[SurfaceVertex],
    faces: &[[usize; 3]],
    boundary_edges: &HashMap<(usize, usize), BoundaryEdge>,
) -> (Vec<Lin>, Vec<Tri>) {
    mesh_build::build_indexed_surface(vertices, faces, boundary_edges)
}

pub(super) fn build_indexed_tris(vertices: &[Float3], faces: &[[usize; 3]]) -> Vec<Tri> {
    mesh_build::build_indexed_tris(vertices, faces, default_ink())
}

pub(crate) fn mesh_to_indexed_surface(mesh: &Mesh) -> IndexedSurface {
    let groups = mesh_position_groups(mesh);
    let group_count = groups
        .iter()
        .copied()
        .max()
        .map(|group| group + 1)
        .unwrap_or(0);
    let mut vertices = vec![
        SurfaceVertex {
            pos: Float3::ZERO,
            col: default_ink(),
            uv: Float2::ZERO,
        };
        group_count
    ];
    let mut seen = vec![false; group_count];

    let mut assign_vertex = |group: usize, pos: Float3, col: Float4, uv: Float2| {
        if !seen[group] {
            seen[group] = true;
            vertices[group] = SurfaceVertex { pos, col, uv };
        }
    };

    for (idx, dot) in mesh.dots.iter().enumerate() {
        assign_vertex(groups[dot_slot(idx)], dot.pos, dot.col, Float2::ZERO);
    }
    for (idx, tri) in mesh.tris.iter().enumerate() {
        assign_vertex(
            groups[tri_a_slot(mesh, idx)],
            tri.a.pos,
            tri.a.col,
            tri.a.uv,
        );
        assign_vertex(
            groups[tri_b_slot(mesh, idx)],
            tri.b.pos,
            tri.b.col,
            tri.b.uv,
        );
        assign_vertex(
            groups[tri_c_slot(mesh, idx)],
            tri.c.pos,
            tri.c.col,
            tri.c.uv,
        );
    }
    for (idx, lin) in mesh.lins.iter().enumerate() {
        assign_vertex(
            groups[line_a_slot(mesh, idx)],
            lin.a.pos,
            lin.a.col,
            Float2::ZERO,
        );
        assign_vertex(
            groups[line_b_slot(mesh, idx)],
            lin.b.pos,
            lin.b.col,
            Float2::ZERO,
        );
    }

    let faces = mesh
        .tris
        .iter()
        .enumerate()
        .map(|(idx, _)| {
            [
                groups[tri_a_slot(mesh, idx)],
                groups[tri_b_slot(mesh, idx)],
                groups[tri_c_slot(mesh, idx)],
            ]
        })
        .collect();

    let boundary_edges = mesh
        .lins
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            decode_mesh_ref(line.inv).map(|_| {
                (
                    (
                        groups[line_a_slot(mesh, idx)],
                        groups[line_b_slot(mesh, idx)],
                    ),
                    BoundaryEdge {
                        a_col: line.a.col,
                        b_col: line.b.col,
                        norm: line.norm,
                    },
                )
            })
        })
        .collect();

    IndexedSurface {
        vertices,
        faces,
        boundary_edges,
    }
}

pub(super) fn mesh_to_indexed_lines(mesh: &Mesh) -> IndexedLineMesh {
    let groups = mesh_position_groups(mesh);
    let group_count = groups
        .iter()
        .copied()
        .max()
        .map(|group| group + 1)
        .unwrap_or(0);
    let mut vertices = vec![
        SurfaceVertex {
            pos: Float3::ZERO,
            col: default_ink(),
            uv: Float2::ZERO,
        };
        group_count
    ];
    let mut seen = vec![false; group_count];

    let mut assign_vertex = |group: usize, pos: Float3, col: Float4| {
        if !seen[group] {
            seen[group] = true;
            vertices[group] = SurfaceVertex {
                pos,
                col,
                uv: Float2::ZERO,
            };
        }
    };

    for (idx, dot) in mesh
        .dots
        .iter()
        .enumerate()
        .filter(|(_, dot)| dot.is_dom_sib && dot.col.w > f32::EPSILON)
    {
        assign_vertex(groups[dot_slot(idx)], dot.pos, dot.col);
    }
    for (idx, line) in mesh
        .lins
        .iter()
        .enumerate()
        .filter(|(_, line)| line.is_dom_sib)
    {
        assign_vertex(groups[line_a_slot(mesh, idx)], line.a.pos, line.a.col);
        assign_vertex(groups[line_b_slot(mesh, idx)], line.b.pos, line.b.col);
    }

    let segments = mesh
        .lins
        .iter()
        .enumerate()
        .filter(|(_, line)| line.is_dom_sib)
        .map(|(idx, _)| {
            [
                groups[line_a_slot(mesh, idx)],
                groups[line_b_slot(mesh, idx)],
            ]
        })
        .collect();

    IndexedLineMesh { vertices, segments }
}

pub(crate) fn tessellate_planar_loops(
    contours: &[Vec<Float3>],
    normal: Float3,
) -> Result<(Vec<Lin>, Vec<Tri>), ExecutorError> {
    tessellate_planar_loops_with_options(contours, normal, false)
}

fn tessellate_planar_loops_with_options(
    contours: &[Vec<Float3>],
    normal: Float3,
    normalize_input: bool,
) -> Result<(Vec<Lin>, Vec<Tri>), ExecutorError> {
    let contours: Vec<_> = contours
        .iter()
        .filter(|contour| contour.len() >= 3)
        .cloned()
        .collect();
    if contours.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let normal = resolve_planar_normal(&contours, normal);

    let mut source_boundary_edges = HashMap::<(usize, usize), BoundaryEdge>::new();
    let mut source_offset = 0usize;
    for contour in &contours {
        for i in 0..contour.len() {
            let a = source_offset + i;
            let b = source_offset + (i + 1) % contour.len();
            let edge = BoundaryEdge {
                a_col: default_ink(),
                b_col: default_ink(),
                norm: normal,
            };
            source_boundary_edges.insert((a, b), edge);
            source_boundary_edges.insert(
                (b, a),
                BoundaryEdge {
                    a_col: edge.b_col,
                    b_col: edge.a_col,
                    norm: edge.norm,
                },
            );
        }
        source_offset += contour.len();
    }

    let tess = libtess2::triangulate(
        contours.iter().map(Vec::as_slice),
        TessellationOptions {
            winding_rule: WindingRule::NonZero,
            normal: Some(normal),
            constrained_delaunay: true,
            reverse_contours: false,
            normalize_input,
        },
    )
    .map_err(|error| {
        ExecutorError::invalid_operation(format!("failed to tessellate polygon: {error}"))
    })?;

    let surface_vertices: Vec<_> = tess
        .vertices
        .iter()
        .copied()
        .map(|pos| SurfaceVertex {
            pos,
            col: default_ink(),
            uv: Float2::ZERO,
        })
        .collect();

    let mut boundary_edges = HashMap::<(usize, usize), BoundaryEdge>::new();
    for face in &tess.triangles {
        for (a, b) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])].into_iter() {
            let edge = match (tess.source_vertex_indices[a], tess.source_vertex_indices[b]) {
                (Some(source_a), Some(source_b)) => source_boundary_edges
                    .get(&(source_a, source_b))
                    .copied()
                    .unwrap_or(BoundaryEdge {
                        a_col: default_ink(),
                        b_col: default_ink(),
                        norm: normal,
                    }),
                _ => BoundaryEdge {
                    a_col: default_ink(),
                    b_col: default_ink(),
                    norm: normal,
                },
            };
            boundary_edges.insert((a, b), edge);
        }
    }

    Ok(build_indexed_surface(
        &surface_vertices,
        &tess.triangles,
        &boundary_edges,
    ))
}

fn resolve_planar_normal(contours: &[Vec<Float3>], requested: Float3) -> Float3 {
    normalize_nonzero(requested)
        .or_else(|| contour_area_normal(contours))
        .unwrap_or(Float3::Z)
}

fn first_nonzero_line_normal(lines: &[Lin]) -> Option<Float3> {
    lines.iter().find_map(|line| normalize_nonzero(line.norm))
}

fn contour_area_normal(contours: &[Vec<Float3>]) -> Option<Float3> {
    let normal =
        contours
            .iter()
            .filter(|contour| contour.len() >= 3)
            .fold(Float3::ZERO, |acc, contour| {
                acc + (0..contour.len()).fold(Float3::ZERO, |sum, idx| {
                    sum + contour[idx].cross(contour[(idx + 1) % contour.len()])
                })
            });
    normalize_nonzero(normal)
}

fn normalize_nonzero(vec: Float3) -> Option<Float3> {
    let len = vec.len();
    (len > NORMAL_EPSILON).then_some(vec / len)
}

fn closed_line_contours(mesh: &Mesh) -> Option<Vec<Vec<Float3>>> {
    let primary_lines: Vec<_> = mesh
        .lins
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| line.is_dom_sib.then_some(idx))
        .collect();
    if primary_lines.is_empty()
        || primary_lines
            .iter()
            .any(|&idx| mesh.lins[idx].prev < 0 || mesh.lins[idx].next < 0)
    {
        return None;
    }

    let mut visited = vec![false; mesh.lins.len()];
    let mut contours = Vec::new();
    for start in primary_lines {
        if visited[start] {
            continue;
        }

        let mut contour = Vec::new();
        let mut cursor = start;
        loop {
            if visited[cursor] {
                return None;
            }
            visited[cursor] = true;
            contour.push(mesh.lins[cursor].a.pos);

            let next = mesh.lins[cursor].next as usize;
            if next >= mesh.lins.len()
                || !mesh.lins[next].is_dom_sib
                || mesh.lins[next].prev != cursor as i32
            {
                return None;
            }
            cursor = next;
            if cursor == start {
                break;
            }
        }

        if contour.len() >= 3 {
            contours.push(contour);
        }
    }

    Some(contours)
}

pub(crate) fn uprank_mesh(mesh: &Mesh) -> Result<Option<Mesh>, ExecutorError> {
    let mut out = mesh.clone();
    if !out.tris.is_empty() {
        return Ok(Some(out));
    }

    if out.lins.is_empty() && out.dots.len() >= 2 {
        out.lins = out
            .dots
            .windows(2)
            .map(|pair| default_lin(pair[0].pos, pair[1].pos, pair[0].norm))
            .collect();
    }

    let Some(contours) = closed_line_contours(&out) else {
        return Ok(None);
    };

    let normal = first_nonzero_line_normal(&out.lins)
        .or_else(|| contour_area_normal(&contours))
        .unwrap_or(Float3::Z);
    let (lins, tris) = tessellate_planar_loops_with_options(&contours, normal, true)?;
    out.lins = lins;
    out.tris = tris;
    out.debug_assert_consistent_topology();
    Ok(Some(out))
}

pub(super) fn mesh_from_parts(dots: Vec<Dot>, lins: Vec<Lin>, tris: Vec<Tri>) -> Value {
    mesh_from_parts_with_dot_radius(dots, lins, tris, 0.0)
}

pub(super) fn mesh_from_parts_with_dot_radius(
    dots: Vec<Dot>,
    lins: Vec<Lin>,
    tris: Vec<Tri>,
    dot_radius: f32,
) -> Value {
    let mut uniform = Uniforms::default();
    uniform.dot_radius = dot_radius;
    let mut mesh = Mesh {
        dots,
        lins,
        tris,
        uniform,
        tag: vec![],
        version: Mesh::fresh_version(),
    };
    mesh.normalize_line_dot_topology();
    mesh.debug_assert_consistent_topology();
    Value::Mesh(Arc::new(mesh))
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
        Float3::Y
    };
    let x = seed.cross(normal).normalize();
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
        let value = value.elide_wrappers_rec(executor).await?;
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
            Value::Stateful(ref s) => {
                let resolved = executor.eval_stateful(s).await?;
                read_mesh_tree(executor, resolved, name).await
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
    let value = value.elide_wrappers_rec(executor).await?;
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
        value => Ok(TagFilter::Exact(match value.elide_cached_wrappers_rec() {
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

/// returns `None` for nil (match all), otherwise delegates to `read_tag_filter`.
pub(super) fn read_optional_tag_filter(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Option<TagFilter>, ExecutorError> {
    let value = executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_cached_wrappers_rec();
    if matches!(value, Value::Nil) {
        return Ok(None);
    }
    read_tag_filter(executor, stack_idx, index, name).map(Some)
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

fn pack_tree_children(children: Vec<MeshTree>) -> Option<MeshTree> {
    match children.len() {
        0 => None,
        1 => children.into_iter().next(),
        _ => Some(MeshTree::List(children)),
    }
}

pub(super) fn split_tree_by_tag(
    tree: MeshTree,
    tags: &HashSet<isize>,
) -> (Option<MeshTree>, Option<MeshTree>) {
    match tree {
        MeshTree::Mesh(mesh) => {
            if mesh.tag.iter().any(|tag| tags.contains(tag)) {
                (Some(MeshTree::Mesh(mesh)), None)
            } else {
                (None, Some(MeshTree::Mesh(mesh)))
            }
        }
        MeshTree::List(children) => {
            let mut matched = Vec::new();
            let mut unmatched = Vec::new();
            for child in children {
                let (child_matched, child_unmatched) = split_tree_by_tag(child, tags);
                if let Some(child_matched) = child_matched {
                    matched.push(child_matched);
                }
                if let Some(child_unmatched) = child_unmatched {
                    unmatched.push(child_unmatched);
                }
            }
            (pack_tree_children(matched), pack_tree_children(unmatched))
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
            let tags = list_value(
                mesh.tag
                    .iter()
                    .copied()
                    .map(|tag| Value::Integer(tag as i64)),
            );
            let value = executor.invoke_lambda(lambda, vec![tags]).await?;
            let value = value.elide_wrappers_rec(executor).await?;
            value.check_truthy()
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
    raw.elide_wrappers_rec(executor).await
}

pub(super) async fn invoke_callable_many<A>(
    executor: &mut Executor,
    callable: &Value,
    args: &[A],
    name: &'static str,
) -> Result<Vec<Value>, ExecutorError>
where
    A: AsRef<[Value]>,
{
    let raw: Vec<Value> = match callable.clone().elide_lvalue() {
        Value::Lambda(lambda) => {
            executor
                .eagerly_invoke_lambda_many(&lambda, args, None)
                .await?
        }
        Value::Operator(operator) => {
            executor
                .eagerly_invoke_lambda_many(&operator.0, args, None)
                .await?
        }
        other => {
            return Err(ExecutorError::type_error_for(
                "lambda / operator",
                other.type_name(),
                name,
            ));
        }
    };
    Ok(raw)
}

pub(super) fn split_tree_by_tag_filter<'a>(
    executor: &'a mut Executor,
    tree: MeshTree,
    filter: &'a TagFilter,
) -> Pin<Box<dyn Future<Output = Result<(Option<MeshTree>, Option<MeshTree>), ExecutorError>> + 'a>>
{
    Box::pin(async move {
        match tree {
            tree @ MeshTree::Mesh(_) => match filter {
                TagFilter::Exact(tags) => Ok(split_tree_by_tag(tree, tags)),
                TagFilter::Predicate(_) => {
                    let MeshTree::Mesh(mesh) = tree else {
                        unreachable!()
                    };
                    let keep = mesh_matches_tag_filter(executor, filter, &mesh).await?;
                    if keep {
                        Ok((Some(MeshTree::Mesh(mesh)), None))
                    } else {
                        Ok((None, Some(MeshTree::Mesh(mesh))))
                    }
                }
            },
            MeshTree::List(children) => {
                if let TagFilter::Exact(tags) = filter {
                    return Ok(split_tree_by_tag(MeshTree::List(children), tags));
                }

                let mut matched = Vec::new();
                let mut unmatched = Vec::new();
                for child in children {
                    let (child_matched, child_unmatched) =
                        split_tree_by_tag_filter(executor, child, filter).await?;
                    if let Some(child_matched) = child_matched {
                        matched.push(child_matched);
                    }
                    if let Some(child_unmatched) = child_unmatched {
                        unmatched.push(child_unmatched);
                    }
                }
                Ok((pack_tree_children(matched), pack_tree_children(unmatched)))
            }
        }
    })
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

#[cfg(test)]
mod tests {
    use executor::value::Value;
    use geo::{
        mesh::{Dot, Lin, LinVertex, Mesh, Uniforms},
        simd::{Float3, Float4},
    };

    use super::{
        mesh_from_parts, mesh_position_groups, mesh_ref, mesh_to_indexed_lines, polygon_basis,
        push_closed_polyline, tessellate_planar_loops, uprank_mesh,
    };

    fn mesh_from_contours(contours: &[Vec<Float3>]) -> Mesh {
        let mut lins = Vec::new();
        for contour in contours {
            push_closed_polyline(&mut lins, contour, Float3::Z);
        }

        Mesh {
            dots: Vec::new(),
            lins,
            tris: Vec::new(),
            uniform: Uniforms::default(),
            tag: Vec::new(),
            version: Mesh::fresh_version(),
        }
    }

    fn square(center_x: f32, center_y: f32, half_extent: f32) -> Vec<Float3> {
        vec![
            Float3::new(center_x - half_extent, center_y - half_extent, 0.0),
            Float3::new(center_x + half_extent, center_y - half_extent, 0.0),
            Float3::new(center_x + half_extent, center_y + half_extent, 0.0),
            Float3::new(center_x - half_extent, center_y + half_extent, 0.0),
        ]
    }

    fn mesh_from_tessellated(contours: &[Vec<Float3>], normal: Float3) -> Mesh {
        let (lins, tris) =
            tessellate_planar_loops(contours, normal).expect("polygon tessellation should succeed");
        let Value::Mesh(mesh) = mesh_from_parts(Vec::new(), lins, tris) else {
            panic!("expected mesh");
        };
        mesh.as_ref().clone()
    }

    #[test]
    fn position_groups_follow_negative_dot_inverse_refs() {
        let mesh = Mesh {
            dots: vec![Dot {
                pos: Float3::ZERO,
                norm: Float3::Z,
                col: Float4::ONE,
                inv: mesh_ref(0),
                is_dom_sib: false,
            }],
            lins: vec![Lin {
                a: LinVertex {
                    pos: Float3::ZERO,
                    col: Float4::ONE,
                },
                b: LinVertex {
                    pos: Float3::X,
                    col: Float4::ONE,
                },
                norm: Float3::Z,
                prev: -1,
                next: -1,
                inv: -1,
                is_dom_sib: true,
            }],
            tris: Vec::new(),
            uniform: Uniforms::default(),
            tag: Vec::new(),
            version: Mesh::fresh_version(),
        };
        let groups = mesh_position_groups(&mesh);
        assert_eq!(groups[0], groups[1]);
    }

    #[test]
    fn mesh_from_parts_normalizes_open_line_topology() {
        let Value::Mesh(mesh) = mesh_from_parts(
            vec![],
            vec![Lin {
                a: LinVertex {
                    pos: Float3::ZERO,
                    col: Float4::ONE,
                },
                b: LinVertex {
                    pos: Float3::X,
                    col: Float4::ONE,
                },
                norm: Float3::Z,
                prev: -1,
                next: -1,
                inv: -1,
                is_dom_sib: true,
            }],
            vec![],
        ) else {
            panic!("expected mesh");
        };

        assert_eq!(mesh.lins.len(), 2);
        assert_eq!(mesh.dots.len(), 4);
        assert!(mesh.lins.iter().all(|line| line.inv != -1));
        assert!(mesh.dots.iter().all(|dot| dot.inv != -1));
        assert_eq!(mesh_to_indexed_lines(&mesh).segments.len(), 1);
    }

    #[test]
    fn polygon_basis_uses_xy_axes_for_default_normal() {
        let (x, y, normal) = polygon_basis(Float3::Z);
        assert_eq!(x, Float3::X);
        assert_eq!(y, Float3::Y);
        assert_eq!(normal, Float3::Z);
    }

    #[test]
    fn tessellated_planar_polygons_have_consistent_boundary_topology() {
        let mut inner = square(0.0, 0.0, 0.4);
        inner.reverse();
        let cases = [
            (
                "quad with reversed normal",
                vec![square(0.0, 0.0, 1.0)],
                -Float3::Z,
            ),
            (
                "concave polygon",
                vec![vec![
                    Float3::new(-1.0, -1.0, 0.0),
                    Float3::new(1.0, -1.0, 0.0),
                    Float3::new(0.35, 0.0, 0.0),
                    Float3::new(1.0, 1.0, 0.0),
                    Float3::new(-1.0, 1.0, 0.0),
                ]],
                Float3::Z,
            ),
            (
                "polygon with a hole",
                vec![square(0.0, 0.0, 1.0), inner],
                Float3::Z,
            ),
        ];

        for (name, contours, normal) in cases {
            let mesh = mesh_from_tessellated(&contours, normal);

            assert!(!mesh.tris.is_empty(), "{name} should produce triangles");
            assert!(
                mesh.has_consistent_topology(),
                "{name}: {}",
                mesh.topology_mismatch_report()
                    .unwrap_or_else(|| "no mismatch report".into())
            );
            assert!(
                mesh.lins.iter().all(|line| line.inv < -1),
                "{name}: every boundary line should be owned by a triangle"
            );
        }
    }

    #[test]
    fn uprank_recovers_planar_normal_when_first_boundary_normal_is_zero() {
        let mut mesh = mesh_from_contours(&[square(0.0, 0.0, 1.0)]);
        mesh.lins[0].norm = Float3::ZERO;

        let upranked = uprank_mesh(&mesh).expect("uprank should succeed");
        let upranked = upranked.expect("closed contours should uprank");

        assert!(!upranked.tris.is_empty());
        assert!(upranked.has_consistent_topology());
        assert!(
            upranked.lins.iter().all(|line| line.norm == Float3::Z),
            "upranked boundary normals should use the nonzero planar normal"
        );
    }

    #[test]
    fn uprank_handles_many_duplicate_contours() {
        let mesh = mesh_from_contours(&vec![square(0.0, 0.0, 1.0); 8]);
        let upranked = uprank_mesh(&mesh).expect("uprank should succeed");
        let upranked = upranked.expect("closed contours should uprank");

        assert!(!upranked.tris.is_empty());
        assert!(upranked.has_consistent_topology());
    }

    #[test]
    fn uprank_handles_nearly_overlapping_duplicate_contours() {
        let mesh = mesh_from_contours(&[
            square(0.0, 0.0, 1.0),
            square(0.01, 0.0, 1.0),
            square(-0.01, 0.0, 1.0),
        ]);
        let upranked = uprank_mesh(&mesh).expect("uprank should succeed");
        let upranked = upranked.expect("closed contours should uprank");

        assert!(!upranked.tris.is_empty());
        assert!(upranked.has_consistent_topology());
    }

    #[test]
    fn uprank_handles_duplicate_annulus_families() {
        let mut hole_a = square(0.0, 0.0, 0.6);
        hole_a.reverse();
        let mut hole_b = square(0.02, 0.0, 0.6);
        hole_b.reverse();
        let mesh = mesh_from_contours(&[
            square(0.0, 0.0, 1.2),
            hole_a,
            square(0.02, 0.0, 1.2),
            hole_b,
        ]);
        let upranked = uprank_mesh(&mesh).expect("uprank should succeed");
        let upranked = upranked.expect("closed contours should uprank");

        assert!(!upranked.tris.is_empty());
        assert!(upranked.has_consistent_topology());
    }
}
