use std::path::{Path, PathBuf};

use gpui::{
    App, AppContext, Bounds, Context, Entity, Pixels, Size, WeakEntity, Window, WindowBounds,
    point, px, size,
};
use serde::{Deserialize, Serialize};
use structs::assets::Assets;
use ui_cli_shared::doc_type::DocumentType;

use crate::document_view::{DocumentView, OpenDocument};

pub const CHECK_FOR_WRONGLY_IMPORTED_EXTENSION: bool = false;
const DEFAULT_SCENE_FILES: &[&str] = &[
    "(Tutorial) Monocurl Overview.mcs",
    "(Tutorial) Language Basics.mcs",
    "(Tutorial) Meshes.mcs",
    "(Tutorial) Animations.mcs",
    "(Example) Fractal.mcs",
    "(Example) Flow Field.mcs",
    "(Example) Riemann Sum.mcs",
    "(Example) Geometry Proof.mcs",
    "(Example) Text.mcs",
    "(Example) Algorithm.mcs",
    "(Example) 3D Camera Animation.mcs",
    "(Example) Image.mcs",
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
    pub window_bounds: Option<Bounds<Pixels>>,
}

#[derive(Clone, Debug)]
pub struct WindowState {
    pub screen: ActiveScreen,
    pub recently_opened: Vec<RecentlyOpened>,
    pub open_documents: Vec<OpenDocument>,
    window_bounds: Option<Bounds<Pixels>>,
}

impl WindowState {
    pub fn initial_window_bounds(
        min_size: Size<Pixels>,
        default_size: Size<Pixels>,
        cx: &App,
    ) -> Bounds<Pixels> {
        Self::load_saved_window_bounds()
            .and_then(|bounds| Self::validate_window_bounds(bounds, min_size, cx))
            .unwrap_or_else(|| Bounds::centered(None, default_size, cx))
    }

    fn load_saved_window_bounds() -> Option<Bounds<Pixels>> {
        let path = Self::save_file();
        let data = std::fs::read_to_string(path).ok()?;
        let state: WindowStateSerde = serde_json::from_str(&data).ok()?;
        state.window_bounds
    }

    fn validate_window_bounds(
        bounds: Bounds<Pixels>,
        min_size: Size<Pixels>,
        cx: &App,
    ) -> Option<Bounds<Pixels>> {
        let bounds_x = f32::from(bounds.origin.x);
        let bounds_y = f32::from(bounds.origin.y);
        let bounds_w = f32::from(bounds.size.width);
        let bounds_h = f32::from(bounds.size.height);
        if !bounds_x.is_finite()
            || !bounds_y.is_finite()
            || !bounds_w.is_finite()
            || !bounds_h.is_finite()
            || bounds_w <= 0.0
            || bounds_h <= 0.0
        {
            return None;
        }

        let displays = cx.displays();
        let saved_display_bounds = displays
            .iter()
            .map(|display| display.visible_bounds())
            .find(|display_bounds| Self::bounds_intersect(bounds, *display_bounds));
        let should_center = saved_display_bounds.is_none();
        let display_bounds = saved_display_bounds
            .or_else(|| cx.primary_display().map(|display| display.visible_bounds()))?;

        Self::fit_window_bounds(bounds, min_size, display_bounds, should_center)
    }

    fn fit_window_bounds(
        bounds: Bounds<Pixels>,
        min_size: Size<Pixels>,
        display_bounds: Bounds<Pixels>,
        center: bool,
    ) -> Option<Bounds<Pixels>> {
        let bounds_x = f32::from(bounds.origin.x);
        let bounds_y = f32::from(bounds.origin.y);
        let bounds_w = f32::from(bounds.size.width);
        let bounds_h = f32::from(bounds.size.height);
        let display_x = f32::from(display_bounds.origin.x);
        let display_y = f32::from(display_bounds.origin.y);
        let display_w = f32::from(display_bounds.size.width);
        let display_h = f32::from(display_bounds.size.height);
        if !display_x.is_finite()
            || !display_y.is_finite()
            || !display_w.is_finite()
            || !display_h.is_finite()
            || display_w <= 0.0
            || display_h <= 0.0
        {
            return None;
        }

        let min_w = f32::from(min_size.width).min(display_w);
        let min_h = f32::from(min_size.height).min(display_h);
        let width = bounds_w.clamp(min_w, display_w);
        let height = bounds_h.clamp(min_h, display_h);
        let (x, y) = if center {
            (
                display_x + (display_w - width) * 0.5,
                display_y + (display_h - height) * 0.5,
            )
        } else {
            (
                bounds_x.clamp(display_x, (display_x + display_w - width).max(display_x)),
                bounds_y.clamp(display_y, (display_y + display_h - height).max(display_y)),
            )
        };

        Some(Bounds::new(
            point(px(x), px(y)),
            size(px(width), px(height)),
        ))
    }

