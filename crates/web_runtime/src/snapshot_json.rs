use executor::{
    error::{RuntimeCallFrame, RuntimeError},
    scene_snapshot::{BackgroundSnapshot, CameraSnapshot},
    transcript::{SectionTranscript, TranscriptEntry},
    value::MeshAttributePathSegment,
};
use geo::{
    mesh::{Dot, Lin, LinVertex, Mesh, Tri, TriVertex, Uniforms},
    simd::{Float2, Float3, Float4},
};
use serde::Serialize;

pub fn runtime_iteration_to_json(
    iteration: runtime::RuntimeIteration,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&SerializableRuntimeIteration::from(iteration))
}

fn vec2(value: Float2) -> [f64; 2] {
    value.to_array().map(f64::from)
}

fn vec3(value: Float3) -> [f64; 3] {
    value.to_array().map(f64::from)
}

fn vec4(value: Float4) -> [f64; 4] {
    value.to_array().map(f64::from)
}

fn color4((r, g, b, a): (f32, f32, f32, f32)) -> [f64; 4] {
    [r.into(), g.into(), b.into(), a.into()]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableRuntimeIteration {
    snapshots: Vec<SerializableExecutionSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_frame_interval: Option<f64>,
}

impl From<runtime::RuntimeIteration> for SerializableRuntimeIteration {
    fn from(iteration: runtime::RuntimeIteration) -> Self {
        Self {
            snapshots: iteration
                .snapshots
                .into_iter()
                .map(SerializableExecutionSnapshot::from)
                .collect(),
            next_frame_interval: iteration
                .next_frame_interval
                .map(|interval| interval.as_secs_f64()),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableExecutionSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<SerializableBackgroundSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    camera: Option<SerializableCameraSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    camera_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    meshes: Option<Vec<SerializableMesh>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<SerializableRuntimeError>,
    current_timestamp: SerializableTimestamp,
    status: &'static str,
    is_loading: bool,
    slide_count: usize,
    slide_names: Vec<Option<String>>,
    slide_durations: Vec<Option<f64>>,
    minimum_slide_durations: Vec<Option<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<SerializableParameterSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transcript: Option<Vec<SerializableTranscriptSection>>,
}

impl From<runtime::ExecutionSnapshot> for SerializableExecutionSnapshot {
    fn from(snapshot: runtime::ExecutionSnapshot) -> Self {
        Self {
            background: snapshot
                .background
                .as_ref()
                .map(SerializableBackgroundSnapshot::from),
            camera: snapshot
                .camera
                .as_ref()
                .map(SerializableCameraSnapshot::from),
            camera_version: snapshot.camera_version,
            meshes: snapshot.meshes.map(|meshes| {
                meshes
                    .into_iter()
                    .map(|mesh| SerializableMesh::from(mesh.as_ref()))
                    .collect()
            }),
            errors: snapshot
                .errors
                .iter()
                .map(SerializableRuntimeError::from)
                .collect(),
            current_timestamp: SerializableTimestamp {
                slide: snapshot.current_timestamp.slide,
                time: snapshot.current_timestamp.time,
            },
            status: execution_status(snapshot.status),
            is_loading: snapshot.is_loading,
            slide_count: snapshot.slide_count,
            slide_names: snapshot.slide_names,
            slide_durations: snapshot.slide_durations,
            minimum_slide_durations: snapshot.minimum_slide_durations,
            parameters: snapshot
                .parameters
                .as_ref()
                .map(SerializableParameterSnapshot::from),
            transcript: snapshot.transcript.map(|sections| {
                sections
                    .iter()
                    .map(|section| SerializableTranscriptSection::from(section.as_ref()))
                    .collect()
            }),
        }
    }
}

fn execution_status(status: runtime::ExecutionStatus) -> &'static str {
    match status {
        runtime::ExecutionStatus::Playing => "playing",
        runtime::ExecutionStatus::Paused => "paused",
        runtime::ExecutionStatus::RuntimeError => "runtimeError",
        runtime::ExecutionStatus::CompileError => "compileError",
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableRuntimeError {
    message: String,
    span: SerializableSpan,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    callstack: Vec<SerializableRuntimeCallFrame>,
}

impl From<&RuntimeError> for SerializableRuntimeError {
    fn from(error: &RuntimeError) -> Self {
        Self {
            message: error.error.to_string(),
            span: SerializableSpan {
                start: error.span.start,
                end: error.span.end,
            },
            hint: error.hint.clone(),
            callstack: error
                .callstack
                .iter()
                .map(SerializableRuntimeCallFrame::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableRuntimeCallFrame {
    section: u16,
    span: SerializableSpan,
}

impl From<&RuntimeCallFrame> for SerializableRuntimeCallFrame {
    fn from(frame: &RuntimeCallFrame) -> Self {
        Self {
            section: frame.section,
            span: SerializableSpan {
                start: frame.span.start,
                end: frame.span.end,
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableTimestamp {
    slide: usize,
    time: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableBackgroundSnapshot {
    color: [f64; 4],
}

impl From<&BackgroundSnapshot> for SerializableBackgroundSnapshot {
    fn from(snapshot: &BackgroundSnapshot) -> Self {
        Self {
            color: color4(snapshot.color),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableCameraSnapshot {
    position: [f64; 3],
    look_at: [f64; 3],
    up: [f64; 3],
    near: f64,
    far: f64,
}

impl From<&CameraSnapshot> for SerializableCameraSnapshot {
    fn from(snapshot: &CameraSnapshot) -> Self {
        Self {
            position: vec3(snapshot.position),
            look_at: vec3(snapshot.look_at),
            up: vec3(snapshot.up),
            near: snapshot.near.into(),
            far: snapshot.far.into(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableMesh {
    version: u64,
    tag: Vec<isize>,
    uniform: SerializableUniforms,
    dots: Vec<SerializableDot>,
    lines: Vec<SerializableLine>,
    triangles: Vec<SerializableTriangle>,
}

impl From<&Mesh> for SerializableMesh {
    fn from(mesh: &Mesh) -> Self {
        Self {
            version: mesh.version,
            tag: mesh.tag.clone(),
            uniform: SerializableUniforms::from(&mesh.uniform),
            dots: mesh.dots.iter().map(SerializableDot::from).collect(),
            lines: mesh.lins.iter().map(SerializableLine::from).collect(),
            triangles: mesh.tris.iter().map(SerializableTriangle::from).collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableUniforms {
    alpha: f64,
    stroke_miter_radius_scale: f64,
    stroke_radius: f64,
    dot_radius: f64,
    dot_vertex_count: u16,
    smooth: bool,
    gloss: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    z_index: i32,
}

impl From<&Uniforms> for SerializableUniforms {
    fn from(uniforms: &Uniforms) -> Self {
        Self {
            alpha: uniforms.alpha,
            stroke_miter_radius_scale: uniforms.stroke_miter_radius_scale.into(),
            stroke_radius: uniforms.stroke_radius.into(),
            dot_radius: uniforms.dot_radius.into(),
            dot_vertex_count: uniforms.dot_vertex_count,
            smooth: uniforms.smooth,
            gloss: uniforms.gloss.into(),
            image: uniforms
                .img
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            z_index: uniforms.z_index,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableDot {
    position: [f64; 3],
    normal: [f64; 3],
    color: [f64; 4],
    inverse: i32,
    is_dominant_sibling: bool,
}

impl From<&Dot> for SerializableDot {
    fn from(dot: &Dot) -> Self {
        Self {
            position: vec3(dot.pos),
            normal: vec3(dot.norm),
            color: vec4(dot.col),
            inverse: dot.inv,
            is_dominant_sibling: dot.is_dom_sib,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableLineVertex {
    position: [f64; 3],
    color: [f64; 4],
}

impl From<&LinVertex> for SerializableLineVertex {
    fn from(vertex: &LinVertex) -> Self {
        Self {
            position: vec3(vertex.pos),
            color: vec4(vertex.col),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableLine {
    a: SerializableLineVertex,
    b: SerializableLineVertex,
    normal: [f64; 3],
    previous: i32,
    next: i32,
    inverse: i32,
    is_dominant_sibling: bool,
}

impl From<&Lin> for SerializableLine {
    fn from(line: &Lin) -> Self {
        Self {
            a: SerializableLineVertex::from(&line.a),
            b: SerializableLineVertex::from(&line.b),
            normal: vec3(line.norm),
            previous: line.prev,
            next: line.next,
            inverse: line.inv,
            is_dominant_sibling: line.is_dom_sib,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableTriangleVertex {
    position: [f64; 3],
    color: [f64; 4],
    uv: [f64; 2],
}

impl From<&TriVertex> for SerializableTriangleVertex {
    fn from(vertex: &TriVertex) -> Self {
        Self {
            position: vec3(vertex.pos),
            color: vec4(vertex.col),
            uv: vec2(vertex.uv),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableTriangle {
    a: SerializableTriangleVertex,
    b: SerializableTriangleVertex,
    c: SerializableTriangleVertex,
    edge_ab: i32,
    edge_bc: i32,
    edge_ca: i32,
    is_dominant_sibling: bool,
}

impl From<&Tri> for SerializableTriangle {
    fn from(triangle: &Tri) -> Self {
        Self {
            a: SerializableTriangleVertex::from(&triangle.a),
            b: SerializableTriangleVertex::from(&triangle.b),
            c: SerializableTriangleVertex::from(&triangle.c),
            edge_ab: triangle.ab,
            edge_bc: triangle.bc,
            edge_ca: triangle.ca,
            is_dominant_sibling: triangle.is_dom_sib,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableParameterSnapshot {
    params: Vec<SerializableParameterEntrySnapshot>,
    meshes: Vec<SerializableMeshEntrySnapshot>,
}

impl From<&runtime::ParameterSnapshot> for SerializableParameterSnapshot {
    fn from(snapshot: &runtime::ParameterSnapshot) -> Self {
        Self {
            params: snapshot
                .params
                .iter()
                .map(SerializableParameterEntrySnapshot::from)
                .collect(),
            meshes: snapshot
                .meshes
                .iter()
                .map(SerializableMeshEntrySnapshot::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableParameterEntrySnapshot {
    target: SerializablePresentationUpdateTarget,
    name: String,
    value: SerializableParameterValue,
    locked: bool,
}

impl From<&runtime::ParameterEntrySnapshot> for SerializableParameterEntrySnapshot {
    fn from(snapshot: &runtime::ParameterEntrySnapshot) -> Self {
        Self {
            target: SerializablePresentationUpdateTarget::from(&snapshot.target),
            name: snapshot.name.clone(),
            value: SerializableParameterValue::from(&snapshot.value),
            locked: snapshot.locked,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableMeshEntrySnapshot {
    leader_index: usize,
    name: String,
    locked: bool,
    attributes: Vec<SerializableMeshAttributeSnapshot>,
}

impl From<&runtime::MeshEntrySnapshot> for SerializableMeshEntrySnapshot {
    fn from(snapshot: &runtime::MeshEntrySnapshot) -> Self {
        Self {
            leader_index: snapshot.leader_index,
            name: snapshot.name.clone(),
            locked: snapshot.locked,
            attributes: snapshot
                .attributes
                .iter()
                .map(SerializableMeshAttributeSnapshot::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableMeshAttributeSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<SerializablePresentationUpdateTarget>,
    name: String,
    value: SerializableParameterValue,
    children: Vec<SerializableMeshAttributeSnapshot>,
}

impl From<&runtime::MeshAttributeSnapshot> for SerializableMeshAttributeSnapshot {
    fn from(snapshot: &runtime::MeshAttributeSnapshot) -> Self {
        Self {
            target: snapshot
                .target
                .as_ref()
                .map(SerializablePresentationUpdateTarget::from),
            name: snapshot.name.clone(),
            value: SerializableParameterValue::from(&snapshot.value),
            children: snapshot
                .children
                .iter()
                .map(SerializableMeshAttributeSnapshot::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum SerializablePresentationUpdateTarget {
    Param {
        leader_index: usize,
    },
    MeshAttribute {
        leader_index: usize,
        path: Vec<SerializableMeshAttributePathSegment>,
    },
}

impl From<&runtime::PresentationUpdateTarget> for SerializablePresentationUpdateTarget {
    fn from(target: &runtime::PresentationUpdateTarget) -> Self {
        match target {
            runtime::PresentationUpdateTarget::Param { leader_index } => Self::Param {
                leader_index: *leader_index,
            },
            runtime::PresentationUpdateTarget::MeshAttribute { leader_index, path } => {
                Self::MeshAttribute {
                    leader_index: *leader_index,
                    path: path
                        .iter()
                        .map(SerializableMeshAttributePathSegment::from)
                        .collect(),
                }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum SerializableMeshAttributePathSegment {
    ListIndex { index: usize },
    FunctionArgument { index: usize },
    OperatorOperand,
    OperatorArgument { index: usize },
}

impl From<&MeshAttributePathSegment> for SerializableMeshAttributePathSegment {
    fn from(segment: &MeshAttributePathSegment) -> Self {
        match segment {
            MeshAttributePathSegment::ListIndex(index) => Self::ListIndex { index: *index },
            MeshAttributePathSegment::FunctionArgument(index) => {
                Self::FunctionArgument { index: *index }
            }
            MeshAttributePathSegment::OperatorOperand => Self::OperatorOperand,
            MeshAttributePathSegment::OperatorArgument(index) => {
                Self::OperatorArgument { index: *index }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum SerializableParameterValue {
    Int { value: i64 },
    VectorInt { value: Vec<i64> },
    Float { value: f64 },
    VectorFloat { value: Vec<f64> },
    Complex { re: f64, im: f64 },
    Camera { value: SerializableCameraSnapshot },
    Other,
}

impl From<&runtime::ParameterValue> for SerializableParameterValue {
    fn from(value: &runtime::ParameterValue) -> Self {
        match value {
            runtime::ParameterValue::Int(value) => Self::Int { value: *value },
            runtime::ParameterValue::VectorInt(value) => Self::VectorInt {
                value: value.clone(),
            },
            runtime::ParameterValue::Float(value) => Self::Float { value: *value },
            runtime::ParameterValue::VectorFloat(value) => Self::VectorFloat {
                value: value.clone(),
            },
            runtime::ParameterValue::Complex { re, im } => Self::Complex { re: *re, im: *im },
            runtime::ParameterValue::Camera(value) => Self::Camera {
                value: SerializableCameraSnapshot::from(value),
            },
            runtime::ParameterValue::Other => Self::Other,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableTranscriptSection {
    entries: Vec<SerializableTranscriptEntry>,
}

impl From<&SectionTranscript> for SerializableTranscriptSection {
    fn from(section: &SectionTranscript) -> Self {
        Self {
            entries: section
                .entries
                .iter()
                .map(SerializableTranscriptEntry::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableTranscriptEntry {
    span: SerializableSpan,
    section: u16,
    is_root: bool,
    text: String,
}

impl From<&TranscriptEntry> for SerializableTranscriptEntry {
    fn from(entry: &TranscriptEntry) -> Self {
        Self {
            span: SerializableSpan {
                start: entry.span.start,
                end: entry.span.end,
            },
            section: entry.section,
            is_root: entry.is_root,
            text: entry.text().to_string(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableSpan {
    start: usize,
    end: usize,
}
