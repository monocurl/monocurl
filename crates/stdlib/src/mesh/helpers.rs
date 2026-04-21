use std::{
    collections::{HashMap, HashSet},
    future::Future,
    ops::Range,
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
    mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
    simd::{Float2, Float3, Float4},
};
use libtess2::{TessellationOptions, WindingRule};

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

#[derive(Clone, Copy, Debug)]
pub(super) struct SurfaceVertex {
    pub pos: Float3,
    pub col: Float4,
    pub uv: Float2,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct BoundaryEdge {
    pub a_col: Float4,
    pub b_col: Float4,
    pub norm: Float3,
}

#[derive(Clone, Debug)]
pub(super) struct IndexedSurface {
    pub vertices: Vec<SurfaceVertex>,
    pub faces: Vec<[usize; 3]>,
    pub boundary_edges: HashMap<(usize, usize), BoundaryEdge>,
}

#[derive(Clone, Debug)]
pub(super) struct IndexedLineMesh {
    pub vertices: Vec<SurfaceVertex>,
    pub segments: Vec<[usize; 2]>,
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
                        Some(filter) => mesh_matches_tag_filter(executor, filter, arc.as_ref()).await?,
                        None => true,
                    };
                    if keep {
                        f(Arc::make_mut(arc));
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
            "list of length 2",
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
            "list of length 3",
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
            "list of length 4",
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
        anti: -1,
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
        anti: -1,
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
        }
        if dot.anti >= 0 {
            let anti = dot.anti as usize;
            if anti < mesh.dots.len() {
                dsu.union(dot_slot(idx), dot_slot(anti));
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

        if lin.anti >= 0 {
            let anti = lin.anti as usize;
            if anti < mesh.lins.len() {
                dsu.union(line_a_slot(mesh, idx), line_a_slot(mesh, anti));
                dsu.union(line_b_slot(mesh, idx), line_b_slot(mesh, anti));
            }
        }
    }

    for (tri_idx, tri) in mesh.tris.iter().enumerate() {
        if tri.anti >= 0 {
            let anti = tri.anti as usize;
            if anti < mesh.tris.len() {
                dsu.union(tri_a_slot(mesh, tri_idx), tri_b_slot(mesh, anti));
                dsu.union(tri_b_slot(mesh, tri_idx), tri_a_slot(mesh, anti));
                dsu.union(tri_c_slot(mesh, tri_idx), tri_c_slot(mesh, anti));
            }
        }

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
    -2 - idx as i32
}

pub(super) fn shift_line_refs(lines: &mut [Lin], delta: usize) {
    let delta = delta as i32;
    for line in lines {
        for value in [
            &mut line.prev,
            &mut line.next,
            &mut line.inv,
            &mut line.anti,
        ] {
            if *value >= 0 {
                *value += delta;
            }
        }
    }
}

pub(super) fn push_open_polyline(
    out: &mut Vec<Lin>,
    points: &[Float3],
    normal: Float3,
) -> Range<usize> {
    let start = out.len();
    if points.len() < 2 {
        return start..start;
    }

    let mut lines: Vec<_> = points
        .windows(2)
        .enumerate()
        .map(|(i, w)| {
            let mut lin = default_lin(w[0], w[1], normal);
            lin.prev = if i == 0 { -1 } else { i as i32 - 1 };
            lin.next = if i + 1 == points.len() - 1 {
                -1
            } else {
                i as i32 + 1
            };
            lin
        })
        .collect();
    shift_line_refs(&mut lines, start);
    out.extend(lines);
    start..out.len()
}

pub(crate) fn push_closed_polyline(
    out: &mut Vec<Lin>,
    points: &[Float3],
    normal: Float3,
) -> Range<usize> {
    let start = out.len();
    if points.len() < 2 {
        return start..start;
    }

    let mut lines = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        let mut lin = default_lin(points[i], points[(i + 1) % points.len()], normal);
        lin.prev = ((i + points.len() - 1) % points.len()) as i32;
        lin.next = ((i + 1) % points.len()) as i32;
        lines.push(lin);
    }
    shift_line_refs(&mut lines, start);
    out.extend(lines);
    start..out.len()
}

pub(super) fn build_indexed_surface(
    vertices: &[SurfaceVertex],
    faces: &[[usize; 3]],
    boundary_edges: &HashMap<(usize, usize), BoundaryEdge>,
) -> (Vec<Lin>, Vec<Tri>) {
    let mut tris: Vec<_> = faces
        .iter()
        .map(|face| Tri {
            a: TriVertex {
                pos: vertices[face[0]].pos,
                col: vertices[face[0]].col,
                uv: vertices[face[0]].uv,
            },
            b: TriVertex {
                pos: vertices[face[1]].pos,
                col: vertices[face[1]].col,
                uv: vertices[face[1]].uv,
            },
            c: TriVertex {
                pos: vertices[face[2]].pos,
                col: vertices[face[2]].col,
                uv: vertices[face[2]].uv,
            },
            ab: -1,
            bc: -1,
            ca: -1,
            anti: -1,
            is_dom_sib: false,
        })
        .collect();

    let mut edge_map = HashMap::<(usize, usize), (usize, usize, usize, usize)>::new();
    for (tri_idx, face) in faces.iter().enumerate() {
        for (edge_idx, (a, b)) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])]
            .into_iter()
            .enumerate()
        {
            let key = canonical_index_edge_key(a, b);
            if let Some((other_tri, other_edge, _, _)) = edge_map.remove(&key) {
                set_tri_edge(&mut tris[tri_idx], edge_idx, other_tri as i32);
                set_tri_edge(&mut tris[other_tri], other_edge, tri_idx as i32);
            } else {
                edge_map.insert(key, (tri_idx, edge_idx, a, b));
            }
        }
    }

    let mut boundary_items: Vec<_> = edge_map.into_values().collect();
    boundary_items.sort_unstable_by_key(|(tri_idx, edge_idx, _, _)| (*tri_idx, *edge_idx));

    let mut lins = Vec::with_capacity(boundary_items.len());
    let mut edge_to_line = HashMap::<(usize, usize), usize>::with_capacity(boundary_items.len());
    for (tri_idx, edge_idx, a, b) in boundary_items {
        let template = boundary_edges
            .get(&(a, b))
            .copied()
            .unwrap_or(BoundaryEdge {
                a_col: vertices[a].col,
                b_col: vertices[b].col,
                norm: Float3::ZERO,
            });
        let line_idx = lins.len();
        let mut line = default_lin(vertices[a].pos, vertices[b].pos, template.norm);
        line.a.col = template.a_col;
        line.b.col = template.b_col;
        line.inv = mesh_ref(tri_idx);
        set_tri_edge(&mut tris[tri_idx], edge_idx, mesh_ref(line_idx));
        lins.push(line);
        edge_to_line.insert((a, b), line_idx);
    }

    let mut incoming = HashMap::<usize, Vec<usize>>::new();
    let mut outgoing = HashMap::<usize, Vec<usize>>::new();
    for (&(a, b), &line_idx) in &edge_to_line {
        outgoing.entry(a).or_default().push(line_idx);
        incoming.entry(b).or_default().push(line_idx);
    }

    for (&(a, b), &line_idx) in &edge_to_line {
        lins[line_idx].prev = incoming
            .get(&a)
            .and_then(|candidates| (candidates.len() == 1).then_some(candidates[0] as i32))
            .unwrap_or(-1);
        lins[line_idx].next = outgoing
            .get(&b)
            .and_then(|candidates| (candidates.len() == 1).then_some(candidates[0] as i32))
            .unwrap_or(-1);
    }

    (lins, tris)
}

