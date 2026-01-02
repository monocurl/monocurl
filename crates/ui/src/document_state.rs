use structs::{rope::{Rope, TextAggregate}, text::Span8};


#[derive(Default)]
pub struct DocumentState {
    pub text_rope: Rope<TextAggregate>,
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
}
