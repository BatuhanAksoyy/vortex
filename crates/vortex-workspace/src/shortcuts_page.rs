//! The keyboard shortcuts page, beside the work like the settings page:
//! every command with its keys, searched by name, id or key. A command's
//! keys are recorded by pressing them and written to the user's
//! `keybindings.json`, keeping its comments.

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, Focusable, KeyDownEvent,
    Keystroke, SharedString, Subscription, TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, InputEvent, InputState},
        kbd::Kbd,
        v_flex,
    },
    div,
};
use vortex_chat::scale::{BAR_HEIGHT, ScaledButton as _};
use vortex_keymap::{Keymap, Platform};

use crate::commands::{COMMANDS, CommandSpec};

pub enum ShortcutsPageEvent {
    /// The file was written; the window binds the keys again.
    Changed,
    Close,
    OpenFile,
}

pub struct ShortcutsPage {
    /// The user's `keybindings.json`.
    file: PathBuf,
    /// What the file holds, empty while there is none.
    user: String,
    keymap: Keymap,
    search: Entity<InputState>,
    /// The command whose keys are being pressed, and what takes them
    /// before any binding can act on them.
    recording: Option<(&'static str, Subscription)>,
    error: Option<SharedString>,
    focus: FocusHandle,
    _search: Subscription,
}

impl EventEmitter<ShortcutsPageEvent> for ShortcutsPage {}

impl ShortcutsPage {
    pub fn new(file: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search shortcuts"));
        let subscription = cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let mut page = Self {
            file,
            user: String::new(),
            keymap: Keymap::default(),
            search,
            recording: None,
            error: None,
            focus: cx.focus_handle(),
            _search: subscription,
        };
        page.reread(cx);
        page
    }

    /// Reads the file again, after it changed here or elsewhere.
    pub fn reread(&mut self, cx: &mut Context<Self>) {
        self.user = std::fs::read_to_string(&self.file).unwrap_or_default();
        match vortex_keymap::resolve(Some(&self.user), Platform::current()) {
            Ok(keymap) => self.keymap = keymap,
            Err(error) => self.error = Some(format!("{error:#}").into()),
        }
        cx.notify();
    }

    /// The keys bound to `command`, in GPUI syntax.
    fn keys(&self, command: &str) -> Vec<&str> {
        let bound = self.keymap.bindings.iter();
        let bound = bound.filter(|binding| binding.command == command);
        bound.map(|binding| binding.keystrokes.as_str()).collect()
    }

    /// Waits for the keys of `command`; Escape gives up.
    fn record(&mut self, command: &'static str, cx: &mut Context<Self>) {
        let page = cx.entity().downgrade();
        let taking = cx.intercept_keystrokes(move |event, _, cx| {
            let keystroke = event.keystroke.clone();
            if keystroke.is_modifier_key() {
                return;
            }
            cx.stop_propagation();
            let _ = page.update(cx, |page, cx| page.pressed(&keystroke, cx));
        });
        self.recording = Some((command, taking));
        self.error = None;
        cx.notify();
    }

    fn pressed(&mut self, keystroke: &Keystroke, cx: &mut Context<Self>) {
        let Some((command, _)) = self.recording.take() else {
            return;
        };
        if keystroke.key == "escape" && !keystroke.modifiers.modified() {
            cx.notify();
            return;
        }
        let keys = vscode_keys(keystroke);
        self.write(|user| vortex_keymap::rebind(user, command, Some(&keys)), cx);
    }

    fn write(
        &mut self,
        change: impl FnOnce(&str) -> anyhow::Result<String>,
        cx: &mut Context<Self>,
    ) {
        let written = change(&self.user).and_then(|text| {
            if let Some(folder) = self.file.parent() {
                std::fs::create_dir_all(folder)?;
            }
            std::fs::write(&self.file, text)?;
            Ok(())
        });
        match written {
            Ok(()) => {
                self.reread(cx);
                cx.emit(ShortcutsPageEvent::Changed);
            }
            Err(error) => self.error = Some(format!("Not saved: {error:#}").into()),
        }
        cx.notify();
    }

