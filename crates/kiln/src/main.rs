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
    take_login_environment();
    let folder = project_folder();

    gpui_kit::application()
        .with_assets(kiln_workspace::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            kiln_workspace::init(cx);
            kiln_workspace::watch_updates(cx);

            let accounts = cx.new(|_| load_accounts());
            kiln_workspace::open_first_window(folder, accounts, kiln_accounts::data_dir(), cx);
        });
}

/// Started from Finder or the Dock, an app gets the system's bare `PATH`,
/// and Kiln would find none of the CLIs, git or Node the user installed.
/// From an app bundle Kiln starts itself again through the user's login
/// shell, in the same process, to run with the environment a terminal has.
#[cfg(target_os = "macos")]
fn take_login_environment() {
    use std::os::unix::process::CommandExt as _;
    /// Set once Kiln runs through the user's login shell, so it does so once.
    const THROUGH_SHELL: &str = "KILN_LOGIN_SHELL";

    if std::env::var_os(THROUGH_SHELL).is_some() || kiln_update::running_bundle().is_none() {
        return;
    }
    let Ok(program) = std::env::current_exe() else {
        return;
    };
    // macOS's own login shell, should the app be given none.
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let error = std::process::Command::new(shell)
        .args(["-l", "-i", "-c", "exec \"$0\" \"$@\""])
        .arg(program)
        .args(std::env::args_os().skip(1))
        .env(THROUGH_SHELL, "1")
        // An interactive shell must not wait on the keyboard.
        .stdin(std::process::Stdio::null())
        .exec();
    log::warn!("Kiln runs without the login shell's environment: {error}");
}

#[cfg(not(target_os = "macos"))]
fn take_login_environment() {}

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
