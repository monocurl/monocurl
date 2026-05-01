use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt, model::*,
    service::RequestContext, transport::stdio,
};
use serde_json::json;

const LANGUAGE_SEMANTICS: &str = include_str!("../docs/language-semantics.md");
const AI_OVERVIEW: &str = include_str!("../docs/ai-overview.md");
const LANGUAGE_BASICS: &str = include_str!("../docs/language-basics.md");
const MESHES_AND_OPERATORS: &str = include_str!("../docs/meshes-and-operators.md");
const ANIMATIONS: &str = include_str!("../docs/animations.md");
const PARAMS_CAMERA_BACKGROUND: &str = include_str!("../docs/params-camera-background.md");
const DEBUGGING_PATTERNS: &str = include_str!("../docs/debugging-patterns.md");
const CHEAT_SHEET: &str = include_str!("../docs/cheat-sheet.md");
const STDLIB_DOCS: &str = include_str!("../docs/stdlib.md");
const CLI_DOCS: &str = include_str!("../docs/cli.md");
const RIEMANN_RECTANGLES_EXAMPLE: &str = include_str!("../docs/riemann-rectangles.mcs");

const STD_UTIL: &str = include_str!("../docs/std/util.mcl");
const STD_MATH: &str = include_str!("../docs/std/math.mcl");
const STD_COLOR: &str = include_str!("../docs/std/color.mcl");
const STD_MESH: &str = include_str!("../docs/std/mesh.mcl");
const STD_ANIM: &str = include_str!("../docs/std/anim.mcl");
const STD_SCENE: &str = include_str!("../docs/std/scene.mcl");

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
        title: "Monocurl AI Context Index",
        description: "Index of split Monocurl authoring context resources.",
        mime_type: "text/markdown",
        text: LANGUAGE_SEMANTICS,
    },
    DocResource {
        uri: "monocurl://docs/ai-overview",
        name: "ai-overview",
        title: "Monocurl AI Overview",
        description: "Project overview, scene skeleton, init/slide rules, UI notes, and timeline shortcuts.",
        mime_type: "text/markdown",
        text: AI_OVERVIEW,
    },
    DocResource {
        uri: "monocurl://docs/language-basics",
        name: "language-basics",
        title: "Monocurl Language Basics",
        description: "Values, assignment, control flow, lambdas, block accumulation, calls, operators, set_default, and references.",
        mime_type: "text/markdown",
        text: LANGUAGE_BASICS,
    },
    DocResource {
        uri: "monocurl://docs/meshes",
        name: "meshes",
        title: "Monocurl Meshes And Operators",
        description: "Mesh values, mesh trees, tags, filters, and text tags.",
        mime_type: "text/markdown",
        text: MESHES_AND_OPERATORS,
    },
    DocResource {
        uri: "monocurl://docs/animations",
        name: "animations",
        title: "Monocurl Animations",
        description: "Leader/follower semantics, Wait, Set, Lerp, morphs, animation blocks, parallelism, and rates.",
        mime_type: "text/markdown",
        text: ANIMATIONS,
    },
    DocResource {
        uri: "monocurl://docs/params-camera",
        name: "params-camera",
        title: "Monocurl Params, Camera, And Background",
        description: "Params, stateful values, presentation controls, camera, and background.",
        mime_type: "text/markdown",
        text: PARAMS_CAMERA_BACKGROUND,
    },
    DocResource {
        uri: "monocurl://docs/debugging-patterns",
        name: "debugging-patterns",
        title: "Monocurl Debugging, Patterns, And Examples",
        description: "Print transcripts, authoring patterns, anti-patterns, examples to imitate, and formatting conventions.",
        mime_type: "text/markdown",
        text: DEBUGGING_PATTERNS,
    },
    DocResource {
        uri: "monocurl://docs/cheat-sheet",
        name: "cheat-sheet",
        title: "Monocurl Cheat Sheet",
        description: "Compact imports, common constructors, operators, animations, utilities, colors, and scene constants.",
        mime_type: "text/markdown",
        text: CHEAT_SHEET,
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
        uri: "monocurl://docs/cli",
        name: "cli-usage",
        title: "Monocurl Binary and CLI",
        description: "How to launch the shared GUI/CLI binary and run image, video, and transcript commands.",
        mime_type: "text/markdown",
        text: CLI_DOCS,
    },
    DocResource {
        uri: "monocurl://examples/riemann-rectangles",
        name: "riemann-rectangles-example",
        title: "Riemann Rectangles Example Scene",
        description: "Complete Monocurl scene demonstrating graph helpers, tags, text tags, transcript prints, and multi-slide animation flow.",
        mime_type: "text/x-monocurl",
        text: RIEMANN_RECTANGLES_EXAMPLE,
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
}

impl ServerHandler for MonocurlDocs {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_resources().build())
            .with_server_info(
                Implementation::new("monocurl-mcp", env!("CARGO_PKG_VERSION"))
                    .with_title("Monocurl Documentation"),
            )
            .with_protocol_version(ProtocolVersion::V_2025_11_25)
            .with_instructions(
                "Use resources/list and resources/read to load split Monocurl authoring context, stdlib documentation, CLI invocation guidance, and raw stdlib wrapper sources. Validation and execution should be handled outside this documentation server with the monocurl binary."
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
                .any(|resource| resource.raw.uri == "monocurl://docs/ai-overview")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://docs/language-basics")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://docs/animations")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://docs/cheat-sheet")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://docs/stdlib")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://docs/cli")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.raw.uri == "monocurl://examples/riemann-rectangles")
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
    fn finds_split_context_resource() {
        let resource = MonocurlDocs::find_resource("monocurl://docs/language-basics").unwrap();
        assert!(resource.text.contains("Block Accumulation"));
        assert!(resource.text.contains("Reference parameters"));
    }

    #[test]
    fn finds_complete_example_resource() {
        let resource =
            MonocurlDocs::find_resource("monocurl://examples/riemann-rectangles").unwrap();
        assert!(resource.text.contains("slide \"Left Rectangles\""));
        assert!(resource.text.contains("TransSubsetTo"));
    }

    #[test]
    fn reports_unknown_resource() {
        assert!(MonocurlDocs::find_resource("monocurl://missing").is_none());
    }
}
