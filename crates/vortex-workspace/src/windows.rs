//! Vortex's windows, as editors have them: the first one reopens what was left
//! open, each further one ("New Window") starts on the folders to choose
//! from, recent ones first. Vortex quits when its last window closes.

use std::{path::PathBuf, rc::Rc, sync::Arc};

use gpui_kit::{
    App, AppContext as _, Bounds, Context, Entity, Global, WindowBounds, WindowOptions,
    component::TitleBar, px, size,
};
use vortex_accounts::Accounts;
use vortex_agent_claude::{ClaudeBackend, ClaudeCli};
use vortex_agent_codex::{CodexBackend, CodexCli};
use vortex_chat::Backends;

use crate::{
    cli_version::CliVersion, commands::ActiveWorkspace, programs::Programs, workspace::Workspace,
};

/// What every window shares.
struct Windows {
    accounts: Entity<Accounts>,
    /// Where Vortex keeps its data; `None` keeps none.
    data_dir: Option<PathBuf>,
}

impl Global for Windows {}

/// How many windows are open.
#[derive(Default)]
struct OpenWindows(usize);

impl Global for OpenWindows {}

/// Opens the first window: in `folder` if given, with the layout left at the
/// last quit.
pub fn open_first(
    folder: Option<PathBuf>,
    accounts: Entity<Accounts>,
    data_dir: Option<PathBuf>,
    cx: &mut App,
) {
    if let Some(dir) = &data_dir {
        cx.set_global(vortex_chat::LastModels::load(dir.join("models.json")));
    }
    let notes = data_dir.as_deref().and_then(whats_new);
    cx.set_global(Windows { accounts, data_dir });
    if !open(folder, true, notes, cx) {
        cx.quit();
    }
}

/// What changed since the Vortex that last ran with the data in `dir`.
fn whats_new(dir: &std::path::Path) -> Option<String> {
    // A Vortex that has run kept its layout there; before the version was
    // kept too, that tells one from a first run.
    let ran_before = dir.join("layouts.json").exists();
    vortex_update::whats_new::since_last_run(dir, ran_before)
        .inspect_err(|error| log::warn!("cannot keep the version that ran: {error:#}"))
        .ok()
        .flatten()
}

/// Opens another window on the folders to choose from. The layout stays
/// the first window's.
pub fn open_another(cx: &mut App) {
    open(None, false, None, cx);
}

/// Opens a window, with `notes` of what changed since the last run shown
/// over it.
fn open(folder: Option<PathBuf>, first: bool, notes: Option<String>, cx: &mut App) -> bool {
    let Some(shared) = cx.try_global::<Windows>() else {
        return false;
    };
    let (accounts, data_dir) = (shared.accounts.clone(), shared.data_dir.clone());
    let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(640.), px(400.))),
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui_kit::WindowDecorations::Client),
        ..TitleBar::window_options()
    };
    let programs = crate::programs::programs(cx);
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        let Programs { claude, codex } = programs;
        let backend: Rc<dyn vortex_agent::AgentBackend> =
            Rc::new(ClaudeBackend::new(claude.clone()));
        let codex_backend: Rc<dyn vortex_agent::AgentBackend> =
            Rc::new(CodexBackend::new(codex.clone()));
        let backends = Backends::new(backend.clone(), vec![codex_backend]);
        let claude_cli = Arc::new(ClaudeCli::new(claude));
        let claude_version = cx.new(|cx| CliVersion::new(claude_cli, cx));
        let codex_cli = Arc::new(CodexCli::new(codex));
        let codex_version = cx.new(|cx| CliVersion::new(codex_cli, cx));
        // Deferred: the dialog needs the window's Root, made once this returns.
        if let Some(notes) = notes {
            window.defer(cx, move |window, cx| {
                crate::whats_new::show(notes, window, cx);
            });
        }
        cx.new(|cx| {
            let workspace = Workspace::new(folder, backend, accounts, window, cx)
                .offering(backends)
                .showing_cli_version("claude", claude_version, window, cx)
                .showing_cli_version("codex", codex_version, window, cx);
            let Some(dir) = data_dir else {
                return workspace;
            };
            let layouts = dir.join("layouts.json");
            if first {
                workspace.keeping_layouts_in(layouts)
            } else {
                workspace.keeping_data_beside(&layouts)
            }
        })
    });
    if let Err(error) = &opened {
        log::error!("cannot open a Vortex window: {error:#}");
    }
    opened.is_ok()
}

/// Counts `workspace`'s window as open until it is released, and makes it
/// the one commands act on.
pub(crate) fn track(window: &gpui_kit::Window, cx: &mut Context<Workspace>) {
    make_active(window, cx);
    cx.default_global::<OpenWindows>().0 += 1;
    cx.on_release(|_, cx| {
        let open = cx.default_global::<OpenWindows>();
        open.0 = open.0.saturating_sub(1);
        if open.0 == 0 {
            cx.quit();
        }
    })
    .detach();
}

/// Commands act on the window the user works in.
pub(crate) fn make_active(window: &gpui_kit::Window, cx: &mut Context<Workspace>) {
    let workspace = cx.entity().downgrade();
    cx.set_global(ActiveWorkspace {
        window: window.window_handle(),
        workspace,
    });
}
