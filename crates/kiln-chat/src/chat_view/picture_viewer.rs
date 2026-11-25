//! A picture at full size, over the whole window. Escape, the close button
//! on it or a click beside it closes it.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FocusHandle, Focusable as _, KeyDownEvent, ObjectFit, TestSupportExt,
    Window, anchored,
    component::{
        ActiveTheme, IconName,
        button::{Button, ButtonVariants},
    },
    deferred, div, img, point, px,
};
use kiln_agent::Picture;

use super::ChatView;
use crate::{pictures, scale::ScaledButton as _};

pub(super) struct PictureViewer {
    shown: Option<Picture>,
    focus: FocusHandle,
}

impl PictureViewer {
    pub(super) fn new(cx: &mut Context<ChatView>) -> Self {
        Self {
            shown: None,
            focus: cx.focus_handle(),
        }
    }
}

impl ChatView {
    pub fn view_picture(&mut self, picture: Picture, window: &mut Window, cx: &mut Context<Self>) {
        self.viewer.shown = Some(picture);
        window.focus(&self.viewer.focus, cx);
        cx.notify();
    }

    fn close_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.viewer.shown.take().is_none() {
            return;
        }
        // Back to what was under it: the subagents view, or the box.
        let focus = match &self.agents {
            Some(agents) => agents.read(cx).focus_handle(),
            None => self.composer.read(cx).focus_handle(cx),
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    fn on_viewer_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            self.close_picture(window, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn render_picture_viewer(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let picture = self.viewer.shown.as_ref()?;
        let image = pictures::image(picture)?;
        let theme = cx.theme();
        let size = window.viewport_size();
        let viewer = div()
            .id("chat-picture-viewer")
            .test_support()
            .track_focus(&self.viewer.focus)
            .on_key_down(cx.listener(Self::on_viewer_key))
            .on_click(cx.listener(|this, _, window, cx| this.close_picture(window, cx)))
            .occlude()
            .w(size.width)
            .h(size.height)
            .flex()
            .items_center()
            .justify_center()
            .p_6()
            .bg(theme.overlay)
            .child(
                // A click on the picture itself leaves it open.
                div()
                    .id("chat-picture")
                    .test_support()
                    .relative()
                    .flex()
                    // A tiny picture keeps room for its close button.
                    .min_w(px(96.))
                    .min_h(px(96.))
                    .max_w_full()
                    .max_h_full()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        img(image)
                            .max_w_full()
                            .max_h_full()
                            .object_fit(ObjectFit::Contain)
                            .rounded(theme.radius)
                            .shadow_lg(),
                    )
                    .child(Self::close_button(cx)),
            );
        // Drawn above every pane, not inside the chat's own bounds.
        Some(
            deferred(anchored().position(point(px(0.), px(0.))).child(viewer))
                .with_priority(1)
                .into_any_element(),
        )
    }

    fn close_button(cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        div()
            .absolute()
            .top_2()
            .right_2()
            .p_0p5()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_md()
            .child(
                Button::new("chat-picture-close")
                    .ghost()
                    .control_icon(IconName::Close)
                    .tooltip("Close")
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_picture(window, cx);
                    })),
            )
    }
}
