use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use anyhow::{Result, anyhow, bail};
use hayro_interpret::{InterpreterSettings, Pdf};
use tectonic::{
    config::PersistentConfig,
    driver::{OutputFormat, ProcessingSessionBuilder},
    io::{InputHandle, IoProvider, OpenResult},
    status::{NoopStatusBackend, StatusBackend},
};
use tectonic_bundles::{Bundle, dir::DirBundle, zip::ZipBundle};

pub(crate) fn is_available() -> bool {
    true
}

pub(crate) fn bundle_is_available() -> bool {
    local_bundle_path().is_some()
}

pub(crate) fn render_svg_document(document: &str) -> Result<String> {
    let pdf = render_pdf_document(document)?;
    pdf_to_svg(pdf)
}

fn render_pdf_document(document: &str) -> Result<Vec<u8>> {
    let mut status = NoopStatusBackend::default();
    let config = PersistentConfig::open(false)
        .map_err(|e| anyhow!("failed to open tectonic config: {e}"))?;

    let bundle: Box<dyn Bundle> = if let Some(bundle_path) = local_bundle_path() {
        Box::new(LocalThenDefaultBundle::new(open_local_bundle(
            &bundle_path,
        )?))
    } else {
        config
            .default_bundle(false, &mut status)
            .map_err(|e| anyhow!("failed to load tectonic bundle: {e}"))?
    };

    let format_cache_path = config
        .format_cache_path()
        .map_err(|e| anyhow!("failed to get format cache path: {e}"))?;

    let mut sb = ProcessingSessionBuilder::default();
    sb.bundle(bundle)
        .primary_input_buffer(document.as_bytes())
        .tex_input_name("texput.tex")
        .format_name("latex")
        .format_cache_path(format_cache_path)
        .keep_logs(false)
        .keep_intermediates(false)
        .print_stdout(false)
        .output_format(OutputFormat::Pdf)
        .do_not_write_output_files();

    let mut sess = sb
        .create(&mut status)
        .map_err(|e| anyhow!("failed to initialize LaTeX processing: {e}"))?;
    sess.run(&mut status)
        .map_err(|e| anyhow!("LaTeX engine failed: {e}"))?;

    sess.into_file_data()
        .remove("texput.pdf")
        .map(|f| f.data)
        .ok_or_else(|| anyhow!("LaTeX engine produced no output"))
}

fn open_local_bundle(path: &Path) -> Result<Box<dyn Bundle>> {
    if path.is_dir() {
        return Ok(Box::new(DirBundle::new(path)));
    }
    match path.extension().and_then(|e| e.to_str()) {
        Some("zip") => {
            let bundle = ZipBundle::open(path)
                .map_err(|e| anyhow!("failed to open zip bundle {}: {e}", path.display()))?;
            Ok(Box::new(bundle))
        }
        _ => Err(anyhow!(
            "unrecognized bundle format at {} (expected a directory or .zip file)",
            path.display()
        )),
    }
}

fn pdf_to_svg(pdf_data: Vec<u8>) -> Result<String> {
    let pdf = Pdf::new(Arc::new(pdf_data))
        .map_err(|error| anyhow!("failed to parse generated PDF: {error:?}"))?;
    let pages = pdf.pages();
    let mut pages = pages.iter();
    let page = pages
        .next()
        .ok_or_else(|| anyhow!("generated PDF contained no pages"))?;
    if pages.next().is_some() {
        bail!("generated LaTeX output had more than one PDF page");
    }
    let svg = hayro_svg::convert(page, &InterpreterSettings::default());
    Ok(expand_glyph_uses(&svg))
}

struct LocalThenDefaultBundle {
    local: Box<dyn Bundle>,
    fallback: Option<Box<dyn Bundle>>,
    default_index: Option<Arc<HashSet<String>>>,
}

