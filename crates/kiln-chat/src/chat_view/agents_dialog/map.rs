//! The agent map: the conversation on the left and, to the right of each
//! agent, the subagents it started.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Div, Hsla, Rems, SharedString, TestSupportExt,
    assets::IconName as Lucide,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    div, px, rems,
};

use super::{
    AgentsDialog, ChatView, Page,
    state::{State, main_state},
};
use crate::{
    scale::{ScaledButton as _, ScaledText as _},
    subagent_view::{self, below_label},
    transcript::{Subagent, Transcript},
};

const NODE_WIDTH: Rems = rems(16.);
const NODE_HEIGHT: f32 = 3.25;
/// Above and below each node, so the lines between siblings meet.
const NODE_GAP: f32 = 0.375;
/// Where a node's line comes in, down from the top of its place.
const LINE_TOP: Rems = rems(NODE_GAP + NODE_HEIGHT / 2.);
const LINE_LENGTH: Rems = rems(1.25);

pub fn render(chat: &ChatView, cx: &Context<AgentsDialog>) -> AnyElement {
    let theme = cx.theme();
    let agents = chat.transcript.subagents();
    let header = h_flex()
        .flex_none()
        .gap_1()
        .p_2()
        .border_b_1()
        .border_color(theme.border)
        .child(
            Button::new("chat-agents-list-show")
                .ghost()
                .row_labelled(Lucide::ArrowLeft, "Subagents")
                .tooltip("Back to the List")
                .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
        )
        .child("Agent Map")
        .child(
            div()
                .text_caption()
                .text_color(theme.muted_foreground)
                .children(below_label(agents.len(), agents.running())),
        );
    let tree = h_flex()
        .flex_none()
        .items_start()
        .p_3()
        .child(main_node(chat, cx))
        .children(children(None, &chat.transcript, cx));
    v_flex()
        .size_full()
        .child(header)
        .child(
            div()
                .id("chat-agents-map")
                .test_support()
                .flex_1()
                .min_h_0()
                .flex()
                .items_start()
                .overflow_scroll()
                .child(tree),
        )
        .into_any_element()
}

fn main_node(chat: &ChatView, cx: &Context<AgentsDialog>) -> impl IntoElement + use<> {
    let agents = chat.transcript.subagents();
    let state = main_state(chat);
    let title = SharedString::from(chat.title().unwrap_or("This conversation").to_owned());
    let caption: Vec<String> = std::iter::once(state.label().to_owned())
        .chain(below_label(agents.len(), agents.running()))
        .collect();
    let body = words(title.clone(), caption.join(" · "), cx);
    node(
        "chat-agents-map-main".into(),
        title,
        body,
        border(state, cx),
    )
    .on_click(cx.listener(|this, _, window, cx| this.go(Page::Card(None), window, cx)))
}

/// Only what may need the user stands out.
fn border(state: State, cx: &Context<AgentsDialog>) -> Hsla {
    match state {
        State::Working | State::Waiting | State::Failed => state.color(cx),
        State::Stopped | State::Finished | State::Idle => cx.theme().border,
    }
}

fn agent_node(agent: &Subagent, transcript: &Transcript, cx: &Context<AgentsDialog>) -> Button {
    let id = agent.id;
    let state = State::of(agent, transcript);
    let name = SharedString::from(agent.description.clone());
    let status = match state {
        State::Waiting => "Waiting for your permission".to_owned(),
        _ => subagent_view::status(agent),
    };
    let body = h_flex()
        .w_full()
        .items_start()
        .gap_2()
        .child(div().pt_1p5().child(state.dot(cx)))
        .child(words(name.clone(), status.clone(), cx));
    node(
        format!("chat-agents-map-{id}").into(),
        name,
        body,
        border(state, cx),
    )
    .tooltip(status)
    .on_click(cx.listener(move |this, _, window, cx| {
        this.go(Page::Card(Some(id)), window, cx);
    }))
}

fn words(name: SharedString, caption: String, cx: &Context<AgentsDialog>) -> Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .child(div().truncate().child(name))
        .child(
            div()
                .truncate()
                .text_caption()
                .text_color(cx.theme().muted_foreground)
                .child(caption),
        )
}

fn node(id: SharedString, name: SharedString, body: Div, color: Hsla) -> Button {
    Button::new(id)
        .outline()
        .flex_none()
        .control()
        .w(NODE_WIDTH)
        .h(rems(NODE_HEIGHT))
        .my(rems(NODE_GAP))
        .border_color(color)
        .accessibility_label(name)
        .child(body.w_full())
}

/// What `parent` started, each beside its own, with the lines to them.
fn children(
    parent: Option<u64>,
    transcript: &Transcript,
    cx: &Context<AgentsDialog>,
) -> Option<AnyElement> {
    let started: Vec<&Subagent> = transcript.subagents().children(parent).collect();
    let last = started.len().checked_sub(1)?;
    let line = cx.theme().muted_foreground.opacity(0.5);
    let rows = started.iter().enumerate().map(|(index, agent)| {
        h_flex()
            .flex_none()
            .relative()
            .items_start()
            .pl(LINE_LENGTH)
            .child(across(line).absolute().left_0().top(LINE_TOP))
            .children(down(index, last, line))
            .child(agent_node(agent, transcript, cx))
            .children(children(Some(agent.id), transcript, cx))
    });
    Some(
        h_flex()
            .flex_none()
            .items_start()
            .child(across(line).flex_none().mt(LINE_TOP))
            .child(v_flex().flex_none().children(rows))
            .into_any_element(),
    )
}

fn across(color: Hsla) -> Div {
    div().w(LINE_LENGTH).h(px(1.)).bg(color)
}

/// The line siblings hang from, which starts at the first and ends at the
/// last; `None` for an only one.
fn down(index: usize, last: usize, color: Hsla) -> Option<Div> {
    let line = div().absolute().left_0().w(px(1.)).bg(color);
    match index {
        0 if last == 0 => None,
        0 => Some(line.top(LINE_TOP).bottom_0()),
        _ if index == last => Some(line.top_0().h(LINE_TOP)),
        _ => Some(line.top_0().bottom_0()),
    }
}
