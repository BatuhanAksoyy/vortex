//! The lanes beside each commit in the history: the commit's dot and the
//! lines through its row, each in the color of the branch it belongs to.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, ContentMask, Hsla, PathBuilder, Pixels, Point, Window, canvas,
    component::ActiveTheme as _, fill, point, px, size,
};
use vortex_git::GraphRow;

/// One lane's width.
const LANE: f32 = 12.;
/// Lanes past this many are cut off, leaving the commits' subjects room.
const MOST_LANES: usize = 6;
const LINE: f32 = 1.5;
const DOT: f32 = 7.;

/// The lanes' colors, from the theme's chart series.
#[derive(Clone, Copy)]
pub(super) struct Palette([Hsla; 5]);

impl Palette {
    pub(super) fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self([
            theme.chart_1,
            theme.chart_2,
            theme.chart_3,
            theme.chart_4,
            theme.chart_5,
        ])
    }

    fn lane(self, lane: usize) -> Hsla {
        self.0[lane % self.0.len()]
    }
}

/// How wide the graph is beside every row: as wide as its widest row, up
/// to `MOST_LANES`, so the subjects line up.
pub(super) fn width(rows: &[GraphRow]) -> Pixels {
    let lanes = rows.iter().map(GraphRow::width).max().unwrap_or(1);
    px(offset(lanes.min(MOST_LANES)))
}

/// The row's part of the graph, as tall as the row.
pub(super) fn render(row: GraphRow, width: Pixels, palette: Palette) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                paint(&row, bounds, palette, window);
            });
        },
    )
    .flex_none()
    .w(width)
}

fn paint(row: &GraphRow, bounds: Bounds<Pixels>, palette: Palette, window: &mut Window) {
    let at = |lane: usize, y: Pixels| point(bounds.left() + px(offset(lane) + LANE / 2.), y);
    let (top, middle, bottom) = (bounds.top(), bounds.center().y, bounds.bottom());
    // A line takes the color of the branch's own end: where it comes from
    // above the dot, where it goes below it.
    for edge in &row.above {
        let color = palette.lane(edge.from);
        line(at(edge.from, top), at(edge.to, middle), color, window);
    }
    for edge in &row.below {
        let color = palette.lane(edge.to);
        line(at(edge.from, middle), at(edge.to, bottom), color, window);
    }
    let dot = Bounds::centered_at(at(row.lane, middle), size(px(DOT), px(DOT)));
    window.paint_quad(fill(dot, palette.lane(row.lane)).corner_radii(px(DOT / 2.)));
}

/// Straight down, or curving from one lane over to another.
fn line(from: Point<Pixels>, to: Point<Pixels>, color: Hsla, window: &mut Window) {
    let mut path = PathBuilder::stroke(px(LINE));
    path.move_to(from);
    if from.x == to.x {
        path.line_to(to);
    } else {
        let bend = from.y + (to.y - from.y) / 2.;
        path.cubic_bezier_to(to, point(from.x, bend), point(to.x, bend));
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

/// Where lane `lane` starts from the graph's left edge.
fn offset(lane: usize) -> f32 {
    f32::from(u16::try_from(lane).unwrap_or(u16::MAX)) * LANE
}
