use std::path::PathBuf;

use gpui::{App, AppContext, Context, Entity};
use serde::{Deserialize, Serialize};
use server::doc_type::DocumentType;

use crate::document::{DocumentView, OpenDocument};

pub const CHECK_FOR_WRONGLY_IMPORTED_EXTENSION: bool = false;

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
struct WindowStateSerde {
    pub screen: ActiveScreenSerde,
    pub recently_opened: Vec<PathBuf>,
    pub open_documents: Vec<PathBuf>
}

#[derive(Clone, Debug)]
pub struct WindowState {
    pub screen: ActiveScreen,
    pub recently_opened: Vec<PathBuf>,
    pub open_documents: Vec<OpenDocument>
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

    fn load_saved_state(cx: &mut Context<Self>) -> Option<Self> {
        let path = Self::save_file();
        if path.exists() {
            let data = std::fs::read_to_string(&path).ok()?;
            let state: WindowStateSerde = serde_json::from_str(&data).ok()?;

            let weak_state = cx.weak_entity();
            let open_documents: Vec<_> = state.open_documents.into_iter().map(|path| OpenDocument {
                path: path.clone(),
                view: cx.new(|cx| DocumentView::new(path, weak_state.clone(), cx)),
            }).collect();

            let screen = match state.screen {
                ActiveScreenSerde::Home => ActiveScreen::Home,
                ActiveScreenSerde::Document(path) => {
                    if let Some(doc) = open_documents.iter().find(|doc| doc.path == path) {
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
            })
        }
        else {
            None
        }
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
        if let Some(saved) = Self::load_saved_state(cx) {
            log::info!("Successfuly loaded window state from previous run");
            return saved;
        }
        else {
            log::info!("Creating new window state");

            // default files
            let mut recent = vec![];
            for i in 0..30 {
                recent.push(PathBuf::from(format!("/Users/manubhat/Recent Document {i}.txt")));
            }

            let ret = Self {
                screen: ActiveScreen::Home,
                recently_opened: recent,
                open_documents: vec![]
            };
            ret.save();
            ret
        }
    }

    pub fn save(&self) {
        let serde = WindowStateSerde {
            screen: match &self.screen {
                ActiveScreen::Home => ActiveScreenSerde::Home,
                ActiveScreen::Document(doc) => ActiveScreenSerde::Document(doc.path.clone()),
            },
            recently_opened: self.recently_opened.clone(),
            open_documents: self.open_documents.iter().map(|doc| doc.path.clone()).collect()
        };

        let data = serde_json::to_string_pretty(&serde).expect("Could not serialize window state");
        let path = Self::save_file();
        std::fs::write(path, data).ok()
            .unwrap_or_else(|| {
                log::warn!("Unable to save window state")
            });
    }

    pub fn remove(&mut self, path: PathBuf) {
        self.recently_opened.retain(|p| p != &path);
        self.open_documents.retain(|doc| doc.path != path);
        if let ActiveScreen::Document(current_doc) = &self.screen {
            if current_doc.path == path {
                self.screen = self.open_documents.first()
                    .map(|doc| ActiveScreen::Document(doc.clone()))
                    .unwrap_or(ActiveScreen::Home);
            }
        }

        self.save();
    }

    pub fn add(&mut self, path: PathBuf) -> Result<(), String> {
        if CHECK_FOR_WRONGLY_IMPORTED_EXTENSION {
            match path.extension().map(|ext| ext.to_string_lossy().to_lowercase()) {
                Some(ext) if ext == DocumentType::Library.extension() => {
                    Ok(())
                },
                Some(ext) if ext == DocumentType::Scene.extension() => {
                    Ok(())
                },
                _ => {
                    log::error!("Unsupported file type: {:?}", path.extension());
                    Err(format!("Unsupported file type: {:?}", path.extension()))
                }
            }?;
        }

        self.recently_opened.retain(|p| p != &path);
        self.recently_opened.insert(0, path.clone());
        self.save();

        Ok(())
    }

    pub fn navigate_to_home(&mut self) {
        self.screen = ActiveScreen::Home;

        self.save();
    }

    pub fn navigate_to(&mut self, path: PathBuf, window_state: Entity<WindowState>, cx: &mut App) {
        // move the selected option to the front of recent documents
        self.recently_opened.retain(|p| p != &path);
        self.recently_opened.insert(0, path.clone());

        if !self.open_documents.iter().any(|doc| doc.path == path) {
            self.open_documents.push(OpenDocument { path: path.clone(), view: cx.new(|cx| DocumentView::new(path.clone(), window_state.downgrade(), cx)) });
        }

        self.screen = ActiveScreen::Document(
            self.open_documents
                .iter()
                .find(|doc| doc.path == path)
                .unwrap()
                .clone(),
        );

        self.save();
    }
}

pub enum TimestampState {
    Paused,
    Play,
}

// pub struct Document {

//     // each one of these components acts as a state machine, and uses
//     // channels to communicate information to other components
//     lexer: Lexer,
//     parser: Parser,
//     compiler: Compiler,
//     autocompletor: AutoCompletor,
//     executor: Executor,
// }

// #[derive(Clone, Debug)]
// pub struct DocumentState {
//     text_editor_state

// }
