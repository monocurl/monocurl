use std::path::{Path, PathBuf};

use structs::assets::Assets;
use ui_cli_shared::doc_type::DocumentType;

use super::{
    ImportBackend,
    backend::{ImportedFile, ImportedFileContent, OpenDocumentRopes},
};

#[derive(Default)]
pub struct FilesystemImportBackend {
    pub open_tab_ropes: OpenDocumentRopes,
}

impl FilesystemImportBackend {
    fn candidate_roots(working_directory: Option<&Path>) -> Vec<PathBuf> {
        if let Some(working_directory) = working_directory {
            vec![working_directory.to_path_buf(), Assets::std_lib()]
        } else {
            vec![Assets::std_lib()]
        }
    }

    fn import_path(mut root: PathBuf, relative_path: &Path) -> PathBuf {
        root.push(relative_path);
        root.set_extension(DocumentType::Library.extension());
        root
    }
}

impl ImportBackend for FilesystemImportBackend {
    fn import_file(
        &self,
        working_directory: Option<&Path>,
        relative_path: &Path,
    ) -> Option<ImportedFile> {
        for root in Self::candidate_roots(working_directory) {
            let is_stdlib = root == Assets::std_lib();
            let path = Self::import_path(root, relative_path);

            if let Some((lex_rope, text_rope)) = self.open_tab_ropes.get(&path) {
                return Some(ImportedFile {
                    path: path.clone(),
                    content: ImportedFileContent::Ropes {
                        lex_rope: lex_rope.clone(),
                        text_rope: text_rope.clone(),
                    },
                    is_stdlib,
                });
            }

            if let Ok(content) = std::fs::read_to_string(&path) {
                return Some(ImportedFile {
                    path,
                    content: ImportedFileContent::Text(content),
                    is_stdlib,
                });
            }
        }

        None
    }

    fn missing_import_hint(
        &self,
        working_directory: Option<&Path>,
        relative_path: &Path,
    ) -> Option<String> {
        let searched = Self::candidate_roots(working_directory)
            .into_iter()
            .map(|root| display_absolute(&Self::import_path(root, relative_path)))
            .collect::<Vec<_>>()
            .join(", ");
        let mut hint = format!("Searched {searched}");

        if !Assets::std_lib().exists() {
            let asset_dirs = Assets::candidate_dirs()
                .iter()
                .map(|dir| display_absolute(dir))
                .collect::<Vec<_>>()
                .join(", ");
            hint.push_str(&format!(
                ". The Monocurl assets directory was not found (tried {asset_dirs}); \
                 set {} to its absolute path",
                Assets::DIR_ENV_VAR
            ));
        }

        Some(hint)
    }
}

fn display_absolute(path: &Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{FilesystemImportBackend, ImportBackend};

    #[test]
    fn missing_import_hint_lists_searched_paths() {
        let hint = FilesystemImportBackend::default()
            .missing_import_hint(Some(Path::new("/scene")), Path::new("nope/module"))
            .expect("filesystem imports always explain a miss");
        assert!(hint.contains("/scene/nope/module.mcl"), "{hint}");
    }
}
