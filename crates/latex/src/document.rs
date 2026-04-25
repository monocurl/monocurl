use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use anyhow::{Result, bail};

pub(crate) const SPAN_ID_PREFIX: &str = "mc-span-";

const SHARED_MACROS: &str = "\\newcommand{\\pin}[2]{{\\color[RGB]{#1,255,255} #2}}\n\
\\renewcommand{\\P}[2]{{\\color[RGB]{#1,255,255} #2}}\n\
\\newcommand{\\rowpin}[3]{{\\color[RGB]{255,#1,#2} #3}}\n\
\\newcommand{\\RP}[3]{{\\color[RGB]{255,#2,#1} #3}}\n";

const LATEX_PREAMBLE: &str = "\\documentclass[preview]{standalone}\n\
\\usepackage{amsmath}\n\
\\usepackage{amssymb}\n\
\\usepackage{amsfonts}\n\
\\usepackage{xcolor}\n\
\\usepackage{graphicx}\n\
";

const LATEX_POSTAMBLE: &str = "\n\\end{document}\n";
const TEXT_TAG_MACRO: &str = "\\text_tag";
const TEXT_TAG_SHORTCUT_PREFIX: &str = "\\tag";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SpanMarker {
    pub id: String,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TaggedSpan {
    pub tag: Vec<isize>,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TaggedSource {
    pub source: String,
    pub spans: Vec<TaggedSpan>,
}

pub(crate) fn build_text_document(text: &str) -> String {
    format!("{LATEX_PREAMBLE}{SHARED_MACROS}\\begin{{document}}\n{text}{LATEX_POSTAMBLE}")
}

pub(crate) fn build_tex_document(tex: &str) -> String {
    format!("{LATEX_PREAMBLE}{SHARED_MACROS}\\begin{{document}}\n\\[\n{tex}\n\\]{LATEX_POSTAMBLE}")
}

pub(crate) fn build_latex_document(body: &str) -> String {
    format!("{LATEX_PREAMBLE}{SHARED_MACROS}\\begin{{document}}\n{body}{LATEX_POSTAMBLE}")
}

pub(crate) fn build_mathjax_source(tex: &str, markers: &[SpanMarker]) -> Result<String> {
    Ok(format!(
        "{SHARED_MACROS}{}",
        apply_span_markers(tex, markers)?
    ))
}

pub(crate) fn parse_text_tags(source: &str) -> Result<TaggedSource> {
    parse_text_tags_impl(source)
}

pub(crate) fn apply_legacy_text_tags(source: &str, spans: &[TaggedSpan]) -> Result<String> {
    if spans.is_empty() {
        return Ok(source.to_owned());
    }

    for span in spans {
        if span.range.start >= span.range.end || span.range.end > source.len() {
            bail!("text tag span is out of bounds");
        }
        if !source.is_char_boundary(span.range.start) || !source.is_char_boundary(span.range.end) {
            bail!("text tag span is not aligned to UTF-8 boundaries");
        }
        if !(1..=2).contains(&span.tag.len()) {
            bail!("text tags support one or two tag components");
        }
    }
    validate_nested_ranges(
        spans.iter().map(|span| span.range.clone()),
        "text tag spans are not properly nested",
    )?;

    Ok(apply_wrappers(
        source,
        spans.iter().map(|span| Wrapper {
            range: span.range.clone(),
            open: legacy_text_tag_open(&span.tag),
            close: "}".into(),
        }),
    ))
}

pub(crate) fn text_tag_marker_id(index: usize) -> String {
    format!("text-tag-{index}")
}

pub(crate) fn strip_span_prefix(id: &str) -> Option<&str> {
    id.strip_prefix(SPAN_ID_PREFIX)
}

fn parse_text_tags_impl(source: &str) -> Result<TaggedSource> {
    let mut out = String::new();
    let mut spans = Vec::new();
    let mut cursor = 0usize;

    while cursor < source.len() {
        if let Some((tag, body_source, after_body)) = parse_text_tag_at(source, cursor)? {
            let body = parse_text_tags_impl(body_source)?;
            if body.source.is_empty() {
                bail!("\\text_tag body must not be empty");
            }
            let start = out.len();
            out.push_str(&body.source);
            let end = out.len();
            spans.push(TaggedSpan {
                tag,
                range: start..end,
            });
            let offset = start;
            spans.extend(body.spans.into_iter().map(|span| TaggedSpan {
                tag: span.tag,
                range: (span.range.start + offset)..(span.range.end + offset),
            }));
            cursor = after_body;
            continue;
        }

        let ch = source[cursor..]
            .chars()
            .next()
            .expect("cursor is within source");
        out.push(ch);
        cursor += ch.len_utf8();
    }

    Ok(TaggedSource { source: out, spans })
}

fn apply_span_markers(source: &str, markers: &[SpanMarker]) -> Result<String> {
    if markers.is_empty() {
        return Ok(source.to_owned());
    }

    let mut markers = markers.to_vec();
    markers.sort_unstable_by_key(|marker| (marker.range.start, marker.range.end));

    let mut seen_ids = HashSet::new();
    for marker in &markers {
        if marker.range.start >= marker.range.end || marker.range.end > source.len() {
            bail!("span marker `{}` is out of bounds", marker.id);
        }
        if !source.is_char_boundary(marker.range.start)
            || !source.is_char_boundary(marker.range.end)
        {
            bail!(
                "span marker `{}` is not aligned to UTF-8 boundaries",
                marker.id
            );
        }
        let dom_id = span_dom_id(&marker.id);
        if !seen_ids.insert(dom_id.clone()) {
            bail!("duplicate span marker id `{}`", marker.id);
        }
    }
    validate_nested_ranges(
        markers.iter().map(|marker| marker.range.clone()),
        "span markers are not properly nested",
    )?;

    Ok(apply_wrappers(
        source,
        markers.into_iter().map(|marker| Wrapper {
            range: marker.range,
            open: format!("\\cssId{{{}}}{{", span_dom_id(&marker.id)),
            close: "}".into(),
        }),
    ))
}

fn span_dom_id(id: &str) -> String {
    let sanitized = id
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => ch,
            _ => '_',
        })
        .collect::<String>();
    format!("{SPAN_ID_PREFIX}{sanitized}")
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Wrapper {
    range: Range<usize>,
    open: String,
    close: String,
}

fn validate_nested_ranges(
    ranges: impl IntoIterator<Item = Range<usize>>,
    crossing_message: &str,
) -> Result<()> {
    let mut ranges = ranges.into_iter().collect::<Vec<_>>();
    ranges.sort_unstable_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));

    let mut stack = Vec::new();
    for range in ranges {
        while stack.last().is_some_and(|&end| range.start >= end) {
            stack.pop();
        }
        if let Some(&parent_end) = stack.last() {
            if range.end > parent_end {
                bail!("{crossing_message}");
            }
        }
        stack.push(range.end);
    }

    Ok(())
}

