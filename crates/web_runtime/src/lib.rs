mod snapshot_json;

use std::{collections::HashMap, path::PathBuf};

use compiler::{
    cache::CompilerCache,
    compiler::{CompileError, CompileWarning, compile},
};
use executor::scene_snapshot::CameraSnapshot;
use geo::simd::Float3;
use lexer::{lexer::Lexer, token::Token};
use parser::{
    import_context::{MemoryImportBackend, ParseImportContext},
    parser::{Diagnostic as ParseDiagnostic, Parser},
};
use serde::{Deserialize, Serialize};
use structs::rope::{Attribute, RLEData, Rope};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
type JsValue = String;

#[cfg(target_arch = "wasm32")]
fn js_error(message: String) -> JsValue {
    JsValue::from_str(&message)
}

#[cfg(not(target_arch = "wasm32"))]
fn js_error(message: String) -> JsValue {
    message
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Runtime {
    controller: runtime::RuntimeController,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Runtime {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        Self {
            controller: runtime::RuntimeController::new(),
        }
    }

    pub fn needs_work(&self) -> bool {
        self.controller.needs_work()
    }

    pub fn is_playing(&self) -> bool {
        self.controller.is_playing()
    }

    pub fn seek_to(&self, slide: usize, time: f64) {
        self.controller.apply_command(
            runtime::RuntimeCommand::SeekTo {
                target: executor::time::Timestamp::new(slide, time),
            },
            0.0,
        );
    }

    pub fn toggle_play(&self, now_seconds: f64) {
        self.controller
            .apply_command(runtime::RuntimeCommand::TogglePlay, now_seconds);
    }

    pub fn set_presentation_mode(&self) {
        self.controller.apply_command(
            runtime::RuntimeCommand::SetPlaybackMode(runtime::PlaybackMode::Presentation),
            0.0,
        );
    }

    pub fn set_preview_mode(&self) {
        self.controller.apply_command(
            runtime::RuntimeCommand::SetPlaybackMode(runtime::PlaybackMode::Preview),
            0.0,
        );
    }

    pub fn set_web_mode(&self) {
        self.controller.apply_command(
            runtime::RuntimeCommand::SetPlaybackMode(runtime::PlaybackMode::Web),
            0.0,
        );
    }

    pub fn update_parameters(&self, updates_json: &str, now_seconds: f64) -> Result<(), JsValue> {
        let updates = parse_parameter_updates(updates_json)
            .map_err(|error| js_error(format!("failed to decode parameter updates: {error}")))?;
        self.controller.apply_command(
            runtime::RuntimeCommand::UpdateParameters { updates },
            now_seconds,
        );
        Ok(())
    }

    pub fn load_source(&self, source: &str, imports_json: &str) -> Result<String, JsValue> {
        self.load_source_with_root_path("main.mcs", source, imports_json)
    }

    pub fn load_source_with_root_path(
        &self,
        root_path: &str,
        source: &str,
        imports_json: &str,
    ) -> Result<String, JsValue> {
        let imported_files = parse_import_map(imports_json)
            .map_err(|error| js_error(format!("failed to decode import map: {error}")))?;
        let report = self.compile_source(root_path, source, imported_files);
        serde_json::to_string(&report).map_err(|error| js_error(error.to_string()))
    }

    pub async fn step(&self, now_seconds: f64) -> usize {
        self.controller
            .run_iteration(now_seconds)
            .await
            .snapshots
            .len()
    }

    pub async fn step_json(&self, now_seconds: f64) -> Result<String, JsValue> {
        snapshot_json::runtime_iteration_to_json(self.controller.run_iteration(now_seconds).await)
            .map_err(|error| js_error(error.to_string()))
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    fn compile_source(
        &self,
        root_path: &str,
        source: &str,
        imported_files: HashMap<String, String>,
    ) -> CompilationReport {
        let text_rope = Rope::from_text(source);
        let lex_rope = lex_rope_from_str(source);
        let mut import_backend = MemoryImportBackend::new();
        insert_embedded_stdlib(&mut import_backend);
        for (path, source) in imported_files {
            insert_import_source(&mut import_backend, &path, source);
        }

        let mut parse_context =
            ParseImportContext::with_backend(PathBuf::from(root_path), import_backend);
        let (bundles, parse_artifacts) =
            Parser::parse(&mut parse_context, lex_rope, text_rope, None);
        let mut compiler_cache = CompilerCache::default();
        let compile_result = compile(&mut compiler_cache, None, &bundles);

        let mut diagnostics = Vec::new();
        diagnostics.extend(
            parse_artifacts
                .error_diagnostics
                .iter()
                .map(CompilationDiagnostic::parse_error),
        );
        diagnostics.extend(
            compile_result
                .errors
                .iter()
                .map(CompilationDiagnostic::compile_error),
        );
        diagnostics.extend(
            compile_result
                .warnings
                .iter()
                .map(CompilationDiagnostic::compile_warning),
        );

        let ok = parse_artifacts.error_diagnostics.is_empty() && compile_result.errors.is_empty();
        let slides = if ok {
            let library_sections = compile_result.bytecode.library_sections();
            compile_result
                .bytecode
                .sections
                .iter()
                .enumerate()
                .skip(compile_result.bytecode.non_slide_sections())
                .map(|(section_index, section)| SlideMetadata {
                    index: section_index.saturating_sub(library_sections),
                    name: section.name.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };

        self.controller.apply_command(
            runtime::RuntimeCommand::UpdateBytecode {
                bytecode: ok.then_some(compile_result.bytecode),
            },
            0.0,
        );

        CompilationReport {
            ok,
            diagnostics,
            slides,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompilationReport {
    ok: bool,
    diagnostics: Vec<CompilationDiagnostic>,
    slides: Vec<SlideMetadata>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SlideMetadata {
    index: usize,
    name: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompilationDiagnostic {
    kind: &'static str,
    title: String,
    message: String,
    span: SerializableSpan,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableSpan {
    start: usize,
    end: usize,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ImportMap {
    Files(HashMap<String, String>),
    FileEntries(Vec<ImportMapEntry>),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportMapEntry {
    path: String,
    source: String,
    #[serde(default)]
    is_stdlib: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ParameterUpdateEntry {
    target: WebPresentationUpdateTarget,
    value: WebParameterValue,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum WebPresentationUpdateTarget {
    Param { leader_index: usize },
    MeshAttribute { leader_index: usize, name: String },
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum WebParameterValue {
    Int { value: i64 },
    VectorInt { value: Vec<i64> },
    Float { value: f64 },
    VectorFloat { value: Vec<f64> },
    Complex { re: f64, im: f64 },
    Camera { value: WebCameraSnapshot },
    Other,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebCameraSnapshot {
    position: [f32; 3],
    look_at: [f32; 3],
    up: [f32; 3],
    near: f32,
    far: f32,
}

impl CompilationDiagnostic {
    fn parse_error(diagnostic: &ParseDiagnostic) -> Self {
        Self {
            kind: "parseError",
            title: diagnostic.title.clone(),
            message: diagnostic.message.clone(),
            span: SerializableSpan::from(&diagnostic.span),
        }
    }

    fn compile_error(error: &CompileError) -> Self {
        Self {
            kind: "compileError",
            title: "Compile Error".to_string(),
            message: error.message.clone(),
            span: SerializableSpan::from(&error.span),
        }
    }

    fn compile_warning(warning: &CompileWarning) -> Self {
        Self {
            kind: "compileWarning",
            title: "Compile Warning".to_string(),
            message: warning.message.clone(),
            span: SerializableSpan::from(&warning.span),
        }
    }
}

impl From<&std::ops::Range<usize>> for SerializableSpan {
    fn from(span: &std::ops::Range<usize>) -> Self {
        Self {
            start: span.start,
            end: span.end,
        }
    }
}

fn parse_parameter_updates(
    json: &str,
) -> Result<HashMap<runtime::PresentationUpdateTarget, runtime::ParameterValue>, serde_json::Error>
{
    let entries: Vec<ParameterUpdateEntry> = serde_json::from_str(json)?;
    Ok(entries
        .into_iter()
        .map(|entry| (entry.target.into(), entry.value.into()))
        .collect())
}

impl From<WebPresentationUpdateTarget> for runtime::PresentationUpdateTarget {
    fn from(target: WebPresentationUpdateTarget) -> Self {
        match target {
            WebPresentationUpdateTarget::Param { leader_index } => Self::Param { leader_index },
            WebPresentationUpdateTarget::MeshAttribute { leader_index, name } => {
                Self::MeshAttribute { leader_index, name }
            }
        }
    }
}

impl From<WebParameterValue> for runtime::ParameterValue {
    fn from(value: WebParameterValue) -> Self {
        match value {
            WebParameterValue::Int { value } => Self::Int(value),
            WebParameterValue::VectorInt { value } => Self::VectorInt(value),
            WebParameterValue::Float { value } => Self::Float(value),
            WebParameterValue::VectorFloat { value } => Self::VectorFloat(value),
            WebParameterValue::Complex { re, im } => Self::Complex { re, im },
            WebParameterValue::Camera { value } => Self::Camera(value.into()),
            WebParameterValue::Other => Self::Other,
        }
    }
}

impl From<WebCameraSnapshot> for CameraSnapshot {
    fn from(snapshot: WebCameraSnapshot) -> Self {
        Self {
            position: Float3::new(
                snapshot.position[0],
                snapshot.position[1],
                snapshot.position[2],
            ),
            look_at: Float3::new(
                snapshot.look_at[0],
                snapshot.look_at[1],
                snapshot.look_at[2],
            ),
            up: Float3::new(snapshot.up[0], snapshot.up[1], snapshot.up[2]),
            near: snapshot.near,
            far: snapshot.far,
        }
    }
}

fn parse_import_map(json: &str) -> Result<HashMap<String, String>, serde_json::Error> {
    if json.trim().is_empty() {
        return Ok(HashMap::new());
    }

    match serde_json::from_str(json)? {
        ImportMap::Files(files) => Ok(files),
        ImportMap::FileEntries(entries) => Ok(entries
            .into_iter()
            .map(|entry| {
                let path = if entry.is_stdlib && !entry.path.starts_with("std.") {
                    format!("std.{}", entry.path)
                } else {
                    entry.path
                };
                (path, entry.source)
            })
            .collect()),
    }
}

fn insert_import_source(import_backend: &mut MemoryImportBackend, path: &str, source: String) {
    let is_stdlib = path.starts_with("std.") || path.starts_with("std/");
    if path.contains('/') || path.ends_with(".mcl") {
        import_backend.insert_path(path, source, is_stdlib);
    } else {
        import_backend.insert_module(path, source, is_stdlib);
    }
}

fn insert_embedded_stdlib(import_backend: &mut MemoryImportBackend) {
    for (module, source) in [
        ("std.math", include_str!("../../../assets/std/std/math.mcl")),
        ("std.anim", include_str!("../../../assets/std/std/anim.mcl")),
        (
            "std.color",
            include_str!("../../../assets/std/std/color.mcl"),
        ),
        ("std.util", include_str!("../../../assets/std/std/util.mcl")),
        ("std.mesh", include_str!("../../../assets/std/std/mesh.mcl")),
        (
            "std.scene",
            include_str!("../../../assets/std/std/scene.mcl"),
        ),
    ] {
        import_backend.insert_module(module, source, true);
    }
}

fn lex_rope_from_str(source: &str) -> Rope<Attribute<Token>> {
    Rope::default().replace_range(
        0..0,
        Lexer::new(source.chars()).map(|(attribute, codeunits)| RLEData {
            codeunits,
            attribute,
        }),
    )
}
