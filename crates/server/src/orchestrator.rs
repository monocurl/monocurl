// use crate::state::EditorState;

pub trait OrchestratorService {}

pub struct Orchestrator {}

impl Orchestrator {
    pub fn new_project() {}

    pub fn load_project() {}

    // transfer information such as camera
    pub fn executor_set_state() {}

    pub fn alloc() {}

    pub fn buffer_updated() {}
}

// maybe have this be a trait and depending on rendering / live we do different things?
pub struct Editor {
    // state: EditorState,
}

impl Editor {
    pub fn new() {}

    pub fn update_buffer() {}

    pub fn set_autocomplete() {}

    pub fn set_error() {}

    pub fn set_parameters() {}

    pub fn set_viewport_state() {}

    pub fn timeline_seek() {}
}
