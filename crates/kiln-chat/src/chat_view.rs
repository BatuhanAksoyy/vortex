use std::{collections::HashSet, path::PathBuf, rc::Rc};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, FocusHandle, Focusable, Subscription, Task, TestSupportExt, Window,
    component::{
        ActiveTheme, IconName, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        input::{InputEvent, Textarea, TextareaState},
        message_scroller::{MessageScroller, MessageScrollerState},
        v_flex,
    },
    div,
};
use kiln_agent::{AgentBackend, AgentEvent, Decision, SessionCommand, SessionSpec};

use crate::{
    entry_view::{self, EntryContext},
    transcript::{Change, Status, Transcript},
};

const COMPOSER_MAX_ROWS: usize = 10;

/// A running agent process. Dropping it stops the process.
struct LiveSession {
    commands: async_channel::Sender<SessionCommand>,
    _driver: Task<()>,
    _events: Task<()>,
}

pub struct ChatView {
    backend: Rc<dyn AgentBackend>,
    cwd: PathBuf,
    transcript: Transcript,
    expanded: HashSet<u64>,
    session: Option<LiveSession>,
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}

impl ChatView {
    pub fn new(
        backend: Rc<dyn AgentBackend>,
        cwd: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .auto_grow(1, COMPOSER_MAX_ROWS)
                .placeholder("Message Claude…");
            state.set_submit_on_enter(true, cx);
            state
        });
        let subscription = cx.subscribe_in(&composer, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.send(window, cx);
            }
        });
        Self {
            backend,
            cwd,
            transcript: Transcript::default(),
            expanded: HashSet::new(),
            session: None,
            scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
            composer,
            _subscriptions: vec![subscription],
        }
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.transcript.status()
    }

    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().trim().to_owned();
        if text.is_empty() || self.status() == Status::Running {
            return;
        }
        let Some(commands) = self.live_commands(cx) else {
            return;
        };
        let change = self.transcript.push_user(text.clone());
        self.apply_change(change, cx);
        let _ = commands.try_send(SessionCommand::Send(text));
        self.composer
            .update(cx, |composer, cx| composer.clean(window, cx));
    }

    pub fn interrupt(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            let _ = session.commands.try_send(SessionCommand::Interrupt);
        }
        cx.notify();
    }

    pub fn decide(&mut self, request_id: &str, decision: Decision, cx: &mut Context<Self>) {
        if !self.transcript.decide(request_id, decision) {
            return;
        }
        if let Some(session) = &self.session {
            let request_id = request_id.to_owned();
            let _ = session.commands.try_send(SessionCommand::Permission {
                request_id,
                decision,
            });
        }
        let updated = self.transcript.index_of_permission(request_id);
        self.apply_change(
            Change {
                updated,
                appended: 0,
            },
            cx,
        );
    }

    pub fn toggle_expanded(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.scroller.update(cx, MessageScrollerState::remeasure);
    }

    /// The running session's command channel, starting a session if needed.
    fn live_commands(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<async_channel::Sender<SessionCommand>> {
        if self.session.is_none() {
            match self.start_session(cx) {
                Ok(session) => self.session = Some(session),
                Err(error) => {
                    let change = self.transcript.apply(AgentEvent::Exited {
                        error: Some(format!("{error:#}")),
                    });
                    self.apply_change(change, cx);
                    return None;
                }
            }
        }
        self.session
            .as_ref()
            .map(|session| session.commands.clone())
    }

    fn start_session(&self, cx: &mut Context<Self>) -> anyhow::Result<LiveSession> {
        let session = self.backend.spawn(SessionSpec {
            cwd: self.cwd.clone(),
            env: Vec::new(),
        })?;
        let events = session.events;
        let events_task = cx.spawn(async move |this, cx| {
            while let Ok(event) = events.recv().await
                && this.update(cx, |this, cx| this.on_event(event, cx)).is_ok()
            {}
        });
        Ok(LiveSession {
            commands: session.commands,
            _driver: cx.background_spawn(session.driver),
            _events: events_task,
        })
    }

    fn on_event(&mut self, event: AgentEvent, cx: &mut Context<Self>) {
        let exited = matches!(event, AgentEvent::Exited { .. });
        let change = self.transcript.apply(event);
        self.apply_change(change, cx);
        if exited {
            self.session = None;
        }
    }

    fn apply_change(&mut self, change: Change, cx: &mut Context<Self>) {
        self.scroller.update(cx, |scroller, cx| {
            if let Some(index) = change.updated {
                scroller.remeasure_items(index..index + 1, cx);
            }
            if change.appended > 0 {
                scroller.append(change.appended, cx);
            }
        });
        cx.notify();
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        let status = match self.status() {
            Status::Idle if self.session.is_some() => "Ready",
            Status::Idle => "Not started",
            Status::Running => "Working…",
            Status::Ended => "Ended",
        };
        h_flex()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().child("Claude"))
            .when_some(self.transcript.model(), |this, model| {
                this.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(model.to_owned()),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .id("chat-status")
                    .test_support()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
    }

    fn render_transcript(&self, cx: &Context<Self>) -> impl IntoElement {
        if self.transcript.len() == 0 {
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Ask Claude about this project.")
                .into_any_element();
        }
        let view = cx.entity().downgrade();
        let row_view = view.clone();
        MessageScroller::new(
            "chat-transcript",
            self.scroller.clone(),
            move |index, _, cx| {
                row_view
                    .upgrade()
                    .and_then(|view| {
                        let this = view.read(cx);
                        let (id, entry) = this.transcript.entry(index)?;
                        let context = EntryContext {
                            id,
                            expanded: this.expanded.contains(&id),
                            view: view.downgrade(),
                        };
                        Some(entry_view::render(entry, context, cx))
                    })
                    .unwrap_or_else(|| div().into_any_element())
            },
        )
        .flex_1()
        .px_3()
        .into_any_element()
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.status() == Status::Running;
        let action = if running {
            Button::new("chat-stop")
                .icon(IconName::Square)
                .small()
                .tooltip("Stop")
                .on_click(cx.listener(|this, _, window, cx| this.interrupt(window, cx)))
        } else {
            Button::new("chat-send")
                .icon(IconName::ArrowUp)
                .primary()
                .small()
                .tooltip("Send")
                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
        };
        h_flex()
            .items_end()
            .gap_2()
            .p_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .id("chat-composer")
                    .test_support()
                    .flex_1()
                    .child(Textarea::new(&self.composer)),
            )
            .child(action)
    }
}

impl Focusable for ChatView {
    fn focus_handle(&self, cx: &gpui_kit::App) -> FocusHandle {
        self.composer.read(cx).focus_handle(cx)
    }
}

impl Render for ChatView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("chat")
            .test_support()
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_header(cx))
            .child(self.render_transcript(cx))
            .child(self.render_composer(cx))
    }
}
