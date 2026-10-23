use std::time::Instant;

use smithay_client_toolkit::seat::pointer::{
    AxisScroll, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, CursorIcon, PointerEvent, PointerEventKind,
    PointerHandler,
};
use wayland_client::{
    Connection, QueueHandle,
    protocol::wl_pointer::{AxisRelativeDirection, AxisSource, WlPointer},
};

use crate::input::PIXELS_PER_LINE;
use crate::scale::ScaleFactor;
use crate::{Button, Cursor, Point, Scroll, ScrollSource};

use super::WaylandState;

impl PointerHandler for WaylandState {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlPointer,
        events: &[PointerEvent],
    ) {
        let mut handled_on = Vec::new();

        for event in events {
            if self.handle_pointer(event) {
                handled_on.push(event.surface.clone());
            }
        }

        /*
         * a service a handler changed already wakes the windows that read it;
         * only the window the pointer is on redraws for its own state, like a
         * scroll offset, so hovering never redraws every window
         */
        for surface in handled_on {
            self.request_frame_on(&surface);
        }
    }
}

impl WaylandState {
    // returns whether a handler ran
    fn handle_pointer(&mut self, event: &PointerEvent) -> bool {
        // every event names the surface it happened on
        let Some(window) = self.window(&event.surface) else {
            return false;
        };

        let pointer = &mut window.pointer;

        let (x, y) = event.position;

        match &event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                let hovered = pointer.move_to(x as f32, y as f32);
                let dragged = pointer.drag();
                let moved = pointer.report_motion();

                let cursor = pointer.cursor();

                self.show_cursor(cursor);

                hovered || dragged || moved
            }

            PointerEventKind::Leave { .. } => {
                let left = pointer.leave();

                self.forget_cursor();

                left
            }

            PointerEventKind::Press { button, .. } => {
                // side buttons like back and forward have no amane button yet
                let Some(button) = to_button(*button) else {
                    return false;
                };

                let pressed = pointer.press(button);

                // only the left button drags
                let dragged = button == Button::Left && pointer.start_drag();

                pressed || dragged
            }

            PointerEventKind::Release { button, .. } => {
                let Some(button) = to_button(*button) else {
                    return false;
                };

                let ended = button == Button::Left && pointer.stop_drag();
                let let_go = pointer.let_go(button);
                let clicked = pointer.release(button);

                ended || let_go || clicked
            }

            PointerEventKind::Axis {
                horizontal,
                vertical,
                source,
                ..
            } => {
                let scroll = to_scroll(horizontal, vertical, *source, window.scale_factor);
                let moved = fingers_moved(horizontal, vertical, window.scale_factor);

                pointer.scroll(scroll, moved, Instant::now())
            }
        }
    }

    /*
     * uses cursor-shape when the compositor has it,
     * otherwise the pointer draws the icon from the cursor theme
     */
    pub fn show_cursor(&mut self, cursor: Cursor) {
        if self.cursor_shown == Some(cursor) {
            return;
        }

        let Some(pointer) = &self.pointer_device else {
            return;
        };

        // a missing icon in the theme leaves the last cursor in place
        if pointer.set_cursor(&self.connection, icon(cursor)).is_err() {
            return;
        }

        self.cursor_shown = Some(cursor);
    }

    // the compositor puts its own cursor back once the pointer leaves
    pub fn forget_cursor(&mut self) {
        self.cursor_shown = None;
    }
}

fn to_button(code: u32) -> Option<Button> {
    match code {
        BTN_LEFT => Some(Button::Left),
        BTN_RIGHT => Some(Button::Right),
        BTN_MIDDLE => Some(Button::Middle),
        _ => None,
    }
}

fn to_scroll(
    horizontal: &AxisScroll,
    vertical: &AxisScroll,
    source: Option<AxisSource>,
    scale_factor: ScaleFactor,
) -> Scroll {
    Scroll {
        x: lines(horizontal, scale_factor),
        y: lines(vertical, scale_factor),
        source: to_source(source, horizontal, vertical),
        stop: horizontal.stop || vertical.stop,
    }
}

fn to_source(source: Option<AxisSource>, horizontal: &AxisScroll, vertical: &AxisScroll) -> ScrollSource {
    match source {
        Some(AxisSource::Finger) => ScrollSource::Finger,
        Some(AxisSource::Continuous) => ScrollSource::Continuous,
        Some(_) => ScrollSource::Wheel,

        // a compositor that doesn't say: only a wheel counts in steps
        None if [horizontal, vertical]
            .iter()
            .any(|axis| axis.value120 != 0 || axis.discrete != 0) =>
        {
            ScrollSource::Wheel
        }
        None => ScrollSource::Continuous,
    }
}

