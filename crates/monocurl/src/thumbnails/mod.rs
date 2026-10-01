//! cached scene thumbnails for the home screen, rendered one at a time in the background

mod cache;

use std::{
    collections::HashMap,
    fs, mem,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use exporter::{ExportKind, ExportRequest, ExportSettings, ImageExportTimestamp};
use gpui::{App, AppContext, Context, Entity, Global, Priority, RenderImage};
use image::{Frame, imageops::FilterType};
use renderer::RenderSize;
use sha2::{Digest, Sha256};
use structs::assets::Assets;
use ui_cli_shared::doc_type::DocumentType;

use cache::{Fingerprint, Outcome, ThumbnailCache};

pub const THUMBNAIL_WIDTH: u32 = 320;
pub const THUMBNAIL_HEIGHT: u32 = 180;
/// rendered at twice the stored size and downsampled, which keeps thin strokes and text legible
const RENDER_SIZE: RenderSize = RenderSize::new(2 * THUMBNAIL_WIDTH, 2 * THUMBNAIL_HEIGHT);
const SECONDS_INTO_LAST_SLIDE: f64 = 2.0;
const MAX_CACHED: usize = 300;
/// saves arrive every few seconds while typing, so wait for the document to settle
const SAVE_SETTLE: Duration = Duration::from_secs(10);
const BUNDLED_DIR: &str = "thumbnails";
const BUNDLED_MANIFEST: &str = "manifest.json";

struct GlobalThumbnails(Entity<Thumbnails>);

impl Global for GlobalThumbnails {}

enum Job {
    Load(Vec<(PathBuf, PathBuf)>),
    Render {
        source: PathBuf,
        fingerprint: Fingerprint,
        partial: PathBuf,
        image: PathBuf,
    },
}

enum Finished {
    Loaded(Vec<(PathBuf, Arc<RenderImage>)>),
    Rendered {
        source: PathBuf,
        fingerprint: Fingerprint,
        result: Result<Arc<RenderImage>>,
    },
}

pub struct Thumbnails {
    cache: Option<ThumbnailCache>,
    images: HashMap<PathBuf, Arc<RenderImage>>,
    to_load: Vec<PathBuf>,
    /// source to the instant it may be rendered
    to_render: HashMap<PathBuf, Instant>,
    busy: bool,
    wake_at: Option<Instant>,
}

impl Thumbnails {
    pub fn init(cx: &mut App) {
        let thumbnails = cx.new(|_| Self::new());
        cx.set_global(GlobalThumbnails(thumbnails));
    }

    pub fn get(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<GlobalThumbnails>()
            .map(|global| global.0.clone())
    }

    fn new() -> Self {
        let cache = dirs::data_local_dir().map(|dir| {
            let mut cache = ThumbnailCache::open(dir.join("Monocurl").join("thumbnails"));
            cache.prune(MAX_CACHED);
            cache.save();
            cache
        });
        Self {
            cache,
            images: HashMap::new(),
            to_load: Vec::new(),
            to_render: HashMap::new(),
            busy: false,
            wake_at: None,
        }
    }

    pub fn image(&self, source: &Path) -> Option<Arc<RenderImage>> {
        self.images.get(&normalize(source)).cloned()
    }

    /// shows what is cached for each scene and queues the missing or stale ones
    pub fn refresh<'a>(
        &mut self,
        sources: impl IntoIterator<Item = &'a Path>,
        cx: &mut Context<Self>,
    ) {
        let Some(cache) = &mut self.cache else {
            return;
        };

        let now = Instant::now();
        for source in sources.into_iter().filter(|s| is_scene(s)).map(normalize) {
            let Some(fingerprint) = Fingerprint::of(&source) else {
                continue;
            };
            if !self.images.contains_key(&source)
                && !self.to_load.contains(&source)
                && cache.image_path(&source).exists()
            {
                self.to_load.push(source.clone());
            }
            if !cache.is_current(&source, fingerprint) {
                self.to_render.insert(source, now);
            }
        }
        cache.save();
        self.pump(cx);
    }

    /// re-renders a saved scene once it stops changing
    pub fn source_saved(source: &Path, cx: &mut App) {
        if !is_scene(source) {
            return;
        }
        let Some(thumbnails) = Self::get(cx) else {
            return;
        };
        thumbnails.update(cx, |this, cx| {
            this.to_render
                .insert(normalize(source), Instant::now() + SAVE_SETTLE);
            this.pump(cx);
        });
    }

    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(job) = self.next_job(cx) else {
            return;
        };

        self.busy = true;
        cx.spawn(async move |this, cx| {
            let finished = cx
                .background_executor()
                .spawn_with_priority(Priority::Low, async move { job.run() })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.finish(finished, cx);
                this.pump(cx);
            });
        })
        .detach();
    }

    fn next_job(&mut self, cx: &mut Context<Self>) -> Option<Job> {
        let cache = self.cache.as_mut()?;
        if !self.to_load.is_empty() {
            let loads = mem::take(&mut self.to_load)
                .into_iter()
                .map(|source| (cache.image_path(&source), source))
                .collect();
            return Some(Job::Load(loads));
        }

        let now = Instant::now();
        // bundled examples first since they only need a copy
        while let Some(source) = self
            .to_render
            .iter()
            .filter(|(_, at)| **at <= now)
            .min_by_key(|(source, at)| (!is_bundled_example(source), **at))
            .map(|(source, _)| source.clone())
        {
            self.to_render.remove(&source);
            let Some(fingerprint) = Fingerprint::of(&source) else {
                continue;
            };
            if cache.is_current(&source, fingerprint) {
                continue;
            }
            return Some(Job::Render {
                partial: cache.partial_path(&source),
                image: cache.image_path(&source),
                source,
                fingerprint,
            });
        }

        self.schedule_wake(cx);
        None
    }

    fn schedule_wake(&mut self, cx: &mut Context<Self>) {
        let Some(earliest) = self.to_render.values().min().copied() else {
            return;
        };
        if self.wake_at.is_some_and(|at| at <= earliest) {
            return;
        }

        self.wake_at = Some(earliest);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(earliest.saturating_duration_since(Instant::now()))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.wake_at == Some(earliest) {
                    this.wake_at = None;
                }
                this.pump(cx);
            });
        })
        .detach();
    }

    fn finish(&mut self, finished: Finished, cx: &mut Context<Self>) {
        match finished {
            Finished::Loaded(images) => {
                for (source, image) in images {
                    self.show(source, image, cx);
                }
            }
            Finished::Rendered {
                source,
                fingerprint,
                result,
            } => {
                let outcome = match result {
                    Ok(image) => {
                        self.show(source.clone(), image, cx);
                        Outcome::Rendered
                    }
                    Err(err) => {
                        let reason = format!("{err:#}");
                        let reason = reason.lines().next().unwrap_or_default();
                        log::warn!("no thumbnail for {}: {reason}", source.display());
                        Outcome::Failed
                    }
                };
                if let Some(cache) = &mut self.cache {
                    cache.record(&source, fingerprint, outcome);
                    cache.save();
                }
            }
        }
        cx.notify();
    }

    fn show(&mut self, source: PathBuf, image: Arc<RenderImage>, cx: &mut Context<Self>) {
        if let Some(previous) = self.images.insert(source, image) {
            cx.drop_image(previous, None);
        }
    }
}

