use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context as AnyhowContext, Result, anyhow, bail};
use walkdir::WalkDir;

pub(super) fn sync_dir_filtered(
    source: &Path,
    destination: &Path,
    should_skip: impl Fn(&Path) -> bool,
) -> Result<()> {
    if !source.is_dir() {
        bail!("source directory does not exist: {}", source.display());
    }

    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    delete_stale_entries(source, destination, &should_skip)?;
    copy_entries(source, destination, &should_skip)?;
    copy_permissions(source, destination)?;
    Ok(())
}

fn delete_stale_entries(
    source: &Path,
    destination: &Path,
    should_skip: &impl Fn(&Path) -> bool,
) -> Result<()> {
    for entry in WalkDir::new(destination)
        .min_depth(1)
        .contents_first(true)
        .follow_links(false)
    {
        let entry = entry.with_context(|| format!("failed to read {}", destination.display()))?;
        let relative_path = entry
            .path()
            .strip_prefix(destination)
            .with_context(|| format!("failed to relativize {}", entry.path().display()))?;

        if should_skip(relative_path) {
            continue;
        }

        match fs::symlink_metadata(source.join(relative_path)) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                remove_path(entry.path()).with_context(|| {
                    format!("failed to remove stale {}", entry.path().display())
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect source entry {}",
                        source.join(relative_path).display()
                    )
                });
            }
        }
    }

    Ok(())
}

fn copy_entries(
    source: &Path,
    destination: &Path,
    should_skip: &impl Fn(&Path) -> bool,
) -> Result<()> {
    let mut directories = Vec::new();

    for entry in WalkDir::new(source).min_depth(1).follow_links(false) {
        let entry = entry.with_context(|| format!("failed to read {}", source.display()))?;
        let relative_path = entry
            .path()
            .strip_prefix(source)
            .with_context(|| format!("failed to relativize {}", entry.path().display()))?;

        if should_skip(relative_path) {
            continue;
        }

        let destination_path = destination.join(relative_path);
        let file_type = entry.file_type();

        if file_type.is_dir() {
            prepare_destination_dir(&destination_path)?;
            directories.push((entry.path().to_path_buf(), destination_path));
        } else if file_type.is_symlink() {
            copy_symlink(entry.path(), &destination_path)?;
        } else if file_type.is_file() {
            copy_file(entry.path(), &destination_path)?;
        } else {
            bail!("unsupported update bundle entry {}", entry.path().display());
        }
    }

    for (source_dir, destination_dir) in directories.into_iter().rev() {
        copy_permissions(&source_dir, &destination_dir)?;
    }

    Ok(())
}

fn prepare_destination_dir(path: &Path) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && !metadata.is_dir()
    {
        remove_path(path)
            .with_context(|| format!("failed to replace {} with a directory", path.display()))?;
    }

    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    ensure_parent(destination)?;

    if let Ok(metadata) = fs::symlink_metadata(destination)
        && !metadata.is_file()
    {
        remove_path(destination)
            .with_context(|| format!("failed to replace {}", destination.display()))?;
    }

    let temp_path = temp_path_for(destination)?;
    fs::copy(source, &temp_path).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    copy_permissions(source, &temp_path)?;

    #[cfg(windows)]
    if fs::symlink_metadata(destination).is_ok() {
        remove_path(destination)
            .with_context(|| format!("failed to replace {}", destination.display()))?;
    }

    if let Err(error) = fs::rename(&temp_path, destination) {
        let _ = fs::remove_file(&temp_path);
        return Err(error)
            .with_context(|| format!("failed to move {} into place", destination.display()));
    }

    Ok(())
}

fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    ensure_parent(destination)?;

    if fs::symlink_metadata(destination).is_ok() {
        remove_path(destination)
            .with_context(|| format!("failed to replace {}", destination.display()))?;
    }

    create_symlink(source, destination)
}

#[cfg(unix)]
fn create_symlink(source: &Path, destination: &Path) -> Result<()> {
    let target =
        fs::read_link(source).with_context(|| format!("failed to read {}", source.display()))?;
    std::os::unix::fs::symlink(&target, destination).with_context(|| {
        format!(
            "failed to create symlink {} -> {}",
            destination.display(),
            target.display()
        )
    })
}

