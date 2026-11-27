//! The scripted backend under Codex's name, and a chat that offers both
//! harnesses.

use std::{cell::RefCell, path::Path, rc::Rc};

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt as _;
use kiln_agent::{
    AgentBackend, Features, PastMessage, PastSession, Pending, Session, SessionSpec, SignIn,
};

use super::common::{FakeBackend, Harness, Script};

/// The scripted backend under Codex's name.
pub struct Codex(FakeBackend);

impl AgentBackend for Codex {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn features(&self) -> Features {
        Features {
            skills_and_hooks: false,
            prompt_cache: false,
        }
    }

    fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session> {
        self.0.spawn(spec)
    }

    fn past_sessions(&self, cwd: &Path) -> Pending<Vec<PastSession>> {
        self.0.past_sessions(cwd)
    }

    fn past_messages(&self, cwd: &Path, id: &str) -> Pending<Vec<PastMessage>> {
        self.0.past_messages(cwd, id)
    }

    fn sign_in(&self, env: Vec<(String, String)>) -> anyhow::Result<SignIn> {
        self.0.sign_in(env)
    }
}

/// A chat that also offers Codex, and the script of Codex's sessions.
pub fn offering_codex(cx: &mut TestAppContext) -> (Harness, Rc<RefCell<Script>>) {
    let codex = Rc::new(RefCell::new(Script::default()));
    let backend: Rc<dyn AgentBackend> = Rc::new(Codex(FakeBackend(codex.clone())));
    (Harness::with(cx, Some(backend)), codex)
}

pub fn shown(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> bool {
    harness.with_window(cx, move |window, _| window.try_find(id).is_some())
}

pub fn agent(harness: &Harness, cx: &mut TestAppContext) -> &'static str {
    cx.update(|cx| harness.view.read(cx).agent_name())
}

pub fn click(harness: &Harness, cx: &mut TestAppContext, id: &'static str) {
    harness.with_window(cx, |window, cx| window.click(id, cx));
}
