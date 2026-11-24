//! The mouse, as the protocol takes it.

use serde_json::{Value, json};

/// The modifier keys held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const ALT: Self = Self(1);
    pub const CONTROL: Self = Self(2);
    /// Command on macOS.
    pub const META: Self = Self(4);
    pub const SHIFT: Self = Self(8);

    /// These, with `other` as well when `held`.
    #[must_use]
    pub fn with(self, other: Self, held: bool) -> Self {
        Self(if held { self.0 | other.0 } else { self.0 })
    }

    pub(crate) fn has(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub(crate) fn bits(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    Back,
    Forward,
}

impl Button {
    fn name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Middle => "middle",
            Self::Right => "right",
            Self::Back => "back",
            Self::Forward => "forward",
        }
    }

    fn bit(self) -> u8 {
        match self {
            Self::Left => 1,
            Self::Right => 2,
            Self::Middle => 4,
            Self::Back => 8,
            Self::Forward => 16,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    Pressed,
    Released,
    Moved,
}

/// Places are in page pixels from the page's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouseInput {
    pub kind: MouseKind,
    pub x: f32,
    pub y: f32,
    /// The button pressed or released, or held while moving.
    pub button: Option<Button>,
    pub clicks: usize,
    pub modifiers: Modifiers,
}

impl MouseInput {
    pub(crate) fn params(self) -> Value {
        let kind = match self.kind {
            MouseKind::Pressed => "mousePressed",
            MouseKind::Released => "mouseReleased",
            MouseKind::Moved => "mouseMoved",
        };
        let held = match self.kind {
            MouseKind::Released => 0,
            MouseKind::Pressed | MouseKind::Moved => self.button.map_or(0, Button::bit),
        };
        json!({
            "type": kind,
            "x": self.x,
            "y": self.y,
            "button": self.button.map_or("none", Button::name),
            "buttons": held,
            "clickCount": self.clicks,
            "modifiers": self.modifiers.bits(),
        })
    }
}

/// How far the page is asked to move, right and down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wheel {
    pub x: f32,
    pub y: f32,
    pub right: f32,
    pub down: f32,
    pub modifiers: Modifiers,
}

impl Wheel {
    pub(crate) fn params(self) -> Value {
        json!({
            "type": "mouseWheel",
            "x": self.x,
            "y": self.y,
            "deltaX": self.right,
            "deltaY": self.down,
            "modifiers": self.modifiers.bits(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_holds_its_button() {
        let at = |kind| MouseInput {
            kind,
            x: 1.,
            y: 2.,
            button: Some(Button::Right),
            clicks: 1,
            modifiers: Modifiers::META.with(Modifiers::SHIFT, true),
        };
        let moved = at(MouseKind::Moved).params();
        assert_eq!(moved["buttons"], 2);
        assert_eq!(moved["modifiers"], 12);
        assert_eq!(at(MouseKind::Released).params()["buttons"], 0);
    }
}
