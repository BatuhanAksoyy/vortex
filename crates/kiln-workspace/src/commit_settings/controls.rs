//! The page's inputs and pickers, and how each one saves: a picker or
//! switch at once, a text when the user leaves it.

use gpui_kit::{
    App, AppContext as _, Context, Entity, SharedString, Subscription, Window,
    component::{
        input::{InputEvent, InputState, TextareaState},
        select::{SearchableVec, SelectEvent, SelectItem, SelectState},
    },
};
use serde_json::Value;

use super::{
    AGENT, CommitSettingsPage, DIRECTIVES, FORBID, Field, MODEL, PUSH, REQUIRE, SUBJECT_MAX,
    TRIGGER,
};

const ROWS: (usize, usize) = (2, 6);

/// A value of a setting with a fixed set, under the name the user reads.
#[derive(Clone)]
pub(crate) struct Choice {
    value: SharedString,
    title: SharedString,
}

impl SelectItem for Choice {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &SharedString {
        &self.value
    }
}

fn choices(all: &[(&str, &str)]) -> Vec<Choice> {
    let choice = |(value, title): &(&str, &str)| Choice {
        value: (*value).to_owned().into(),
        title: (*title).to_owned().into(),
    };
    all.iter().map(choice).collect()
}

type ChoiceSelect = SelectState<Vec<Choice>>;
type NameSelect = SelectState<SearchableVec<SharedString>>;

/// What the controls show for the tab's scope.
pub(super) struct Shown {
    pub trigger: SharedString,
    pub push: SharedString,
    pub agent: SharedString,
    pub model: SharedString,
    pub directives: String,
    pub require: String,
    pub forbid: String,
    pub subject_max: String,
}

pub(crate) struct Controls {
    pub trigger: Entity<ChoiceSelect>,
    pub push: Entity<ChoiceSelect>,
    pub agent: Entity<NameSelect>,
    pub model: Entity<NameSelect>,
    /// What the two pickers above offer.
    agents: Vec<SharedString>,
    models: Vec<SharedString>,
    pub directives: Entity<TextareaState>,
    pub require: Entity<TextareaState>,
    pub forbid: Entity<TextareaState>,
    pub subject_max: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl Controls {
    pub(super) fn new(
        agents: Vec<String>,
        models: Vec<String>,
        window: &mut Window,
        cx: &mut Context<CommitSettingsPage>,
    ) -> Self {
        let trigger = choices(&[
            ("agent", "When asked"),
            ("task_done", "When the work is done"),
        ]);
        let push = choices(&[
            ("never", "Never"),
            ("manual", "By hand"),
            ("after_commit", "After each commit"),
            ("session_end", "When the chat closes"),
        ]);
        let agents: Vec<SharedString> = agents.into_iter().map(Into::into).collect();
        let models: Vec<SharedString> = models.into_iter().map(Into::into).collect();
        let names = |all: &[SharedString]| SearchableVec::new(all.to_vec());
        let trigger = cx.new(|cx| SelectState::new(trigger, None, window, cx));
        let push = cx.new(|cx| SelectState::new(push, None, window, cx));
        let agent = cx.new(|cx| SelectState::new(names(&agents), None, window, cx));
        let model =
            cx.new(|cx| SelectState::new(names(&models), None, window, cx).searchable(true));
        let [directives, require, forbid] = [
            "One per line, such as: Write in English",
            "Regular expressions, one per line",
            "Regular expressions, one per line",
        ]
        .map(|placeholder| text_lines(placeholder, window, cx));
        let subject_max = cx.new(|cx| InputState::new(window, cx).placeholder("0 for no limit"));
        let subscriptions = vec![
            on_pick(&trigger, &TRIGGER, window, cx),
            on_pick(&push, &PUSH, window, cx),
            on_pick(&agent, &AGENT, window, cx),
            on_pick(&model, &MODEL, window, cx),
            on_leave(&directives, &DIRECTIVES, window, cx),
            on_leave(&require, &REQUIRE, window, cx),
            on_leave(&forbid, &FORBID, window, cx),
            cx.subscribe_in(&subject_max, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    this.save_subject_max(window, cx);
                }
            }),
        ];
        Self {
            trigger,
            push,
            agent,
            model,
            agents,
            models,
            directives,
            require,
            forbid,
            subject_max,
            _subscriptions: subscriptions,
        }
    }

    pub(super) fn show(&self, shown: &Shown, window: &mut Window, cx: &mut App) {
        for (select, value) in [(&self.trigger, &shown.trigger), (&self.push, &shown.push)] {
            select.update(cx, |select, cx| {
                select.set_selected_value(value, window, cx);
            });
        }
        set_name(&self.agent, &self.agents, &shown.agent, window, cx);
        set_name(&self.model, &self.models, &shown.model, window, cx);
        for (input, text) in [
            (&self.directives, &shown.directives),
            (&self.require, &shown.require),
            (&self.forbid, &shown.forbid),
        ] {
            input.update(cx, |input, cx| input.set_value(text.clone(), window, cx));
        }
        let limit = shown.subject_max.clone();
        self.subject_max
            .update(cx, |input, cx| input.set_value(limit, window, cx));
    }
}

