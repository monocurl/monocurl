use std::sync::Arc;

use structs::text::Span8;

use crate::{
    heap::with_heap,
    value::{Value, container::Map},
};

const MAX_ENTRY_LEN: usize = 160;

/// transcript payload. currently string-only, but kept as an enum so richer
/// display structures can be added without changing the transport surface
#[derive(Clone, Debug)]
pub enum TranscriptEntryKind {
    String(String),
}

impl TranscriptEntryKind {
    pub fn as_str(&self) -> &str {
        match self {
            Self::String(s) => s,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TranscriptEntry {
    pub span: Span8,
    pub section: u16,
    /// whether the originating section is in the root module; non-root prints
    /// (library imports) are included in the console transcript only and not
    /// rendered inline in the editor
    pub is_root: bool,
    pub kind: TranscriptEntryKind,
}

impl TranscriptEntry {
    pub fn text(&self) -> &str {
        self.kind.as_str()
    }
}

#[derive(Clone, Debug, Default)]
pub struct SectionTranscript {
    pub entries: Vec<TranscriptEntry>,
}

#[derive(Clone, Debug, Default)]
pub struct Transcript {
    /// frozen-once-advanced section transcripts; index = section index
    pub sections: Vec<Arc<SectionTranscript>>,
}

impl Transcript {
    pub fn clear(&mut self) {
        self.sections.clear();
    }

    /// truncate transcript so it only retains the entries that belong to
    /// sections strictly before `section_idx`. used when restoring to an
    /// earlier cache point
    pub fn truncate_to(&mut self, section_idx: usize) {
        if self.sections.len() > section_idx {
            self.sections.truncate(section_idx);
        }
    }

    pub fn append(&mut self, section_idx: usize, entry: TranscriptEntry) {
        while self.sections.len() <= section_idx {
            self.sections.push(Arc::new(SectionTranscript::default()));
        }
        Arc::make_mut(&mut self.sections[section_idx])
            .entries
            .push(entry);
    }

    pub fn iter_entries(&self) -> impl Iterator<Item = &TranscriptEntry> {
        self.sections.iter().flat_map(|s| s.entries.iter())
    }
}

/// stringify a value for transcript display. fully resolves lvalues / leaders
/// recursively (since reactive wrappers may not be evaluable here without an
/// async executor); live function / operator / stateful values are shown as
/// short placeholders.
pub fn stringify_for_transcript(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out, 0);
    if out.chars().count() > MAX_ENTRY_LEN {
        let mut cap = String::new();
        let prefix_len = MAX_ENTRY_LEN.saturating_sub(3);
        for (i, ch) in out.chars().enumerate() {
            if i >= prefix_len {
                break;
            }
            cap.push(ch);
        }
        cap.push_str("...");
        cap
    } else {
        out
    }
}

const MAX_DEPTH: usize = 6;

fn write_value(value: &Value, out: &mut String, depth: usize) {
    if depth > MAX_DEPTH {
        out.push_str("...");
        return;
    }
    match value {
        Value::Nil => out.push_str("nil"),
        Value::Integer(n) => {
            use std::fmt::Write;
            let _ = write!(out, "{}", n);
        }
        Value::Float(f) => {
            use std::fmt::Write;
            if f.fract() == 0.0 && f.is_finite() {
                let _ = write!(out, "{:.1}", f);
            } else {
                let _ = write!(out, "{}", f);
            }
        }
        Value::Complex { re, im } => {
            use std::fmt::Write;
            if *re == 0.0 {
                let _ = write!(out, "{}i", im);
            } else if *im < 0.0 {
                let _ = write!(out, "{} - {}i", re, -im);
            } else {
                let _ = write!(out, "{} + {}i", re, im);
            }
        }
        Value::String(s) => {
            out.push('"');
            for ch in s.chars() {
                match ch {
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    '\t' => out.push_str("\\t"),
                    _ => out.push(ch),
                }
            }
            out.push('"');
        }
        Value::Mesh(_) => out.push_str("<mesh>"),
        Value::PrimitiveAnim(_) => out.push_str("<primitive_anim>"),
        Value::Lambda(_) => out.push_str("<lambda>"),
        Value::Operator(_) => out.push_str("<operator>"),
        Value::AnimBlock(_) => out.push_str("<anim>"),
        Value::List(list) => {
            out.push('[');
            for (i, el) in list.elements().iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let inner = with_heap(|h| h.get(el.key()).clone());
                write_value(&inner, out, depth + 1);
                if out.chars().count() > MAX_ENTRY_LEN + 10 {
                    out.push_str(", ...");
                    break;
                }
            }
            out.push(']');
        }
        Value::Map(map) => write_map(map, out, depth),
        Value::Stateful(_) => out.push_str("<stateful>"),
        Value::Leader(leader) => {
            let inner = with_heap(|h| h.get(leader.leader_rc.key()).clone());
            write_value(&inner, out, depth + 1);
        }
        Value::InvokedOperator(_) => out.push_str("<live operator>"),
        Value::InvokedFunction(_) => out.push_str("<live function>"),
        Value::Lvalue(rc) => {
            let inner = with_heap(|h| h.get(rc.key()).clone());
            write_value(&inner, out, depth + 1);
        }
        Value::WeakLvalue(weak) => {
            let inner = with_heap(|h| h.get(weak.key()).clone());
            write_value(&inner, out, depth + 1);
        }
    }
}

fn write_map(map: &Map, out: &mut String, depth: usize) {
    out.push('{');
    let mut first = true;
    for (key, value_rc) in map.iter() {
        if !first {
            out.push_str(", ");
        }
        first = false;
        write_hashable_key(key, out);
        out.push_str(" -> ");
        let inner = with_heap(|h| h.get(value_rc.key()).clone());
        write_value(&inner, out, depth + 1);
        if out.chars().count() > MAX_ENTRY_LEN + 10 {
            out.push_str(", ...");
            break;
        }
    }
    out.push('}');
}

fn write_hashable_key(key: &crate::value::container::HashableKey, out: &mut String) {
    use crate::value::container::HashableKey;
    use std::fmt::Write;
    match key {
        HashableKey::Integer(n) => {
            let _ = write!(out, "{}", n);
        }
        HashableKey::Float(bits) => {
            let _ = write!(out, "{}", HashableKey::float_value(*bits));
        }
        HashableKey::String(s) => {
            out.push('"');
            out.push_str(s);
            out.push('"');
        }
        HashableKey::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_hashable_key(item, out);
            }
            out.push(']');
        }
    }
}
