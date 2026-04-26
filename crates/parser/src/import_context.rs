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

// context mainly related about finding additional imports
#[derive(Default)]
pub struct ParseImportContext {
    pub root_file_path: PathBuf,
    pub open_tab_ropes: HashMap<PathBuf, (Rope<Attribute<Token>>, Rope<TextAggregate>)>,
    pub cached_parses: HashMap<PathBuf, (Arc<SectionBundle>, ParseArtifacts)>,
}

pub(crate) struct FileResult {
    pub path: PathBuf,
    pub tokens: Vec<(Token, Range<usize>)>,
    pub text_rope: Rope<TextAggregate>,
    pub is_stdlib: bool,
}

impl ParseImportContext {
    pub fn reset(&mut self) {
        self.root_file_path = PathBuf::new();
        self.open_tab_ropes.clear();
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
        let paths = if let Some(working_directory) = working_directory {
            vec![working_directory.to_path_buf(), Assets::std_lib()]
        } else {
            vec![Assets::std_lib()]
        };

        for mut p in paths {
            let is_stdlib = p == Assets::std_lib();
            p.push(relative_path);
            p.set_extension(DocumentType::Library.extension());

            if let Some((lex_rope, text_rope)) = self.open_tab_ropes.get(&p) {
                return Some(FileResult {
                    path: p.clone(),
                    tokens: flatten_rope(lex_rope),
                    text_rope: text_rope.clone(),
                    is_stdlib,
                });
            } else {
                let Ok(content) = std::fs::read_to_string(&p) else {
                    continue;
                };
                let text_rope = Rope::from_str(&content);
                let filtered = Lexer::new(content.chars());
                return Some(FileResult {
                    path: p.clone(),
                    tokens: flatten_lex_stream(filtered).collect(),
                    text_rope: text_rope,
                    is_stdlib,
                });
            }
        }

        None
    }
}
