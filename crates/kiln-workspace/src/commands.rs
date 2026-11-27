//! Every user-facing command, defined once. Keybindings, the command palette
//! and menus all resolve to these actions.
//!
//! Handlers are registered at application level so a command works wherever
//! focus is, including inside dialogs that are not descendants of the
//! workspace view.

use gpui_kit::{Action, AnyWindowHandle, App, Context, Global, WeakEntity, Window, actions};

use crate::workspace::Workspace;

actions!(
    kiln,
    [
        QuickOpen,
        ShowCommands,
        ShowChat,
        ShowFiles,
        ShowGit,
        ShowSearch,
        ToggleSidePanel,
        ToggleBrowser,
        ToggleChanges,
        ShowSettings,
        ShowShortcuts,
        OpenSettings,
        CheckForUpdates,
        OpenCommitSettings,
        OpenDefaultSettings,
        OpenKeybindings,
        OpenDefaultKeybindings,
        OpenFolder,
        NewWindow,
        Save,
        CloseEditor,
        SplitEditor,
        NextEditor,
        PreviousEditor,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        Quit,
    ]
);

pub struct CommandSpec {
    /// VSCode command id where an equivalent exists, so VSCode keybindings apply.
    pub id: &'static str,
    pub label: &'static str,
    build: fn() -> Box<dyn Action>,
}

impl CommandSpec {
    pub fn action(&self) -> Box<dyn Action> {
        (self.build)()
    }
}

pub const COMMANDS: &[CommandSpec] = &[
    spec("workbench.action.quickOpen", "Go to File", || {
        Box::new(QuickOpen)
    }),
    spec("workbench.action.showCommands", "Show All Commands", || {
        Box::new(ShowCommands)
    }),
    spec("workbench.action.chat.open", "Show Chat", || {
        Box::new(ShowChat)
    }),
    spec("workbench.view.explorer", "Show Files", || {
        Box::new(ShowFiles)
    }),
    spec("workbench.view.scm", "Show Changes", || Box::new(ShowGit)),
    spec("workbench.action.findInFiles", "Search in Files", || {
        Box::new(ShowSearch)
    }),
    spec(
        "workbench.action.toggleSidebarVisibility",
        "Toggle Side Panel",
        || Box::new(ToggleSidePanel),
    ),
    spec("simpleBrowser.show", "Toggle Browser", || {
        Box::new(ToggleBrowser)
    }),
    spec("git.openChange", "Show File Changes", || {
        Box::new(ToggleChanges)
    }),
    spec("workbench.action.openSettings", "Open Settings", || {
        Box::new(ShowSettings)
    }),
    spec("update.checkForUpdate", "Check for Updates…", || {
        Box::new(CheckForUpdates)
    }),
    spec(
        "workbench.action.openSettingsJson",
        "Open User Settings (JSON)",
        || Box::new(OpenSettings),
    ),
    spec("kiln.openCommitSettings", "Git: Commit Settings", || {
        Box::new(OpenCommitSettings)
    }),
    spec(
        "workbench.action.openRawDefaultSettings",
        "Open Default Settings (JSON)",
        || Box::new(OpenDefaultSettings),
    ),
    spec(
        "workbench.action.openGlobalKeybindings",
        "Keyboard Shortcuts",
        || Box::new(ShowShortcuts),
    ),
    spec(
        "workbench.action.openGlobalKeybindingsFile",
        "Open Keyboard Shortcuts (JSON)",
        || Box::new(OpenKeybindings),
    ),
    spec(
        "workbench.action.openDefaultKeybindingsFile",
        "Open Default Keyboard Shortcuts (JSON)",
        || Box::new(OpenDefaultKeybindings),
    ),
    spec(
        "workbench.action.files.openFolder",
        "Open Folder…",
        || Box::new(OpenFolder),
    ),
    spec("workbench.action.newWindow", "New Window", || {
        Box::new(NewWindow)
    }),
    spec("workbench.action.files.save", "Save", || Box::new(Save)),
    spec("workbench.action.closeActiveEditor", "Close Editor", || {
        Box::new(CloseEditor)
    }),
    spec("workbench.action.splitEditor", "Split Editor", || {
        Box::new(SplitEditor)
    }),
    spec("workbench.action.nextEditor", "Next Editor", || {
        Box::new(NextEditor)
    }),
    spec("workbench.action.previousEditor", "Previous Editor", || {
        Box::new(PreviousEditor)
    }),
    spec("workbench.action.zoomIn", "Zoom In", || Box::new(ZoomIn)),
    spec("workbench.action.zoomOut", "Zoom Out", || Box::new(ZoomOut)),
    spec("workbench.action.zoomReset", "Reset Zoom", || {
        Box::new(ZoomReset)
    }),
    spec("workbench.action.quit", "Quit", || Box::new(Quit)),
];

