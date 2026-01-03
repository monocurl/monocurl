use structs::{rope::{RLEAggregate, Rope, TextAggregate}, text::Span8};


#[derive(Default)]
pub struct DocumentState {
    pub text_rope: Rope<TextAggregate>,
    pub lex_rope: Rope<RLEAggregate<i32>>,
    pub static_analysis_rope: Rope<RLEAggregate<i32>>,

    pub dirty_range: Option<Span8>,
    pub version: usize,

    pub listeners: Vec<Box<dyn FnMut(Span8, &str, &Rope<TextAggregate>, usize) + Send>>,
}

impl DocumentState {

    pub fn add_listener(&mut self, f: impl FnMut(Span8, &str, &Rope<TextAggregate>, usize) + Send + 'static) {
        self.listeners.push(Box::new(f));
    }

    pub fn notify_listeners(&mut self, span: Span8, new_text: &str) {
        for listener in &mut self.listeners {
            listener(span.clone(), new_text, &self.text_rope, self.version);
        }
    }

    pub fn set_lex_rope(&self, for_version: usize, dirty_range: Span8) {
        if for_version != self.version {
            return;
        }

    }

    pub fn set_static_analysis_rope(&self, for_version: usize, dirty_range: Span8) {
        if for_version != self.version {
            return;
        }

    }

    pub fn set_diagnostic_state(&self) {

    }
}
