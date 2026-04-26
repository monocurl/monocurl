use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use exporter::EXPORT_CANCELLED_MESSAGE;
use gpui::*;
use structs::rope::{Attribute, Rope, TextAggregate};

use crate::{
    actions::{
        CloseActiveDocument, EpsilonBackward, EpsilonForward, ExportImage, ExportVideo, NextSlide,
        PlayOrShowPauseHint, PrevSlide, Redo, SaveActiveDocument, SaveActiveDocumentCustomPath,
        SceneEnd, SceneStart, SyncViewportCamera, ToggleParamsPanel, TogglePlaying,
        TogglePresentationMode, Undo, UnfocusEditor, ZoomIn, ZoomOut,
    },
    components::split_pane::Split,
    editor::editor_view::Editor,
    navbar_view::Navbar,
    services::{PlaybackMode, ServiceManager},
    state::{
        document_state::DocumentState,
        textual_state::LexData,
        window_state::{ActiveScreen, WindowState},
    },
    theme::ThemeSettings,
    timeline::timeline_view::Timeline,
    viewport::viewport_view::Viewport,
};

mod actions;
mod export;
mod render;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-s", SaveActiveDocument, None),
        KeyBinding::new("secondary-shift-s", SaveActiveDocumentCustomPath, None),
        KeyBinding::new("secondary-w", CloseActiveDocument, None),
        KeyBinding::new("secondary-z", Undo, None),
        KeyBinding::new("secondary-shift-z", Redo, None),
        KeyBinding::new("secondary-p", TogglePresentationMode, None),
        KeyBinding::new("secondary-t", ToggleParamsPanel, Some("presenter")),
        KeyBinding::new("secondary-l", SyncViewportCamera, None),
        KeyBinding::new("escape", TogglePresentationMode, Some("presenter")),
        KeyBinding::new("escape", UnfocusEditor, Some("!presenter")),
        KeyBinding::new("left", PrevSlide, None),
        KeyBinding::new("right", NextSlide, None),
        KeyBinding::new("space", PlayOrShowPauseHint, Some("!editor")),
        KeyBinding::new("shift-space", TogglePlaying, Some("!editor")),
        KeyBinding::new("secondary-shift-space,", TogglePlaying, None),
        KeyBinding::new(",", PrevSlide, Some("!editor")),
        KeyBinding::new("secondary-,", PrevSlide, None),
        KeyBinding::new(".", NextSlide, Some("!editor")),
        KeyBinding::new("secondary-.", NextSlide, None),
        KeyBinding::new("<", SceneStart, Some("!editor")),
        KeyBinding::new("secondary-<", SceneStart, None),
        KeyBinding::new(">", SceneEnd, Some("!editor")),
        KeyBinding::new("secondary->", SceneEnd, None),
        KeyBinding::new(";", EpsilonBackward, Some("!editor")),
        KeyBinding::new("secondary-;", EpsilonBackward, None),
        KeyBinding::new("'", EpsilonForward, Some("!editor")),
        KeyBinding::new("secondary-'", EpsilonForward, None),
        KeyBinding::new("secondary-=", ZoomIn, None),
        KeyBinding::new("secondary--", ZoomOut, None),
    ]);
}

#[derive(Clone, Debug)]
pub struct OpenDocument {
    pub internal_path: PathBuf,
    pub user_path: Option<PathBuf>,
    pub view: Entity<DocumentView>,
    pub dirty: Entity<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestedExport {
    Image,
    Video,
}

impl RequestedExport {
    fn action_label(self) -> &'static str {
        match self {
            Self::Image => "Image",
            Self::Video => "Video",
        }
    }

    fn progress_title(self) -> &'static str {
        match self {
            Self::Image => "Exporting Image",
            Self::Video => "Exporting Video",
        }
    }

    fn success_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Complete",
            Self::Video => "Video Export Complete",
        }
    }

    fn failure_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Failed",
            Self::Video => "Video Export Failed",
        }
    }

    fn canceled_title(self) -> &'static str {
        match self {
            Self::Image => "Image Export Canceled",
            Self::Video => "Video Export Canceled",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Image => "png",
            Self::Video => "mp4",
        }
    }

    fn open_label(self) -> &'static str {
        match self {
            Self::Image => "Open Image",
            Self::Video => "Open Video",
        }
    }
}

#[derive(Clone, Debug, Default)]
struct ExportOverlayState {
    kind: Option<RequestedExport>,
    running: bool,
    cancel_requested: bool,
    message: String,
    completed: usize,
    total: usize,
    error: Option<String>,
    output_path: Option<PathBuf>,
}

impl ExportOverlayState {
    fn start(kind: RequestedExport) -> Self {
        Self {
            kind: Some(kind),
            running: true,
            cancel_requested: false,
            message: format!("Starting {} export", kind.action_label().to_lowercase()),
            completed: 0,
            total: 0,
            error: None,
            output_path: None,
        }
    }

    fn visible(&self) -> bool {
        self.kind.is_some()
    }

    fn progress_ratio(&self) -> f32 {
        if self.total == 0 {
            0.35
        } else {
            (self.completed as f32 / self.total as f32).clamp(0.0, 1.0)
        }
    }

    fn succeeded(&self) -> bool {
        !self.running && self.error.is_none() && self.output_path.is_some()
    }

    fn cancelled(&self) -> bool {
        !self.running && self.error.as_deref() == Some(EXPORT_CANCELLED_MESSAGE)
    }
}

pub struct DocumentView {
    internal_path: PathBuf,
    user_path: Option<PathBuf>,

    was_fullscreen_before_presenting: bool,
    is_presenting: bool,

    dirty: Entity<bool>,
    state: DocumentState,
    services: Entity<ServiceManager>,
    window_state: WeakEntity<WindowState>,

    navbar: Entity<Navbar>,
    editor: Entity<Editor>,
    viewport: Entity<Viewport>,
    timeline: Entity<Timeline>,

    export_overlay: ExportOverlayState,
    export_cancel_flag: Option<Arc<AtomicBool>>,
    export_poll_task: Option<Task<()>>,

    focus_handle: FocusHandle,
}

fn dirty_file(internal: &PathBuf, user: &Option<PathBuf>) -> bool {
    let Some(user) = user else {
        return true;
    };

    let content_ip = std::fs::read_to_string(internal);
    let content_up = std::fs::read_to_string(user);

    match (content_ip, content_up) {
        (Ok(ci), Ok(cu)) => ci != cu,
        _ => true,
    }
}
