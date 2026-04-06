use std::{collections::HashMap, ops::Range, path::{Path, PathBuf}, sync::Arc};

use lexer::{lexer::Lexer, token::Token};
use structs::{assets::Assets, rope::{RLEAggregate, Rope, TextAggregate}};

use crate::{ast::SectionBundle, flatten_lex_stream, flatten_rope, parser::ParseArtifacts};

pub enum Error {
    NotFound
}

// context mainly related about finding additional imports
pub struct ParseState {
    pub open_tab_ropes: HashMap<PathBuf, (Rope<RLEAggregate<Token>>, Rope<TextAggregate>)>,
    pub cached_parses: HashMap<PathBuf, (Arc<SectionBundle>, ParseArtifacts)>,
}

pub(crate) struct ContentResult {
    pub path: PathBuf,
    pub tokens: Vec<(Token, Range<usize>)>,
    pub text_rope: Rope<TextAggregate>,
    pub is_stdlib: bool,
}

impl ParseState {
    pub fn cache_get(&self, path: &Path) -> Option<(Arc<SectionBundle>, ParseArtifacts)> {
        self.cached_parses.get(path).cloned()
    }

    pub fn set_cache(&mut self, path: PathBuf, bundle: Arc<SectionBundle>, artifacts: ParseArtifacts) {
        self.cached_parses.insert(path, (bundle, artifacts));
    }

    pub fn file_content(&self, working_directory: &Path, relative_path: &Path) -> Option<ContentResult> {
        let paths = [working_directory.to_path_buf(), Assets::std_lib()];
        for mut p in paths {
            let is_stdlib = p == Assets::std_lib();
            p.push(relative_path);

            if let Some((lex_rope, text_rope)) = self.open_tab_ropes.get(&p) {
                return Some(ContentResult {
                    path: p.clone(),
                    tokens: flatten_rope(lex_rope),
                    text_rope: text_rope.clone(),
                    is_stdlib,
                });
            }
            else {
                let Ok(content) = std::fs::read_to_string(&p) else {
                    continue;
                };
                let text_rope = Rope::from_str(&content);
                let filtered = Lexer::new(content.chars());
                return Some(ContentResult {
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