/*
 * how the fingers themselves moved, in logical pixels: natural scrolling
 * turns the scroll the other way round from the fingers, and says so
 */
fn fingers_moved(horizontal: &AxisScroll, vertical: &AxisScroll, scale_factor: ScaleFactor) -> Point {
    let moved = |axis: &AxisScroll| {
        let pixels = scale_factor.logical(axis.absolute as f32);

        match axis.relative_direction {
            Some(AxisRelativeDirection::Inverted) => -pixels,
            _ => pixels,
        }
    };

    Point {
        x: moved(horizontal),
        y: moved(vertical),
    }
}

fn lines(axis: &AxisScroll, scale_factor: ScaleFactor) -> f32 {
    // a wheel reports its steps in 120ths, which is exact
    if axis.value120 != 0 {
        return axis.value120 as f32 / 120.0;
    }

    // older compositors report whole steps instead
    if axis.discrete != 0 {
        return axis.discrete as f32;
    }

    // a touchpad only reports pixels
    let pixels = scale_factor.logical(axis.absolute as f32);

    pixels / PIXELS_PER_LINE
}

fn icon(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::Move => CursorIcon::Move,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
        Cursor::Wait => CursorIcon::Wait,
        Cursor::Crosshair => CursorIcon::Crosshair,

        Cursor::ResizeTop => CursorIcon::NResize,
        Cursor::ResizeBottom => CursorIcon::SResize,
        Cursor::ResizeLeft => CursorIcon::WResize,
        Cursor::ResizeRight => CursorIcon::EResize,
        Cursor::ResizeTopLeft => CursorIcon::NwResize,
        Cursor::ResizeTopRight => CursorIcon::NeResize,
        Cursor::ResizeBottomLeft => CursorIcon::SwResize,
        Cursor::ResizeBottomRight => CursorIcon::SeResize,
        Cursor::ResizeHorizontal => CursorIcon::EwResize,
        Cursor::ResizeVertical => CursorIcon::NsResize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_converts_touchpad_pixels_but_preserves_wheel_steps() {
        let factor = ScaleFactor::new(2.0);
        let touchpad = AxisScroll {
            absolute: 30.0,
            ..AxisScroll::default()
        };
        let wheel = AxisScroll {
            absolute: 30.0,
            value120: 120,
            ..AxisScroll::default()
        };
        let old_wheel = AxisScroll {
            absolute: 30.0,
            discrete: 1,
            ..AxisScroll::default()
        };

        assert_eq!(lines(&touchpad, factor), 1.0);
        assert_eq!(lines(&touchpad, ScaleFactor::default()), 2.0);
        assert_eq!(lines(&wheel, factor), 1.0);
        assert_eq!(lines(&old_wheel, factor), 1.0);
    }

    #[test]
    fn the_source_comes_from_the_compositor_or_the_steps() {
        let still = AxisScroll::default();
        let wheel = AxisScroll {
            value120: 120,
            ..AxisScroll::default()
        };

        assert_eq!(
            to_source(Some(AxisSource::Finger), &still, &still),
            ScrollSource::Finger
        );
        assert_eq!(
            to_source(Some(AxisSource::WheelTilt), &still, &still),
            ScrollSource::Wheel
        );
        assert_eq!(to_source(None, &still, &wheel), ScrollSource::Wheel);
        assert_eq!(to_source(None, &still, &still), ScrollSource::Continuous);
    }

    #[test]
    fn natural_scrolling_turns_the_fingers_back_round() {
        let natural = AxisScroll {
            absolute: -30.0,
            relative_direction: Some(AxisRelativeDirection::Inverted),
            ..AxisScroll::default()
        };
        let plain = AxisScroll {
            absolute: 30.0,
            relative_direction: Some(AxisRelativeDirection::Identical),
            ..AxisScroll::default()
        };

        let factor = ScaleFactor::default();

        // both had the fingers move right
        assert_eq!(fingers_moved(&natural, &AxisScroll::default(), factor).x, 30.0);
        assert_eq!(fingers_moved(&plain, &AxisScroll::default(), factor).x, 30.0);
    }
}
