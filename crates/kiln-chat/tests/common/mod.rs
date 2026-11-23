//! A chat panel in a test window, driven through a scripted backend.

use std::{cell::RefCell, path::Path, rc::Rc};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions, px,
    size,
};
use kiln_accounts::Accounts;
use kiln_agent::{
    Account, AgentBackend, AgentEvent, PastMessage, PastSession, Pending, Session, SessionCommand,
    SessionSpec, SignIn,
};
use kiln_chat::{ChatView, Status};

/// The test's ends of the most recently spawned session.
#[derive(Default)]
pub struct Script {
    pub events: Option<async_channel::Sender<AgentEvent>>,
    pub commands: Option<async_channel::Receiver<SessionCommand>>,
    pub spawned: usize,
    pub last_spec: Option<SessionSpec>,
    pub sign_in: Option<ScriptedSignIn>,
    /// Stored conversations, by id, that `past_messages` hands out.
    pub stored: Vec<(String, Vec<PastMessage>)>,
}

/// The test's ends of the latest sign-in: the page to send, the result to
/// finish it with, and the environment it was started in.
pub type ScriptedSignIn = (
    async_channel::Sender<String>,
    async_channel::Sender<anyhow::Result<Account>>,
    Vec<(String, String)>,
);

pub struct FakeBackend(pub Rc<RefCell<Script>>);

impl AgentBackend for FakeBackend {
    fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session> {
        let (event_tx, events) = async_channel::unbounded();
        let (commands, command_rx) = async_channel::unbounded();
        let mut script = self.0.borrow_mut();
        script.events = Some(event_tx);
        script.commands = Some(command_rx);
        script.spawned += 1;
        script.last_spec = Some(spec);
        Ok(Session {
            events,
            commands,
            driver: Box::pin(async {}),
        })
    }

    fn past_sessions(&self, _: &Path) -> Pending<Vec<PastSession>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn past_messages(&self, _: &Path, session_id: &str) -> Pending<Vec<PastMessage>> {
        let script = self.0.borrow();
        let stored = script
            .stored
            .iter()
            .find(|(id, _)| id == session_id)
            .map(|(_, messages)| messages.clone());
        Box::pin(async move { stored.ok_or_else(|| anyhow::anyhow!("no stored conversation")) })
    }

    fn sign_in(&self, env: Vec<(String, String)>) -> anyhow::Result<SignIn> {
        let (url_tx, url) = async_channel::unbounded();
        let (done_tx, done) = async_channel::unbounded();
        self.0.borrow_mut().sign_in = Some((url_tx, done_tx, env));
        Ok(SignIn {
            url,
            done: Box::pin(async move {
                done.recv()
                    .await
                    .unwrap_or_else(|_| Err(anyhow::anyhow!("the test dropped the sign-in")))
            }),
        })
    }
}

pub struct Harness {
    pub window: AnyWindowHandle,
    pub view: Entity<ChatView>,
    pub script: Rc<RefCell<Script>>,
    /// Holds the account list and credential stores for the test's length.
    _data: tempfile::TempDir,
}

impl Harness {
    pub fn new(cx: &mut TestAppContext) -> Self {
        cx.update(gpui_kit::init);
        let script = Rc::new(RefCell::new(Script::default()));
        let backend = Rc::new(FakeBackend(script.clone()));
        let data = tempfile::tempdir().expect("temp dir");
        let accounts = cx
            .update(|cx| cx.new(|_| Accounts::load(data.path().to_owned()).expect("account list")));
        let (window, view) = cx.update(|cx| open(backend, accounts, cx));
        cx.run_until_parked();
        Self {
            window,
            view,
            script,
            _data: data,
        }
    }

    pub fn emit(&self, cx: &mut TestAppContext, event: AgentEvent) {
        let sender = self
            .script
            .borrow()
            .events
            .clone()
            .expect("session started");
        sender.try_send(event).expect("send event");
        cx.run_until_parked();
    }

    pub fn commands(&self) -> Vec<SessionCommand> {
        let script = self.script.borrow();
        let receiver = script.commands.as_ref().expect("session started");
        std::iter::from_fn(|| receiver.try_recv().ok()).collect()
    }

    pub fn with_window<R>(
        &self,
        cx: &mut TestAppContext,
        f: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::App) -> R,
    ) -> R {
        let result = cx
            .update_window(self.window, |_, window, cx| {
                window.render_frame(cx);
                f(window, cx)
            })
            .expect("window");
        cx.run_until_parked();
        result
    }

    pub fn type_and_send(&self, cx: &mut TestAppContext, text: &str) {
        self.with_window(cx, |window, cx| window.click("chat-composer", cx));
        self.with_window(cx, |window, cx| window.input(text, cx));
        self.with_window(cx, |window, cx| window.press("enter", cx));
    }

    pub fn status(&self, cx: &mut TestAppContext) -> Status {
        cx.update(|cx| self.view.read(cx).status())
    }
}

/// A chat in a window of its own.
pub fn open(
    backend: Rc<FakeBackend>,
    accounts: Entity<Accounts>,
    cx: &mut gpui_kit::App,
) -> (AnyWindowHandle, Entity<ChatView>) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            gpui_kit::Point::default(),
            size(px(800.), px(700.)),
        ))),
        ..Default::default()
    };
    let build = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
        cx.new(|cx| ChatView::new(backend, accounts, std::env::temp_dir(), window, cx))
    };
    gpui_kit::open_window(options, cx, build).expect("open test window")
}