impl LocalThenDefaultBundle {
    fn new(local: Box<dyn Bundle>) -> Self {
        Self {
            local,
            fallback: None,
            default_index: load_default_index(),
        }
    }

    fn ensure_fallback(&mut self, status: &mut dyn StatusBackend) -> Result<()> {
        if self.fallback.is_none() {
            let config = PersistentConfig::open(false)
                .map_err(|e| anyhow!("failed to open tectonic config: {e}"))?;
            let fallback = config
                .default_bundle(false, status)
                .map_err(|e| anyhow!("failed to load fallback tectonic bundle: {e}"))?;
            self.fallback = Some(fallback);
        }
        Ok(())
    }

    fn should_try_fallback(&self, name: &str) -> bool {
        self.default_index
            .as_ref()
            .is_none_or(|index| index.contains(name))
    }
}

impl IoProvider for LocalThenDefaultBundle {
    fn input_open_name(
        &mut self,
        name: &str,
        status: &mut dyn StatusBackend,
    ) -> OpenResult<InputHandle> {
        match self.local.input_open_name(name, status) {
            OpenResult::NotAvailable if self.should_try_fallback(name) => {
                match self.ensure_fallback(status) {
                    Ok(()) => self
                        .fallback
                        .as_mut()
                        .unwrap()
                        .input_open_name(name, status),
                    Err(error) => OpenResult::Err(error),
                }
            }
            result => result,
        }
    }

    fn input_open_name_with_abspath(
        &mut self,
        name: &str,
        status: &mut dyn StatusBackend,
    ) -> OpenResult<(InputHandle, Option<PathBuf>)> {
        match self.local.input_open_name_with_abspath(name, status) {
            OpenResult::NotAvailable if self.should_try_fallback(name) => {
                match self.ensure_fallback(status) {
                    Ok(()) => self
                        .fallback
                        .as_mut()
                        .unwrap()
                        .input_open_name_with_abspath(name, status),
                    Err(error) => OpenResult::Err(error),
                }
            }
            result => result,
        }
    }
}

impl Bundle for LocalThenDefaultBundle {
    fn get_digest(
        &mut self,
        status: &mut dyn StatusBackend,
    ) -> Result<tectonic::io::digest::DigestData> {
        match self.local.get_digest(status) {
            Ok(digest) => Ok(digest),
            Err(_) => {
                self.ensure_fallback(status)?;
                self.fallback.as_mut().unwrap().get_digest(status)
            }
        }
    }

    fn all_files(&mut self, status: &mut dyn StatusBackend) -> Result<Vec<String>> {
        let mut files = self.local.all_files(status)?;
        if let Some(fallback) = self.fallback.as_mut() {
            files.extend(fallback.all_files(status)?);
        }
        Ok(files)
    }
}

fn load_default_index() -> Option<Arc<HashSet<String>>> {
    static DEFAULT_INDEX: OnceLock<Option<Arc<HashSet<String>>>> = OnceLock::new();
    DEFAULT_INDEX
        .get_or_init(|| {
            let index_path = resource_path(&["tectonic", "default_bundle_v33.index"])?;
            let index = fs::read_to_string(index_path).ok()?;
            Some(Arc::new(
                index
                    .lines()
                    .filter_map(|line| line.split_whitespace().next())
                    .map(str::to_owned)
                    .collect(),
            ))
        })
        .clone()
}