fn apply_wrappers(source: &str, wrappers: impl IntoIterator<Item = Wrapper>) -> String {
    let wrappers = wrappers.into_iter().collect::<Vec<_>>();
    let mut opens = HashMap::<usize, Vec<(usize, String)>>::new();
    let mut closes = HashMap::<usize, Vec<(usize, String)>>::new();

    for wrapper in wrappers {
        opens
            .entry(wrapper.range.start)
            .or_default()
            .push((wrapper.range.end, wrapper.open));
        closes
            .entry(wrapper.range.end)
            .or_default()
            .push((wrapper.range.start, wrapper.close));
    }

    for open in opens.values_mut() {
        open.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    }
    for close in closes.values_mut() {
        close.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    }

    let mut out = String::with_capacity(
        source.len()
            + opens
                .values()
                .map(|entries| entries.iter().map(|(_, text)| text.len()).sum::<usize>())
                .sum::<usize>()
            + closes
                .values()
                .map(|entries| entries.iter().map(|(_, text)| text.len()).sum::<usize>())
                .sum::<usize>(),
    );

    for (index, ch) in source.char_indices() {
        if let Some(entries) = closes.get(&index) {
            for (_, text) in entries {
                out.push_str(text);
            }
        }
        if let Some(entries) = opens.get(&index) {
            for (_, text) in entries {
                out.push_str(text);
            }
        }
        out.push(ch);
    }

    if let Some(entries) = closes.get(&source.len()) {
        for (_, text) in entries {
            out.push_str(text);
        }
    }
    if let Some(entries) = opens.get(&source.len()) {
        for (_, text) in entries {
            out.push_str(text);
        }
    }

    out
}

