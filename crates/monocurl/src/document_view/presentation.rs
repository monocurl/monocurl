use gpui::*;

use crate::actions::{
    EpsilonBackward, EpsilonForward, ExportImage, ExportSlidesAsVideos, ExportVideo, NextSlide,
    PlayOrShowPauseHint, PrevSlide, SceneEnd, SceneStart, SyncViewportCamera, ToggleParamsPanel,
    TogglePlaying, TogglePresentationMode,
};
use crate::theme::FontSet;
use crate::viewport::viewport_view::Viewport;

pub(super) struct ControlsWindow {
    document: WeakEntity<super::DocumentView>,
    viewport: Entity<Viewport>,
    focus_handle: FocusHandle,
}

impl ControlsWindow {
    pub(super) fn new(
        document: WeakEntity<super::DocumentView>,
        viewport: Entity<Viewport>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&viewport, |_this, _, cx| cx.notify()).detach();
        Self {
            document,
            viewport,
            focus_handle: cx.focus_handle(),
        }
    }
}

impl Render for ControlsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if window.focused(cx).is_none() {
            window.focus(&self.focus_handle);
        }

        div()
            .size_full()
            .font_family(FontSet::UI)
            .key_context("document presenter")
            .track_focus(&self.focus_handle)
            .on_action(
                cx.listener(|this, action: &TogglePresentationMode, window, cx| {
                    this.document
                        .update(cx, |document, cx| {
                            document.toggle_presentation(action, window, cx);
                        })
                        .ok();
                }),
            )
            .on_action(cx.listener(|this, action: &ToggleParamsPanel, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.toggle_params_panel(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(
                cx.listener(|this, action: &SyncViewportCamera, window, cx| {
                    this.document
                        .update(cx, |document, cx| {
                            document.sync_viewport_camera(action, window, cx);
                        })
                        .ok();
                }),
            )
            .on_action(
                cx.listener(|this, action: &PlayOrShowPauseHint, window, cx| {
                    this.document
                        .update(cx, |document, cx| {
                            document.play_or_show_pause_hint(action, window, cx);
                        })
                        .ok();
                }),
            )
            .on_action(cx.listener(|this, action: &TogglePlaying, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.toggle_playing(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(cx.listener(|this, action: &PrevSlide, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.prev_slide(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(cx.listener(|this, action: &NextSlide, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.next_slide(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(cx.listener(|this, action: &SceneStart, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.scene_start(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(cx.listener(|this, action: &SceneEnd, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.scene_end(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(cx.listener(|this, action: &EpsilonForward, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.epsilon_forward(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(cx.listener(|this, action: &EpsilonBackward, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.epsilon_backward(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(cx.listener(|this, action: &ExportImage, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.export_image(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(cx.listener(|this, action: &ExportVideo, window, cx| {
                this.document
                    .update(cx, |document, cx| {
                        document.export_video(action, window, cx);
                    })
                    .ok();
            }))
            .on_action(
                cx.listener(|this, action: &ExportSlidesAsVideos, window, cx| {
                    this.document
                        .update(cx, |document, cx| {
                            document.export_slides_as_videos(action, window, cx);
                        })
                        .ok();
                }),
            )
            .child(self.viewport.clone())
    }
}
