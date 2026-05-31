use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context as AnyhowContext, Result, bail};
use sha2::{Digest, Sha256};

use super::manifest::UpdateAsset;

pub(super) fn download_asset(
    client: &reqwest::blocking::Client,
    asset: &UpdateAsset,
    download_dir: &Path,
) -> Result<PathBuf> {
    fs::create_dir_all(download_dir)
        .with_context(|| format!("failed to create {}", download_dir.display()))?;
    let file_name = Path::new(&asset.name)
        .file_name()
        .context("update asset name must be a file name")?;
    let target_path = download_dir.join(file_name);
    let mut response = client
        .get(&asset.url)
        .send()
        .with_context(|| format!("failed to download {}", asset.url))?
        .error_for_status()
        .with_context(|| format!("download request failed for {}", asset.url))?;
    let mut file = fs::File::create(&target_path)
        .with_context(|| format!("failed to create {}", target_path.display()))?;
    io::copy(&mut response, &mut file)
        .with_context(|| format!("failed to write {}", target_path.display()))?;
    file.flush()
        .with_context(|| format!("failed to flush {}", target_path.display()))?;
    Ok(target_path)
}

pub(super) fn verify_asset(path: &Path, expected_sha256: &str) -> Result<()> {
    if expected_sha256.trim().is_empty() {
        return Ok(());
    }

    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        bail!("downloaded update checksum mismatch: expected {expected_sha256}, got {actual}");
    }

    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
