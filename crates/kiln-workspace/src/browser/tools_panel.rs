//! The panel under the page for looking into it: what its console says,
//! the requests it makes, what it keeps, and what it costs.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, SharedString, TestSupportExt,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        tab::{Tab, TabBar},
        v_flex,
    },
    px,
};

use super::Browser;

const HEIGHT: f32 = 260.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Console,
    Network,
    Storage,
    Usage,
}

const TOOLS: [Tool; 4] = [Tool::Console, Tool::Network, Tool::Storage, Tool::Usage];

impl Tool {
    fn title(self) -> &'static str {
        match self {
            Self::Console => "Console",
            Self::Network => "Network",
            Self::Storage => "Storage",
            Self::Usage => "Usage",
        }
    }
}

#[derive(Default)]
pub(super) struct Tools {
    pub open: bool,
    pub tool: Tool,
}

impl Browser {
    /// Whether `tool` is in sight, so what it shows is worth keeping fresh.
    pub(super) fn showing_tool(&self, tool: Tool) -> bool {
        self.tools.open && self.tools.tool == tool
    }

    pub(super) fn tools_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let errors = self.console.errors().count();
        let tooltip = match errors {
            0 => "Developer Tools".to_owned(),
            1 => "Developer Tools, 1 error".to_owned(),
            count => format!("Developer Tools, {count} errors"),
        };
        Button::new("browser-console")
            .ghost()
            .small()
            .icon(Lucide::Terminal)
            .tooltip(tooltip)
            .selected(self.tools.open)
            .when(errors > 0, |button| {
                button.label(SharedString::from(errors.to_string()))
            })
            .when(errors > 0, |button| button.text_color(cx.theme().danger))
            .on_click(cx.listener(|this, _, _, cx| {
                let open = !this.tools.open;
                this.set_tools_open(open, cx);
            }))
    }

    fn set_tools_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.tools.open = open;
        if open {
            self.tool_shown(cx);
        }
        cx.notify();
    }

    fn pick_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        self.tools.tool = tool;
        self.tool_shown(cx);
        cx.notify();
    }

    /// What a tool shows is read when it comes into sight.
    fn tool_shown(&mut self, cx: &mut Context<Self>) {
        match self.tools.tool {
            Tool::Storage => self.read_storage(cx),
            Tool::Usage => self.watch_usage(cx),
            Tool::Console | Tool::Network => {}
        }
    }

    pub(super) fn render_tools(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.tools.open {
            return None;
        }
        let theme = cx.theme();
        let picked = TOOLS.iter().position(|tool| *tool == self.tools.tool);
        let tabs = TabBar::new("browser-tools")
            .underline()
            .small()
            .when_some(picked, TabBar::selected_index)
            .on_click(cx.listener(|this, at: &usize, _, cx| {
                if let Some(tool) = TOOLS.get(*at) {
                    this.pick_tool(*tool, cx);
                }
            }))
            .children(TOOLS.map(|tool| Tab::new().label(tool.title())))
            .suffix(
                h_flex()
                    .gap_1()
                    .px_2()
                    .child(self.tool_actions(cx))
                    .child(close_button(cx)),
            );
        let body = match self.tools.tool {
            Tool::Console => self.render_console(cx),
            Tool::Network => self.render_network(cx),
            Tool::Storage => self.render_storage(cx),
            Tool::Usage => self.render_usage(cx),
        };
        let panel = v_flex()
            .id("browser-tools-panel")
            .test_support()
            .flex_none()
            .h(px(HEIGHT))
            .border_t_1()
            .border_color(theme.border)
            .child(tabs)
            .child(v_flex().flex_1().min_h_0().child(body));
        Some(panel.into_any_element())
    }

    fn tool_actions(&self, cx: &Context<Self>) -> AnyElement {
        match self.tools.tool {
            Tool::Console => self.console_actions(cx).into_any_element(),
            Tool::Network => self.network_actions(cx).into_any_element(),
            Tool::Storage => Self::storage_actions(cx).into_any_element(),
            Tool::Usage => h_flex().into_any_element(),
        }
    }
}

fn close_button(cx: &Context<Browser>) -> impl IntoElement + use<> {
    Button::new("browser-tools-close")
        .ghost()
        .xsmall()
        .icon(IconName::Close)
        .tooltip("Close Developer Tools")
        .on_click(cx.listener(|this, _, _, cx| this.set_tools_open(false, cx)))
}