    fn bounds_intersect(a: Bounds<Pixels>, b: Bounds<Pixels>) -> bool {
        let a_left = f32::from(a.origin.x);
        let a_top = f32::from(a.origin.y);
        let a_right = a_left + f32::from(a.size.width);
        let a_bottom = a_top + f32::from(a.size.height);
        let b_left = f32::from(b.origin.x);
        let b_top = f32::from(b.origin.y);
        let b_right = b_left + f32::from(b.size.width);
        let b_bottom = b_top + f32::from(b.size.height);

        a_left < b_right && a_right > b_left && a_top < b_bottom && a_bottom > b_top
    }

    fn current_window_bounds(window: &Window) -> Bounds<Pixels> {
        match window.window_bounds() {
            WindowBounds::Fullscreen(bounds) => bounds,
            WindowBounds::Windowed(_) | WindowBounds::Maximized(_) => window.bounds(),
        }
    }

    pub fn save_window_bounds(&mut self, window: &Window) {
        if window.is_fullscreen() {
            return;
        }

        let bounds = Self::current_window_bounds(window);
        if self.window_bounds == Some(bounds) {
            return;
        }

        self.window_bounds = Some(bounds);
        self.save();
    }

    fn focus_active_document(&self, window: &mut Window, cx: &mut App) {
        let ActiveScreen::Document(document) = &self.screen else {
            return;
        };

        document.view.update(cx, |view, _| {
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

    fn default_state(window: &mut Window, _cx: &mut Context<Self>) -> Self {
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
            window_bounds: Some(Self::current_window_bounds(window)),
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
            window_bounds: state.window_bounds,
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
            window_bounds: self.window_bounds,
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

    fn validate_import_path(path: &Path) -> Result<(), String> {
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

        Ok(())
    }

    pub fn import(&mut self, path: PathBuf) -> Result<(), String> {
        self.import_many(vec![path])
    }

    pub fn import_many(&mut self, paths: Vec<PathBuf>) -> Result<(), String> {
        let mut unique_paths = Vec::with_capacity(paths.len());
        for path in paths {
            if !unique_paths.contains(&path) {
                unique_paths.push(path);
            }
        }
        let mut paths = unique_paths;

        if paths.is_empty() {
            return Ok(());
        }

        for path in &paths {
            Self::validate_import_path(path)?;
        }

        self.recently_opened
            .retain(|recent| !paths.contains(&recent.path));

        for path in paths.drain(..).rev() {
            self.recently_opened.insert(0, RecentlyOpened { path });
        }

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
        document_view.update(cx, |view, cx| {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    #[test]
    fn offscreen_window_bounds_are_centered_with_saved_size() {
        let restored = WindowState::fit_window_bounds(
            test_bounds(4000.0, 3000.0, 900.0, 700.0),
            size(px(520.0), px(420.0)),
            test_bounds(0.0, 0.0, 1920.0, 1080.0),
            true,
        )
        .unwrap();

        assert_eq!(restored, test_bounds(510.0, 190.0, 900.0, 700.0));
    }

    #[test]
    fn onscreen_window_bounds_keep_origin_when_possible() {
        let restored = WindowState::fit_window_bounds(
            test_bounds(1800.0, 900.0, 900.0, 700.0),
            size(px(520.0), px(420.0)),
            test_bounds(0.0, 0.0, 1920.0, 1080.0),
            false,
        )
        .unwrap();

        assert_eq!(restored, test_bounds(1020.0, 380.0, 900.0, 700.0));
    }
}
