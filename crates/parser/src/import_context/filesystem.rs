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
}

impl ImportBackend for FilesystemImportBackend {
    fn import_file(
        &self,
        working_directory: Option<&Path>,
        relative_path: &Path,
    ) -> Option<ImportedFile> {
        for mut path in Self::candidate_roots(working_directory) {
            let is_stdlib = path == Assets::std_lib();
            path.push(relative_path);
            path.set_extension(DocumentType::Library.extension());

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
}
