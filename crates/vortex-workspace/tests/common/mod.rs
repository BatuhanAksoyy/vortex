//! A workspace window over a backend with three stored conversations.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, SystemTime},
};

use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size,
};
use vortex_accounts::Accounts;
use vortex_agent::{AgentBackend, PastMessage, PastSession, Pending, Session, SessionSpec, SignIn};
use vortex_workspace::Workspace;

/// Hands out three stored conversations and records each process start.
#[derive(Clone, Default)]
pub struct Stored(pub Rc<RefCell<Vec<SessionSpec>>>);

impl AgentBackend for Stored {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session> {
        self.0.borrow_mut().push(spec);
        let (events_tx, events) = async_channel::unbounded();
        let (commands, commands_rx) = async_channel::unbounded();
        Ok(Session {
            pid: None,
            events,
            commands,
            driver: Box::pin(hold(events_tx, commands_rx)),
        })
    }

    fn sign_in(&self, _: Vec<(String, String)>) -> anyhow::Result<SignIn> {
        anyhow::bail!("no sign-in in workspace tests")
    }

    fn past_sessions(&self, _: &Path) -> Pending<Vec<PastSession>> {
        let now = SystemTime::now();
        Box::pin(async move {
            Ok(vec![
                session("s-new", "Fix the build", now - Duration::from_mins(2)),
                session("s-old", "Plan the release", now - Duration::from_hours(72)),
                // Named by the CLI, not after its first message.
                session("s-named", "Ship v2", now - Duration::from_hours(76)),
                PastSession {
                    open_elsewhere: true,
                    ..session("s-busy", "Review the PR", now - Duration::from_hours(80))
                },
            ])
        })
    }

    fn past_messages(&self, _: &Path, id: &str) -> Pending<Vec<PastMessage>> {
        let title = match id {
            "s-new" => "Fix the build",
            "s-busy" => "Review the PR",
            "s-named" => "Start the work on the next version",
            _ => "Plan the release",
        };
        Box::pin(async move { Ok(vec![PastMessage::User(title.into())]) })
    }
}

/// Holds both test ends open for the session's length, and switches modes
/// as the agent does when asked.
async fn hold(
    events: async_channel::Sender<vortex_agent::AgentEvent>,
    commands: async_channel::Receiver<vortex_agent::SessionCommand>,
) {
    while let Ok(command) = commands.recv().await {
        if let vortex_agent::SessionCommand::SetMode(mode) = command {
            let _ = events
                .send(vortex_agent::AgentEvent::ModeChanged(mode))
                .await;
        }
    }
}

/// No added accounts: chats run with the usual sign-in.
pub fn no_accounts() -> Accounts {
    Accounts::empty(std::env::temp_dir().join("vortex-session-tests"))
}

fn session(id: &str, title: &str, updated_at: SystemTime) -> PastSession {
    PastSession {
        id: id.into(),
        title: title.into(),
        updated_at,
        open_elsewhere: false,
        agent: "claude",
    }
}

/// A Vortex window started in `folder`, if given, that keeps its chat layout
/// in `layouts`.
pub fn open_in(
    cx: &mut TestAppContext,
    backend: impl AgentBackend + 'static,
    layouts: Option<PathBuf>,
    accounts: Accounts,
    folder: Option<PathBuf>,
) -> AnyWindowHandle {
    cx.update(gpui_kit::init);
    cx.update(vortex_workspace::init);
    let (window, _) = cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                gpui_kit::Point::default(),
                size(px(1280.), px(800.)),
            ))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let accounts = cx.new(|_| accounts);
            cx.new(|cx| {
                let workspace = Workspace::new(folder, Rc::new(backend), accounts, window, cx);
                match layouts {
                    Some(path) => workspace.keeping_layouts_in(path),
                    None => workspace,
                }
            })
        })
        .expect("open test window")
    });
    cx.run_until_parked();
    window
}

pub fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) {
    cx.update_window(window, |_, window, cx| window.click(id, cx))
        .expect("window");
    cx.run_until_parked();
}

pub fn labels(cx: &mut TestAppContext, window: AnyWindowHandle, label: &'static str) -> usize {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.find_all_by_label(label).len()
    })
    .expect("window")
}

pub fn spawned(backend: &Stored) -> usize {
    backend.0.borrow().len()
}

pub fn chats(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
) -> Vec<gpui_kit::Bounds<gpui_kit::Pixels>> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .find_all("chat")
            .iter()
            .map(gpui_kit::test::ElementSnapshot::bounds)
            .collect()
    })
    .expect("window")
}

/// Opens `id` from the conversations page of the chat whose name is
/// rightmost, the one split off last.
pub fn pick(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) {
    let title = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let titles = window.find_all("chat-title");
            let rightmost = titles
                .iter()
                .max_by(|a, b| {
                    f32::from(a.bounds().origin.x).total_cmp(&f32::from(b.bounds().origin.x))
                })
                .expect("a chat");
            rightmost.bounds().center()
        })
        .expect("window");
    cx.update_window(window, |_, window, cx| window.drag(title, title, cx))
        .expect("window");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.click(
            gpui_kit::ElementId::Name(format!("conversation-{id}").into()),
            cx,
        );
    })
    .expect("window");
    cx.run_until_parked();
}
