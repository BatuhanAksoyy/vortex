use std::path::PathBuf;

use gpui_kit::{
    AppContext as _, Bounds, WindowBounds, WindowOptions, component::TitleBar, px, size,
};
use kiln_accounts::Accounts;
use kiln_agent_claude::ClaudeBackend;
use kiln_workspace::Workspace;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let root = project_root();

    gpui_kit::application()
        .with_assets(kiln_workspace::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            kiln_workspace::init(cx);

            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.), px(400.))),
                #[cfg(target_os = "linux")]
                window_decorations: Some(gpui_kit::WindowDecorations::Client),
                ..TitleBar::window_options()
            };
            let accounts = cx.new(|_| load_accounts());
            let opened = gpui_kit::open_window(options, cx, |window, cx| {
                let backend = std::rc::Rc::new(ClaudeBackend::new("claude"));
                cx.new(|cx| Workspace::new(root, backend, accounts, window, cx))
            });
            if let Err(error) = opened {
                log::error!("cannot open the Kiln window: {error:#}");
                cx.quit();
            }
        });
}

/// The accounts added in Kiln. An unreadable list leaves only the default
/// account; adding one then starts a new list.
fn load_accounts() -> Accounts {
    let dir = kiln_accounts::data_dir().unwrap_or_else(|| std::env::temp_dir().join("kiln"));
    Accounts::load(dir.clone()).unwrap_or_else(|error| {
        log::error!("{error:#}");
        Accounts::empty(dir)
    })
}

/// The folder given as the first argument, or the current directory.
fn project_root() -> PathBuf {
    let path = std::env::args_os().nth(1).map_or_else(
        || std::env::current_dir().unwrap_or_default(),
        PathBuf::from,
    );
    std::fs::canonicalize(&path).unwrap_or(path)
}