    fn shown(&self, cx: &App) -> Vec<&'static CommandSpec> {
        let query = self.search.read(cx).value().trim().to_lowercase();
        COMMANDS
            .iter()
            .filter(|spec| {
                query.is_empty()
                    || spec.label.to_lowercase().contains(&query)
                    || spec.id.to_lowercase().contains(&query)
                    || self.keys(spec.id).iter().any(|keys| keys.contains(&query))
            })
            .collect()
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .flex_none()
            .h(BAR_HEIGHT)
            .px_2()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().text_sm().child("Keyboard Shortcuts"))
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&self.search).small().cleanable(true)),
            )
            .child(
                Button::new("shortcuts-open-file")
                    .ghost()
                    .control()
                    .label("Edit in keybindings.json")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ShortcutsPageEvent::OpenFile))),
            )
            .child(
                Button::new("shortcuts-close")
                    .ghost()
                    .control_icon(IconName::Close)
                    .tooltip("Close Keyboard Shortcuts")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ShortcutsPageEvent::Close))),
            )
    }

    fn render_row(&self, spec: &'static CommandSpec, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let command = spec.id;
        let recording = self
            .recording
            .as_ref()
            .is_some_and(|(it, _)| *it == command);
        let keys = self.keys(command);
        let shown = render_keys(&keys, recording, cx);
        let changed = vortex_keymap::changes(&self.user, command);
        h_flex()
            .id(SharedString::from(format!("shortcut-{command}")))
            .test_support()
            .px_3()
            .py_1()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().truncate().child(spec.label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(command),
                    ),
            )
            .child(shown)
            .child(row_buttons(
                command,
                RowState {
                    recording,
                    bound: !keys.is_empty(),
                    changed,
                },
                cx,
            ))
            .into_any_element()
    }
}

/// The keys, or what stands in for them.
fn render_keys(keys: &[&str], recording: bool, cx: &Context<ShortcutsPage>) -> AnyElement {
    let theme = cx.theme();
    if recording {
        return div()
            .text_sm()
            .text_color(theme.primary)
            .child("Press the keys, or Escape")
            .into_any_element();
    }
    if keys.is_empty() {
        return div()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child("—")
            .into_any_element();
    }
    let os = cx.platform_info().os;
    let chords = keys.iter().flat_map(|keys| keys.split_whitespace());
    let shown = chords.filter_map(|chord| Keystroke::parse_with_os(chord, os).ok().map(Kbd::new));
    h_flex().gap_1().children(shown).into_any_element()
}

struct RowState {
    recording: bool,
    bound: bool,
    changed: bool,
}

fn row_buttons(
    command: &'static str,
    RowState {
        recording,
        bound,
        changed,
    }: RowState,
    cx: &Context<ShortcutsPage>,
) -> impl IntoElement + use<> {
    h_flex()
        .flex_none()
        .gap_0p5()
        .child(
            Button::new(SharedString::from(format!("shortcut-change-{command}")))
                .ghost()
                .control_icon(Lucide::Pencil)
                .tooltip("Change Keys")
                .when(!recording, |button| {
                    button.on_click(cx.listener(move |this, _, _, cx| this.record(command, cx)))
                }),
        )
        .when(bound, |row| {
            row.child(
                Button::new(SharedString::from(format!("shortcut-remove-{command}")))
                    .ghost()
                    .control_icon(Lucide::Trash)
                    .tooltip("Remove Keys")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.write(|user| vortex_keymap::rebind(user, command, None), cx);
                    })),
            )
        })
        .when(changed, |row| {
            row.child(
                Button::new(SharedString::from(format!("shortcut-reset-{command}")))
                    .ghost()
                    .control()
                    .label("Reset")
                    .tooltip("Back to the Default Keys")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.write(|user| vortex_keymap::reset(user, command), cx);
                    })),
            )
        })
}

/// `keystroke` as `keybindings.json` writes it, such as `cmd+shift+p`.
fn vscode_keys(keystroke: &Keystroke) -> String {
    let modifiers = keystroke.modifiers;
    let platform = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "meta"
    };
    [
        (modifiers.control, "ctrl"),
        (modifiers.alt, "alt"),
        (modifiers.shift, "shift"),
        (modifiers.platform, platform),
    ]
    .into_iter()
    .filter_map(|(held, name)| held.then_some(name))
    .chain([keystroke.key.as_str()])
    .collect::<Vec<_>>()
    .join("+")
}

impl Focusable for ShortcutsPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ShortcutsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let rows: Vec<AnyElement> = self
            .shown(cx)
            .into_iter()
            .map(|spec| self.render_row(spec, cx))
            .collect();
        v_flex()
            .id("shortcuts-page")
            .test_support()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" && this.recording.is_none() {
                    cx.stop_propagation();
                    cx.emit(ShortcutsPageEvent::Close);
                }
            }))
            .size_full()
            .bg(theme.background)
            .child(self.render_header(cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .id("shortcuts-error")
                        .test_support()
                        .px_3()
                        .py_1()
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .child(
                div()
                    .id("shortcuts-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
}
