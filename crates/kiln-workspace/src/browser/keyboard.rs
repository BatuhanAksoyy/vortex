//! The keyboard over the page. Kiln's own shortcuts come first; the rest is
//! the page's.

use std::ops::Range;

use gpui_kit::{
    App, Bounds, ClipboardEntry, ClipboardItem, Context, EntityInputHandler, Focusable, KeyBinding,
    KeyDownEvent, KeyUpEvent, Keystroke, NoAction, Pixels, Point, UTF16Selection, Window,
};
use kiln_browser::Key;

use super::{Browser, KEY_CONTEXT, page_view::modifiers};

/// Leaves to the page the keys that elsewhere move the focus.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(
        ["tab", "shift-tab"].map(|key| KeyBinding::new(key, NoAction {}, Some(KEY_CONTEXT))),
    );
}

impl Browser {
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stroke = &event.keystroke;
        // Left to the input method, which sends what it makes of it.
        if self.page().is_none() || composes(stroke) {
            return;
        }
        cx.stop_propagation();
        if self.picking.on && stroke.key == "escape" {
            self.set_picking(false, cx);
            return;
        }
        if stroke.modifiers.secondary(cx) && self.command(&stroke.key, window, cx) {
            return;
        }
        if let Some(page) = self.page() {
            page.key_down(&key(stroke, event.is_held));
        }
    }

    pub(super) fn on_key_up(&mut self, event: &KeyUpEvent, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.page() {
            page.key_up(&key(&event.keystroke, false));
        }
    }

    /// What the panel does itself; `false` leaves the key to the page.
    fn command(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match key {
            "l" => self.address.focus_handle(cx).focus(window, cx),
            "r" => self.reload(),
            "[" => self.go_back(),
            "]" => self.go_forward(),
            "c" => self.copy(false, cx),
            "x" => self.copy(true, cx),
            "v" => self.paste_clipboard(cx),
            _ => return false,
        }
        true
    }

    /// Pages have no clipboard of their own without a window.
    pub(super) fn copy(&self, cut: bool, cx: &mut Context<Self>) {
        let Some(page) = self.page().cloned() else {
            return;
        };
        cx.spawn(async move |_, cx| {
            let Ok(text) = page.selection().await else {
                return;
            };
            if text.is_empty() {
                return;
            }
            cx.update(|cx| cx.write_to_clipboard(ClipboardItem::new_string(text)));
            if cut {
                let delete = Key {
                    name: "backspace".to_owned(),
                    text: None,
                    modifiers: kiln_browser::Modifiers::default(),
                    repeated: false,
                };
                page.key_down(&delete);
                page.key_up(&delete);
            }
        })
        .detach();
    }

    pub(super) fn paste_clipboard(&self, cx: &Context<Self>) {
        let (Some(page), Some(item)) = (self.page(), cx.read_from_clipboard()) else {
            return;
        };
        if let Some(text) = item.text() {
            page.insert(&text);
            return;
        }
        let image = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            ClipboardEntry::String(_) | ClipboardEntry::ExternalPaths(_) => None,
        });
        if let Some(image) = image {
            page.paste_image(image.format().mime_type(), image.bytes());
        }
    }
}

/// A key that begins another character, as `^` before `a` does.
fn composes(stroke: &Keystroke) -> bool {
    let held = stroke.modifiers;
    let plain = !held.control && !held.platform && !held.function;
    plain && stroke.key_char.is_none() && stroke.key.chars().count() == 1
}

fn key(stroke: &Keystroke, repeated: bool) -> Key {
    Key {
        name: stroke.key.clone(),
        text: stroke.key_char.clone(),
        modifiers: modifiers(stroke.modifiers),
        repeated,
    }
}

/// The page keeps its text to itself; input methods only hand text over.
impl EntityInputHandler for Browser {
    fn text_for_range(
        &mut self,
        _: Range<usize>,
        _: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        (self.marked > 0).then_some(0..self.marked)
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = 0;
    }

    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.marked = 0;
        if let Some(page) = self.page() {
            page.insert(text);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.marked = text.encode_utf16().count();
        if let Some(page) = self.page() {
            page.compose(text);
        }
    }

    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
