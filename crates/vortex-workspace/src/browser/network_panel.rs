//! The page's requests, and what came back for the one chosen.

use std::ops::Range;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, SharedString, Task, TestSupportExt, UniformListScrollHandle,
    component::{
        ActiveTheme, IconName, Sizable as _, Theme,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    div, px, uniform_list,
};
use vortex_browser::{Body, NetworkEvent, NetworkLog, NetworkRequest};
use vortex_chat::{
    format_memory,
    scale::{ScaledButton as _, ScaledText as _},
};

use super::{Browser, tools_panel::Tool};

const STATUS_WIDTH: f32 = 52.;
const KIND_WIDTH: f32 = 72.;
const SIZE_WIDTH: f32 = 68.;
const TIME_WIDTH: f32 = 64.;

#[derive(Default)]
pub(super) struct Network {
    log: NetworkLog,
    chosen: Option<String>,
    /// What came back for the request chosen; `None` while it is read.
    body: Option<Result<Body, String>>,
    reading: Option<Task<()>>,
    scroll: UniformListScrollHandle,
}

impl Browser {
    pub(super) fn on_network(&mut self, event: NetworkEvent, cx: &mut Context<Self>) {
        self.network.log.apply(event);
        if self.showing_tool(Tool::Network) {
            cx.notify();
        }
    }

    /// A new document makes new requests.
    pub(super) fn clear_network(&mut self) {
        self.network.log.clear();
        self.network.chosen = None;
        self.network.body = None;
        self.network.reading = None;
    }

    fn choose_request(&mut self, id: String, cx: &mut Context<Self>) {
        self.network.body = None;
        self.network.reading = None;
        self.network.chosen = Some(id.clone());
        let done = self.chosen_request().is_some_and(NetworkRequest::done);
        if let Some(page) = self.page().cloned().filter(|_| done) {
            self.network.reading = Some(cx.spawn(async move |this, cx| {
                let body = page.response_body(&id).await;
                let _ = this.update(cx, |this, cx| this.show_body(body, cx));
            }));
        }
        cx.notify();
    }

    fn show_body(&mut self, body: anyhow::Result<Body>, cx: &mut Context<Self>) {
        self.network.body = Some(body.map_err(|error| format!("{error:#}")));
        cx.notify();
    }

    fn chosen_request(&self) -> Option<&NetworkRequest> {
        let chosen = self.network.chosen.as_ref()?;
        let requests = self.network.log.requests();
        requests.iter().find(|request| request.id == *chosen)
    }

    pub(super) fn network_actions(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let count = self.network.log.requests().len();
        let summary = match count {
            1 => "1 request".to_owned(),
            count => format!("{count} requests"),
        };
        h_flex()
            .gap_2()
            .child(
                div()
                    .text_caption()
                    .text_color(cx.theme().muted_foreground)
                    .child(summary),
            )
            .child(
                Button::new("browser-network-clear")
                    .ghost()
                    .small()
                    .label("Clear")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.clear_network();
                        cx.notify();
                    })),
            )
    }

    pub(super) fn render_network(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let count = self.network.log.requests().len();
        let list = uniform_list(
            "browser-network-rows",
            count,
            cx.processor(|this, range: Range<usize>, _, cx| this.network_rows(range, cx)),
        )
        .track_scroll(&self.network.scroll)
        .flex_1()
        .min_h_0();
        let compact = self.network.chosen.is_some();
        let requests = v_flex()
            .id("browser-network-list")
            .test_support()
            .flex_1()
            .min_w_0()
            .h_full()
            .text_caption()
            .child(heading(compact, theme))
            .when(count == 0, |list| {
                list.child(
                    div()
                        .p_2()
                        .text_color(theme.muted_foreground)
                        .child("No requests yet. Reload the page to see all of them."),
                )
            })
            .child(list);
        h_flex()
            .size_full()
            .items_start()
            .child(requests)
            .children(
                self.chosen_request()
                    .map(|request| self.request_detail(request, cx)),
            )
            .into_any_element()
    }

    fn network_rows(&self, range: Range<usize>, cx: &Context<Self>) -> Vec<AnyElement> {
        let requests = self.network.log.requests();
        let rows = requests.get(range).unwrap_or_default();
        rows.iter()
            .map(|request| self.network_row(request, cx))
            .collect()
    }

    fn network_row(&self, request: &NetworkRequest, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let compact = self.network.chosen.is_some();
        let chosen = self.network.chosen.as_ref() == Some(&request.id);
        let failed = request.failed.is_some()
            || request.response.as_ref().is_some_and(|it| it.status >= 400);
        let id = request.id.clone();
        let hover = theme.list_hover;
        columns(
            h_flex()
                .id(SharedString::from(format!(
                    "browser-request-{}",
                    request.id
                )))
                .test_support()
                .aria_label(SharedString::from(request.url.clone()))
                .w_full()
                .cursor_pointer()
                .hover(move |row| row.bg(hover))
                .when(chosen, |row| row.bg(theme.list_active))
                .when(failed, |row| row.text_color(theme.danger))
                .on_click(cx.listener(move |this, _, _, cx| this.choose_request(id.clone(), cx))),
            [
                request.name().to_owned(),
                status(request),
                request.kind.clone(),
                request.size.map_or_default(format_memory),
                request.duration().map_or_default(time),
            ],
            compact,
        )
        .into_any_element()
    }

    fn request_detail(&self, request: &NetworkRequest, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let response = request.response.as_ref();
        let summary = format!("{} · {}", request.method, status(request));
        let mut parts = vec![
            div().child(request.url.clone()).into_any_element(),
            muted(summary, theme),
        ];
        if let Some(response) = response {
            parts.push(headers("Response Headers", &response.headers, theme));
        }
        parts.push(headers("Request Headers", &request.headers, theme));
        parts.push(section("Response", theme));
        parts.push(self.body_text(request, theme));
        v_flex()
            .id("browser-request-detail")
            .test_support()
            .w_2_3()
            .h_full()
            .border_l_1()
            .border_color(theme.border)
            .child(
                h_flex().justify_end().p_1().child(
                    Button::new("browser-request-close")
                        .ghost()
                        .row_icon(IconName::Close)
                        .tooltip("Close Details")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.network.chosen = None;
                            cx.notify();
                        })),
                ),
            )
            .child(
                v_flex()
                    .id("browser-request-parts")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap_1()
                    .px_2()
                    .pb_2()
                    .text_caption()
                    .children(parts),
            )
            .into_any_element()
    }

    fn body_text(&self, request: &NetworkRequest, theme: &Theme) -> AnyElement {
        let text = match (&self.network.body, request.done()) {
            (_, false) => "Not received yet.".to_owned(),
            (None, true) => "Reading…".to_owned(),
            (Some(Err(error)), true) => format!("Not available: {error}"),
            (Some(Ok(Body::Binary(size))), true) => {
                format!(
                    "{} of binary data.",
                    format_memory(u64::try_from(*size).unwrap_or(0))
                )
            }
            (Some(Ok(Body::Text { text, cut })), true) => {
                let more = if *cut { "\n…" } else { "" };
                return div()
                    .font_family(theme.mono_font_family.clone())
                    .child(format!("{text}{more}"))
                    .into_any_element();
            }
        };
        muted(text, theme)
    }
}

