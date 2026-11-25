//! The sizes controls, icons and small text have across the app, so a button
//! in one panel matches its neighbour in the next. Everything is relative to
//! the interface's text size and grows with it.
//!
//! The component library's own small sizes are drawn for larger text than
//! Kiln's; its icons come out too small here, so buttons draw their own.

use gpui_kit::prelude::*;
use gpui_kit::{
    Pixels, Rems, SharedString, Styled,
    component::{Icon, Sizable as _, button::Button, h_flex},
    px, rems,
};

/// An icon in a bar or the composer strip: 16 px at the default text size.
const CONTROL_ICON: Rems = rems(1.15);
/// An icon in a row, a list or a panel: 14 px.
const ROW_ICON: Rems = rems(1.);
/// Text beside or under the main text: 11.5 px.
const CAPTION: Rems = rems(0.82);

/// How tall the bar at the top of each pane is. The dock draws its tab bar
/// in fixed pixels, and the panes beside it line up with it.
pub const BAR_HEIGHT: Pixels = px(32.);

pub trait ScaledButton: Sized {
    /// A button in a bar, the composer strip or a dialog.
    #[must_use]
    fn control(self) -> Self;
    #[must_use]
    fn control_icon(self, icon: impl Into<Icon>) -> Self;
    #[must_use]
    fn control_labelled(self, icon: impl Into<Icon>, label: impl Into<SharedString>) -> Self;
    /// A button in a row or a panel's own heading.
    #[must_use]
    fn row_icon(self, icon: impl Into<Icon>) -> Self;
    #[must_use]
    fn row_labelled(self, icon: impl Into<Icon>, label: impl Into<SharedString>) -> Self;
}

impl ScaledButton for Button {
    fn control(self) -> Self {
        self.small().h_7().px_2()
    }

    fn control_icon(self, icon: impl Into<Icon>) -> Self {
        let icon = icon.into().size(CONTROL_ICON);
        self.small().h_7().px_1p5().child(icon)
    }

    fn control_labelled(self, icon: impl Into<Icon>, label: impl Into<SharedString>) -> Self {
        let icon = icon.into().size(CONTROL_ICON);
        labelled(self.small().h_7().px_1p5(), icon, label.into())
    }

    fn row_icon(self, icon: impl Into<Icon>) -> Self {
        let icon = icon.into().size(ROW_ICON);
        self.small().size_6().p_0().child(icon)
    }

    fn row_labelled(self, icon: impl Into<Icon>, label: impl Into<SharedString>) -> Self {
        labelled(self.small(), icon.into().size(ROW_ICON), label.into())
    }
}

/// Built as a child, as the button sizes an icon of its own to itself.
fn labelled(button: Button, icon: Icon, label: SharedString) -> Button {
    button
        .accessibility_label(label.clone())
        .child(h_flex().gap_1().min_w_0().child(icon).child(label))
}

pub trait ScaledIcon: Sized {
    /// An icon in a row of text.
    #[must_use]
    fn in_row(self) -> Self;
    /// An icon alone in a control.
    #[must_use]
    fn in_control(self) -> Self;
}

impl ScaledIcon for Icon {
    fn in_row(self) -> Self {
        self.size(ROW_ICON)
    }

    fn in_control(self) -> Self {
        self.size(CONTROL_ICON)
    }
}

pub trait ScaledText: Styled {
    /// Text beside or under the main text.
    #[must_use]
    fn text_caption(self) -> Self {
        self.text_size(CAPTION)
    }
}

impl<T: Styled> ScaledText for T {}
