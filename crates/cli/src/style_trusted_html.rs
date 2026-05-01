use std::{
    collections::HashMap,
    io::{self, Read, Write},
    path::PathBuf,
};

use anyhow::{Context, Result};
use compiler::{
    cache::CompilerCache,
    compiler::{compile, static_analysis_rope},
};
use lexer::{
    lexer::Lexer,
    token::{Token, TokenCategory},
};
use parser::{import_context::ParseImportContext, parser::Parser};
use structs::rope::{Attribute, RLEData, Rope};
use ui_cli_shared::static_analysis::StaticAnalysisData;

pub(crate) fn run_command() -> Result<()> {
    let mut source = String::new();
    io::stdin()
        .read_to_string(&mut source)
        .context("failed to read source from stdin")?;

    let root_path = std::env::current_dir()
        .context("failed to resolve current directory")?
        .join("stdin.mcs");
    let html = style_trusted_html(&source, root_path);

    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    stdout.write_all(html.as_bytes())?;
    Ok(())
}

fn style_trusted_html(source: &str, root_path: PathBuf) -> String {
    let text_rope = Rope::from_str(source);
    let lex_rope = lex_rope_from_str(source);
    let mut import_context = ParseImportContext {
        root_file_path: root_path,
        open_tab_ropes: HashMap::new(),
        cached_parses: HashMap::new(),
    };

    let (bundles, _parse_artifacts) = Parser::parse(
        &mut import_context,
        lex_rope.clone(),
        text_rope.clone(),
        None,
    );
    let mut compiler_cache = CompilerCache::default();
    let compile_result = compile(&mut compiler_cache, None, &bundles);
    let analysis_rope = static_analysis_rope(&compile_result, text_rope.codeunits());

    styled_source_html(source, &lex_rope, &analysis_rope)
}

fn lex_rope_from_str(source: &str) -> Rope<Attribute<Token>> {
    Rope::default().replace_range(
        0..0,
        Lexer::new(source.chars()).map(|(attribute, codeunits)| RLEData {
            codeunits,
            attribute,
        }),
    )
}

fn styled_source_html(
    source: &str,
    lex_rope: &Rope<Attribute<Token>>,
    analysis_rope: &Rope<Attribute<StaticAnalysisData>>,
) -> String {
    let mut html = String::new();
    let mut offset = 0;
    let mut lex_it = lex_rope.iterator(0);
    let mut analysis_it = analysis_rope.iterator(0);
    let mut lex_item = next_nonempty_run(&mut lex_it);
    let mut analysis_item = next_nonempty_run(&mut analysis_it);

    while offset < source.len() {
        let lex_count = lex_item
            .as_ref()
            .map_or(source.len() - offset, |(count, _)| *count);
        let analysis_count = analysis_item
            .as_ref()
            .map_or(source.len() - offset, |(count, _)| *count);
        let chunk_size = lex_count.min(analysis_count).min(source.len() - offset);
        let token = lex_item
            .as_ref()
            .map_or(Token::Illegal, |(_, token)| *token);
        let analysis = analysis_item
            .as_ref()
            .map_or(StaticAnalysisData::None, |(_, analysis)| *analysis);

        push_styled_span(
            &mut html,
            &source[offset..offset + chunk_size],
            token,
            analysis,
        );
        offset += chunk_size;

        if let Some((count, _)) = &mut lex_item {
            *count -= chunk_size;
            if *count == 0 {
                lex_item = next_nonempty_run(&mut lex_it);
            }
        }
        if let Some((count, _)) = &mut analysis_item {
            *count -= chunk_size;
            if *count == 0 {
                analysis_item = next_nonempty_run(&mut analysis_it);
            }
        }
    }

    html
}

fn next_nonempty_run<T: Copy>(it: &mut impl Iterator<Item = (usize, T)>) -> Option<(usize, T)> {
    it.find(|(count, _)| *count > 0)
}

fn push_styled_span(html: &mut String, raw: &str, token: Token, analysis: StaticAnalysisData) {
    if raw.is_empty() {
        return;
    }

    let token_class = token_class_name(token);
    html.push_str("<span class=\"mc-token ");
    html.push_str(token_category_class_name(token.category()));
    html.push(' ');
    html.push_str(&token_class);
    if let Some(class) = analysis.class_name() {
        html.push(' ');
        html.push_str(class);
    }
    html.push_str("\">");
    push_escaped_html(html, raw);
    html.push_str("</span>");
}

fn token_category_class_name(category: TokenCategory) -> &'static str {
    match category {
        TokenCategory::Unknown => "mc-unknown",
        TokenCategory::Whitespace => "mc-whitespace",
        TokenCategory::Operator => "mc-operator",
        TokenCategory::Punctutation => "mc-punctuation",
        TokenCategory::ControlFlow => "mc-control-flow",
        TokenCategory::NonControlFlowKeyword => "mc-non-control-flow-keyword",
        TokenCategory::ArgumentLabel => "mc-argument-label",
        TokenCategory::Identifier => "mc-identifier",
        TokenCategory::NumericLiteral => "mc-numeric-literal",
        TokenCategory::TextLiteral => "mc-text-literal",
        TokenCategory::Comment => "mc-comment",
    }
}

fn token_class_name(token: Token) -> String {
    let mut class = String::from("mc-token-");
    for (index, ch) in format!("{token:?}").chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index != 0 {
                class.push('-');
            }
            class.push(ch.to_ascii_lowercase());
        } else if ch == '_' {
            class.push('-');
        } else {
            class.push(ch);
        }
    }
    class
}

fn push_escaped_html(html: &mut String, raw: &str) {
    for ch in raw.chars() {
        match ch {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            '"' => html.push_str("&quot;"),
            '\'' => html.push_str("&#39;"),
            _ => html.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_trusted_html_escapes_html_and_marks_invocations() {
        let source = "let f = |x| x\nf(1 < 2)";
        let html = style_trusted_html(source, PathBuf::from("stdin.mcs"));

        assert!(html.contains("mc-token-let"));
        assert!(html.contains("mc-function-invocation\">f</span>"));
        assert!(html.contains("&lt;"));
    }
}
