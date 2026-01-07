use std::path::{PathBuf};

use gpui::{App, AppContext, Context, Entity, PromptButton, PromptLevel, ScrollHandle, Window};
use serde::{Deserialize, Serialize};
use server::doc_type::DocumentType;

use crate::document_view::{DocumentView, OpenDocument};

pub const CHECK_FOR_WRONGLY_IMPORTED_EXTENSION: bool = false;

#[derive(Clone, Debug, Serialize, Deserialize)]
enum ActiveScreenSerde {
    Home,
    // virtual path buf
    Document(PathBuf),
}

#[derive(Clone, Debug)]
pub enum ActiveScreen {
    Home,
    Document(OpenDocument),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenDocumentSerde {
    pub internal_path: PathBuf,
    pub user_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentlyOpened {
    pub internal_path: PathBuf,
    pub user_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WindowStateSerde {
    pub screen: ActiveScreenSerde,
    pub recently_opened: Vec<RecentlyOpened>,
    pub open_documents: Vec<OpenDocumentSerde>
}

#[derive(Clone, Debug)]
pub struct WindowState {
    pub screen: ActiveScreen,
    pub recently_opened: Vec<RecentlyOpened>,
    pub open_documents: Vec<OpenDocument>,

    // a bit hacky to put here, but basically necessary since each view has its own
    // navbar (which itself is necessary to allow for presentation mode)
    pub navbar_scroll: ScrollHandle
}

impl WindowState {

    fn save_file() -> PathBuf {
        let mut path = dirs::data_local_dir().expect("Could not find local data directory");
        path.push("Monocurl");
        if !path.exists() {
            std::fs::create_dir_all(&path).expect("Could not create settings directory");
        }
        path.push("window_state.json");
        path
    }

    fn allocate_internal_file(extension: &str) -> PathBuf {
        let mut path = dirs::data_local_dir().expect("Could not find local data directory");
        path.push("Monocurl");
        path.push("save_files");
        if !path.exists() {
            std::fs::create_dir_all(&path).expect("Could not create internal storage directory");
        }
        let random_id = uuid::Uuid::new_v4();
        path.push(random_id.to_string() + "." + extension);
        path
    }

    fn load_saved_state(window: &mut Window, cx: &mut Context<Self>) -> Option<Self> {
        let path = Self::save_file();
        if path.exists() {
            let data = std::fs::read_to_string(&path).ok()?;
            let state: WindowStateSerde = serde_json::from_str(&data).ok()?;

            let weak_state = cx.weak_entity();
            let open_documents: Vec<_> = state.open_documents.into_iter().map(|serde| {
                let dirty = cx.new(|_cx| false);
                OpenDocument {
                    internal_path: serde.internal_path.clone(),
                    user_path: serde.user_path.clone(),
                    view: cx.new(|cx| DocumentView::new(serde.internal_path, serde.user_path, weak_state.clone(), dirty.clone(), window, cx)),
                    dirty
                }
            }).collect();

            let screen = match state.screen {
                ActiveScreenSerde::Home => ActiveScreen::Home,
                ActiveScreenSerde::Document(path) => {
                    if let Some(doc) = open_documents.iter().find(|doc| doc.internal_path == path) {
                        ActiveScreen::Document(doc.clone())
                    }
                    else {
                        ActiveScreen::Home
                    }
                }
            };

            let recently_opened = state.recently_opened;

            Some(WindowState {
                screen,
                recently_opened,
                open_documents,
                navbar_scroll: ScrollHandle::new(),
            })
        }
        else {
            None
        }
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        if let Some(saved) = Self::load_saved_state(window, cx) {
            log::info!("Successfuly loaded window state from previous run");
            return saved;
        }
        else {
            log::info!("Creating new window state");

            // default files
            let mut recent = vec![];
            for i in 0..30 {
                let internal = Self::allocate_internal_file(DocumentType::Scene.extension());
                let user = PathBuf::from(format!("/Users/manubhat/Recent Document {i}.txt"));

                // try copying to internal file
                if !internal.exists() {
                    let Ok(_) = std::fs::copy(&user, &internal) else {
                        log::warn!("Could not copy {user:?} to internal storage at {internal:?}");
                        continue;
                    };
                }

                recent.push(RecentlyOpened {
                    internal_path: internal,
                    user_path: Some(user),
                });
            }

            let ret = Self {
                screen: ActiveScreen::Home,
                recently_opened: recent,
                open_documents: vec![],
                navbar_scroll: ScrollHandle::new(),
            };
            ret.save();
            ret
        }
    }

    pub fn navbar_scroll(&self) -> &ScrollHandle {
        &self.navbar_scroll
    }

    pub fn open_documents(&self) -> impl Iterator<Item = &OpenDocument> {
        self.open_documents.iter()
    }

    pub fn save(&self) {
        let serde = WindowStateSerde {
            screen: match &self.screen {
                ActiveScreen::Home => ActiveScreenSerde::Home,
                ActiveScreen::Document(doc) => ActiveScreenSerde::Document(doc.internal_path.clone()),
            },
            recently_opened: self.recently_opened.clone(),
            open_documents: self.open_documents.iter().map(|doc| OpenDocumentSerde {
                internal_path: doc.internal_path.clone(),
                user_path: doc.user_path.clone()
            }).collect()
        };

        let data = serde_json::to_string_pretty(&serde).expect("Could not serialize window state");
        let path = Self::save_file();
        std::fs::write(path, data).ok()
            .unwrap_or_else(|| {
                log::warn!("Unable to save window state")
            });
    }

    pub fn create_new_document(&mut self, dtype: DocumentType) -> PathBuf {
        let internal = Self::allocate_internal_file(dtype.extension());

        self.recently_opened.insert(0, RecentlyOpened {
            internal_path: internal.clone(),
            user_path: None
        });
        let content = dtype.default_file();
        let _ = std::fs::write(&internal, content);

        self.save();

        internal
    }

    pub fn import(&mut self, user_path: PathBuf) -> Result<(), String> {
        if CHECK_FOR_WRONGLY_IMPORTED_EXTENSION {
            match user_path.extension().map(|ext| ext.to_string_lossy().to_lowercase()) {
                Some(ext) if ext == DocumentType::Library.extension() => {
                    Ok(())
                },
                Some(ext) if ext == DocumentType::Scene.extension() => {
                    Ok(())
                },
                _ => {
                    log::error!("Unsupported file type: {:?}", user_path.extension());
                    Err(format!("Unsupported file type: {:?}", user_path.extension()))
                }
            }?;
        }

        // simply reorder if already exists
        if let Some(index) = self.recently_opened.iter().position(|o| o.user_path.as_ref() == Some(&user_path)) {
            let old = self.recently_opened.remove(index);
            self.recently_opened.insert(0, old);
        }
        else {
            let internal = Self::allocate_internal_file(
                user_path.extension()
                    .map(|ext| ext.to_string_lossy())
                    .unwrap_or_default().as_ref()
            );
            // copy to internal
            let _ = std::fs::copy(&user_path, &internal)
                .inspect_err(|e| log::error!("Failed to copy file: {e}"));

            self.recently_opened.insert(0, RecentlyOpened {
                internal_path: internal,
                user_path: Some(user_path.clone())
            });
        }
        self.save();

        Ok(())
    }

    fn close_project(&mut self, internal_path: &PathBuf) {
        self.open_documents.retain(|p| &p.internal_path != internal_path);
        if let ActiveScreen::Document(current_doc) = &self.screen {
            if &current_doc.internal_path == internal_path {
                self.screen = self.open_documents.first()
                    .map(|doc| ActiveScreen::Document(doc.clone()))
                    .unwrap_or(ActiveScreen::Home);
            }
        }

        self.save();
    }

    pub fn close_tab(&mut self, internal_path: &PathBuf, cx: &mut Context<Self>, window: &mut gpui::Window) {
        let Some(document) = self.open_documents.iter()
            .find(|p| &p.internal_path == internal_path) else {
            log::warn!("Tried to close tab for non-open document: {:?}", internal_path);
            return;
        };

        // warn if not the same
        let diff = *document.dirty.read(cx);

        fn actually_close(this: &mut WindowState, user_path: &Option<PathBuf>, internal_path: &PathBuf) {
            if let Some(path) = &user_path {
                // if user path exists, copy it to internal path (resetting any progress)

                match std::fs::copy(path, internal_path) {
                    Ok(_) => { },
                    Err(err) => {
                        log::warn!("Could not copy user file to internal path: {}", err);
                    }
                }

                this.close_project(internal_path);
            }
            else {
                // otherwise, completely forget project (nothing to save)
                this.forget_project(internal_path);
            }
        }

        let user_path = document.user_path.clone();
        let internal_path = internal_path.clone();

        if diff {
            let confirm = window.prompt(
                PromptLevel::Warning,
                "Close Tab?",
                Some("You have unsaved changes!"),
                &[PromptButton::Cancel("Cancel".into()), PromptButton::Ok("Close Anyways".into())],
                cx
            );

            cx.spawn(async move |this, app| {
                let Some(this) = this.upgrade() else {
                    return;
                };

                if confirm.await == Ok(1) {
                    let _ = app.update(move |cx| {
                        let _ = this.update(cx, move |this, _cx| {
                            actually_close(this, &user_path, &internal_path);
                        });
                    });
                }
            }).detach();
        }
        else {
            actually_close(self, &user_path, &internal_path);
        }
    }

    pub fn set_user_path(&mut self, internal_path: &PathBuf, user_path: PathBuf) {
        for doc in self.open_documents.iter_mut() {
            if &doc.internal_path == internal_path {
                doc.user_path = Some(user_path.clone());
            }
        }

        for recent in self.recently_opened.iter_mut() {
            if &recent.internal_path == internal_path {
                recent.user_path = Some(user_path.clone());
            }
        }

        self.save();
    }

    pub fn forget_project(&mut self, internal_path: &PathBuf) {
        self.recently_opened.retain(|doc| &doc.internal_path != internal_path);
        self.close_project(internal_path);

        // we can safely delete the internal file
        if internal_path.exists() {
            std::fs::remove_file(internal_path)
                .expect("Internal file should exist")
        }

        self.save();
    }

    pub fn navigate_to_home(&mut self) {
        self.screen = ActiveScreen::Home;

        self.save();
    }

    pub fn navigate_to(&mut self, user_path: Option<PathBuf>, internal_path: PathBuf, window_state: Entity<WindowState>, window: &mut Window, cx: &mut App) {
        self.recently_opened.retain(|p| p.user_path != user_path);
        self.recently_opened.insert(0, RecentlyOpened {
            internal_path: internal_path.clone(),
            user_path: user_path.clone()
        });

        if !self.open_documents.iter().any(|doc| doc.internal_path == internal_path) {
            self.open_documents.push({
                let dirty = cx.new(|_cx| { false });
                OpenDocument {
                    internal_path: internal_path.clone(),
                    user_path: user_path.clone(),
                    view: cx.new(|cx| DocumentView::new( internal_path.clone(), user_path.clone(), window_state.downgrade(), dirty.clone(), window, cx)),
                    dirty: dirty
                }
            });
        }

        self.screen = ActiveScreen::Document(
            self.open_documents
                .iter()
                .find(|doc| doc.internal_path == internal_path)
                .unwrap()
                .clone(),
        );

        self.save();
    }
}
