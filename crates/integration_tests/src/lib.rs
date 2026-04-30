use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use compiler::cache::CompilerCache;
use executor::{
    heap::with_heap,
    value::{Value, container::HashableKey},
};
use lexer::{lexer::Lexer, token::Token};
use parser::{
    ast::{Section, SectionBundle, SectionType},
    parser::SectionParser,
};
use structs::{
    assets::Assets,
    rope::{Rope, TextAggregate},
    text::Span8,
};

pub fn lex_source(src: &str) -> Vec<(Token, Span8)> {
    Lexer::token_stream(src.chars())
        .into_iter()
        .filter(|(token, _)| token != &Token::Whitespace && token != &Token::Comment)
        .collect()
}

pub fn parse_section(src: &str, section_type: SectionType) -> (Section, Vec<String>) {
    let tokens = lex_source(src);
    let rope: Rope<TextAggregate> = Rope::from_str(src);
    let mut parser = SectionParser::new(tokens, rope, section_type.clone(), None, None);
    let stmts = parser.parse_statement_list();
    let errors = parser
        .artifacts()
        .error_diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.clone())
        .collect();

    (
        Section {
            body: stmts,
            section_type,
            name: None,
        },
        errors,
    )
}

pub fn make_section_bundle(
    file_path: impl Into<PathBuf>,
    file_index: usize,
    imported_files: Vec<usize>,
    sections: Vec<Section>,
    root_import_span: Option<Span8>,
) -> Arc<SectionBundle> {
    Arc::new(SectionBundle {
        file_path: file_path.into(),
        file_index,
        imported_files,
        sections,
        root_import_span,
        was_cached: false,
    })
}

pub fn compile_bundles(bundles: &[Arc<SectionBundle>]) -> compiler::compiler::CompileResult {
    compiler::compiler::compile(&mut CompilerCache::default(), None, bundles)
}

pub fn stdlib_path(name: &str) -> PathBuf {
    Assets::std_lib().join(format!("std/{name}.mcl"))
}

pub fn stdlib_bundle(name: &str) -> Arc<SectionBundle> {
    load_stdlib_bundle_with_import_span(stdlib_path(name), 0..0)
}

pub fn stdlib_bundle_with_import_span(name: &str, import_span: Span8) -> Arc<SectionBundle> {
    load_stdlib_bundle_with_import_span(stdlib_path(name), import_span)
}

pub fn stdlib_bundles<const N: usize>(names: [&str; N]) -> [Arc<SectionBundle>; N] {
    names.map(stdlib_bundle)
}

pub fn load_stdlib_bundle_with_import_span(
    path: impl AsRef<Path>,
    import_span: Span8,
) -> Arc<SectionBundle> {
    let src = fs::read_to_string(path.as_ref()).expect("failed to read stdlib file");
    let (section, errors) = parse_section(&src, SectionType::StandardLibrary);
    assert!(errors.is_empty(), "stdlib parse errors: {errors:?}");

    make_section_bundle(
        path.as_ref().to_path_buf(),
        0,
        vec![],
        vec![section],
        Some(import_span),
    )
}

pub fn make_imported_bundle(
    src: &str,
    section_type: SectionType,
    import_span: Span8,
) -> Arc<SectionBundle> {
    let (section, errors) = parse_section(src, section_type);
    assert!(
        errors.is_empty(),
        "imported bundle parse errors: {errors:?}"
    );

    make_section_bundle(
        PathBuf::from("imported.mcl"),
        0,
        vec![],
        vec![section],
        Some(import_span),
    )
}

pub fn inspect_block(title: &str, lines: impl IntoIterator<Item = String>) -> String {
    let mut out = format!("--- {title} ---");
    for line in lines {
        out.push('\n');
        out.push_str(&line);
    }
    out
}

pub fn print_inspection(title: &str, lines: impl IntoIterator<Item = String>) {
    eprintln!("{}", inspect_block(title, lines));
}

pub fn value_summary(value: &Value) -> String {
    value_summary_at(value, 2)
}

fn value_summary_at(value: &Value, depth: usize) -> String {
    if depth == 0 {
        return value.type_name().to_string();
    }

    match value {
        Value::Nil => "nil".to_string(),
        Value::Float(value) => format!("{value:?}"),
        Value::Integer(value) => value.to_string(),
        Value::Complex { re, im } => format!("{re:?} + {im:?}i"),
        Value::String(value) => format!("{value:?}"),
        Value::Mesh(mesh) => format!(
            "mesh(tag={:?}, dots={}, lins={}, tris={})",
            mesh.tag,
            mesh.dots.len(),
            mesh.lins.len(),
            mesh.tris.len()
        ),
        Value::PrimitiveAnim(_) => "primitive anim".to_string(),
        Value::Lambda(lambda) => format!("lambda(args={})", lambda.arg_names.len()),
        Value::Operator(_) => "operator".to_string(),
        Value::AnimBlock(_) => "anim block".to_string(),
        Value::Map(map) => {
            let entries = map
                .iter()
                .take(6)
                .map(|(key, value)| {
                    let value = with_heap(|heap| heap.get(value.key()).clone());
                    format!(
                        "{}: {}",
                        hashable_key_summary(key),
                        value_summary_at(&value, depth - 1)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if map.len() > 6 { ", ..." } else { "" };
            format!("map(len={}, {{{entries}{suffix}}})", map.len())
        }
        Value::List(list) => {
            let entries = list
                .elements()
                .iter()
                .take(8)
                .map(|value| {
                    let value = with_heap(|heap| heap.get(value.key()).clone());
                    value_summary_at(&value, depth - 1)
                })
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if list.len() > 8 { ", ..." } else { "" };
            format!("list(len={}, [{entries}{suffix}])", list.len())
        }
        Value::Stateful(_) => "stateful".to_string(),
        Value::Leader(leader) => {
            let leader_value = with_heap(|heap| heap.get(leader.leader_rc.key()).clone());
            let follower_value = with_heap(|heap| heap.get(leader.follower_rc.key()).clone());
            format!(
                "leader(target={}, current={})",
                value_summary_at(&leader_value, depth - 1),
                value_summary_at(&follower_value, depth - 1)
            )
        }
        Value::InvokedOperator(_) => "live operator".to_string(),
        Value::InvokedFunction(invoked) => {
            format!("live function(args={})", invoked.body.arguments.len())
        }
        Value::Lvalue(value) => {
            let value = with_heap(|heap| heap.get(value.key()).clone());
            format!("lvalue({})", value_summary_at(&value, depth - 1))
        }
        Value::WeakLvalue(value) => {
            let value = with_heap(|heap| heap.get(value.key()).clone());
            format!("weak lvalue({})", value_summary_at(&value, depth - 1))
        }
    }
}

fn hashable_key_summary(key: &HashableKey) -> String {
    match key {
        HashableKey::Integer(value) => value.to_string(),
        HashableKey::Float(bits) => HashableKey::float_value(*bits).to_string(),
        HashableKey::String(value) => format!("{value:?}"),
        HashableKey::List(values) => {
            let values = values
                .iter()
                .map(hashable_key_summary)
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{values}]")
        }
    }
}