impl Job {
    fn run(self) -> Finished {
        match self {
            Job::Load(loads) => Finished::Loaded(
                loads
                    .into_iter()
                    .filter_map(|(image, source)| Some((source, decode(&image).ok()?)))
                    .collect(),
            ),
            Job::Render {
                source,
                fingerprint,
                partial,
                image,
            } => {
                let result = render(&source, &partial)
                    .and_then(|()| {
                        fs::rename(&partial, &image).context("unable to store thumbnail")
                    })
                    .and_then(|()| decode(&image));
                if result.is_err() {
                    let _ = fs::remove_file(&partial);
                }
                Finished::Rendered {
                    source,
                    fingerprint,
                    result,
                }
            }
        }
    }
}

fn render(source: &Path, output: &Path) -> Result<()> {
    if let Some(output_dir) = output.parent() {
        fs::create_dir_all(output_dir)?;
    }
    if let Some(bundled) = bundled_thumbnail(source) {
        fs::copy(bundled, output)?;
        return Ok(());
    }

    let request = ExportRequest {
        root_text: fs::read_to_string(source).context("unable to read scene")?,
        root_path: source.to_path_buf(),
        open_documents: HashMap::new(),
        output_path: output.to_path_buf(),
        kind: ExportKind::Image {
            timestamp: ImageExportTimestamp::LastSlide {
                time: SECONDS_INTO_LAST_SLIDE,
            },
        },
        settings: ExportSettings {
            render_size: RENDER_SIZE,
            ..ExportSettings::default()
        },
    };
    exporter::export_scene(request, Arc::new(AtomicBool::new(false)), |_| {})?;

    image::open(output)?
        .resize_exact(THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT, FilterType::Triangle)
        .save_with_format(output, image::ImageFormat::Png)?;
    Ok(())
}

fn decode(path: &Path) -> Result<Arc<RenderImage>> {
    let mut pixels = image::open(path)?.into_rgba8();
    // gpui expects bgra
    for pixel in pixels.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Ok(Arc::new(RenderImage::new([Frame::new(pixels)])))
}

/// the shipped thumbnail of an example scene, as long as the example is unmodified
fn bundled_thumbnail(source: &Path) -> Option<PathBuf> {
    let examples = fs::canonicalize(Assets::default_scene("")).ok()?;
    let source = fs::canonicalize(source).ok()?;
    if source.parent()? != examples {
        return None;
    }

    let stem = source.file_stem()?.to_str()?;
    let dir = examples.join(BUNDLED_DIR);
    let manifest: HashMap<String, String> =
        serde_json::from_str(&fs::read_to_string(dir.join(BUNDLED_MANIFEST)).ok()?).ok()?;
    let text = fs::read_to_string(&source).ok()?;

    let image = dir.join(format!("{stem}.png"));
    (manifest.get(stem)? == &content_digest(&text) && image.exists()).then_some(image)
}

/// line endings are normalized so a crlf checkout still matches
fn content_digest(text: &str) -> String {
    Sha256::digest(text.replace("\r\n", "\n").as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_bundled_example(source: &Path) -> bool {
    source
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|dir| dir == "default_scenes")
}

fn is_scene(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(DocumentType::Scene.extension()))
}

fn normalize(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_ignores_line_endings() {
        assert_eq!(content_digest("a\r\nb\n"), content_digest("a\nb\n"));
    }

    #[test]
    fn bundled_manifest_covers_every_thumbnail() {
        let dir = Assets::default_scene(BUNDLED_DIR);
        let manifest: HashMap<String, String> =
            serde_json::from_str(&fs::read_to_string(dir.join(BUNDLED_MANIFEST)).unwrap()).unwrap();
        for stem in manifest.keys() {
            let image = image::open(dir.join(format!("{stem}.png"))).unwrap();
            assert_eq!(
                (image.width(), image.height()),
                (THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT)
            );
            assert!(Assets::default_scene(format!("{stem}.mcs")).exists());
        }
    }
}
