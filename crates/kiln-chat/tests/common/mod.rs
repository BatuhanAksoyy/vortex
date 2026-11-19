//! A chat panel in a test window, driven through a scripted backend.

use std::{cell::RefCell, rc::Rc};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions, px,
    size,
};
use kiln_agent::{AgentBackend, AgentEvent, Session, SessionCommand, SessionSpec};
use kiln_chat::{ChatView, Status};

/// The test's ends of the most recently spawned session.
#[derive(Default)]
pub struct Script {
    pub events: Option<async_channel::Sender<AgentEvent>>,
    pub commands: Option<async_channel::Receiver<SessionCommand>>,
    pub spawned: usize,
    pub last_spec: Option<SessionSpec>,
}

pub struct FakeBackend(Rc<RefCell<Script>>);

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
}

pub struct Harness {
    pub window: AnyWindowHandle,
    pub view: Entity<ChatView>,
    pub script: Rc<RefCell<Script>>,
}

impl Harness {
    pub fn new(cx: &mut TestAppContext) -> Self {
        cx.update(gpui_kit::init);
        let script = Rc::new(RefCell::new(Script::default()));
        let backend = Rc::new(FakeBackend(script.clone()));
        let (window, view) = cx.update(|cx| open(backend, cx));
        cx.run_until_parked();
        Self {
            window,
            view,
            script,
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

fn open(backend: Rc<FakeBackend>, cx: &mut gpui_kit::App) -> (AnyWindowHandle, Entity<ChatView>) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            gpui_kit::Point::default(),
            size(px(800.), px(700.)),
        ))),
        ..Default::default()
    };
    let build = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
        cx.new(|cx| ChatView::new(backend, std::env::temp_dir(), window, cx))
    };
    gpui_kit::open_window(options, cx, build).expect("open test window")
}
