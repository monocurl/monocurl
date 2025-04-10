use std::path::PathBuf;
use quarve::state::StoreContainerSource;
use quarve::view::text::{AttributeSet, TextViewState};
use quarve_derive::StoreContainer;

// note that a reference to editor is passed to
// each ivp
#[derive(StoreContainer)]
pub struct Editor {
    #[quarve(ignore)]
    location: PathBuf,

    // media set by ui
    // read by compiler (and cached)
    media: Store

    // slides should only be set by ui
    // read by lexer (and partially for autocompletion)
    slides: StoreContainerSource<TextViewState<>>,

    // current autocompletion list
    autocompletion: Store<>,

    // current meshes (plus camera and background)
    // set by executor
    // read by renderering thread
    viewport: Store<>,

    // set by executor
    // also sometimes set by the ui
    // for certain cases
    // read by ui
    parameter_variables: Store<>,

    // timeline info
    // set by executor
    // read by ui
    timeline_slides: Store<>,

    // playing, paused, exporting, etc
    // set by executor when its finished, or by ui generally
    // ready by executor to determine when to seek
    presentation_state: Store<>,

    // current timestamp
    // set by multiple
    // read by multiple
    timestamp: Store<>,
}

impl Editor {

}

//

struct TextAttributes;

impl AttributeSet for TextAttributes {
    type CharAttribute = ();
    type RunAttribute = ();
    type PageAttribute = ();
}

#[derive(StoreContainer)]
struct EditorState {

}
