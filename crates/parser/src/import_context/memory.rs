use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use ui_cli_shared::doc_type::DocumentType;

use super::{
    ImportBackend,
    backend::{ImportedFile, ImportedFileContent},
};

#[derive(Default)]
pub struct MemoryImportBackend {
    files: HashMap<PathBuf, MemoryImportFile>,
}

struct MemoryImportFile {
    text: String,
    is_stdlib: bool,
}

impl MemoryImportBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_module(
        &mut self,
        module_path: impl AsRef<str>,
        text: impl Into<String>,
        is_stdlib: bool,
    ) {
        let mut path = PathBuf::new();
        for segment in module_path.as_ref().split('.') {
            path.push(segment);
        }
        path.set_extension(DocumentType::Library.extension());
        self.insert_path(path, text, is_stdlib);
    }

    pub fn insert_path(
        &mut self,
        path: impl Into<PathBuf>,
        text: impl Into<String>,
        is_stdlib: bool,
    ) {
        self.files.insert(
            path.into(),
            MemoryImportFile {
                text: text.into(),
                is_stdlib,
            },
        );
    }

    fn candidates(working_directory: Option<&Path>, relative_path: &Path) -> Vec<PathBuf> {
        let mut relative = relative_path.to_path_buf();
        relative.set_extension(DocumentType::Library.extension());

        let mut candidates = Vec::new();
        if let Some(working_directory) = working_directory {
            candidates.push(working_directory.join(&relative));
        }
        candidates.push(relative);
        candidates
    }
}

impl ImportBackend for MemoryImportBackend {
    fn import_file(
        &self,
        working_directory: Option<&Path>,
        relative_path: &Path,
    ) -> Option<ImportedFile> {
        for path in Self::candidates(working_directory, relative_path) {
            if let Some(file) = self.files.get(&path) {
                return Some(ImportedFile {
                    path,
                    content: ImportedFileContent::Text(file.text.clone()),
                    is_stdlib: file.is_stdlib,
                });
            }
        }
        None
    }
}