fn parse_text_tag_spec(source: &str) -> Result<Vec<isize>> {
    let source = source.trim();
    if source.is_empty() {
        return Ok(Vec::new());
    }

    let source = source
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(source);
    let source = source.trim();
    if source.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for part in source.split(',') {
        let part = part.trim();
        if part.is_empty() {
            bail!("\\text_tag contains an empty tag component");
        }
        out.push(part.parse()?);
    }
    Ok(out)
}

fn parse_text_tag_at(source: &str, cursor: usize) -> Result<Option<(Vec<isize>, &str, usize)>> {
    if source[cursor..].starts_with(TEXT_TAG_MACRO) {
        let mut next = skip_ascii_whitespace(source, cursor + TEXT_TAG_MACRO.len());
        if source.as_bytes().get(next) == Some(&b'{') {
            let (tag_source, after_tag) = parse_braced_group(source, next)?;
            next = skip_ascii_whitespace(source, after_tag);
            if source.as_bytes().get(next) != Some(&b'{') {
                bail!("\\text_tag requires a second braced argument");
            }

            let (body_source, after_body) = parse_braced_group(source, next)?;
            return Ok(Some((
                parse_text_tag_spec(tag_source)?,
                body_source,
                after_body,
            )));
        }
    }

    if let Some((tag, body_source, after_body)) = parse_numbered_text_tag_shortcut(source, cursor)?
    {
        return Ok(Some((vec![tag], body_source, after_body)));
    }

    Ok(None)
}

fn parse_numbered_text_tag_shortcut(
    source: &str,
    cursor: usize,
) -> Result<Option<(isize, &str, usize)>> {
    if !source[cursor..].starts_with(TEXT_TAG_SHORTCUT_PREFIX) {
        return Ok(None);
    }

    let mut next = cursor + TEXT_TAG_SHORTCUT_PREFIX.len();
    let digits_start = next;
    while source.as_bytes().get(next).is_some_and(u8::is_ascii_digit) {
        next += 1;
    }
    if next == digits_start {
        return Ok(None);
    }

    let tag = source[digits_start..next].parse()?;
    next = skip_ascii_whitespace(source, next);
    if source.as_bytes().get(next) != Some(&b'{') {
        return Ok(None);
    }

    let (body_source, after_body) = parse_braced_group(source, next)?;
    Ok(Some((tag, body_source, after_body)))
}

fn parse_braced_group(source: &str, open_brace: usize) -> Result<(&str, usize)> {
    if source.as_bytes().get(open_brace) != Some(&b'{') {
        bail!("expected braced group");
    }

    let body_start = open_brace + 1;
    let mut depth = 1usize;
    for (offset, ch) in source[body_start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let close_brace = body_start + offset;
                    return Ok((&source[body_start..close_brace], close_brace + 1));
                }
            }
            _ => {}
        }
    }

    bail!("unterminated braced group")
}

fn skip_ascii_whitespace(source: &str, mut cursor: usize) -> usize {
    while source
        .as_bytes()
        .get(cursor)
        .is_some_and(u8::is_ascii_whitespace)
    {
        cursor += 1;
    }
    cursor
}

