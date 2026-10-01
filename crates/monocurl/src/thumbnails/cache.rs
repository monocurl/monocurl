use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const INDEX_FILE: &str = "index.json";
const IMAGE_EXTENSION: &str = "png";
const PARTIAL_SUFFIX: &str = ".partial.png";

/// identifies one version of a source file; any change re-renders its thumbnail
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    modified_secs: u64,
    modified_nanos: u32,
    len: u64,
}

impl Fingerprint {
    pub fn of(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
        Some(Self {
            modified_secs: modified.as_secs(),
            modified_nanos: modified.subsec_nanos(),
            len: metadata.len(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Rendered,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    source: PathBuf,
    fingerprint: Fingerprint,
    outcome: Outcome,
    last_used: u64,
}

/// on disk: `<dir>/<key>.png` per source plus `<dir>/index.json` holding what each png was made from
pub struct ThumbnailCache {
    dir: PathBuf,
    entries: HashMap<String, Entry>,
}

impl ThumbnailCache {
    pub fn open(dir: PathBuf) -> Self {
        let entries = fs::read_to_string(dir.join(INDEX_FILE))
            .ok()
            .and_then(|data| serde_json::from_str(&data).ok())
            .unwrap_or_default();
        Self { dir, entries }
    }

    pub fn key(source: &Path) -> String {
        Sha256::digest(source.to_string_lossy().as_bytes())[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn image_path(&self, source: &Path) -> PathBuf {
        self.dir
            .join(Self::key(source))
            .with_extension(IMAGE_EXTENSION)
    }

    /// where a render is written before it atomically replaces the image
    pub fn partial_path(&self, source: &Path) -> PathBuf {
        self.dir
            .join(format!("{}{PARTIAL_SUFFIX}", Self::key(source)))
    }

    /// whether the entry was made from exactly this version of the source; refreshes its last use
    pub fn is_current(&mut self, source: &Path, fingerprint: Fingerprint) -> bool {
        match self.entries.get_mut(&Self::key(source)) {
            Some(entry) if entry.source == source && entry.fingerprint == fingerprint => {
                entry.last_used = now_secs();
                true
            }
            _ => false,
        }
    }

    pub fn record(&mut self, source: &Path, fingerprint: Fingerprint, outcome: Outcome) {
        self.entries.insert(
            Self::key(source),
            Entry {
                source: source.to_path_buf(),
                fingerprint,
                outcome,
                last_used: now_secs(),
            },
        );
    }

    /// forgets entries whose source is gone, keeps the `max_entries` most recently used,
    /// and deletes every file in the directory that no entry owns
    pub fn prune(&mut self, max_entries: usize) {
        self.entries.retain(|_, entry| entry.source.exists());
        if self.entries.len() > max_entries {
            let mut by_use: Vec<_> = self
                .entries
                .iter()
                .map(|(key, entry)| (entry.last_used, key.clone()))
                .collect();
            by_use.sort_unstable_by(|a, b| b.cmp(a));
            for (_, key) in by_use.drain(max_entries..) {
                self.entries.remove(&key);
            }
        }

        let Ok(listing) = fs::read_dir(&self.dir) else {
            return;
        };
        for file in listing.flatten() {
            let path = file.path();
            let owned = path.extension().is_some_and(|ext| ext == IMAGE_EXTENSION)
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| self.entries.contains_key(stem));
            if !owned && path.file_name().is_some_and(|name| name != INDEX_FILE) {
                let _ = fs::remove_file(path);
            }
        }
    }

    pub fn save(&self) {
        let result = fs::create_dir_all(&self.dir).and_then(|()| {
            let data = serde_json::to_string(&self.entries).map_err(std::io::Error::other)?;
            fs::write(self.dir.join(INDEX_FILE), data)
        });
        if let Err(err) = result {
            log::warn!("unable to save thumbnail index: {err}");
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint_after_write(path: &Path, text: &str) -> Fingerprint {
        fs::write(path, text).unwrap();
        Fingerprint::of(path).unwrap()
    }

    #[test]
    fn changed_source_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("scene.mcs");
        let mut cache = ThumbnailCache::open(dir.path().join("thumbnails"));

        let first = fingerprint_after_write(&source, "a");
        assert!(!cache.is_current(&source, first));
        cache.record(&source, first, Outcome::Failed);
        assert!(cache.is_current(&source, first));

        let second = fingerprint_after_write(&source, "ab");
        assert!(!cache.is_current(&source, second));
    }

    #[test]
    fn index_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("scene.mcs");
        let fingerprint = fingerprint_after_write(&source, "a");

        let mut cache = ThumbnailCache::open(dir.path().join("thumbnails"));
        cache.record(&source, fingerprint, Outcome::Rendered);
        cache.save();

        let mut reopened = ThumbnailCache::open(dir.path().join("thumbnails"));
        assert!(reopened.is_current(&source, fingerprint));
    }

    #[test]
    fn prune_drops_missing_sources_and_orphans() {
        let dir = tempfile::tempdir().unwrap();
        let thumbnails = dir.path().join("thumbnails");
        fs::create_dir_all(&thumbnails).unwrap();
        let kept = dir.path().join("kept.mcs");
        let removed = dir.path().join("removed.mcs");

        let mut cache = ThumbnailCache::open(thumbnails.clone());
        for source in [&kept, &removed] {
            let fingerprint = fingerprint_after_write(source, "a");
            fs::write(cache.image_path(source), b"png").unwrap();
            cache.record(source, fingerprint, Outcome::Rendered);
        }
        fs::write(thumbnails.join("stray.png"), b"png").unwrap();
        fs::write(cache.partial_path(&kept), b"png").unwrap();
        fs::remove_file(&removed).unwrap();

        cache.prune(10);

        assert!(cache.image_path(&kept).exists());
        assert!(!cache.image_path(&removed).exists());
        assert!(!thumbnails.join("stray.png").exists());
        assert!(!cache.partial_path(&kept).exists());
    }

    #[test]
    fn prune_caps_by_last_use() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = ThumbnailCache::open(dir.path().join("thumbnails"));
        let sources: Vec<_> = (0..4)
            .map(|i| dir.path().join(format!("{i}.mcs")))
            .collect();
        for (i, source) in sources.iter().enumerate() {
            let fingerprint = fingerprint_after_write(source, "a");
            cache.record(source, fingerprint, Outcome::Rendered);
            cache
                .entries
                .get_mut(&ThumbnailCache::key(source))
                .unwrap()
                .last_used = i as u64;
        }

        cache.prune(2);

        let remaining: Vec<_> = sources
            .iter()
            .map(|source| cache.entries.contains_key(&ThumbnailCache::key(source)))
            .collect();
        assert_eq!(remaining, [false, false, true, true]);
    }
}
