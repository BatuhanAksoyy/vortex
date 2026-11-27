//! The Kiln window: layout, the session list, chat tabs, editor tabs, pickers, and the
//! commands that drive them.

mod appearance;
mod browser;
mod chat_panel;
mod cli_version;
mod commands;
mod commit_review;
mod commit_settings;
mod config;
mod conversation_picker;
mod editor_pane;
mod file_icon;
mod files_page;
mod git_page;
mod name_prompt;
mod pickers;
mod project;
mod project_picker;
mod pushing;
mod recent_projects;
mod repo_status;
mod saved_layout;
mod search_page;
mod session_list;
mod settings_page;
mod shortcuts_page;
mod side_panel;
mod updates;
mod windows;
mod workspace;

use gpui_kit::App;

pub use cli_version::{CliVersion, CliVersionEvent};
pub use kiln_chat::Assets;
pub use updates::watch as watch_updates;
pub use windows::open_first as open_first_window;
pub use workspace::Workspace;

/// Registers themes, configuration state and application-level commands.
/// Call once, after `gpui_kit::init`.
pub fn init(cx: &mut App) {
    appearance::register_themes(cx);
    // Before the configuration, which keeps the bindings it finds.
    browser::bind_keys(cx);
    config::init(cx);
    commands::register(cx);
}