#[cfg(windows)]
fn create_symlink(source: &Path, destination: &Path) -> Result<()> {
    let target =
        fs::read_link(source).with_context(|| format!("failed to read {}", source.display()))?;
    let target_metadata = fs::metadata(source)
        .with_context(|| format!("failed to inspect symlink target {}", source.display()))?;

    if target_metadata.is_dir() {
        std::os::windows::fs::symlink_dir(&target, destination)
    } else {
        std::os::windows::fs::symlink_file(&target, destination)
    }
    .with_context(|| {
        format!(
            "failed to create symlink {} -> {}",
            destination.display(),
            target.display()
        )
    })
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(_source: &Path, destination: &Path) -> Result<()> {
    bail!(
        "cannot copy symlink {} on this platform",
        destination.display()
    )
}

fn copy_permissions(source: &Path, destination: &Path) -> Result<()> {
    let permissions = fs::symlink_metadata(source)
        .with_context(|| format!("failed to inspect {}", source.display()))?
        .permissions();
    fs::set_permissions(destination, permissions)
        .with_context(|| format!("failed to set permissions on {}", destination.display()))
}

fn remove_path(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    let file_type = metadata.file_type();

    if file_type.is_symlink() || file_type.is_file() {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
    } else if file_type.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
    } else {
        bail!("unsupported path type {}", path.display())
    }
}

fn ensure_parent(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))
}

fn temp_path_for(destination: &Path) -> Result<PathBuf> {
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("path has no parent: {}", destination.display()))?;
    let file_name = destination
        .file_name()
        .ok_or_else(|| anyhow!("path has no file name: {}", destination.display()))?
        .to_string_lossy();

    for attempt in 0..1000 {
        let candidate = parent.join(format!(
            ".{file_name}.monocurl-update-{}-{attempt}.tmp",
            std::process::id()
        ));
        if fs::symlink_metadata(&candidate).is_err() {
            return Ok(candidate);
        }
    }

    bail!(
        "could not create temporary path next to {}",
        destination.display()
    )
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use anyhow::Result;

    use super::sync_dir_filtered;

    #[test]
    fn sync_dir_mirrors_files_and_deletes_stale_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");

        fs::create_dir_all(source.join("nested"))?;
        fs::create_dir_all(destination.join("nested"))?;
        fs::write(source.join("root.txt"), "new root")?;
        fs::write(source.join("nested/kept.txt"), "new nested")?;
        fs::write(destination.join("root.txt"), "old root")?;
        fs::write(destination.join("nested/stale.txt"), "stale")?;

        sync_dir_filtered(&source, &destination, |_| false)?;

        assert_eq!(
            fs::read_to_string(destination.join("root.txt"))?,
            "new root"
        );
        assert_eq!(
            fs::read_to_string(destination.join("nested/kept.txt"))?,
            "new nested"
        );
        assert!(!destination.join("nested/stale.txt").exists());

        Ok(())
    }

    #[test]
    fn sync_dir_preserves_skipped_destination_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");

        fs::create_dir_all(&source)?;
        fs::create_dir_all(&destination)?;
        fs::write(source.join("copied.txt"), "copied")?;
        fs::write(source.join("Icon\r"), "source icon")?;
        fs::write(destination.join("Icon\r"), "destination icon")?;

        sync_dir_filtered(&source, &destination, |relative_path| {
            relative_path
                .file_name()
                .is_some_and(|name| name == "Icon\r")
        })?;

        assert_eq!(
            fs::read_to_string(destination.join("Icon\r"))?,
            "destination icon"
        );
        assert_eq!(
            fs::read_to_string(destination.join("copied.txt"))?,
            "copied"
        );

        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn sync_dir_copies_symlinks() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");

        fs::create_dir_all(&source)?;
        fs::write(source.join("target.txt"), "target")?;
        std::os::unix::fs::symlink("target.txt", source.join("link.txt"))?;

        sync_dir_filtered(&source, &destination, |_| false)?;

        let link_target = fs::read_link(destination.join("link.txt"))?;
        assert_eq!(link_target, PathBuf::from("target.txt"));
        assert_eq!(fs::read_to_string(destination.join("link.txt"))?, "target");

        Ok(())
    }
}