fn heading(compact: bool, theme: &Theme) -> impl IntoElement + use<> {
    columns(
        h_flex()
            .flex_none()
            .font_weight(FontWeight::SEMIBOLD)
            .border_b_1()
            .border_color(theme.border),
        ["Name", "Status", "Type", "Size", "Time"].map(str::to_owned),
        compact,
    )
}

/// One row of the table, the name taking what room is left; beside the
/// details, only the name and the status.
fn columns<E: ParentElement + Styled>(
    row: E,
    [name, status, kind, size, time]: [String; 5],
    compact: bool,
) -> E {
    let fixed = |width: f32, text: String| div().flex_none().w(px(width)).truncate().child(text);
    let row = row
        .gap_2()
        .px_2()
        .py_0p5()
        .child(div().flex_1().min_w_0().truncate().child(name))
        .child(fixed(STATUS_WIDTH, status));
    if compact {
        return row;
    }
    row.child(fixed(KIND_WIDTH, kind))
        .child(fixed(SIZE_WIDTH, size).text_right())
        .child(fixed(TIME_WIDTH, time).text_right())
}

fn status(request: &NetworkRequest) -> String {
    match (&request.failed, &request.response) {
        (Some(_), _) => "Failed".to_owned(),
        (None, Some(response)) => response.status.to_string(),
        (None, None) => "Pending".to_owned(),
    }
}

fn time(milliseconds: f64) -> String {
    if milliseconds < 1000. {
        format!("{milliseconds:.0} ms")
    } else {
        format!("{:.2} s", milliseconds / 1000.)
    }
}

fn section(title: &str, theme: &Theme) -> AnyElement {
    div()
        .pt_2()
        .font_weight(FontWeight::SEMIBOLD)
        .border_b_1()
        .border_color(theme.border)
        .child(title.to_owned())
        .into_any_element()
}

fn muted(text: String, theme: &Theme) -> AnyElement {
    div()
        .text_color(theme.muted_foreground)
        .child(text)
        .into_any_element()
}

fn headers(title: &str, headers: &[(String, String)], theme: &Theme) -> AnyElement {
    let rows = headers.iter().map(|(name, value)| {
        h_flex()
            .items_start()
            .gap_2()
            .child(
                div()
                    .flex_none()
                    .w_1_3()
                    .truncate()
                    .text_color(theme.muted_foreground)
                    .child(name.clone()),
            )
            .child(div().flex_1().min_w_0().child(value.clone()))
    });
    v_flex()
        .gap_0p5()
        .child(section(title, theme))
        .children(rows)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_is_shown_in_the_unit_that_reads_best() {
        assert_eq!(time(12.4), "12 ms");
        assert_eq!(time(1530.), "1.53 s");
    }
}
