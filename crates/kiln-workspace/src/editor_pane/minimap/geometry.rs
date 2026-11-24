//! Where the file's rows and the part in view lie on the minimap.

use std::ops::Range;

/// A row's height on the minimap, in pixels.
pub const ROW: f32 = 2.;
const MIN_SLIDER: f32 = 16.;

pub struct Geometry {
    /// The minimap's height, in pixels.
    pub height: f32,
    pub rows: f32,
    /// Rows the editor shows at once, and the one it shows first.
    pub visible: f32,
    pub top: f32,
}

impl Geometry {
    /// Where the part in view is marked, in pixels from the top.
    pub fn slider(&self) -> Range<f32> {
        let size = (self.visible * ROW).clamp(MIN_SLIDER, self.height.max(MIN_SLIDER));
        let start = if self.fits() {
            self.top * ROW
        } else {
            let scrolled = (self.top / self.scrollable()).clamp(0., 1.);
            scrolled * (self.height - size).max(0.)
        };
        start..start + size
    }

    /// The row drawn at the minimap's top. A long file's drawing moves along
    /// with the slider.
    pub fn first_row(&self) -> f32 {
        if self.fits() {
            return 0.;
        }
        (self.top - self.slider().start / ROW).max(0.)
    }

    pub fn row_at(&self, y: f32) -> f32 {
        self.first_row() + y / ROW
    }

    /// The row to show first for the slider to start at `start`.
    pub fn top_for_slider(&self, start: f32) -> f32 {
        if self.fits() {
            return (start / ROW).max(0.);
        }
        let slider = self.slider();
        let room = (self.height - (slider.end - slider.start)).max(1.);
        (start / room).clamp(0., 1.) * self.scrollable()
    }

    fn fits(&self) -> bool {
        self.rows * ROW <= self.height
    }

    fn scrollable(&self) -> f32 {
        (self.rows - self.visible).max(1.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(one: f32, other: f32) -> bool {
        (one - other).abs() < 0.01
    }

    #[test]
    fn a_short_file_is_drawn_from_its_top() {
        let geometry = Geometry {
            height: 800.,
            rows: 100.,
            visible: 40.,
            top: 10.,
        };
        assert!(close(geometry.first_row(), 0.));
        assert!(close(geometry.slider().start, 20.));
        assert!(close(geometry.slider().end, 100.));
        assert!(close(geometry.top_for_slider(20.), 10.));
    }

    #[test]
    fn a_long_file_reaches_both_ends() {
        let at = |top| Geometry {
            height: 800.,
            rows: 10_000.,
            visible: 40.,
            top,
        };
        assert!(close(at(0.).slider().start, 0.));
        assert!(close(at(9_960.).slider().end, 800.));
        // The last row is drawn at the bottom.
        assert!(close(at(9_960.).row_at(800.), 10_000.));
        let middle = at(5_000.);
        assert!(close(middle.top_for_slider(middle.slider().start), 5_000.));
    }
}