// hayro-svg stores glyph outlines in <defs> and references them with <use> elements.
// usvg double-applies the <use> transform when resolving defs references (once via a
// temporary parent.abs_transform adjustment, once by re-reading the element's transform
// attribute in convert_group). For regular <path> elements the transform is applied only
// once. This inconsistency makes it impossible to choose a single flip_y direction that
// works for both glyph and non-glyph paths.
//
// Fix: expand every glyph <use> into an inline <path> with the same transform, so usvg
// sees a uniform set of <path> elements and applies each transform exactly once.
fn expand_glyph_uses(svg: &str) -> String {
    let doc = match roxmltree::Document::parse(svg) {
        Ok(doc) => doc,
        Err(_) => return svg.to_string(),
    };

    // Collect defs path data keyed by element id.
    let mut glyph_defs: HashMap<&str, &str> = HashMap::new();
    let mut defs_range: Option<std::ops::Range<usize>> = None;
    for node in doc.descendants() {
        if !node.is_element() || node.tag_name().name() != "defs" {
            continue;
        }
        if node.attribute("id") != Some("outline-glyph") {
            continue;
        }
        defs_range = Some(node.range());
        for child in node.children() {
            if child.is_element() && child.tag_name().name() == "path" {
                if let (Some(id), Some(d)) = (child.attribute("id"), child.attribute("d")) {
                    glyph_defs.insert(id, d);
                }
            }
        }
    }

    if glyph_defs.is_empty() {
        return svg.to_string();
    }

    // Collect use elements that reference glyph defs, along with their byte ranges and the
    // <path> string they should be replaced with. Work back-to-front so byte offsets stay valid.
    let xlink_ns = "http://www.w3.org/1999/xlink";
    let mut replacements: Vec<(std::ops::Range<usize>, String)> = Vec::new();

    for node in doc.descendants() {
        if !node.is_element() || node.tag_name().name() != "use" {
            continue;
        }
        let href = node
            .attribute((xlink_ns, "href"))
            .or_else(|| node.attribute("href"));
        let Some(href) = href else { continue };
        let id = href.trim_start_matches('#');
        let Some(d) = glyph_defs.get(id) else {
            continue;
        };

        let mut path = format!("<path d=\"{}\"", d);
        for attr in node.attributes() {
            // skip the href attribute itself; copy everything else (transform, fill, …)
            let is_href = attr.name() == "href"
                && (attr.namespace() == Some(xlink_ns) || attr.namespace().is_none());
            if !is_href {
                path.push(' ');
                path.push_str(attr.name());
                path.push_str("=\"");
                path.push_str(attr.value());
                path.push('"');
            }
        }
        path.push_str("/>");
        replacements.push((node.range(), path));
    }

    // Also remove the defs block – its paths are now inlined.
    if let Some(range) = defs_range {
        replacements.push((range, String::new()));
    }

    replacements.sort_by(|a, b| b.0.start.cmp(&a.0.start));

    let mut result = svg.to_string();
    for (range, replacement) in replacements {
        result.replace_range(range, &replacement);
    }
    result
}

fn local_bundle_path() -> Option<PathBuf> {
    env_path("MONOCURL_TECTONIC_BUNDLE")
        .or_else(|| resource_path(&["tectonic", "bundle"]))
        .or_else(|| resource_path(&["tectonic", "bundle.zip"]))
}

fn env_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os(name).map(PathBuf::from)?;
    path.exists().then_some(path)
}

fn resource_path(parts: &[&str]) -> Option<PathBuf> {
    if let Some(path) = asset_dir_path(parts) {
        return Some(path);
    }

    let exe_dir = env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));

    if let Some(exe_dir) = exe_dir {
        #[cfg(target_os = "macos")]
        let mut candidate = exe_dir.join("..").join("Resources").join("assets");
        #[cfg(not(target_os = "macos"))]
        let mut candidate = exe_dir.join("assets");

        for part in parts {
            candidate.push(part);
        }
        if candidate.exists() {
            return Some(candidate);
        }
    }

    let mut candidate = PathBuf::from("assets");
    for part in parts {
        candidate.push(part);
    }
    candidate.exists().then_some(candidate)
}

fn asset_dir_path(parts: &[&str]) -> Option<PathBuf> {
    let mut candidate = PathBuf::from(env::var_os("MONOCURL_ASSETS_DIR")?);
    for part in parts {
        candidate.push(part);
    }
    candidate.exists().then_some(candidate)
}
