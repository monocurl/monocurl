use std::path::{Path, PathBuf};

pub struct Assets;
impl Assets {
    fn base_path() -> PathBuf {
        let path = env!("CARGO_MANIFEST_DIR");
        // this is manifest directory of crate instead of workspace
        let mut base = PathBuf::from(path);
        base.pop();
        base.pop();
        base.push("assets");
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

    pub fn font(name: impl AsRef<Path>) -> PathBuf {
        let mut base = Self::base_path();
        base.push("font");
        base.push(name.as_ref());
        base
    }
}
