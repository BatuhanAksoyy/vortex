//! Owns the effective settings and keymap, and reapplies them on change.

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use gpui_kit::{App, Global, KeyBinding, KeyBindingContextPredicate};
use kiln_keymap::{Binding, Keymap, Platform};
use kiln_settings::{Settings, SettingsSources};

use crate::{appearance, commands};

pub struct ConfigState {
    settings: Settings,
    zoom: f32,
    /// Bindings registered by gpui-kit components, kept so the keymap can be
    /// rebuilt from scratch when the user's file changes.
    component_bindings: Vec<KeyBinding>,
    default_fonts: appearance::DefaultFonts,
}

impl Global for ConfigState {}

pub fn init(cx: &mut App) {
    let settings = match kiln_settings::load(&SettingsSources::default()) {
        Ok(loaded) => loaded.settings,
        Err(error) => {
            log::error!("embedded defaults are invalid: {error:#}");
            return;
        }
    };
    let component_bindings = cx.key_bindings().borrow().bindings().cloned().collect();
    let default_fonts = appearance::DefaultFonts::capture(cx);
    cx.set_global(ConfigState {
        settings,
        zoom: 0.0,
        component_bindings,
        default_fonts,
    });
}

pub fn settings(cx: &App) -> &Settings {
    &cx.global::<ConfigState>().settings
}

/// Files whose changes trigger [`reload`].
pub fn watched_files(project_root: Option<&Path>) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = [
        kiln_settings::user_settings_path(),
        kiln_settings::user_keybindings_path(),
    ]
    .into_iter()
    .flatten()
    .collect();
    files.extend(project_layers(project_root));
    files
}

fn project_layers(project_root: Option<&Path>) -> impl Iterator<Item = PathBuf> {
    let paths = project_root.map(kiln_settings::project_settings_paths);
    paths.into_iter().flatten()
}

/// Reloads settings and keybindings from disk and applies them.
/// Returns human-readable problems; the previous good values stay in effect
/// for any file that failed.
pub fn reload(project_root: Option<&Path>, cx: &mut App) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let mut layers: Vec<PathBuf> = kiln_settings::user_settings_path().into_iter().collect();
    layers.extend(project_layers(project_root));
    match kiln_settings::load(&SettingsSources { layers }) {
        Ok(loaded) => {
            diagnostics.extend(
                loaded
                    .diagnostics
                    .into_iter()
                    .map(|d| format!("{}: {}", d.path.display(), d.message)),
            );
            cx.global_mut::<ConfigState>().settings = loaded.settings;
        }
        Err(error) => diagnostics.push(format!("{error:#}")),
    }
    diagnostics.extend(apply_appearance(cx));
    diagnostics.extend(reload_keymap(cx));
    diagnostics
}

pub fn adjust_zoom(delta: Option<f32>, cx: &mut App) {
    let state = cx.global_mut::<ConfigState>();
    state.zoom = delta.map_or(0.0, |delta| (state.zoom + delta).clamp(-8.0, 24.0));
    apply_appearance(cx);
}

fn apply_appearance(cx: &mut App) -> Vec<String> {
    let state = cx.global::<ConfigState>();
    let (settings, zoom, fonts) = (
        state.settings.clone(),
        state.zoom,
        state.default_fonts.clone(),
    );
    appearance::apply(&settings, zoom, &fonts, cx)
}

fn reload_keymap(cx: &mut App) -> Vec<String> {
    let user = kiln_settings::user_keybindings_path().and_then(|p| std::fs::read_to_string(p).ok());
    let keymap = match kiln_keymap::resolve(user.as_deref(), Platform::current()) {
        Ok(keymap) => keymap,
        Err(error) => return vec![format!("{error:#}")],
    };
    let (bindings, diagnostics) = build_bindings(&keymap, cx);
    let mut all = cx.global::<ConfigState>().component_bindings.clone();
    all.extend(bindings);
    cx.clear_key_bindings();
    cx.bind_keys(all);
    diagnostics
}

fn build_bindings(keymap: &Keymap, cx: &App) -> (Vec<KeyBinding>, Vec<String>) {
    let mut diagnostics = keymap.diagnostics.clone();
    let mut bindings = Vec::new();
    let mut unknown_commands = 0_usize;
    for binding in &keymap.bindings {
        match build_binding(binding, cx) {
            Ok(Some(key_binding)) => bindings.push(key_binding),
            Ok(None) => unknown_commands += 1,
            Err(message) => diagnostics.push(message),
        }
    }
    if unknown_commands > 0 {
        diagnostics.push(format!(
            "{unknown_commands} keybinding(s) skipped: their commands do not exist in Kiln"
        ));
    }
    (bindings, diagnostics)
}

/// `Ok(None)` means the command is unknown, which is expected for keybindings
/// imported from VSCode and reported only as a count.
fn build_binding(binding: &Binding, cx: &App) -> Result<Option<KeyBinding>, String> {
    let Some(spec) = commands::find(&binding.command) else {
        return Ok(None);
    };
    let context = binding
        .when
        .as_deref()
        .map(|when| {
            let predicate = context_for_when(when)?;
            KeyBindingContextPredicate::parse(predicate)
                .map(Rc::new)
                .map_err(|error| format!("`{when}`: {error}"))
        })
        .transpose()?;
    KeyBinding::load(&binding.keystrokes, spec.action(), context, false, None, cx)
        .map(Some)
        .map_err(|error| {
            format!(
                "`{}` for `{}`: {error}",
                binding.keystrokes, binding.command
            )
        })
}

/// Supported VSCode `when` clauses and the GPUI key context each maps to.
fn context_for_when(when: &str) -> Result<&'static str, String> {
    match when.trim() {
        "editorTextFocus" | "editorFocus" | "textInputFocus" => Ok("Input && mode == editor"),
        "filesExplorerFocus" | "explorerViewletFocus" => Ok("FileTree"),
        "inQuickOpen" => Ok("Command"),
        other => Err(format!("unsupported `when` clause `{other}`")),
    }
}
