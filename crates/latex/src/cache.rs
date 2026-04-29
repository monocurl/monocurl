use std::{
    collections::HashMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Result;
use geo::mesh::Mesh;
use sha2::{Digest, Sha256};

use crate::{
    svg,
    types::{BackendKind, LatexBackendConfig, RenderQuality, RenderedOutput},
};

const SYSTEM_SVG_UNITS_AT_SCALE_1: f32 = 36.0;
const LATEX_SVG_CACHE_VERSION: &[u8] = b"monocurl-latex-svg-cache-v1";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    backend: BackendKind,
    backend_config: LatexBackendConfig,
    source: String,
    scale_bits: u32,
    quality: RenderQuality,
}

#[derive(Clone)]
struct CacheEntry {
    meshes: Arc<Vec<Arc<Mesh>>>,
    span_mesh_indices: Arc<HashMap<String, Vec<usize>>>,
}

static CACHE: OnceLock<Mutex<HashMap<CacheKey, CacheEntry>>> = OnceLock::new();

pub(crate) fn clear_memory_cache() {
    cache().lock().unwrap().clear();
}

pub(crate) fn render_cached<F>(
    backend: BackendKind,
    backend_config: LatexBackendConfig,
    source: String,
    scale: f32,
    quality: RenderQuality,
    render: F,
) -> Result<RenderedOutput>
where
    F: FnOnce(String) -> Result<svg::RenderedSvg>,
{
    let key = CacheKey {
        backend,
        backend_config,
        source: source.clone(),
        scale_bits: scale.to_bits(),
        quality,
    };

    if let Some(entry) = cache().lock().unwrap().get(&key).cloned() {
        return Ok(RenderedOutput {
            meshes: entry.meshes.iter().cloned().collect(),
            span_mesh_indices: (*entry.span_mesh_indices).clone(),
        });
    }

    let rendered = render(source)?;
    let entry = CacheEntry {
        meshes: Arc::new(rendered.meshes.into_iter().map(Arc::new).collect()),
        span_mesh_indices: Arc::new(rendered.span_mesh_indices),
    };

    cache().lock().unwrap().insert(key, entry.clone());

    Ok(RenderedOutput {
        meshes: entry.meshes.iter().cloned().collect(),
        span_mesh_indices: (*entry.span_mesh_indices).clone(),
    })
}

pub(crate) fn render_svg_with_file_cache<F>(
    backend_config: &LatexBackendConfig,
    source: &str,
    scale: f32,
    quality: RenderQuality,
    render_svg: F,
) -> Result<svg::RenderedSvg>
where
    F: FnOnce(&str) -> Result<String>,
{
    let cache_path = latex_svg_cache_path(backend_config, source);
    if let Ok(svg_source) = fs::read_to_string(&cache_path) {
        if let Ok(rendered) = import_latex_svg(&svg_source, scale, quality) {
            return Ok(rendered);
        }
    }

    let svg_source = render_svg(source)?;
    let rendered = import_latex_svg(&svg_source, scale, quality)?;
    write_latex_svg_file_cache(&cache_path, &svg_source);
    Ok(rendered)
}

fn import_latex_svg(
    svg_source: &str,
    scale: f32,
    quality: RenderQuality,
) -> Result<svg::RenderedSvg> {
    svg::import(
        svg_source,
        scale / SYSTEM_SVG_UNITS_AT_SCALE_1,
        svg_import_options(quality, true),
    )
}

fn latex_svg_cache_path(backend_config: &LatexBackendConfig, source: &str) -> PathBuf {
    latex_svg_cache_path_for_hash(&latex_svg_cache_hash(backend_config, source))
}

fn latex_svg_cache_path_for_hash(hash: &str) -> PathBuf {
    let (dir, file) = hash.split_at(2);
    latex_svg_cache_root().join(dir).join(format!("{file}.svg"))
}

fn latex_svg_cache_root() -> PathBuf {
    std::env::temp_dir().join("monocurl").join("latex_svg")
}

fn latex_svg_cache_hash(backend_config: &LatexBackendConfig, source: &str) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, b"version", LATEX_SVG_CACHE_VERSION);
    hash_backend_config(&mut hasher, backend_config);
    hash_field(&mut hasher, b"source", source.as_bytes());

    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut out, "{byte:02x}").unwrap();
    }
    out
}

fn hash_backend_config(hasher: &mut Sha256, backend_config: &LatexBackendConfig) {
    match backend_config {
        LatexBackendConfig::Bundled => hash_field(hasher, b"backend", b"bundled"),
        LatexBackendConfig::System(config) => {
            hash_field(hasher, b"backend", b"system");
            hash_path_field(hasher, b"system.latex", &config.latex);
            hash_path_field(hasher, b"system.dvisvgm", &config.dvisvgm);
        }
    }
}

fn hash_path_field(hasher: &mut Sha256, label: &[u8], path: &Path) {
    hash_field(hasher, label, path.to_string_lossy().as_bytes());
}

fn hash_field(hasher: &mut Sha256, label: &[u8], value: &[u8]) {
    hasher.update((label.len() as u64).to_le_bytes());
    hasher.update(label);
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn write_latex_svg_file_cache(path: &Path, svg_source: &str) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_ok() {
        let _ = fs::write(path, svg_source);
    }
}

fn cache() -> &'static Mutex<HashMap<CacheKey, CacheEntry>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn svg_import_options(quality: RenderQuality, flip_y: bool) -> svg::ImportOptions {
    svg::ImportOptions {
        curve_sampling: match quality {
            RenderQuality::Normal => svg::CurveSampling::Normal,
            RenderQuality::High => svg::CurveSampling::High,
        },
        flip_y,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn latex_svg_cache_hashes_are_stable_hex_keys() {
        let hash = latex_svg_cache_hash(&LatexBackendConfig::Bundled, "x + y");
        assert_eq!(
            hash,
            "ad6225c2aea4fe96fb4073bfbc9dc9ff6e78553bd50df9ae611507a7b4e1e32e"
        );
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|ch| matches!(ch, '0'..='9' | 'a'..='f')));
        assert_eq!(
            hash,
            latex_svg_cache_hash(&LatexBackendConfig::Bundled, "x + y")
        );
        assert_ne!(
            hash,
            latex_svg_cache_hash(&LatexBackendConfig::Bundled, "x + z")
        );
        assert_ne!(
            hash,
            latex_svg_cache_hash(
                &LatexBackendConfig::System(crate::SystemBackendConfig {
                    latex: PathBuf::from("/usr/bin/latex"),
                    dvisvgm: PathBuf::from("/usr/bin/dvisvgm"),
                }),
                "x + y",
            )
        );
    }

    #[test]
    fn latex_svg_cache_path_uses_git_object_layout() {
        let path = latex_svg_cache_path_for_hash("abcdef");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("cdef.svg")
        );
        let dir = path.parent().and_then(|path| path.file_name());
        assert_eq!(dir.and_then(|name| name.to_str()), Some("ab"));
        let root = path
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.file_name());
        assert_eq!(root.and_then(|name| name.to_str()), Some("latex_svg"));
    }
}
