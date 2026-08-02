use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentationParameter {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Documentation {
    pub name: String,
    pub declaration_span: std::ops::Range<usize>,
    pub source: String,
    pub is_root: bool,
    pub overview: String,
    pub description: String,
    pub parameters: Vec<DocumentationParameter>,
    pub examples: Vec<String>,
}

#[derive(Default)]
struct DocumentationParts {
    overview: Vec<String>,
    description: Vec<String>,
    parameters: Vec<DocumentationParameter>,
    examples: Vec<String>,
}

impl DocumentationParts {
    fn is_empty(&self) -> bool {
        self.overview.is_empty()
            && self.description.is_empty()
            && self.parameters.is_empty()
            && self.examples.is_empty()
    }
}

pub fn extract(source: &str, path: &Path, is_root: bool) -> Vec<Documentation> {
    let mut docs = Vec::new();
    let mut pending = Vec::new();
    let mut offset = 0;

    for line in source.split_inclusive('\n') {
        let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = line_without_newline.trim_start();

        if let Some(comment) = trimmed.strip_prefix("##") {
            pending.push(comment.strip_prefix(' ').unwrap_or(comment).to_string());
        } else if let Some((name, name_offset)) = declaration_name(trimmed) {
            let parts = parse_parts(&pending);
            if !parts.is_empty() {
                let leading_whitespace = line_without_newline.len() - trimmed.len();
                let declaration_start = offset + leading_whitespace + name_offset;
                docs.push(Documentation {
                    name,
                    declaration_span: declaration_start
                        ..declaration_start + name_offset_len(trimmed, name_offset),
                    source: path.display().to_string(),
                    is_root,
                    overview: parts.overview.join("\n"),
                    description: parts.description.join("\n"),
                    parameters: parts.parameters,
                    examples: parts.examples,
                });
            }
            pending.clear();
        } else if !trimmed.is_empty() {
            pending.clear();
        }

        offset += line.len();
    }

    docs
}

fn declaration_name(line: &str) -> Option<(String, usize)> {
    let keywords = ["let", "var", "mesh", "param", "anim"];
    for keyword in keywords {
        let Some(rest) = line.strip_prefix(keyword) else {
            continue;
        };
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim_start();
        let name_len = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_alphanumeric() || *ch == '_')
            .map(|(index, ch)| index + ch.len_utf8())
            .last()
            .unwrap_or(0);
        if name_len == 0 {
            return None;
        }
        let name_offset = line.len() - rest.len();
        return Some((rest[..name_len].to_string(), name_offset));
    }
    None
}

fn name_offset_len(line: &str, name_offset: usize) -> usize {
    line[name_offset..]
        .char_indices()
        .take_while(|(_, ch)| ch.is_alphanumeric() || *ch == '_')
        .map(|(index, ch)| index + ch.len_utf8())
        .last()
        .unwrap_or(0)
}

fn parse_parts(lines: &[String]) -> DocumentationParts {
    #[derive(Clone, Copy)]
    enum Section {
        Overview,
        Description,
        Parameters,
        Example,
        Ignore,
    }

    let mut parts = DocumentationParts::default();
    let mut section = Section::Overview;
    let mut example = Vec::new();

    let finish_example = |parts: &mut DocumentationParts, example: &mut Vec<String>| {
        let content = example.join("\n").trim().to_string();
        if !content.is_empty() {
            parts.examples.push(content);
        }
        example.clear();
    };

    for line in lines {
        let next_section = match line.trim() {
            "[overview]" => Some(Section::Overview),
            "[description]" => Some(Section::Description),
            "[parameters]" => Some(Section::Parameters),
            "[example]" => Some(Section::Example),
            value if value.starts_with('[') && value.ends_with(']') => Some(Section::Ignore),
            _ => None,
        };
        if let Some(next_section) = next_section {
            if matches!(section, Section::Example) {
                finish_example(&mut parts, &mut example);
            }
            section = next_section;
            continue;
        }

        match section {
            Section::Overview => parts.overview.push(line.clone()),
            Section::Description => parts.description.push(line.clone()),
            Section::Parameters => {
                if let Some((name, description)) = line.split_once(':') {
                    parts.parameters.push(DocumentationParameter {
                        name: name.trim().to_string(),
                        description: description.trim().to_string(),
                    });
                }
            }
            Section::Example => example.push(line.clone()),
            Section::Ignore => {}
        }
    }
    if matches!(section, Section::Example) {
        finish_example(&mut parts, &mut example);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attaches_tagged_comment_block_to_following_declaration() {
        let docs = extract(
            "## [overview]\n## Convert a CSS color.\n## [parameters]\n## value: color string\n## [example]\n## hex(\"009ee0\")\nlet hex = |value| value\n",
            Path::new("color.mcl"),
            false,
        );

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].name, "hex");
        assert_eq!(docs[0].overview, "Convert a CSS color.");
        assert_eq!(docs[0].parameters[0].name, "value");
        assert_eq!(docs[0].examples, ["hex(\"009ee0\")"]);
    }

    #[test]
    fn ignores_docs_separated_from_a_declaration_by_code() {
        let docs = extract(
            "## documented\nprint 1\nlet value = 1\n",
            Path::new("scene.mcs"),
            true,
        );
        assert!(docs.is_empty());
    }
}
