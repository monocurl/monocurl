use std::path::PathBuf;

use gpui::{App, AppContext, Context, Entity, WeakEntity, Window};
use serde::{Deserialize, Serialize};
use structs::assets::Assets;
use ui_cli_shared::doc_type::DocumentType;

use crate::document_view::{DocumentView, OpenDocument};

pub const CHECK_FOR_WRONGLY_IMPORTED_EXTENSION: bool = false;
const DEFAULT_SCENE_FILES: &[&str] = &[
    "welcome_to_monocurl.mcs",
    "language_basics.mcs",
    "example_camera_animations.mcs",
    "example_geometry_proof.mcs",
    "example_text_and_equations.mcs",
    "meshes_and_operators.mcs",
    "animations.mcs",
    "example_3d_surface.mcs",
    "example_graphing_riemann_sums.mcs",
    "example_algorithms_binary_search.mcs",
    "example_image_mandala.mcs",
    "parameters.mcs",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
enum ActiveScreenSerde {
    Home,
    Document(PathBuf),
}

#[derive(Clone, Debug)]
pub enum ActiveScreen {
    Home,
    Document(OpenDocument),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenDocumentSerde {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentlyOpened {
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WindowStateSerde {
    pub screen: ActiveScreenSerde,
    pub recently_opened: Vec<RecentlyOpened>,
    pub open_documents: Vec<OpenDocumentSerde>,
}

#[derive(Clone, Debug)]
pub struct WindowState {
    pub screen: ActiveScreen,
    pub recently_opened: Vec<RecentlyOpened>,
    pub open_documents: Vec<OpenDocument>,
}

impl WindowState {
    fn focus_active_document(&self, window: &mut Window, cx: &mut App) {
        let ActiveScreen::Document(document) = &self.screen else {
            return;
        };

        let _ = document.view.update(cx, |view, _| {
            view.focus(window);
        });
    }

    fn save_file() -> PathBuf {
        let mut path = dirs::data_local_dir().expect("Could not find local data directory");
        path.push("Monocurl");
        if !path.exists() {
            std::fs::create_dir_all(&path).expect("Could not create settings directory");
        }
        path.push("window_state.json");
        path
    }

    fn make_open_document(
        path: PathBuf,
        weak_state: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> OpenDocument {
        let view_path = path.clone();
        let dirty = cx.new(|_cx| false);
        OpenDocument {
            path,
            view: cx.new(|cx| DocumentView::new(view_path, weak_state, dirty.clone(), window, cx)),
        }
    }

    fn default_scene_paths() -> Vec<PathBuf> {
        DEFAULT_SCENE_FILES
            .iter()
            .filter_map(|file| {
                let path = Assets::default_scene(file);
                if path.exists() {
                    Some(path)
                } else {
                    log::warn!("Default scene does not exist: {}", path.display());
                    None
                }
            })
            .collect()
    }

    fn default_state(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        let default_paths = Self::default_scene_paths();

        let screen = ActiveScreen::Home;
        let recently_opened = default_paths
            .iter()
            .map(|doc| RecentlyOpened { path: doc.clone() })
            .collect();

        Self {
            screen,
            recently_opened,
            open_documents: Vec::new(),
        }
    }

    fn load_saved_state(window: &mut Window, cx: &mut Context<Self>) -> Option<Self> {
        let path = Self::save_file();
        if !path.exists() {
            return None;
        }

        let data = std::fs::read_to_string(&path).ok()?;
        let state: WindowStateSerde = serde_json::from_str(&data).ok()?;
        let saved_open_document_count = state.open_documents.len();

        let weak_state = cx.weak_entity();
        let open_documents: Vec<_> = state
            .open_documents
            .into_iter()
            .filter_map(|serde| {
                if serde.path.exists() {
                    Some(Self::make_open_document(
                        serde.path,
                        weak_state.clone(),
                        window,
                        cx,
                    ))
                } else {
                    log::warn!(
                        "Saved open document does not exist: {}",
                        serde.path.display()
                    );
                    None
                }
            })
            .collect();

        if saved_open_document_count > 0 && open_documents.is_empty() {
            return None;
        }

        let screen = match state.screen {
            ActiveScreenSerde::Home => ActiveScreen::Home,
            ActiveScreenSerde::Document(path) => open_documents
                .iter()
                .find(|doc| doc.path == path)
                .map(|doc| ActiveScreen::Document(doc.clone()))
                .unwrap_or(ActiveScreen::Home),
        };

        Some(WindowState {
            screen,
            recently_opened: state
                .recently_opened
                .into_iter()
                .filter(|recent| recent.path.exists())
                .collect(),
            open_documents,
        })
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        if let Some(saved) = Self::load_saved_state(window, cx) {
            log::info!("Successfuly loaded window state from previous run");
            saved
        } else {
            log::info!("Creating new window state");
            let ret = Self::default_state(window, cx);
            ret.save();
            ret
        }
    }

    pub fn open_documents(&self) -> impl Iterator<Item = &OpenDocument> {
        self.open_documents.iter()
    }

    pub fn save(&self) {
        let serde = WindowStateSerde {
            screen: match &self.screen {
                ActiveScreen::Home => ActiveScreenSerde::Home,
                ActiveScreen::Document(doc) => ActiveScreenSerde::Document(doc.path.clone()),
            },
            recently_opened: self.recently_opened.clone(),
            open_documents: self
                .open_documents
                .iter()
                .map(|doc| OpenDocumentSerde {
                    path: doc.path.clone(),
                })
                .collect(),
        };

        let data = serde_json::to_string_pretty(&serde).expect("Could not serialize window state");
        let path = Self::save_file();
        std::fs::write(path, data)
            .ok()
            .unwrap_or_else(|| log::warn!("Unable to save window state"));
    }

    pub fn create_new_document(
        &mut self,
        dtype: DocumentType,
        path: PathBuf,
    ) -> Result<(), String> {
        if let Some(parent) = path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            return Err(format!(
                "Could not create directory {}: {}",
                parent.display(),
                err
            ));
        }

        std::fs::write(&path, dtype.default_file())
            .map_err(|err| format!("Could not create {}: {}", path.display(), err))?;
        self.import(path)
    }

    pub fn import(&mut self, path: PathBuf) -> Result<(), String> {
        if CHECK_FOR_WRONGLY_IMPORTED_EXTENSION {
            match path
                .extension()
                .map(|ext| ext.to_string_lossy().to_lowercase())
            {
                Some(ext) if ext == DocumentType::Library.extension() => Ok(()),
                Some(ext) if ext == DocumentType::Scene.extension() => Ok(()),
                _ => {
                    log::error!("Unsupported file type: {:?}", path.extension());
                    Err(format!("Unsupported file type: {:?}", path.extension()))
                }
            }?;
        }

        if !path.exists() {
            return Err(format!("File does not exist: {}", path.display()));
        }

        self.recently_opened.retain(|recent| recent.path != path);
        self.recently_opened.insert(0, RecentlyOpened { path });
        self.save();
        Ok(())
    }

    fn close_project(&mut self, path: &PathBuf) {
        self.open_documents.retain(|doc| &doc.path != path);
        if let ActiveScreen::Document(current_doc) = &self.screen
            && &current_doc.path == path
        {
            self.screen = self
                .open_documents
                .first()
                .map(|doc| ActiveScreen::Document(doc.clone()))
                .unwrap_or(ActiveScreen::Home);
        }

        self.save();
    }

    pub fn close_tab(&mut self, path: &PathBuf, cx: &mut Context<Self>, window: &mut gpui::Window) {
        let Some(document) = self.open_documents.iter().find(|doc| &doc.path == path) else {
            log::warn!("Tried to close tab for non-open document: {:?}", path);
            return;
        };

        let path = path.clone();
        let document_view = document.view.clone();
        let _ = document_view.update(cx, |view, cx| {
            view.save_before_close(cx);
        });
        self.close_project(&path);
        cx.notify();
        window.refresh();
    }

    pub fn set_document_path(&mut self, old_path: &PathBuf, new_path: PathBuf) {
        for doc in self.open_documents.iter_mut() {
            if &doc.path == old_path {
                doc.path = new_path.clone();
            }
        }

        self.recently_opened
            .retain(|recent| recent.path != *old_path && recent.path != new_path);
        self.recently_opened.insert(
            0,
            RecentlyOpened {
                path: new_path.clone(),
            },
        );

        if let ActiveScreen::Document(current) = &self.screen
            && &current.path == old_path
            && let Some(doc) = self.open_documents.iter().find(|doc| doc.path == new_path)
        {
            self.screen = ActiveScreen::Document(doc.clone());
        }

        self.save();
    }

    pub fn forget_project(&mut self, path: &PathBuf) {
        self.recently_opened.retain(|doc| &doc.path != path);
        self.close_project(path);
        self.save();
    }

    pub fn navigate_to_home(&mut self) {
        self.screen = ActiveScreen::Home;
        self.save();
    }

    pub fn navigate_to(
        &mut self,
        path: PathBuf,
        window_state: Entity<WindowState>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.recently_opened.retain(|recent| recent.path != path);
        self.recently_opened
            .insert(0, RecentlyOpened { path: path.clone() });

        if !self.open_documents.iter().any(|doc| doc.path == path) {
            let dirty = cx.new(|_cx| false);
            self.open_documents.push(OpenDocument {
                path: path.clone(),
                view: cx.new(|cx| {
                    DocumentView::new(
                        path.clone(),
                        window_state.downgrade(),
                        dirty.clone(),
                        window,
                        cx,
                    )
                }),
            });
        }

        self.screen = ActiveScreen::Document(
            self.open_documents
                .iter()
                .find(|doc| doc.path == path)
                .unwrap()
                .clone(),
        );

        self.focus_active_document(window, cx);
        self.save();
    }
}
