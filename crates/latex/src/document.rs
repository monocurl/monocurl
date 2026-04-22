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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SpanMarker {
    pub id: String,
    pub range: Range<usize>,
}

pub(crate) fn build_text_document(text: &str) -> String {
    format!("{LATEX_PREAMBLE}{SHARED_MACROS}\\begin{{document}}\n{text}{LATEX_POSTAMBLE}")
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

pub(crate) fn strip_span_prefix(id: &str) -> Option<&str> {
    id.strip_prefix(SPAN_ID_PREFIX)
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

#[cfg(test)]
mod tests {
    use super::{SpanMarker, build_mathjax_source, build_text_document};

    #[test]
    fn text_document_keeps_raw_input() {
        let doc = build_text_document("hello");
        assert!(doc.contains("\nhello\n"));
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
}