const fn spec(
    id: &'static str,
    label: &'static str,
    build: fn() -> Box<dyn Action>,
) -> CommandSpec {
    CommandSpec { id, label, build }
}

pub fn find(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|spec| spec.id == id)
}

/// The workspace that application-level command handlers act on.
pub struct ActiveWorkspace {
    pub window: AnyWindowHandle,
    pub workspace: WeakEntity<Workspace>,
}

impl Global for ActiveWorkspace {}

type Handler = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);

pub fn register(cx: &mut App) {
    on::<QuickOpen>(cx, Workspace::quick_open);
    on::<ShowCommands>(cx, Workspace::show_commands);
    on::<ShowChat>(cx, Workspace::show_chat);
    on::<ShowFiles>(cx, Workspace::show_files);
    on::<ShowGit>(cx, Workspace::show_git);
    on::<ShowSearch>(cx, Workspace::show_search);
    on::<ToggleSidePanel>(cx, Workspace::toggle_side_panel);
    on::<ToggleBrowser>(cx, Workspace::toggle_browser);
    on::<ToggleChanges>(cx, Workspace::toggle_changes);
    on::<ShowSettings>(cx, Workspace::toggle_settings_page);
    on::<ShowShortcuts>(cx, Workspace::toggle_shortcuts_page);
    on::<OpenSettings>(cx, Workspace::open_user_settings);
    on::<CheckForUpdates>(cx, Workspace::check_for_updates);
    on::<OpenCommitSettings>(cx, Workspace::open_commit_settings);
    on::<OpenDefaultSettings>(cx, Workspace::open_default_settings);
    on::<OpenKeybindings>(cx, Workspace::open_user_keybindings);
    on::<OpenDefaultKeybindings>(cx, Workspace::open_default_keybindings);
    on::<OpenFolder>(cx, Workspace::prompt_open_folder);
    on::<Save>(cx, Workspace::save);
    on::<CloseEditor>(cx, Workspace::close_editor);
    on::<SplitEditor>(cx, Workspace::split_editor);
    on::<NextEditor>(cx, Workspace::next_editor);
    on::<PreviousEditor>(cx, Workspace::previous_editor);
    on::<ZoomIn>(cx, Workspace::zoom_in);
    on::<ZoomOut>(cx, Workspace::zoom_out);
    on::<ZoomReset>(cx, Workspace::zoom_reset);
    cx.on_action(|_: &NewWindow, cx: &mut App| crate::windows::open_another(cx));
    cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
}

fn on<A: Action>(cx: &mut App, handler: Handler) {
    cx.on_action(move |_: &A, cx: &mut App| with_workspace(cx, handler));
}

/// Deferred so the handler runs after the dispatching window is released.
fn with_workspace(cx: &mut App, handler: Handler) {
    let Some(active) = cx.try_global::<ActiveWorkspace>() else {
        return;
    };
    let (window, workspace) = (active.window, active.workspace.clone());
    cx.defer(move |cx| {
        let _ = window.update(cx, |_, window, cx| {
            workspace.update(cx, |workspace, cx| handler(workspace, window, cx))
        });
    });
}

#[cfg(test)]
mod tests {
    use super::COMMANDS;
    use std::collections::HashSet;

    #[test]
    fn command_ids_and_labels_are_unique() {
        let ids: HashSet<_> = COMMANDS.iter().map(|c| c.id).collect();
        let labels: HashSet<_> = COMMANDS.iter().map(|c| c.label).collect();
        assert_eq!(ids.len(), COMMANDS.len());
        assert_eq!(labels.len(), COMMANDS.len());
    }

    #[test]
    fn every_default_keybinding_targets_a_known_command() {
        let keymap = kiln_keymap::resolve(None, kiln_keymap::Platform::Linux).unwrap();
        for binding in keymap.bindings {
            assert!(
                super::find(&binding.command).is_some(),
                "{}",
                binding.command
            );
        }
    }
}
