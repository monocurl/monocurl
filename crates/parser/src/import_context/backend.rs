use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use lexer::token::Token;
use structs::rope::{Attribute, Rope, TextAggregate};

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

pub type OpenDocumentRopes =
    std::collections::HashMap<PathBuf, (Rope<Attribute<Token>>, Rope<TextAggregate>)>;

pub type CachedParse = std::collections::HashMap<
    PathBuf,
    (
        Arc<crate::ast::SectionBundle>,
        crate::parser::ParseArtifacts,
    ),
>;
