use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use lexer::{lexer::Lexer, token::Token};
use structs::rope::{Rope, TextAggregate};

use crate::{ast::SectionBundle, flatten_lex_stream, flatten_rope, parser::ParseArtifacts};

use super::{
    ImportBackend,
    backend::{CachedParse, ImportedFileContent, OpenDocumentRopes},
    filesystem::FilesystemImportBackend,
};

// context mainly related about finding additional imports
pub struct ParseImportContext {
    pub root_file_path: PathBuf,
    pub import_backend: Box<dyn ImportBackend + Send>,
    pub cached_parses: CachedParse,
}

pub(crate) struct FileResult {
    pub path: PathBuf,
    pub tokens: Vec<(Token, Range<usize>)>,
    pub text_rope: Rope<TextAggregate>,
    pub is_stdlib: bool,
}

impl Default for ParseImportContext {
    fn default() -> Self {
        Self::new(PathBuf::new())
    }
}

impl ParseImportContext {
    pub fn new(root_file_path: PathBuf) -> Self {
        Self {
            root_file_path,
            import_backend: Box::<FilesystemImportBackend>::default(),
            cached_parses: HashMap::new(),
        }
    }

    pub fn with_open_documents(root_file_path: PathBuf, open_tab_ropes: OpenDocumentRopes) -> Self {
        Self::with_backend(root_file_path, FilesystemImportBackend { open_tab_ropes })
    }

    pub fn with_backend(
        root_file_path: PathBuf,
        import_backend: impl ImportBackend + Send + 'static,
    ) -> Self {
        Self {
            root_file_path,
            import_backend: Box::new(import_backend),
            cached_parses: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.root_file_path = PathBuf::new();
        self.import_backend = Box::<FilesystemImportBackend>::default();
        self.cached_parses.clear();
    }

    pub fn cache_get(&self, path: &PathBuf) -> Option<(Arc<SectionBundle>, ParseArtifacts)> {
        self.cached_parses.get(path).cloned()
    }

    pub fn set_cache(&mut self, path: PathBuf, bundle: &SectionBundle, artifacts: ParseArtifacts) {
        let mut raw = bundle.clone();
        raw.was_cached = true;
        self.cached_parses.insert(path, (Arc::new(raw), artifacts));
    }

    pub(crate) fn file_content(
        &self,
        working_directory: Option<&Path>,
        relative_path: &Path,
    ) -> Option<FileResult> {
        let imported = self
            .import_backend
            .import_file(working_directory, relative_path)?;

        let (tokens, text_rope) = match imported.content {
            ImportedFileContent::Text(content) => {
                let text_rope = Rope::from_text(&content);
                let filtered = Lexer::new(content.chars());
                (flatten_lex_stream(filtered).collect(), text_rope)
            }
            ImportedFileContent::Ropes {
                lex_rope,
                text_rope,
            } => (flatten_rope(&lex_rope), text_rope),
        };

        Some(FileResult {
            path: imported.path,
            tokens,
            text_rope,
            is_stdlib: imported.is_stdlib,
        })
    }
}
