//! a digest per slide of everything its frames depend on, so renders of unchanged
//! slides can be reused

use std::{
    collections::HashMap,
    fmt::{self, Write},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use bytecode::SectionBytecode;

/// 64-bit fnv-1a: stable across runs, unlike the std hashers
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
}

impl Write for Fnv {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.bytes(s.as_bytes());
        Ok(())
    }
}

/// one digest per section, of it and every section before it, since a slide starts
/// from the state all earlier sections leave. a section is hashed through its `Debug`
/// form (instructions, pools, prototypes, spans and source path), together with the
/// contents of any file one of its strings names, the way images and svgs resolve
/// them (absolute, or relative to the section's source file)
pub(crate) fn cumulative_section_digests(sections: &[Arc<SectionBytecode>]) -> Vec<u64> {
    let mut hasher = Fnv::new();
    let mut files: HashMap<PathBuf, u64> = HashMap::new();
    sections
        .iter()
        .map(|section| {
            write!(hasher, "{section:?}").expect("hashing cannot fail");
            for path in referenced_files(section) {
                let digest = *files.entry(path).or_insert_with_key(|path| {
                    let mut file = Fnv::new();
                    file.bytes(&fs::read(path).unwrap_or_default());
                    file.0
                });
                hasher.bytes(&digest.to_le_bytes());
            }
            hasher.0
        })
        .collect()
}

fn referenced_files(section: &SectionBytecode) -> impl Iterator<Item = PathBuf> + '_ {
    let base = section.source_file_path.as_deref().and_then(Path::parent);
    section.string_pool.iter().filter_map(move |string| {
        let path = Path::new(&**string);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            base?.join(path)
        };
        path.is_file().then_some(path)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytecode::{SectionBytecode, SectionFlags};

    use super::cumulative_section_digests;

    fn section(
        name: &str,
        strings: &[&str],
        dir: Option<&std::path::Path>,
    ) -> Arc<SectionBytecode> {
        let mut section = SectionBytecode::new(SectionFlags {
            is_stdlib: false,
            is_library: false,
            is_init: false,
            is_root_module: true,
        });
        section.name = Some(name.into());
        section.string_pool = strings.iter().map(|&s| Arc::from(s)).collect();
        section.source_file_path = dir.map(|dir| dir.join("scene.mcs"));
        Arc::new(section)
    }

    #[test]
    fn a_change_reaches_its_own_and_later_digests_only() {
        let before = cumulative_section_digests(&[
            section("a", &[], None),
            section("b", &[], None),
            section("c", &[], None),
        ]);
        let again = cumulative_section_digests(&[
            section("a", &[], None),
            section("b", &[], None),
            section("c", &[], None),
        ]);
        assert_eq!(before, again);
        let edited = cumulative_section_digests(&[
            section("a", &[], None),
            section("B", &[], None),
            section("c", &[], None),
        ]);
        assert_eq!(before[0], edited[0]);
        assert_ne!(before[1], edited[1]);
        assert_ne!(before[2], edited[2]);
    }

    #[test]
    fn files_named_by_strings_are_part_of_the_digest() {
        let dir =
            std::env::temp_dir().join(format!("monocurl-slide-digest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let image = dir.join("picture.svg");
        std::fs::write(&image, "<svg/>").unwrap();
        let sections = || {
            [
                section("a", &[], Some(&dir)),
                section("b", &["picture.svg", "not a file"], Some(&dir)),
            ]
        };
        let before = cumulative_section_digests(&sections());
        std::fs::write(&image, "<svg></svg>").unwrap();
        let after = cumulative_section_digests(&sections());
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(before[0], after[0]);
        assert_ne!(before[1], after[1]);
    }
}
