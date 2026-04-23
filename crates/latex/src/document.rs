use std::{collections::HashSet, ops::Range};

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
    parse_text_tags_impl(source, true)
}

pub(crate) fn apply_legacy_text_tags(source: &str, spans: &[TaggedSpan]) -> Result<String> {
    if spans.is_empty() {
        return Ok(source.to_owned());
    }

    let mut cursor = 0usize;
    for span in spans {
        if span.range.start >= span.range.end || span.range.end > source.len() {
            bail!("text tag span is out of bounds");
        }
        if !source.is_char_boundary(span.range.start) || !source.is_char_boundary(span.range.end) {
            bail!("text tag span is not aligned to UTF-8 boundaries");
        }
        if span.range.start < cursor {
            bail!("text tag spans overlap");
        }
        if !(1..=2).contains(&span.tag.len()) {
            bail!("text tags support one or two tag components");
        }
        cursor = span.range.end;
    }

    let mut tagged = source.to_owned();
    for span in spans.iter().rev() {
        tagged.insert(span.range.end, '}');
        tagged.insert_str(span.range.start, &legacy_text_tag_open(&span.tag));
    }
    Ok(tagged)
}

pub(crate) fn text_tag_marker_id(index: usize) -> String {
    format!("text-tag-{index}")
}

pub(crate) fn strip_span_prefix(id: &str) -> Option<&str> {
    id.strip_prefix(SPAN_ID_PREFIX)
}

fn parse_text_tags_impl(source: &str, allow_tags: bool) -> Result<TaggedSource> {
    let mut out = String::new();
    let mut spans = Vec::new();
    let mut cursor = 0usize;

    while cursor < source.len() {
        if source[cursor..].starts_with(TEXT_TAG_MACRO) {
            if !allow_tags {
                bail!("nested \\text_tag is not supported");
            }

            let mut next = skip_ascii_whitespace(source, cursor + TEXT_TAG_MACRO.len());
            if source.as_bytes().get(next) == Some(&b'{') {
                let (tag_source, after_tag) = parse_braced_group(source, next)?;
                next = skip_ascii_whitespace(source, after_tag);
                if source.as_bytes().get(next) != Some(&b'{') {
                    bail!("\\text_tag requires a second braced argument");
                }

                let (body_source, after_body) = parse_braced_group(source, next)?;
                let body = parse_text_tags_impl(body_source, false)?;
                let start = out.len();
                out.push_str(&body.source);
                let end = out.len();
                if start == end {
                    bail!("\\text_tag body must not be empty");
                }
                spans.push(TaggedSpan {
                    tag: parse_text_tag_spec(tag_source)?,
                    range: start..end,
                });
                cursor = after_body;
                continue;
            }
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
    let mut cursor = 0usize;
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
        if marker.range.start < cursor {
            bail!("span marker `{}` overlaps a previous marker", marker.id);
        }
        cursor = marker.range.end;

        let dom_id = span_dom_id(&marker.id);
        if !seen_ids.insert(dom_id.clone()) {
            bail!("duplicate span marker id `{}`", marker.id);
        }
    }

    let mut marked = source.to_owned();
    for marker in markers.iter().rev() {
        marked.insert(marker.range.end, '}');
        marked.insert_str(
            marker.range.start,
            &format!("\\cssId{{{}}}{{", span_dom_id(&marker.id)),
        );
    }

    Ok(marked)
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

fn parse_text_tag_spec(source: &str) -> Result<Vec<isize>> {
    let source = source.trim();
    if source.is_empty() {
        bail!("\\text_tag requires at least one tag component");
    }

    let source = source
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(source);
    let mut out = Vec::new();
    for part in source.split(',') {
        let part = part.trim();
        if part.is_empty() {
            bail!("\\text_tag contains an empty tag component");
        }
        out.push(part.parse()?);
    }

    if out.is_empty() {
        bail!("\\text_tag requires at least one tag component");
    }
    Ok(out)
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
        let parsed = parse_text_tags(r"\text_tag{1}{x^2} + \text_tag{[2, 3]}{y}").unwrap();
        assert_eq!(
            parsed,
            TaggedSource {
                source: "x^2 + y".into(),
                spans: vec![
                    TaggedSpan {
                        tag: vec![1],
                        range: 0..3,
                    },
                    TaggedSpan {
                        tag: vec![2, 3],
                        range: 6..7,
                    },
                ],
            }
        );
    }

    #[test]
    fn parse_text_tags_rejects_nested_wrappers() {
        let error = parse_text_tags(r"\text_tag{1}{\text_tag{2}{x}}").unwrap_err();
        assert!(error.to_string().contains("nested \\text_tag"));
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
}
