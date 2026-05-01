use std::{
    collections::HashMap,
    fmt::Write as _,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use geo::mesh::Mesh;
use sha2::{Digest, Sha256};

use crate::{
    svg,
    types::{BackendKind, LatexBackendConfig, RenderQuality, RenderedOutput},
};

const SYSTEM_SVG_UNITS_AT_SCALE_1: f32 = 36.0;
const LATEX_SVG_CACHE_VERSION: &[u8] = b"monocurl-latex-svg-cache-v1";
const LATEX_SVG_FILE_CACHE_MAX_AGE_DAYS: u64 = 30;
const LATEX_SVG_FILE_CACHE_MAX_AGE: Duration =
    Duration::from_secs(60 * 60 * 24 * LATEX_SVG_FILE_CACHE_MAX_AGE_DAYS);

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

pub fn clean_stale_file_cache() -> Result<usize> {
    let cutoff = SystemTime::now()
        .checked_sub(LATEX_SVG_FILE_CACHE_MAX_AGE)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    clean_latex_svg_file_cache_before(&latex_svg_cache_root(), cutoff)
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

fn clean_latex_svg_file_cache_before(root: &Path, cutoff: SystemTime) -> Result<usize> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read LaTeX SVG cache root {}", root.display())
            });
        }
    };

    let mut removed = 0;
    for entry in entries {
        let entry = entry.with_context(|| {
            format!(
                "failed to read LaTeX SVG cache entry under {}",
                root.display()
            )
        })?;
        let path = entry.path();
        let file_type = entry.file_type().with_context(|| {
            format!("failed to inspect LaTeX SVG cache entry {}", path.display())
        })?;

        if file_type.is_dir() {
            removed += clean_latex_svg_file_cache_before(&path, cutoff)?;
            remove_empty_cache_dir(&path);
        } else if file_type.is_file() && is_latex_svg_cache_file(&path) {
            let metadata = entry.metadata().with_context(|| {
                format!("failed to inspect LaTeX SVG cache file {}", path.display())
            })?;
            if metadata.modified().is_ok_and(|modified| modified < cutoff) {
                fs::remove_file(&path).with_context(|| {
                    format!(
                        "failed to remove stale LaTeX SVG cache file {}",
                        path.display()
                    )
                })?;
                removed += 1;
            }
        }
    }

    Ok(removed)
}

fn is_latex_svg_cache_file(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("svg")
}

fn remove_empty_cache_dir(path: &Path) {
    let _ = fs::remove_dir(path);
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
    use std::{
        path::PathBuf,
        time::{Duration, SystemTime},
    };

    use super::*;

    fn test_cache_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "monocurl-latex-cache-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

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

    #[test]
    fn clean_latex_svg_file_cache_removes_svg_files_before_cutoff() {
        let root = test_cache_root("remove");
        let stale_dir = root.join("ab");
        let mixed_dir = root.join("cd");
        fs::create_dir_all(&stale_dir).unwrap();
        fs::create_dir_all(&mixed_dir).unwrap();

        let stale_svg = stale_dir.join("cdef.svg");
        let stale_text = mixed_dir.join("keep.txt");
        fs::write(&stale_svg, "<svg/>").unwrap();
        fs::write(&stale_text, "not cache").unwrap();

        let cutoff = SystemTime::now() + Duration::from_secs(60);
        let removed = clean_latex_svg_file_cache_before(&root, cutoff).unwrap();

        assert_eq!(removed, 1);
        assert!(!stale_svg.exists());
        assert!(!stale_dir.exists());
        assert!(stale_text.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clean_latex_svg_file_cache_keeps_svg_files_after_cutoff() {
        let root = test_cache_root("keep");
        let cache_dir = root.join("ab");
        fs::create_dir_all(&cache_dir).unwrap();

        let fresh_svg = cache_dir.join("cdef.svg");
        fs::write(&fresh_svg, "<svg/>").unwrap();

        let removed = clean_latex_svg_file_cache_before(&root, SystemTime::UNIX_EPOCH).unwrap();

        assert_eq!(removed, 0);
        assert!(fresh_svg.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn clean_latex_svg_file_cache_missing_root_is_noop() {
        let root = test_cache_root("missing");
        fs::remove_dir_all(&root).unwrap();

        let removed = clean_latex_svg_file_cache_before(&root, SystemTime::now()).unwrap();

        assert_eq!(removed, 0);
    }
}
