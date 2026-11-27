//! Drawing the minimap.

use std::{ops::Range, rc::Rc};

use gpui_kit::prelude::*;
use gpui_kit::{
    Bounds, Context, DispatchPhase, Entity, Hsla, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels,
    Window, canvas,
    component::{ActiveTheme as _, input::DisplayMap},
    fill, point, px, size,
};
use vortex_git::LineChange;

use super::{Minimap, Run, geometry::ROW, rows};
use crate::editor_pane::changes::line_color;

const COLUMN: f32 = 1.;
/// The strip along the left edge that shows changes, and the gap after it.
const STRIP: f32 = 3.;
const TEXT_LEFT: f32 = 6.;
const INK: f32 = 1.5;
const TEXT_OPACITY: f32 = 0.75;
/// The rows of a change are tinted over the minimap's width as well.
const TINT_OPACITY: f32 = 0.18;

impl Minimap {
    pub(super) fn drawing(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let minimap = cx.entity();
        let held = self.held.is_some();
        let quads = self.quads(cx);
        canvas(
            move |bounds, _, cx| {
                minimap.update(cx, |this, cx| this.laid_out(bounds, cx));
                minimap
            },
            move |bounds, minimap, window, _| {
                window.with_content_mask(Some(gpui_kit::ContentMask { bounds }), |window| {
                    let placed = quads.into_iter().map(|quad| moved(quad, bounds));
                    placed.for_each(|quad| window.paint_quad(quad));
                });
                if held {
                    follow_drag(&minimap, window);
                }
            },
        )
        .size_full()
    }

    fn laid_out(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        if self.bounds != bounds {
            self.bounds = bounds;
            cx.notify();
        }
    }

    /// Everything drawn, relative to the minimap's top left.
    fn quads(&self, cx: &Context<Self>) -> Vec<PaintQuad> {
        let Some(geometry) = self.geometry(cx) else {
            return Vec::new();
        };
        let theme = cx.theme();
        let map = self.editor.read(cx).display_map();
        let first = geometry.first_row();
        let width = f32::from(self.bounds.size.width);
        let mut quads = Vec::new();
        for change in self.changes.iter() {
            let color = line_color(change.kind, theme);
            let span = change_span(change, map, first);
            if span.end < 0. || span.start > geometry.height {
                continue;
            }
            quads.push(quad(0.0..width, &span, color.opacity(TINT_OPACITY)));
            quads.push(quad(0.0..STRIP, &span, color));
        }
        let ink = theme.foreground;
        for (y, line) in shown_lines(map, first, geometry.height) {
            let runs = self.lines.get(line).map_or(&[][..], Vec::as_slice);
            quads.extend(runs.iter().map(|run| run_quad(run, y, ink)));
        }
        let slider = if self.held.is_some() {
            theme.foreground.opacity(0.28)
        } else {
            theme.foreground.opacity(0.16)
        };
        quads.push(quad(0.0..width, &geometry.slider(), slider));
        quads
    }
}

/// The lines in sight and how far down each is. A wrapped line is drawn on
/// its first row only.
fn shown_lines(map: &DisplayMap, first: f32, height: f32) -> Vec<(f32, usize)> {
    let mut previous = None;
    (0..map.display_row_count())
        .skip_while(|row| rows(*row) + 1. < first)
        .take_while(|row| (rows(*row) - first) * ROW < height)
        .filter_map(|row| {
            let line = map.display_row_to_buffer_line(row);
            let repeated = previous.replace(line) == Some(line);
            (!repeated).then_some(((rows(row) - first) * ROW, line))
        })
        .collect()
}

fn change_span(change: &LineChange, map: &DisplayMap, first: f32) -> Range<f32> {
    let y = |line| (rows(map.buffer_line_to_display_row(line)) - first) * ROW;
    let start = y(change.lines.start);
    // A deletion has no lines of its own.
    let end = y(change.lines.end.max(change.lines.start + 1));
    start..end.max(start + ROW)
}

fn run_quad(run: &Run, y: f32, ink: Hsla) -> PaintQuad {
    let column = |at| TEXT_LEFT + rows(at) * COLUMN;
    let color = run.color.unwrap_or(ink).opacity(TEXT_OPACITY);
    quad(
        column(run.columns.start)..column(run.columns.end),
        &(y..y + INK),
        color,
    )
}

fn quad(x: Range<f32>, y: &Range<f32>, color: Hsla) -> PaintQuad {
    let origin = point(px(x.start), px(y.start));
    let size = size(px(x.end - x.start), px(y.end - y.start));
    fill(Bounds::new(origin, size), color)
}

fn moved(mut quad: PaintQuad, bounds: Bounds<Pixels>) -> PaintQuad {
    quad.bounds.origin += bounds.origin;
    quad
}

/// Follows the pointer wherever it goes until it is released.
fn follow_drag(minimap: &Entity<Minimap>, window: &mut Window) {
    let dragged = Rc::new(minimap.clone());
    let released = dragged.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
        if phase == DispatchPhase::Capture {
            dragged.update(cx, |this, cx| this.on_drag(event.position.y, cx));
        }
    });
    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Capture {
            released.update(cx, |this, cx| {
                this.held = None;
                cx.notify();
            });
        }
    });
}
