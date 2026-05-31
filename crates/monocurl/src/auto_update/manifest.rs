use std::env;

use anyhow::{Context as AnyhowContext, Result, bail};
use semver::{BuildMetadata, Prerelease, Version};
use serde::Deserialize;

const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/monocurl/monocurl/releases/latest/download/latest.json";

#[derive(Debug, Deserialize)]
pub(super) struct UpdateManifest {
    pub(super) version: String,
    pub(super) assets: Vec<UpdateAsset>,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct UpdateAsset {
    pub(super) os: String,
    pub(super) arch: String,
    pub(super) kind: String,
    pub(super) name: String,
    pub(super) url: String,
    pub(super) sha256: String,
}

pub(super) fn fetch_manifest(client: &reqwest::blocking::Client) -> Result<UpdateManifest> {
    let url = update_manifest_url();
    let response = client
        .get(&url)
        .send()
        .with_context(|| format!("failed to fetch update manifest from {url}"))?
        .error_for_status()
        .with_context(|| format!("update manifest request failed for {url}"))?;

    response
        .json()
        .context("failed to parse update manifest JSON")
}

pub(super) fn update_disabled_explanation() -> Option<String> {
    env::var("MONOCURL_UPDATE_EXPLANATION")
        .ok()
        .filter(|message| !message.trim().is_empty())
        .or_else(|| option_env!("MONOCURL_UPDATE_EXPLANATION").map(str::to_string))
}

pub(super) fn matching_asset(manifest: &UpdateManifest) -> Option<&UpdateAsset> {
    matching_asset_for(manifest, env::consts::OS, env::consts::ARCH)
}

pub(super) fn validate_asset_kind(asset: &UpdateAsset) -> Result<()> {
    let expected_kind = match env::consts::OS {
        "macos" => "dmg",
        "linux" => "tar.gz",
        "windows" => "inno",
        os => bail!("auto-updates are not supported on {os}"),
    };

    if asset.kind != expected_kind {
        bail!(
            "update asset {:?} has kind {:?}, expected {:?}",
            asset.name,
            asset.kind,
            expected_kind
        );
    }

    Ok(())
}

pub(super) fn version_is_newer(current_version: &Version, fetched_version: &Version) -> bool {
    normalize_version(fetched_version) > normalize_version(current_version)
}

fn update_manifest_url() -> String {
    env::var("MONOCURL_UPDATE_MANIFEST_URL")
        .ok()
        .filter(|url| !url.trim().is_empty())
        .or_else(|| option_env!("MONOCURL_UPDATE_MANIFEST_URL").map(str::to_string))
        .unwrap_or_else(|| DEFAULT_MANIFEST_URL.to_string())
}

fn matching_asset_for<'a>(
    manifest: &'a UpdateManifest,
    os: &str,
    arch: &str,
) -> Option<&'a UpdateAsset> {
    manifest
        .assets
        .iter()
        .find(|asset| asset.os == os && arch_matches(&asset.arch, arch))
}

fn arch_matches(asset_arch: &str, current_arch: &str) -> bool {
    asset_arch == current_arch
        || matches!(
            (asset_arch, current_arch),
            ("x86_64", "amd64") | ("aarch64", "arm64")
        )
}

fn normalize_version(version: &Version) -> Version {
    let mut normalized = version.clone();
    normalized.pre = Prerelease::EMPTY;
    normalized.build = BuildMetadata::EMPTY;
    normalized
}