pub(super) fn build_indexed_tris(vertices: &[Float3], faces: &[[usize; 3]]) -> Vec<Tri> {
    let vertices: Vec<_> = vertices
        .iter()
        .copied()
        .map(|pos| SurfaceVertex {
            pos,
            col: default_ink(),
            uv: Float2::ZERO,
        })
        .collect();
    build_indexed_surface(&vertices, faces, &HashMap::new()).1
}

pub(super) fn mesh_to_indexed_surface(mesh: &Mesh) -> IndexedSurface {
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

    for (idx, dot) in mesh.dots.iter().enumerate() {
        assign_vertex(groups[dot_slot(idx)], dot.pos, dot.col);
    }
    for (idx, line) in mesh.lins.iter().enumerate() {
        assign_vertex(groups[line_a_slot(mesh, idx)], line.a.pos, line.a.col);
        assign_vertex(groups[line_b_slot(mesh, idx)], line.b.pos, line.b.col);
    }

    let segments = mesh
        .lins
        .iter()
        .enumerate()
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

    let mut lins = Vec::new();
    let mut boundary_edges = HashMap::<(usize, usize), usize>::new();
    let mut source_offset = 0usize;
    for contour in &contours {
        let range = push_closed_polyline(&mut lins, contour, normal);
        for i in 0..contour.len() {
            boundary_edges.insert(
                canonical_index_edge_key(
                    source_offset + i,
                    source_offset + (i + 1) % contour.len(),
                ),
                range.start + i,
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
    .map_err(|err| {
        ExecutorError::invalid_operation(format!("failed to tessellate polygon: {err}"))
    })?;

    let mut tris = build_indexed_tris(&tess.vertices, &tess.triangles);
    for (tri_idx, face) in tess.triangles.iter().enumerate() {
        for (edge_idx, (a, b)) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])]
            .into_iter()
            .enumerate()
        {
            if tri_edge(&tris[tri_idx], edge_idx) >= 0 {
                continue;
            }

            let Some(source_a) = tess.source_vertex_indices[a] else {
                continue;
            };
            let Some(source_b) = tess.source_vertex_indices[b] else {
                continue;
            };
            let Some(&line_idx) = boundary_edges.get(&canonical_index_edge_key(source_a, source_b))
            else {
                continue;
            };

            let line = lins[line_idx];
            let (edge_start, edge_end) = tri_edge_positions(&tris[tri_idx], edge_idx);
            if line.a.pos != edge_start || line.b.pos != edge_end {
                continue;
            }

            set_tri_edge(&mut tris[tri_idx], edge_idx, mesh_ref(line_idx));
            lins[line_idx].inv = mesh_ref(tri_idx);
        }
    }

    Ok((lins, tris))
}
fn closed_line_contours(mesh: &Mesh) -> Option<Vec<Vec<Float3>>> {
    if mesh.lins.is_empty() || mesh.lins.iter().any(|lin| lin.prev < 0 || lin.next < 0) {
        return None;
    }

    let mut visited = vec![false; mesh.lins.len()];
    let mut contours = Vec::new();
    for start in 0..mesh.lins.len() {
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
            if next >= mesh.lins.len() || mesh.lins[next].prev != cursor as i32 {
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

    let normal = out.lins.first().map(|line| line.norm).unwrap_or(Float3::Z);
    let (lins, tris) = tessellate_planar_loops_with_options(&contours, normal, true)?;
    out.lins = lins;
    out.tris = tris;
    debug_assert!(out.has_consistent_topology());
    Ok(Some(out))
}

pub(super) fn mesh_from_parts(dots: Vec<Dot>, lins: Vec<Lin>, tris: Vec<Tri>) -> Value {
    let mesh = Mesh {
        dots,
        lins,
        tris,
        uniform: Uniforms::default(),
        tag: vec![],
    };
    debug_assert!(mesh.has_consistent_topology());
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
        .elide_lvalue();
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

fn canonical_index_edge_key(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

fn set_tri_edge(tri: &mut Tri, edge_idx: usize, value: i32) {
    match edge_idx {
        0 => tri.ab = value,
        1 => tri.bc = value,
        2 => tri.ca = value,
        _ => unreachable!(),
    }
}

fn tri_edge(tri: &Tri, edge_idx: usize) -> i32 {
    match edge_idx {
        0 => tri.ab,
        1 => tri.bc,
        2 => tri.ca,
        _ => unreachable!(),
    }
}

fn tri_edge_positions(tri: &Tri, edge_idx: usize) -> (Float3, Float3) {
    match edge_idx {
        0 => (tri.a.pos, tri.b.pos),
        1 => (tri.b.pos, tri.c.pos),
        2 => (tri.c.pos, tri.a.pos),
        _ => unreachable!(),
    }
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
    use geo::{
        mesh::{Mesh, Uniforms},
        simd::Float3,
    };

    use super::{push_closed_polyline, uprank_mesh};

    fn duplicate_square_mesh(copies: usize) -> Mesh {
        let mut lins = Vec::new();
        let square = [
            Float3::new(-1.0, -1.0, 0.0),
            Float3::new(1.0, -1.0, 0.0),
            Float3::new(1.0, 1.0, 0.0),
            Float3::new(-1.0, 1.0, 0.0),
        ];
        for _ in 0..copies {
            push_closed_polyline(&mut lins, &square, Float3::Z);
        }

        Mesh {
            dots: Vec::new(),
            lins,
            tris: Vec::new(),
            uniform: Uniforms::default(),
            tag: Vec::new(),
        }
    }

    #[test]
    fn uprank_handles_many_duplicate_contours() {
        let mesh = duplicate_square_mesh(8);
        let upranked = uprank_mesh(&mesh).expect("uprank should succeed");
        let upranked = upranked.expect("closed contours should uprank");

        assert!(!upranked.tris.is_empty());
        assert!(upranked.has_consistent_topology());
    }
}
