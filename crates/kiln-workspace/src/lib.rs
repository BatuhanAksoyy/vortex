//! The Kiln window: layout, the session list, chat tabs, editor tabs, pickers, and the
//! commands that drive them.

mod appearance;
mod chat_panel;
mod commands;
mod config;
mod editor_pane;
mod pickers;
mod saved_layout;
mod session_list;
mod workspace;

use gpui_kit::App;

pub use kiln_chat::Assets;
pub use workspace::Workspace;

/// Registers themes, configuration state and application-level commands.
/// Call once, after `gpui_kit::init`.
pub fn init(cx: &mut App) {
    appearance::register_themes(cx);
    config::init(cx);
    commands::register(cx);
}