fn legacy_text_tag_open(tag: &[isize]) -> String {
    match tag {
        [tag] => format!("\\pin{{{tag}}}{{"),
        [row, col] => format!("\\rowpin{{{row}}}{{{col}}}{{"),
        _ => unreachable!("validated by apply_legacy_text_tags"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SpanMarker, TaggedSource, TaggedSpan, apply_legacy_text_tags, build_mathjax_source,
        build_tex_document, build_text_document, parse_text_tags,
    };

    #[test]
    fn text_document_keeps_raw_input() {
        let doc = build_text_document("hello");
        assert!(doc.contains("\nhello\n"));
    }

    #[test]
    fn tex_document_wraps_input_in_display_math() {
        let doc = build_tex_document("x^2");
        assert!(doc.contains("\n\\[\nx^2\n\\]\n"));
    }

    #[test]
    fn mathjax_source_wraps_marked_spans() {
        let marked = build_mathjax_source(
            "x+y",
            &[SpanMarker {
                id: "lhs".into(),
                range: 0..1,
            }],
        )
        .unwrap();
        assert!(marked.contains(r"\cssId{mc-span-lhs}{x}"));
    }

    #[test]
    fn parse_text_tags_strips_wrappers_and_tracks_ranges() {
        let parsed =
            parse_text_tags(r"\text_tag{1}{x^2} + \text_tag{[2, 3]}{y} + \text_tag{}{z}").unwrap();
        assert_eq!(
            parsed,
            TaggedSource {
                source: "x^2 + y + z".into(),
                spans: vec![
                    TaggedSpan {
                        tag: vec![1],
                        range: 0..3,
                    },
                    TaggedSpan {
                        tag: vec![2, 3],
                        range: 6..7,
                    },
                    TaggedSpan {
                        tag: Vec::new(),
                        range: 10..11,
                    },
                ],
            }
        );
    }

    #[test]
    fn parse_text_tags_accepts_numbered_shortcuts() {
        let parsed = parse_text_tags(r"\tag0{x^2} + \tag12{y}").unwrap();
        assert_eq!(
            parsed,
            TaggedSource {
                source: "x^2 + y".into(),
                spans: vec![
                    TaggedSpan {
                        tag: vec![0],
                        range: 0..3,
                    },
                    TaggedSpan {
                        tag: vec![12],
                        range: 6..7,
                    },
                ],
            }
        );
    }

    #[test]
    fn parse_text_tags_supports_nested_wrappers_with_inner_priority() {
        let parsed = parse_text_tags(r"\text_tag{1}{a\text_tag{2}{b}c\tag3{de}f}").unwrap();
        assert_eq!(
            parsed,
            TaggedSource {
                source: "abcdef".into(),
                spans: vec![
                    TaggedSpan {
                        tag: vec![1],
                        range: 0..6,
                    },
                    TaggedSpan {
                        tag: vec![2],
                        range: 1..2,
                    },
                    TaggedSpan {
                        tag: vec![3],
                        range: 3..5,
                    },
                ],
            }
        );
    }

    #[test]
    fn legacy_text_tags_rewrite_to_pin_macros() {
        let tagged = apply_legacy_text_tags(
            "lhs + rhs",
            &[
                TaggedSpan {
                    tag: vec![1],
                    range: 0..3,
                },
                TaggedSpan {
                    tag: vec![2, 3],
                    range: 6..9,
                },
            ],
        )
        .unwrap();
        assert_eq!(tagged, r"\pin{1}{lhs} + \rowpin{2}{3}{rhs}");
    }

    #[test]
    fn legacy_text_tags_preserve_nested_tex_groups() {
        let tagged = apply_legacy_text_tags(
            r"\frac{a}{b}",
            &[
                TaggedSpan {
                    tag: vec![1],
                    range: 0..11,
                },
                TaggedSpan {
                    tag: vec![2],
                    range: 9..10,
                },
            ],
        )
        .unwrap();
        assert_eq!(tagged, r"\pin{1}{\frac{a}{\pin{2}{b}}}");
    }
}
