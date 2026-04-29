use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use compiler::{
    cache::CompilerCache,
    compiler::{CompileError, CompileWarning, compile},
};
use executor::{
    error::{RuntimeCallFrame, RuntimeError},
    executor::{Executor, SeekToResult},
    time::Timestamp,
};
use lexer::{lexer::Lexer, token::Token};
use parser::{
    import_context::ParseImportContext,
    parser::{Diagnostic, Parser},
};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, model::*,
    service::RequestContext, transport::stdio,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use stdlib::registry::registry;
use structs::{
    assets::Assets,
    rope::{Attribute, RLEData, Rope, TextAggregate},
    text::Span8,
};

const LANGUAGE_SEMANTICS: &str = include_str!("../docs/language-semantics.md");
const STDLIB_DOCS: &str = include_str!("../docs/stdlib.md");

const STD_UTIL: &str = include_str!("../../../assets/std/std/util.mcl");
const STD_MATH: &str = include_str!("../../../assets/std/std/math.mcl");
const STD_COLOR: &str = include_str!("../../../assets/std/std/color.mcl");
const STD_MESH: &str = include_str!("../../../assets/std/std/mesh.mcl");
const STD_ANIM: &str = include_str!("../../../assets/std/std/anim.mcl");
const STD_SCENE: &str = include_str!("../../../assets/std/std/scene.mcl");

const TOOL_CHECK_NAME: &str = "monocurl_check";
const TOOL_SEEK_NAME: &str = "monocurl_seek";

#[derive(Clone, Copy)]
struct DocResource {
    uri: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    mime_type: &'static str,
    text: &'static str,
}

