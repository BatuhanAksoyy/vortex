use gpui_kit::{
    App, SharedString,
    component::{Theme, ThemeMode, ThemeRegistry},
    px, rgb,
};
use kiln_chat::Brand;
use kiln_settings::{Settings, ThemeMode as ModeSetting};

const KILN_THEMES: &str = include_str!("../../../assets/themes/kiln.json");
const FALLBACK_LIGHT: &str = "Kiln Light";
const FALLBACK_DARK: &str = "Kiln Dark";

pub fn register_themes(cx: &mut App) {
    if let Err(error) = ThemeRegistry::global_mut(cx).load_themes_from_str(KILN_THEMES) {
        log::error!("bundled Kiln themes failed to load: {error:#}");
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
    let dark = Theme::global(cx).is_dark();
    // Tailwind's blue scale, a step lighter in the dark theme to keep the
    // contrast the orange had.
    let [base, hover, active, link, selection] = if dark {
        [0x3b_82_f6, 0x60_a5_fa, 0x25_63_eb, 0x7f_b0_ff, 0x3b_82_f6]
    } else {
        [0x25_63_eb, 0x1d_4e_d8, 0x1e_40_af, 0x1d_4e_d8, 0xcf_e0_fb]
    };
    Theme::update(cx, |theme| {
        let colors = &mut theme.colors;
        colors.primary = rgb(base).into();
        colors.primary_hover = rgb(hover).into();
        colors.primary_active = rgb(active).into();
        colors.ring = rgb(base).into();
        colors.sidebar_primary = rgb(base).into();
        colors.link = rgb(link).into();
        colors.selection = if dark {
            rgb(selection).opacity(0.3).into()
        } else {
            rgb(selection).into()
        };
    });
}

fn resolve_theme(
    name: &str,
    fallback: &str,
    diagnostics: &mut Vec<String>,
    cx: &App,
) -> Option<std::rc::Rc<gpui_kit::component::ThemeConfig>> {
    let themes = ThemeRegistry::global(cx).themes();
    if let Some(theme) = themes.get(name) {
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
