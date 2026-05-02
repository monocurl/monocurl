use std::env;
use std::path::{Path, PathBuf};

pub struct Assets;
impl Assets {
    fn base_path() -> PathBuf {
        if let Ok(assets_dir) = env::var("MONOCURL_ASSETS_DIR") {
            let assets_dir = PathBuf::from(assets_dir);
            if assets_dir.exists() {
                return assets_dir;
            }
        }

        let exe_dir = env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));

        if let Some(exe_dir) = exe_dir {
            #[cfg(target_os = "macos")]
            let candidate = exe_dir.join("..").join("Resources").join("assets");

            #[cfg(not(target_os = "macos"))]
            let candidate = exe_dir.join("assets");

            if candidate.exists() {
                return candidate;
            }
        }

        PathBuf::from("assets")
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
