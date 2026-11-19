//! The Kiln window: layout, file tree, editor tabs, pickers, and the commands
//! that drive them.

mod appearance;
mod commands;
mod config;
mod editor_pane;
mod file_tree;
mod pickers;
mod workspace;

use gpui_kit::App;

pub use workspace::Workspace;

/// Registers themes, configuration state and application-level commands.
/// Call once, after `gpui_kit::init`.
pub fn init(cx: &mut App) {
    appearance::register_themes(cx);
    config::init(cx);
    commands::register(cx);
}