const RESOURCES: &[DocResource] = &[
    DocResource {
        uri: "monocurl://docs/language-semantics",
        name: "language-semantics",
        title: "Monocurl Language Semantics",
        description: "Compact authoring guide for AI agents writing Monocurl scenes.",
        mime_type: "text/markdown",
        text: LANGUAGE_SEMANTICS,
    },
    DocResource {
        uri: "monocurl://docs/stdlib",
        name: "stdlib-overview",
        title: "Monocurl Standard Library Overview",
        description: "Overview of the public stdlib wrapper modules and authoring conventions.",
        mime_type: "text/markdown",
        text: STDLIB_DOCS,
    },
    DocResource {
        uri: "monocurl://stdlib/util",
        name: "std-util",
        title: "std.util Source",
        description: "Public utility wrappers for collections, strings, conversion, predicates, and live defaults.",
        mime_type: "text/x-monocurl",
        text: STD_UTIL,
    },
    DocResource {
        uri: "monocurl://stdlib/math",
        name: "std-math",
        title: "std.math Source",
        description: "Public scalar, vector, interpolation, statistics, and combinatorics wrappers.",
        mime_type: "text/x-monocurl",
        text: STD_MATH,
    },
    DocResource {
        uri: "monocurl://stdlib/color",
        name: "std-color",
        title: "std.color Source",
        description: "Public color constants and color helper wrappers.",
        mime_type: "text/x-monocurl",
        text: STD_COLOR,
    },
    DocResource {
        uri: "monocurl://stdlib/mesh",
        name: "std-mesh",
        title: "std.mesh Source",
        description: "Public mesh constructors, graphing helpers, styling operators, transforms, tags, and queries.",
        mime_type: "text/x-monocurl",
        text: STD_MESH,
    },
    DocResource {
        uri: "monocurl://stdlib/anim",
        name: "std-anim",
        title: "std.anim Source",
        description: "Public rate functions, primitive animations, follower animations, and animation composition wrappers.",
        mime_type: "text/x-monocurl",
        text: STD_ANIM,
    },
    DocResource {
        uri: "monocurl://stdlib/scene",
        name: "std-scene",
        title: "std.scene Source",
        description: "Public scene, camera, and background wrappers.",
        mime_type: "text/x-monocurl",
        text: STD_SCENE,
    },
];

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SceneInput {
    source: String,
    #[serde(default = "default_root_path")]
    root_path: String,
    #[serde(default)]
    open_documents: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeekInput {
    #[serde(flatten)]
    scene: SceneInput,
    slide: usize,
    #[serde(default)]
    time: Option<f64>,
    #[serde(default)]
    at_end: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpanInfo {
    start: usize,
    end: usize,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolDiagnostic {
    phase: &'static str,
    severity: &'static str,
    title: String,
    message: String,
    span: SpanInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckOutput {
    ok: bool,
    diagnostics: Vec<ToolDiagnostic>,
    slide_count: usize,
    slide_names: Vec<Option<String>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TimestampInfo {
    slide: usize,
    time: Option<f64>,
    at_end: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptEntryOutput {
    text: String,
    span: SpanInfo,
    section: usize,
    is_root: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeCallFrameOutput {
    section: usize,
    section_label: String,
    is_root: bool,
    span: SpanInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeErrorOutput {
    message: String,
    span: SpanInfo,
    hint: Option<String>,
    callstack: Vec<RuntimeCallFrameOutput>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeekOutput {
    ok: bool,
    diagnostics: Vec<ToolDiagnostic>,
    runtime_errors: Vec<RuntimeErrorOutput>,
    seek_error: Option<String>,
    requested: TimestampInfo,
    reached: Option<TimestampInfo>,
    slide_count: usize,
    slide_names: Vec<Option<String>>,
    transcript: Vec<TranscriptEntryOutput>,
}

struct PreparedScene {
    executor: Executor,
    root_text_rope: Rope<TextAggregate>,
    diagnostics: Vec<ToolDiagnostic>,
}

enum BuildSceneResult {
    Ready(PreparedScene),
    Failed { diagnostics: Vec<ToolDiagnostic> },
}

#[derive(Clone)]
struct MonocurlDocs;

impl MonocurlDocs {
    fn find_resource(uri: &str) -> Option<&'static DocResource> {
        RESOURCES.iter().find(|resource| resource.uri == uri)
    }

    fn list_doc_resources() -> Vec<Resource> {
        RESOURCES
            .iter()
            .map(|resource| {
                let priority = if resource.uri.starts_with("monocurl://docs/") {
                    1.0
                } else {
                    0.8
                };

                RawResource::new(resource.uri, resource.name)
                    .with_title(resource.title)
                    .with_description(resource.description)
                    .with_mime_type(resource.mime_type)
                    .with_size(resource.text.len() as u32)
                    .with_priority(priority)
                    .with_audience(vec![Role::Assistant])
            })
            .collect()
    }

    fn tools() -> Vec<Tool> {
        vec![Self::check_tool(), Self::seek_tool()]
    }

    fn check_tool() -> Tool {
        Tool::new(
            TOOL_CHECK_NAME,
            "Parse and compile Monocurl source, returning structured parse/compiler diagnostics.",
            check_input_schema(),
        )
        .with_title("Check Monocurl Source")
        .with_annotations(read_only_tool_annotations())
    }

    fn seek_tool() -> Tool {
        Tool::new(
            TOOL_SEEK_NAME,
            "Compile Monocurl source, execute up to a timestamp, and return runtime errors plus transcript output.",
            seek_input_schema(),
        )
        .with_title("Seek Monocurl Source")
        .with_annotations(read_only_tool_annotations())
    }

    fn run_check(input: SceneInput) -> CheckOutput {
        match build_scene(&input) {
            BuildSceneResult::Ready(prepared) => {
                let slide_count = prepared.executor.real_slide_count();
                let slide_names = prepared.executor.real_slide_names();

                CheckOutput {
                    ok: !has_errors(&prepared.diagnostics),
                    diagnostics: prepared.diagnostics,
                    slide_count,
                    slide_names,
                }
            }
            BuildSceneResult::Failed { diagnostics } => CheckOutput {
                ok: false,
                diagnostics,
                slide_count: 0,
                slide_names: Vec::new(),
            },
        }
    }

    fn run_seek(input: SeekInput) -> SeekOutput {
        let requested_timestamp = match requested_timestamp(&input) {
            Ok(timestamp) => timestamp,
            Err(message) => {
                return SeekOutput {
                    ok: false,
                    diagnostics: Vec::new(),
                    runtime_errors: Vec::new(),
                    seek_error: Some(message),
                    requested: TimestampInfo {
                        slide: input.slide,
                        time: input.time,
                        at_end: input.at_end,
                    },
                    reached: None,
                    slide_count: 0,
                    slide_names: Vec::new(),
                    transcript: Vec::new(),
                };
            }
        };
        let requested = timestamp_info(requested_timestamp);

        match build_scene(&input.scene) {
            BuildSceneResult::Failed { diagnostics } => SeekOutput {
                ok: false,
                diagnostics,
                runtime_errors: Vec::new(),
                seek_error: None,
                requested,
                reached: None,
                slide_count: 0,
                slide_names: Vec::new(),
                transcript: Vec::new(),
            },
            BuildSceneResult::Ready(mut prepared) => {
                let slide_count = prepared.executor.real_slide_count();
                let slide_names = prepared.executor.real_slide_names();

                if let Some(message) = validate_requested_slide(requested_timestamp, slide_count) {
                    return SeekOutput {
                        ok: false,
                        diagnostics: prepared.diagnostics,
                        runtime_errors: Vec::new(),
                        seek_error: Some(message),
                        requested,
                        reached: None,
                        slide_count,
                        slide_names,
                        transcript: Vec::new(),
                    };
                }

                let internal_target = prepared
                    .executor
                    .user_to_internal_timestamp(requested_timestamp);
                let mut seek_error = None;
                let reached = match smol::block_on(prepared.executor.seek_to(internal_target)) {
                    SeekToResult::SeekedTo(timestamp) => Some(timestamp_info(
                        prepared.executor.internal_to_user_timestamp(timestamp),
                    )),
                    SeekToResult::Error(error) => {
                        seek_error = Some(error.to_string());
                        Some(timestamp_info(
                            prepared
                                .executor
                                .internal_to_user_timestamp(prepared.executor.state.timestamp),
                        ))
                    }
                };

                let runtime_errors = runtime_errors(&prepared.executor, &prepared.root_text_rope);
                let transcript = transcript_entries(&prepared.executor, &prepared.root_text_rope);
                let ok = !has_errors(&prepared.diagnostics)
                    && runtime_errors.is_empty()
                    && seek_error.is_none();

                SeekOutput {
                    ok,
                    diagnostics: prepared.diagnostics,
                    runtime_errors,
                    seek_error,
                    requested,
                    reached,
                    slide_count,
                    slide_names,
                    transcript,
                }
            }
        }
    }
}

impl ServerHandler for MonocurlDocs {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_resources()
                .enable_tools()
                .build(),
        )
        .with_server_info(
            Implementation::new("monocurl-mcp", env!("CARGO_PKG_VERSION"))
                .with_title("Monocurl Documentation"),
        )
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
        .with_instructions(
            "Use resources/list and resources/read to load Monocurl language semantics, stdlib documentation, and raw stdlib wrapper sources. Use monocurl_check before proposing source changes, and monocurl_seek when transcript/runtime feedback is useful."
                .to_string(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: Self::list_doc_resources(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let Some(resource) = Self::find_resource(&request.uri) else {
            return Err(McpError::resource_not_found(
                "resource_not_found",
                Some(json!({ "uri": request.uri })),
            ));
        };

        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(resource.text, resource.uri).with_mime_type(resource.mime_type),
        ]))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: Vec::new(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: Self::tools(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let CallToolRequestParams {
            name, arguments, ..
        } = request;

        match name.as_ref() {
            TOOL_CHECK_NAME => {
                let input = parse_arguments(arguments)?;
                structured_result(Self::run_check(input))
            }
            TOOL_SEEK_NAME => {
                let input = parse_arguments(arguments)?;
                structured_result(Self::run_seek(input))
            }
            _ => Err(McpError::invalid_params(
                format!("unknown Monocurl tool `{name}`"),
                None,
            )),
        }
    }
}

fn default_root_path() -> String {
    "scene.mcl".into()
}

fn read_only_tool_annotations() -> ToolAnnotations {
    ToolAnnotations::new()
        .read_only(true)
        .destructive(false)
        .idempotent(true)
        .open_world(false)
}

fn check_input_schema() -> JsonObject {
    object(json!({
        "type": "object",
        "properties": common_scene_input_properties(),
        "required": ["source"],
        "additionalProperties": false
    }))
}

fn seek_input_schema() -> JsonObject {
    let mut properties = common_scene_input_properties();
    properties.insert(
        "slide".into(),
        json!({
            "type": "integer",
            "minimum": 0,
            "description": "User-visible slide number. Visible slides are 1-based; slide 0 with atEnd=true is the pre-scene boundary."
        }),
    );
    properties.insert(
        "time".into(),
        json!({
            "type": ["number", "null"],
            "minimum": 0,
            "description": "Seconds into the requested slide. Defaults to 0. Ignored when atEnd is true."
        }),
    );
    properties.insert(
        "atEnd".into(),
        json!({
            "type": "boolean",
            "description": "Seek to the end of the requested slide instead of a finite offset."
        }),
    );

    object(json!({
        "type": "object",
        "properties": properties,
        "required": ["source", "slide"],
        "additionalProperties": false
    }))
}

fn common_scene_input_properties() -> JsonObject {
    object(json!({
        "source": {
            "type": "string",
            "description": "Complete Monocurl scene source for the root document."
        },
        "rootPath": {
            "type": "string",
            "description": "Path to use for the root scene. Import resolution uses its parent directory. Defaults to scene.mcl."
        },
        "openDocuments": {
            "type": "object",
            "additionalProperties": { "type": "string" },
            "description": "Unsaved imported document contents keyed by path. Relative paths are resolved against rootPath's parent."
        }
    }))
}

fn parse_arguments<T>(arguments: Option<JsonObject>) -> Result<T, McpError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(|err| {
        McpError::invalid_params(format!("invalid Monocurl tool arguments: {err}"), None)
    })
}

fn structured_result<T>(output: T) -> Result<CallToolResult, McpError>
where
    T: Serialize,
{
    let value = serde_json::to_value(output).map_err(|err| {
        McpError::invalid_params(
            format!("failed to serialize Monocurl tool result: {err}"),
            None,
        )
    })?;
    Ok(CallToolResult::structured(value))
}

fn build_scene(input: &SceneInput) -> BuildSceneResult {
    let root_path = PathBuf::from(&input.root_path);
    let root_text_rope = Rope::from_str(&input.source);
    let root_lex_rope = lex_rope_from_str(&input.source);

    let mut import_context = ParseImportContext {
        root_file_path: root_path.clone(),
        open_tab_ropes: open_tab_ropes(&root_path, &input.open_documents),
        cached_parses: HashMap::new(),
    };

    let (bundles, parse_artifacts) = Parser::parse(
        &mut import_context,
        root_lex_rope,
        root_text_rope.clone(),
        None,
    );

    let mut diagnostics: Vec<_> = parse_artifacts
        .error_diagnostics
        .iter()
        .map(|diagnostic| parse_diagnostic(diagnostic, &root_text_rope))
        .collect();

    if has_errors(&diagnostics) {
        return BuildSceneResult::Failed { diagnostics };
    }

    let mut compiler_cache = CompilerCache::default();
    let compile_result = compile(&mut compiler_cache, None, &bundles);
    let has_compile_errors = !compile_result.errors.is_empty();

    diagnostics.extend(
        compile_result
            .errors
            .iter()
            .map(|error| compile_error(error, &root_text_rope)),
    );
    diagnostics.extend(
        compile_result
            .warnings
            .iter()
            .map(|warning| compile_warning(warning, &root_text_rope)),
    );

    if has_compile_errors {
        return BuildSceneResult::Failed { diagnostics };
    }

    let executor = Executor::new(compile_result.bytecode, registry().func_table());
    BuildSceneResult::Ready(PreparedScene {
        executor,
        root_text_rope,
        diagnostics,
    })
}

fn open_tab_ropes(
    root_path: &Path,
    open_documents: &BTreeMap<String, String>,
) -> HashMap<PathBuf, (Rope<Attribute<Token>>, Rope<TextAggregate>)> {
    let mut ropes = HashMap::new();
    let stdlib_root = Assets::std_lib();

    for (path, text) in [
        ("std/util.mcl", STD_UTIL),
        ("std/math.mcl", STD_MATH),
        ("std/color.mcl", STD_COLOR),
        ("std/mesh.mcl", STD_MESH),
        ("std/anim.mcl", STD_ANIM),
        ("std/scene.mcl", STD_SCENE),
    ] {
        insert_open_document(&mut ropes, stdlib_root.join(path), text);
    }

    let root_parent = root_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    for (path, text) in open_documents {
        let path = normalize_open_document_path(root_parent, path);
        insert_open_document(&mut ropes, path, text);
    }

    ropes
}

fn normalize_open_document_path(root_parent: Option<&Path>, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_relative()
        && let Some(root_parent) = root_parent
    {
        return root_parent.join(path);
    }
    path
}

fn insert_open_document(
    ropes: &mut HashMap<PathBuf, (Rope<Attribute<Token>>, Rope<TextAggregate>)>,
    path: PathBuf,
    text: &str,
) {
    ropes.insert(path, (lex_rope_from_str(text), Rope::from_str(text)));
}

fn lex_rope_from_str(text: &str) -> Rope<Attribute<Token>> {
    Rope::default().replace_range(
        0..0,
        Lexer::new(text.chars()).map(|(attribute, codeunits)| RLEData {
            codeunits,
            attribute,
        }),
    )
}

fn parse_diagnostic(
    diagnostic: &Diagnostic,
    root_text_rope: &Rope<TextAggregate>,
) -> ToolDiagnostic {
    ToolDiagnostic {
        phase: "parse",
        severity: "error",
        title: diagnostic.title.clone(),
        message: diagnostic.message.clone(),
        span: span_info(root_text_rope, &diagnostic.span),
    }
}

fn compile_error(error: &CompileError, root_text_rope: &Rope<TextAggregate>) -> ToolDiagnostic {
    ToolDiagnostic {
        phase: "compile",
        severity: "error",
        title: "Compile error".into(),
        message: error.message.clone(),
        span: span_info(root_text_rope, &error.span),
    }
}

fn compile_warning(
    warning: &CompileWarning,
    root_text_rope: &Rope<TextAggregate>,
) -> ToolDiagnostic {
    ToolDiagnostic {
        phase: "compile",
        severity: "warning",
        title: "Compile warning".into(),
        message: warning.message.clone(),
        span: span_info(root_text_rope, &warning.span),
    }
}

fn span_info(text_rope: &Rope<TextAggregate>, span: &Span8) -> SpanInfo {
    let start = text_rope.utf8_prefix_summary(span.start);
    let end = text_rope.utf8_prefix_summary(span.end);
    SpanInfo {
        start: span.start,
        end: span.end,
        line: start.newlines + 1,
        column: start.bytes_utf8_since_newline,
        end_line: end.newlines + 1,
        end_column: end.bytes_utf8_since_newline,
    }
}

fn has_errors(diagnostics: &[ToolDiagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error")
}

fn requested_timestamp(input: &SeekInput) -> Result<Timestamp, String> {
    if input.at_end {
        return Ok(Timestamp::at_end_of_slide(input.slide));
    }

    if input.slide == 0 {
        return Err("slide 0 only supports atEnd=true".into());
    }

    let time = input.time.unwrap_or(0.0);
    if !time.is_finite() || time < 0.0 {
        return Err("time must be a finite, non-negative number".into());
    }

    Ok(Timestamp::new(input.slide, time))
}

fn validate_requested_slide(target: Timestamp, slide_count: usize) -> Option<String> {
    if target.slide == 0 && target.time.is_infinite() {
        return None;
    }

    if target.slide == 0 {
        return Some("slide 0 only supports atEnd=true".into());
    }

    if target.slide > slide_count {
        return Some(format!(
            "requested slide {} but the scene only has {} visible slide(s)",
            target.slide, slide_count
        ));
    }

    None
}

fn timestamp_info(timestamp: Timestamp) -> TimestampInfo {
    TimestampInfo {
        slide: timestamp.slide,
        time: timestamp.time.is_finite().then_some(timestamp.time),
        at_end: !timestamp.time.is_finite(),
    }
}

fn transcript_entries(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
) -> Vec<TranscriptEntryOutput> {
    executor
        .state
        .transcript
        .iter_entries()
        .map(|entry| TranscriptEntryOutput {
            text: entry.text().to_string(),
            span: span_info(root_text_rope, &entry.span),
            section: entry.section as usize,
            is_root: entry.is_root,
        })
        .collect()
}

fn runtime_errors(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
) -> Vec<RuntimeErrorOutput> {
    executor
        .state
        .errors
        .iter()
        .map(|error| runtime_error(error, executor, root_text_rope))
        .collect()
}

fn runtime_error(
    error: &RuntimeError,
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
) -> RuntimeErrorOutput {
    RuntimeErrorOutput {
        message: error.to_string(),
        span: span_info(root_text_rope, &error.span),
        hint: error.hint.clone(),
        callstack: error
            .callstack
            .iter()
            .map(|frame| runtime_call_frame(frame, executor, root_text_rope))
            .collect(),
    }
}

fn runtime_call_frame(
    frame: &RuntimeCallFrame,
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
) -> RuntimeCallFrameOutput {
    let section = frame.section as usize;
    let is_root = executor
        .sections()
        .get(section)
        .is_some_and(|section| section.flags.is_root_module);

    RuntimeCallFrameOutput {
        section,
        section_label: section_label(executor, section),
        is_root,
        span: span_info(root_text_rope, &frame.span),
    }
}

fn section_label(executor: &Executor, section_idx: usize) -> String {
    let Some(section) = executor.sections().get(section_idx) else {
        return format!("<section {section_idx}>");
    };

    if section.flags.is_init {
        let ordinal = executor.sections()[..=section_idx]
            .iter()
            .filter(|section| section.flags.is_root_module && section.flags.is_init)
            .count();
        if ordinal <= 1 {
            "<init>".into()
        } else {
            format!("<init {ordinal}>")
        }
    } else if section.flags.is_library {
        "<prelude>".into()
    } else if section.flags.is_root_module {
        let ordinal = executor.sections()[..=section_idx]
            .iter()
            .filter(|section| {
                section.flags.is_root_module && !section.flags.is_library && !section.flags.is_init
            })
            .count();
        format!("<slide {ordinal}>")
    } else if let Some(name) = &section.source_file_name {
        format!("<{name}>")
    } else if let Some(index) = section.import_display_index {
        format!("<imported library {index}>")
    } else {
        "<imported library>".into()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let service = MonocurlDocs.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_language_and_stdlib_resources() {
        let resources = MonocurlDocs::list_doc_resources();
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://docs/language-semantics")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://docs/stdlib")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://stdlib/mesh")
        );
    }

    #[test]
    fn finds_embedded_stdlib_resource() {
        let resource = MonocurlDocs::find_resource("monocurl://stdlib/mesh").unwrap();
        assert!(resource.text.contains("let Circle"));
        assert!(resource.text.contains("let Text"));
    }

    #[test]
    fn reports_unknown_resource() {
        assert!(MonocurlDocs::find_resource("monocurl://missing").is_none());
    }

    #[test]
    fn lists_check_and_seek_tools() {
        let tools = MonocurlDocs::tools();
        assert!(tools.iter().any(|tool| tool.name == TOOL_CHECK_NAME));
        assert!(tools.iter().any(|tool| tool.name == TOOL_SEEK_NAME));
    }

    #[test]
    fn check_reports_compile_errors() {
        let output = MonocurlDocs::run_check(scene_input("slide\nprint missing_value\n"));
        assert!(!output.ok);
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.phase == "compile"
                && diagnostic.severity == "error"
                && diagnostic.message.contains("missing_value")
        }));
    }

    #[test]
    fn seek_returns_transcript() {
        let output = MonocurlDocs::run_seek(SeekInput {
            scene: scene_input("slide\nprint 1 + 2\n"),
            slide: 1,
            time: None,
            at_end: true,
        });

        assert!(output.ok, "{:?}", output);
        assert_eq!(output.slide_count, 1);
        assert_eq!(
            output
                .transcript
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>(),
            vec!["3"]
        );
    }

    fn scene_input(source: &str) -> SceneInput {
        SceneInput {
            source: source.into(),
            root_path: default_root_path(),
            open_documents: BTreeMap::new(),
        }
    }
}
