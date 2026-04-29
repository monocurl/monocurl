mod cache;
mod config;
mod document;
mod number;
mod render;
mod svg;
mod system;
mod tectonic;
mod types;

pub use config::{
    backend_config, discover_system_backend, set_backend_config, system_backend_status,
};
pub use document::SpanMarker;
pub use number::{
    format_number, render_number, render_number_string_with_quality, render_number_with_quality,
};
pub use render::{
    render_latex, render_latex_with_quality, render_tex, render_tex_marked,
    render_tex_marked_with_quality, render_tex_with_quality, render_text, render_text_with_quality,
};
pub use types::{
    LatexBackendConfig, RenderQuality, RenderedOutput, SystemBackendConfig, SystemBackendStatus,
    SystemToolPaths,
};