fn text_lines(
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<CommitSettingsPage>,
) -> Entity<TextareaState> {
    cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(ROWS.0, ROWS.1)
            .placeholder(placeholder)
    })
}

/// Saves what is picked in `select` as `field`.
fn on_pick<I>(
    select: &Entity<SelectState<I>>,
    field: &'static Field,
    window: &mut Window,
    cx: &mut Context<CommitSettingsPage>,
) -> Subscription
where
    I: gpui_kit::component::select::SelectDelegate,
    I::Item: SelectItem<Value = SharedString>,
{
    cx.subscribe_in(
        select,
        window,
        move |this, _, event: &SelectEvent<I>, window, cx| {
            if let SelectEvent::Confirm(Some(value)) = event {
                this.save(field, Some(Value::String(value.to_string())), window, cx);
            }
        },
    )
}

/// Saves the lines of `input` as `field` once the user leaves it.
fn on_leave(
    input: &Entity<TextareaState>,
    field: &'static Field,
    window: &mut Window,
    cx: &mut Context<CommitSettingsPage>,
) -> Subscription {
    cx.subscribe_in(input, window, move |this, input, event, window, cx| {
        if matches!(event, InputEvent::Blur) {
            let text = input.read(cx).value();
            this.save_lines(field, &text, window, cx);
        }
    })
}

impl CommitSettingsPage {
    fn save_lines(
        &mut self,
        field: &Field,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let lines: Vec<String> = (text.lines())
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect();
        let patterns = field.key != DIRECTIVES.key;
        if patterns && let Err(error) = kiln_git::MessageRules::new(&lines, 0, false) {
            self.refuse(format!("Not a regular expression: {error}"), cx);
            return;
        }
        self.save_changed(field, Value::from(lines), window, cx);
    }

    fn save_subject_max(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.controls.subject_max.read(cx).value();
        let Ok(limit) = text.trim().parse::<u64>() else {
            self.refuse(
                "Enter a whole number for the first line's limit.".to_owned(),
                cx,
            );
            return;
        };
        self.save_changed(&SUBJECT_MAX, Value::from(limit), window, cx);
    }
}

/// Selects `name` among `offered`, offering it first when they lack it: a
/// model the settings name need not be one the agent listed.
fn set_name(
    select: &Entity<NameSelect>,
    offered: &[SharedString],
    name: &SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    let mut items = offered.to_vec();
    if !name.is_empty() && !items.contains(name) {
        items.insert(0, name.clone());
    }
    select.update(cx, |select, cx| {
        select.set_items(SearchableVec::new(items), window, cx);
        select.set_selected_value(name, window, cx);
    });
}
