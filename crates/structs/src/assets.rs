use std::env;
use std::path::{Path, PathBuf};

pub struct Assets;
impl Assets {
    pub const DIR_ENV_VAR: &'static str = "MONOCURL_ASSETS_DIR";

    /// assets directories in lookup order; the first that exists is used
    pub fn candidate_dirs() -> Vec<PathBuf> {
        let mut candidates: Vec<_> = env::var_os(Self::DIR_ENV_VAR)
            .map(PathBuf::from)
            .into_iter()
            .collect();

        if let Some(exe_dir) = env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
        {
            candidates.extend(installed_asset_candidates(&exe_dir));
            candidates.extend(dev_build_asset_candidate(&exe_dir));
        }

        candidates.push(PathBuf::from("assets"));
        candidates
    }

    fn base_path() -> PathBuf {
        Self::candidate_dirs()
            .into_iter()
            .find(|candidate| candidate.exists())
            .unwrap_or_else(|| PathBuf::from("assets"))
    }

    pub fn asset(name: impl AsRef<Path>) -> PathBuf {
        let mut base = Self::base_path();
        base.push(name.as_ref());
        base
    }

    pub fn std_lib() -> PathBuf {
        let mut base = Self::base_path();
        base.push("std");
        base
    }

    pub fn image(name: impl AsRef<Path>) -> PathBuf {
        let mut base = Self::base_path();
        base.push("img");
        base.push(name.as_ref());
        base
    }

    pub fn image_resource(name: impl AsRef<Path>) -> String {
        let mut path = PathBuf::from("img");
        path.push(name.as_ref());
        path.to_string_lossy().into_owned()
    }

    pub fn default_scene(name: impl AsRef<Path>) -> PathBuf {
        let mut base = Self::base_path();
        base.push("default_scenes");
        base.push(name.as_ref());
        base
    }

    pub fn font(name: impl AsRef<Path>) -> PathBuf {
        let mut base = Self::base_path();
        base.push("font");
        base.push(name.as_ref());
        base
    }
}

/// a cargo build lives in `<repo>/target/<profile>/`, next to the repo's `assets`
fn dev_build_asset_candidate(exe_dir: &Path) -> Option<PathBuf> {
    let target_dir = exe_dir
        .parent()
        .filter(|dir| dir.file_name().is_some_and(|name| name == "target"))?;
    Some(target_dir.parent()?.join("assets"))
}

#[cfg(target_os = "macos")]
fn installed_asset_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    vec![exe_dir.join("..").join("Resources").join("assets")]
}

#[cfg(not(target_os = "macos"))]
fn installed_asset_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![exe_dir.join("assets")];

    if exe_dir.file_name().is_some_and(|name| name == "bin")
        && let Some(prefix) = exe_dir.parent()
    {
        candidates.push(prefix.join("assets"));
    }

    candidates
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::dev_build_asset_candidate;

    #[test]
    fn dev_build_candidate_points_at_repo_assets() {
        assert_eq!(
            dev_build_asset_candidate(Path::new("/repo/target/release")),
            Some(PathBuf::from("/repo/assets"))
        );
        assert_eq!(dev_build_asset_candidate(Path::new("/usr/local/bin")), None);
    }
}
