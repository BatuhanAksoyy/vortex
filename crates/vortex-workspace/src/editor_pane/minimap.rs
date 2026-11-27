//! The whole file drawn small beside the editor, as VSCode's minimap: where
//! it changed, what is in view, and a click or drag to move there.

mod geometry;
mod paint;

use std::{ops::Range, rc::Rc, time::Duration};

use gpui_kit::prelude::*;
use gpui_kit::{
    Bounds, Context, Entity, Hsla, MouseButton, MouseDownEvent, Pixels, ScrollEvent, SharedString,
    Subscription, Task, TestSupportExt, Window,
    component::{
        ActiveTheme as _, Theme,
        highlighter::{HighlightTheme, SyntaxHighlighter},
        input::{EditorState, Rope},
    },
    div, point, px,
};
use vortex_git::LineChange;

use geometry::Geometry;

const WIDTH: f32 = 110.;
const MAX_COLUMNS: usize = 120;
const TAB_COLUMNS: usize = 4;
/// Typing redraws it once it pauses.
const DEBOUNCE: Duration = Duration::from_millis(150);
/// Larger files are drawn in one color.
const MAX_HIGHLIGHTED_BYTES: usize = 1024 * 1024;

/// Characters of one color without a space between.
pub struct Run {
    pub columns: Range<usize>,
    pub color: Option<Hsla>,
}

pub struct Minimap {
    editor: Entity<EditorState>,
    language: SharedString,
    lines: Rc<Vec<Vec<Run>>>,
    changes: Rc<Vec<LineChange>>,
    bounds: Bounds<Pixels>,
    /// How far below the slider's top it is held while dragged.
    held: Option<f32>,
    load: Option<Task<()>>,
    _subscriptions: [Subscription; 2],
}

impl Minimap {
    pub fn new(editor: Entity<EditorState>, language: &str, cx: &mut Context<Self>) -> Self {
        let subscriptions = [
            cx.observe(&editor, |_, _, cx| cx.notify()),
            cx.observe_global::<Theme>(|this, cx| this.refresh(Duration::ZERO, cx)),
        ];
        let mut this = Self {
            editor,
            language: language.to_owned().into(),
            lines: Rc::default(),
            changes: Rc::default(),
            bounds: Bounds::default(),
            held: None,
            load: None,
            _subscriptions: subscriptions,
        };
        this.refresh(Duration::ZERO, cx);
        this
    }

    pub fn set_changes(&mut self, changes: Vec<LineChange>, cx: &mut Context<Self>) {
        self.changes = Rc::new(changes);
        cx.notify();
    }

    /// Draws the text again after it changed.
    pub fn text_changed(&mut self, cx: &mut Context<Self>) {
        self.refresh(DEBOUNCE, cx);
    }

    fn refresh(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().clone();
        let theme = cx.theme().highlight_theme.clone();
        let language = self.language.clone();
        self.load = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let read = cx.background_spawn(async move { runs(&text, &language, &theme) });
            let lines = read.await;
            let _ = this.update(cx, |this, cx| {
                this.lines = Rc::new(lines);
                cx.notify();
            });
        }));
    }

    fn geometry(&self, cx: &Context<Self>) -> Option<Geometry> {
        let editor = self.editor.read(cx);
        let line = f32::from(editor.line_height()?);
        let height = f32::from(self.bounds.size.height);
        Some(Geometry {
            height,
            rows: rows(editor.display_map().display_row_count()),
            visible: height / line,
            top: -f32::from(editor.scroll_offset().y) / line,
        })
    }

    fn scroll_to(&self, top: f32, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            if let Some(line) = editor.line_height() {
                let left = editor.scroll_offset().x;
                editor.set_scroll_offset(point(left, -line * top.max(0.)), cx);
            }
        });
    }

    fn on_press(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(geometry) = self.geometry(cx) else {
            return;
        };
        let y = f32::from(event.position.y - self.bounds.top());
        let slider = geometry.slider();
        if slider.contains(&y) {
            self.held = Some(y - slider.start);
        } else {
            // Beside the slider, what was clicked comes to the middle.
            self.scroll_to(geometry.row_at(y) - geometry.visible / 2., cx);
            self.held = Some((slider.end - slider.start) / 2.);
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn on_drag(&self, y: Pixels, cx: &mut Context<Self>) {
        if let (Some(held), Some(geometry)) = (self.held, self.geometry(cx)) {
            let start = f32::from(y - self.bounds.top()) - held;
            self.scroll_to(geometry.top_for_slider(start), cx);
        }
    }

    fn on_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        if let Some(line) = editor.line_height() {
            let moved = event.delta.pixel_delta(line).y;
            let top = -(editor.scroll_offset().y + moved) / line;
            self.scroll_to(top, cx);
        }
    }
}

impl Render for Minimap {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("minimap")
            .test_support()
            .aria_label("Minimap")
            .flex_none()
            .w(px(WIDTH))
            .h_full()
            .border_l_1()
            .border_color(cx.theme().border)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_press))
            .on_scroll(cx.listener(Self::on_wheel))
            .child(self.drawing(cx))
    }
}

/// `count` as the minimap measures rows; a file may have more than `u16` holds.
fn rows(count: usize) -> f32 {
    let part = |count: usize| f32::from(u16::try_from(count).unwrap_or(u16::MAX));
    part(count >> 16) * 65_536. + part(count & 0xffff)
}

/// What to draw of each line.
fn runs(text: &Rope, language: &str, theme: &HighlightTheme) -> Vec<Vec<Run>> {
    let source = text.to_string();
    let styles = if source.len() <= MAX_HIGHLIGHTED_BYTES {
        let mut highlighter = SyntaxHighlighter::new(language);
        highlighter.update(None, text, None);
        highlighter.styles(&(0..source.len()), theme)
    } else {
        Vec::new()
    };
    let mut styles = styles
        .into_iter()
        .map(|(range, style)| (range, style.color))
        .peekable();
    let mut start = 0;
    source
        .split_inclusive('\n')
        .map(|line| {
            let runs = line_runs(line, |at| {
                let at = start + at;
                while styles.next_if(|(range, _)| range.end <= at).is_some() {}
                let style = styles.peek().filter(|(range, _)| range.contains(&at));
                style.and_then(|(_, color)| *color)
            });
            start += line.len();
            runs
        })
        .collect()
}

fn line_runs(line: &str, mut color_at: impl FnMut(usize) -> Option<Hsla>) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut column = 0;
    for (at, char) in line.char_indices() {
        if column >= MAX_COLUMNS {
            break;
        }
        if char.is_whitespace() {
            column += if char == '\t' { TAB_COLUMNS } else { 1 };
            continue;
        }
        let color = color_at(at);
        match runs.last_mut() {
            Some(run) if run.columns.end == column && run.color == color => run.columns.end += 1,
            _ => runs.push(Run {
                columns: column..column + 1,
                color,
            }),
        }
        column += 1;
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_drawn_in_runs_between_spaces() {
        let runs = line_runs("\tlet x = 1;\n", |_| None);
        let columns: Vec<_> = runs.into_iter().map(|run| run.columns).collect();
        assert_eq!(columns, [4..7, 8..9, 10..11, 12..14]);
    }

    #[test]
    fn code_is_drawn_in_its_colors() {
        let theme = HighlightTheme::default_dark();
        let lines = runs(&Rope::from("// note\nlet x = 1;\n"), "rust", &theme);
        assert_eq!(lines.len(), 2);
        assert!(lines[0][0].color.is_some());
        assert_ne!(lines[0][0].color, lines[1][0].color);
    }
}
