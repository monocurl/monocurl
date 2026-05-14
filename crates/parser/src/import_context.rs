use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use lexer::{lexer::Lexer, token::Token};
use structs::{
    assets::Assets,
    rope::{Attribute, Rope, TextAggregate},
};
use ui_cli_shared::doc_type::DocumentType;

use crate::{ast::SectionBundle, flatten_lex_stream, flatten_rope, parser::ParseArtifacts};

pub enum Error {
    NotFound,
}

pub enum ImportedFileContent {
    Text(String),
    Ropes {
        lex_rope: Rope<Attribute<Token>>,
        text_rope: Rope<TextAggregate>,
    },
}

pub struct ImportedFile {
    pub path: PathBuf,
    pub content: ImportedFileContent,
    pub is_stdlib: bool,
}

pub trait ImportBackend: Send {
    fn import_file(
        &self,
        working_directory: Option<&Path>,
        relative_path: &Path,
    ) -> Option<ImportedFile>;
}

#[derive(Default)]
pub struct FilesystemImportBackend {
    pub open_tab_ropes: HashMap<PathBuf, (Rope<Attribute<Token>>, Rope<TextAggregate>)>,
}

#[derive(Default)]
pub struct MemoryImportBackend {
    files: HashMap<PathBuf, MemoryImportFile>,
}

struct MemoryImportFile {
    text: String,
    is_stdlib: bool,
}

// context mainly related about finding additional imports
pub struct ParseImportContext {
    pub root_file_path: PathBuf,
    pub import_backend: Box<dyn ImportBackend + Send>,
    pub cached_parses: HashMap<PathBuf, (Arc<SectionBundle>, ParseArtifacts)>,
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
            cached_parses: Default::default(),
        }
    }

    pub fn with_open_documents(
        root_file_path: PathBuf,
        open_tab_ropes: HashMap<PathBuf, (Rope<Attribute<Token>>, Rope<TextAggregate>)>,
    ) -> Self {
        Self::with_backend(root_file_path, FilesystemImportBackend { open_tab_ropes })
    }

    pub fn with_backend(
        root_file_path: PathBuf,
        import_backend: impl ImportBackend + Send + 'static,
    ) -> Self {
        Self {
            root_file_path,
            import_backend: Box::new(import_backend),
            cached_parses: Default::default(),
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

impl FilesystemImportBackend {
    fn candidate_roots(working_directory: Option<&Path>) -> Vec<PathBuf> {
        let paths = if let Some(working_directory) = working_directory {
            vec![working_directory.to_path_buf(), Assets::std_lib()]
        } else {
            vec![Assets::std_lib()]
        };
        paths
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
