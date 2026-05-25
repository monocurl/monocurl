use std::{collections::HashMap, ops::Range};

use anyhow::{Result, bail};

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LatexDocumentStyle {
    BundledBasic,
    BundledUnicode,
    SystemLatex,
}

#[cfg(not(target_arch = "wasm32"))]
const SYSTEM_LATEX_PREAMBLE: &str = r"\documentclass[preview]{standalone}
\usepackage{amsmath}
\usepackage{amssymb}
\usepackage{amsfonts}
\usepackage{xcolor}
\usepackage{graphicx}
";

#[cfg(not(target_arch = "wasm32"))]
const BUNDLED_UNICODE_LATEX_PREAMBLE: &str = r"\documentclass[preview]{standalone}
\usepackage{amsmath}
\usepackage{amssymb}
\usepackage{amsfonts}
\usepackage{xcolor}
\usepackage{graphicx}
\usepackage{fontspec}
\defaultfontfeatures{Ligatures=TeX,Renderer=HarfBuzz}
\setmainfont{FreeSerif.otf}
\setmonofont{NotoSansMono-Regular.ttf}
\usepackage{xeCJK}
\xeCJKsetup{AutoFallBack=true,CJKmath=true}
\setCJKmainfont{UnBatang.ttf}
\setCJKfallbackfamilyfont{\CJKrmdefault}{FandolSong-Regular.otf}
\setCJKmonofont{UnDotum.ttf}
\newfontfamily\monocurlArabicFont{Amiri-Regular.ttf}[Script=Arabic]
\newfontfamily\monocurlDevanagariFont{Mukta-Regular.ttf}[
  Script=Devanagari
]
";

#[cfg(not(target_arch = "wasm32"))]
const LATEX_BEGIN_DOCUMENT: &str = r"\begin{document}
";
#[cfg(not(target_arch = "wasm32"))]
const LATEX_POSTAMBLE: &str = r"
\end{document}
";
const TEXT_TAG_MACRO: &str = r"\text_tag";
const TEXT_TAG_SHORTCUT_PREFIX: &str = r"\tag";

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

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn build_text_document(text: &str, style: LatexDocumentStyle) -> String {
    build_document(text, "", style)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn build_tex_document(tex: &str, style: LatexDocumentStyle) -> String {
    build_document(
        &format!(
            r"\noindent\(\displaystyle
{tex}
\)"
        ),
        "",
        style,
    )
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn build_latex_document(
    body: &str,
    additional_preamble: &str,
    style: LatexDocumentStyle,
) -> String {
    build_document(body, additional_preamble, style)
}

#[cfg(not(target_arch = "wasm32"))]
fn build_document(body: &str, additional_preamble: &str, style: LatexDocumentStyle) -> String {
    let mut document = String::with_capacity(
        latex_preamble(style).len()
            + additional_preamble.len()
            + usize::from(!additional_preamble.is_empty())
            + LATEX_BEGIN_DOCUMENT.len()
            + body.len()
            + LATEX_POSTAMBLE.len(),
    );
    document.push_str(latex_preamble(style));
    push_additional_preamble(&mut document, additional_preamble);
    document.push_str(LATEX_BEGIN_DOCUMENT);
    document.push_str(body);
    document.push_str(LATEX_POSTAMBLE);
    document
}

#[cfg(not(target_arch = "wasm32"))]
fn latex_preamble(style: LatexDocumentStyle) -> &'static str {
    match style {
        LatexDocumentStyle::BundledBasic | LatexDocumentStyle::SystemLatex => SYSTEM_LATEX_PREAMBLE,
        LatexDocumentStyle::BundledUnicode => BUNDLED_UNICODE_LATEX_PREAMBLE,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn push_additional_preamble(document: &mut String, additional_preamble: &str) {
    if additional_preamble.is_empty() {
        return;
    }

    document.push_str(additional_preamble);
    if !additional_preamble.ends_with('\n') {
        document.push('\n');
    }
}

pub(crate) fn parse_text_tags(source: &str) -> Result<TaggedSource> {
    parse_text_tags_impl(source)
}

pub(crate) fn apply_text_tag_markers(source: &str, spans: &[TaggedSpan]) -> Result<String> {
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
        if span.tag.len() != 1 {
            bail!("text tag markers must have exactly one tag component");
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
            open: text_tag_marker_open(span.tag[0]),
            close: "}".into(),
        }),
    ))
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
        if let Some(&parent_end) = stack.last()
            && range.end > parent_end
        {
            bail!("{crossing_message}");
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

fn text_tag_marker_open(tag: isize) -> String {
    format!(r"{{\color[RGB]{{{tag},255,255}} ")
}

#[cfg(test)]
mod tests {
    use super::{
        LatexDocumentStyle, TaggedSource, TaggedSpan, apply_text_tag_markers, build_latex_document,
        build_tex_document, build_text_document, parse_text_tags,
    };

    #[test]
    fn text_document_keeps_raw_input() {
        let doc = build_text_document("Hello $1 + 4$", LatexDocumentStyle::SystemLatex);
        assert!(doc.contains("\nHello $1 + 4$\n"));
    }

    #[test]
    fn tex_document_wraps_input_in_unindented_inline_displaystyle_math() {
        let doc = build_tex_document("x^2", LatexDocumentStyle::SystemLatex);
        assert!(doc.contains("\n\\noindent\\(\\displaystyle\nx^2\n\\)\n"));
    }

    #[test]
    fn latex_document_inserts_additional_preamble_before_body() {
        let doc = build_latex_document(
            "hello",
            r"\usepackage{fontspec}",
            LatexDocumentStyle::SystemLatex,
        );
        assert!(doc.contains("\\usepackage{fontspec}\n\\begin{document}\nhello"));
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
    fn text_tag_markers_rewrite_to_color_groups() {
        let tagged = apply_text_tag_markers(
            "lhs + rhs",
            &[
                TaggedSpan {
                    tag: vec![1],
                    range: 0..3,
                },
                TaggedSpan {
                    tag: vec![2],
                    range: 6..9,
                },
            ],
        )
        .unwrap();
        assert_eq!(
            tagged,
            r"{\color[RGB]{1,255,255} lhs} + {\color[RGB]{2,255,255} rhs}"
        );
    }

    #[test]
    fn text_tag_markers_preserve_nested_tex_groups() {
        let tagged = apply_text_tag_markers(
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
        assert_eq!(
            tagged,
            r"{\color[RGB]{1,255,255} \frac{a}{{\color[RGB]{2,255,255} b}}}"
        );
    }
}
