use gpui_kit::{
    App, SharedString,
    component::{Theme, ThemeMode, ThemeRegistry},
    px,
};
use vortex_chat::Brand;
use vortex_settings::{Settings, ThemeMode as ModeSetting};

const VORTEX_THEMES: &str = include_str!("../../../assets/themes/vortex.json");
const FALLBACK_LIGHT: &str = "Vortex Light";
const FALLBACK_DARK: &str = "Vortex Dark";

pub fn register_themes(cx: &mut App) {
    if let Err(error) = ThemeRegistry::global_mut(cx).load_themes_from_str(VORTEX_THEMES) {
        log::error!("bundled Vortex themes failed to load: {error:#}");
    }
}

/// Fonts that gpui-kit picked for this platform, restored when a font setting
/// is cleared.
#[derive(Clone)]
pub struct DefaultFonts {
    ui: SharedString,
    mono: SharedString,
}

impl DefaultFonts {
    pub fn capture(cx: &App) -> Self {
        let theme = Theme::global(cx);
        Self {
            ui: theme.font_family.clone(),
            mono: theme.mono_font_family.clone(),
        }
    }
}

/// Applies theme and font settings. `zoom` is a session-only offset on the UI
/// font size. Returns problems to show the user.
pub fn apply(settings: &Settings, zoom: f32, defaults: &DefaultFonts, cx: &mut App) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let light = resolve_theme(&settings.theme.light, FALLBACK_LIGHT, &mut diagnostics, cx);
    let dark = resolve_theme(&settings.theme.dark, FALLBACK_DARK, &mut diagnostics, cx);

    Theme::update(cx, |theme| {
        if let Some(light) = light {
            theme.light_theme = light;
        }
        if let Some(dark) = dark {
            theme.dark_theme = dark;
        }
    });
    match settings.theme.mode {
        ModeSetting::System => Theme::sync_system_appearance(None, cx),
        ModeSetting::Light => Theme::change(ThemeMode::Light, None, cx),
        ModeSetting::Dark => Theme::change(ThemeMode::Dark, None, cx),
    }
    apply_brand(cx);
    Theme::update(cx, |theme| {
        theme.font_size = px((settings.ui.font_size + zoom).max(6.0));
        theme.font_family = font_or_default(&settings.ui.font_family, &defaults.ui);
        theme.mono_font_size = px(settings.editor.font_size);
        theme.mono_font_family = font_or_default(&settings.editor.font_family, &defaults.mono);
    });
    diagnostics
}

/// Puts Codex's blue over the theme's brand color while the user works with
/// Codex; Claude's is the theme's own.
fn apply_brand(cx: &mut App) {
    if cx.try_global::<Brand>().copied().unwrap_or_default() != Brand::Codex {
        return;
    }
    // Codex's look is monochrome: the accent is the theme's own text color,
    // white in a dark theme and near black in a light one.
    Theme::update(cx, |theme| {
        let colors = &mut theme.colors;
        let (text, page) = (colors.foreground, colors.background);
        colors.primary = text;
        colors.primary_hover = text.opacity(0.85);
        colors.primary_active = text.opacity(0.7);
        colors.primary_foreground = page;
        colors.button_primary_foreground = page;
        colors.ring = text.opacity(0.6);
        colors.sidebar_primary = text;
        colors.sidebar_primary_foreground = page;
        colors.link = text;
        colors.selection = text.opacity(0.2);
    });
}

fn resolve_theme(
    name: &str,
    fallback: &str,
    diagnostics: &mut Vec<String>,
    cx: &App,
) -> Option<std::rc::Rc<gpui_kit::component::ThemeConfig>> {
    let themes = ThemeRegistry::global(cx).themes();
    // Vortex's own themes were called Kiln's before, as settings may still
    // name them.
    let renamed = name
        .strip_prefix("Kiln ")
        .map(|rest| format!("Vortex {rest}"));
    if let Some(theme) = themes.get(renamed.as_deref().unwrap_or(name)) {
        return Some(theme.clone());
    }
    diagnostics.push(format!("theme `{name}` not found, using `{fallback}`"));
    themes.get(fallback).cloned()
}

fn font_or_default(setting: &str, default: &SharedString) -> SharedString {
    if setting.trim().is_empty() {
        default.clone()
    } else {
        SharedString::from(setting.to_owned())
    }
}
