use std::path::PathBuf;

use gpui_kit::AppContext as _;
use kiln_accounts::Accounts;

fn main() {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|first| first == "mcp")
    {
        // An agent's tool server; its standard streams are the protocol's.
        if let Err(error) = kiln_bridge::relay() {
            log::error!("{error:#}");
            std::process::exit(1);
        }
        return;
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let folder = project_folder();

    gpui_kit::application()
        .with_assets(kiln_workspace::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            kiln_workspace::init(cx);

            let accounts = cx.new(|_| load_accounts());
            kiln_workspace::open_first_window(folder, accounts, kiln_accounts::data_dir(), cx);
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

/// The folder given as the first argument; without one, Kiln opens what it
/// had open or asks for a folder.
fn project_folder() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::args_os().nth(1)?);
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}
