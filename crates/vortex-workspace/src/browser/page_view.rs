//! The page in the panel: its latest picture, and the mouse over it.

use gpui_kit::prelude::*;
use gpui_kit::{
    Bounds, Context, Corners, DispatchPhase, ElementInputHandler, Entity, Hitbox, HitboxBehavior,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, NavigationDirection, Pixels, Point,
    ScrollEvent, TestSupportExt, Window, canvas, div, px, size,
};
use vortex_browser::{Button, Modifiers, MouseInput, MouseKind, Viewport, Wheel};

use super::{Browser, Device};

/// How far a wheel that counts lines moves the page per line.
const LINE: f32 = 40.;

impl Browser {
    pub(super) fn render_page(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let this = cx.entity();
        let painted = this.clone();
        div()
            .id("browser-page")
            .test_support()
            .track_focus(&self.focus)
            .key_context(super::KEY_CONTEXT)
            .relative()
            .size_full()
            .overflow_hidden()
            .on_any_mouse_down(cx.listener(Self::on_mouse_down))
            .on_scroll(cx.listener(Self::on_wheel))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .child(
                canvas(
                    move |bounds, window, cx| {
                        this.update(cx, |this, _| this.fit(bounds, window));
                        window.insert_hitbox(bounds, HitboxBehavior::Normal)
                    },
                    move |bounds, hitbox, window, cx| paint(&painted, bounds, hitbox, window, cx),
                )
                .size_full(),
            )
            .children(self.render_marks(cx))
    }

    /// Lays the page out to fill `bounds`.
    fn fit(&mut self, bounds: Bounds<Pixels>, window: &Window) {
        self.bounds = bounds;
        self.scale = window.scale_factor();
        let viewport = Viewport {
            width: whole(bounds.size.width),
            height: whole(bounds.size.height),
            phone: self.device == Device::Phone,
        };
        if viewport == self.viewport || viewport.width == 0 || viewport.height == 0 {
            return;
        }
        let other_device = self.viewport.phone != viewport.phone;
        self.viewport = viewport;
        if let Some(page) = self.page() {
            page.resize(viewport);
            // Sites choose the page they send by the device.
            if other_device {
                page.reload();
            }
        }
    }

    fn paint_picture(&mut self, bounds: Bounds<Pixels>, window: &mut Window) {
        if let Some((image, scale)) = &self.picture {
            let pixels = image.size(0);
            // Its own size, not the panel's: while the panel is dragged, the
            // page stays in place instead of stretching.
            let natural = size(
                px(f32::from(u16_of(pixels.width.into())) / scale),
                px(f32::from(u16_of(pixels.height.into())) / scale),
            );
            let place = Bounds::new(bounds.origin, natural);
            let corners = Corners::default();
            if let Err(error) = window.paint_image(place, place, corners, image.clone(), 0, false) {
                log::warn!("cannot draw the page: {error:#}");
            }
        }
        // So the browser draws no faster than the window, and not at all
        // while the window is covered.
        self.ahead = false;
        for frame in std::mem::take(&mut self.unpainted) {
            if let Some(page) = self.page() {
                page.acknowledge(&frame);
            }
        }
    }

    fn at(&self, position: Point<Pixels>) -> (f32, f32) {
        let place = position - self.bounds.origin;
        (f32::from(place.x), f32::from(place.y))
    }

    fn send_mouse(&self, input: MouseInput) {
        if let Some(page) = self.page() {
            page.mouse(input);
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus.focus(window, cx);
        let button = button(event.button);
        self.pressed = Some(button);
        self.last_press = event.position;
        let (x, y) = self.at(event.position);
        self.send_mouse(MouseInput {
            kind: MouseKind::Pressed,
            x,
            y,
            button: Some(button),
            clicks: event.click_count,
            modifiers: modifiers(event.modifiers),
        });
        cx.stop_propagation();
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent) {
        let button = button(event.button);
        if self.pressed != Some(button) {
            return;
        }
        self.pressed = None;
        let (x, y) = self.at(event.position);
        self.send_mouse(MouseInput {
            kind: MouseKind::Released,
            x,
            y,
            button: Some(button),
            clicks: event.click_count,
            modifiers: modifiers(event.modifiers),
        });
    }

    fn on_mouse_move(&self, event: &MouseMoveEvent, hovered: bool) {
        // A drag that began outside is not the page's.
        let foreign = event.pressed_button.is_some() && self.pressed.is_none();
        if foreign || (!hovered && self.pressed.is_none()) {
            return;
        }
        let (x, y) = self.at(event.position);
        self.send_mouse(MouseInput {
            kind: MouseKind::Moved,
            x,
            y,
            button: self.pressed,
            clicks: 0,
            modifiers: modifiers(event.modifiers),
        });
    }

    fn on_wheel(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(LINE));
        let (x, y) = self.at(event.position);
        if let Some(page) = self.page() {
            page.wheel(Wheel {
                x,
                y,
                right: -f32::from(delta.x),
                down: -f32::from(delta.y),
                modifiers: modifiers(event.modifiers),
            });
        }
        cx.stop_propagation();
    }
}

fn paint(
    browser: &Entity<Browser>,
    bounds: Bounds<Pixels>,
    hitbox: Hitbox,
    window: &mut Window,
    cx: &mut gpui_kit::App,
) {
    let (focus, cursor) = browser.update(cx, |this, _| {
        this.paint_picture(bounds, window);
        (this.focus.clone(), this.cursor)
    });
    let input = ElementInputHandler::new(bounds, browser.clone());
    window.handle_input(&focus, input, cx);
    window.set_cursor_style(cursor, &hitbox);
    // On the window, not the element: a drag goes on outside the page.
    let moved = browser.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble {
            let hovered = hitbox.is_hovered(window);
            moved.read(cx).on_mouse_move(event, hovered);
        }
    });
    let released = browser.clone();
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Bubble {
            released.update(cx, |this, _| this.on_mouse_up(event));
        }
    });
}

/// A panel's size in whole pixels.
fn whole(length: Pixels) -> u32 {
    u32::from(length.floor())
}

fn button(button: MouseButton) -> Button {
    match button {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
        MouseButton::Navigate(NavigationDirection::Back) => Button::Back,
        MouseButton::Navigate(NavigationDirection::Forward) => Button::Forward,
    }
}

pub(super) fn modifiers(held: gpui_kit::Modifiers) -> Modifiers {
    Modifiers::default()
        .with(Modifiers::ALT, held.alt)
        .with(Modifiers::CONTROL, held.control)
        .with(Modifiers::META, held.platform)
        .with(Modifiers::SHIFT, held.shift)
}

/// No picture is wider.
fn u16_of(pixels: i32) -> u16 {
    u16::try_from(pixels).unwrap_or(u16::MAX)
}
