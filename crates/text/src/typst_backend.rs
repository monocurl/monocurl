//! Native-only Typst markup backend.
//!
//! Compiles a snippet of Typst markup to a tightly cropped SVG string that the
//! shared SVG -> mesh importer (`crate::svg`) can consume. The heavy lifting is
//! done by the `typst` compiler; `typst-as-lib` supplies a minimal `World`
//! implementation (fonts + source resolution), and `typst-svg` renders the
//! compiled `PagedDocument` to SVG.

use std::{
    borrow::Cow,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

use anyhow::{Result, anyhow, bail};
use typst::{
    diag::{FileError, FileResult},
    foundations::Bytes,
    layout::Abs,
    syntax::{FileId, Source},
};
use typst_as_lib::{
    TypstEngine, TypstTemplateCollection, conversions::IntoFileId, file_resolver::FileResolver,
};
use typst_layout::PagedDocument;
use typst_svg::SvgOptions;

/// Preamble prepended to every user snippet.
///
/// * `width`/`height: auto` + `margin: 0pt` make the page shrink-wrap the
///   content so the resulting SVG is tightly cropped (same spirit as the
///   `standalone`/`preview` LaTeX document class used by the TeX backend).
/// * `fill: none` keeps the page background transparent so no stray background
///   rectangle is imported as a mesh.
const TYPST_PREAMBLE: &str = "\
#set page(width: auto, height: auto, margin: 0pt, fill: none)
#set text(size: 10pt)
";

/// Virtual path of the synthetic main source file.
const MAIN_PATH: &str = "/__monocurl_typst_main.typ";

/// Source text for the compile currently in flight.
///
/// Every `render_typst_svg` call takes `TYPST_LOCK` before touching this, so the
/// custom [`FileResolver`] only ever observes a fully-written value belonging to
/// the caller that holds the lock -- no data race despite the resolver living
/// inside the shared engine.
static CURRENT_SOURCE: Mutex<String> = Mutex::new(String::new());

/// Serialises Typst compilation. Typst leans on a process-global `comemo` cache;
/// serialising keeps that (and [`CURRENT_SOURCE`]) sane, and matches the
/// effectively-serial nature of the tectonic LaTeX backend.
static TYPST_LOCK: Mutex<()> = Mutex::new(());

/// Bundled font data handed to the Typst compiler.
///
/// Includes the `typst-assets` default set (New Computer Modern, Libertinus
/// Serif, DejaVu Sans Mono, ...) so stock Typst documents render as they do on
/// typst.app, plus the fonts Monocurl already ships under `assets/font/`.
fn bundled_fonts() -> Vec<Bytes> {
    let mut fonts: Vec<Bytes> = typst_assets::fonts().map(Bytes::new).collect();
    fonts.push(Bytes::new(
        include_bytes!("../../../assets/font/IBMPlexMono-Regular.ttf").as_slice(),
    ));
    fonts.push(Bytes::new(
        include_bytes!("../../../assets/font/IBMPlexMono-Italic.ttf").as_slice(),
    ));
    fonts.push(Bytes::new(
        include_bytes!("../../../assets/font/Lilex-Regular.ttf").as_slice(),
    ));
    fonts
}

/// A [`FileResolver`] that serves [`CURRENT_SOURCE`] for the synthetic main file
/// and nothing else (imports / packages / local files are intentionally
/// unsupported for inline snippets).
struct CurrentSourceResolver {
    main_id: FileId,
}

impl FileResolver for CurrentSourceResolver {
    fn resolve_binary(&self, id: FileId) -> FileResult<Cow<'_, Bytes>> {
        Err(FileError::NotFound(id_path(id)))
    }

    fn resolve_source(&self, id: FileId) -> FileResult<Cow<'_, Source>> {
        if id == self.main_id {
            let text = CURRENT_SOURCE.lock().unwrap_or_else(|e| e.into_inner()).clone();
            Ok(Cow::Owned(Source::new(id, text)))
        } else {
            Err(FileError::NotFound(id_path(id)))
        }
    }
}

fn id_path(id: FileId) -> PathBuf {
    PathBuf::from(id.vpath().get_without_slash())
}

struct SharedEngine {
    engine: TypstEngine<TypstTemplateCollection>,
    main_id: FileId,
}

/// The Typst engine is built once (fonts parsed once, `FontBook` derived once)
/// and reused for every render. `comemo_evict_max_age(Some(30))` keeps the
/// process-global memo cache from growing without bound while still letting font
/// parsing be reused across calls.
fn shared_engine() -> &'static SharedEngine {
    static ENGINE: OnceLock<SharedEngine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let main_id = MAIN_PATH.into_file_id();
        let mut builder = TypstEngine::builder().fonts(bundled_fonts());
        builder.comemo_evict_max_age(Some(30));
        let engine = builder
            .add_file_resolver(CurrentSourceResolver { main_id })
            .build();
        SharedEngine { engine, main_id }
    })
}

/// Compile Typst `markup` to an SVG string.
///
/// The snippet is wrapped in [`TYPST_PREAMBLE`]; any Typst error (parse, type,
/// layout) is surfaced as an `anyhow` error with the collected diagnostics.
pub fn render_typst_svg(markup: &str) -> Result<String> {
    let shared = shared_engine();
    let source = format!("{TYPST_PREAMBLE}\n{markup}");

    let _guard: MutexGuard<'_, ()> = TYPST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    *CURRENT_SOURCE.lock().unwrap_or_else(|e| e.into_inner()) = source;

    let doc: PagedDocument = shared
        .engine
        .compile(shared.main_id)
        .output
        .map_err(|error| anyhow!("typst compilation failed: {error}"))?;

    if doc.pages().is_empty() {
        bail!("typst produced no pages");
    }

    Ok(typst_svg::svg_merged(&doc, &SvgOptions::default(), Abs::zero()))
}
