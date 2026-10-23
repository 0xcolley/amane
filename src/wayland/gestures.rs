use std::time::Instant;

use smithay_client_toolkit::reexports::protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_hold_v1::{self, ZwpPointerGestureHoldV1},
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gesture_swipe_v1::{self, ZwpPointerGestureSwipeV1},
    zwp_pointer_gestures_v1::{self, ZwpPointerGesturesV1},
};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    protocol::{wl_pointer::WlPointer, wl_surface::WlSurface},
};

use crate::Point;

use super::WaylandState;

// holding fingers still arrived in version 3
const HOLD_SINCE: u32 = 3;

/*
 * the touchpad gestures the compositor passes on for one pointer: swipes
 * of three or more fingers it doesn't use itself, pinches, and fingers
 * resting on the pad, which stop a glide
 */
pub struct Gestures {
    swipe: ZwpPointerGestureSwipeV1,
    pinch: ZwpPointerGesturePinchV1,
    hold: Option<ZwpPointerGestureHoldV1>,
}

impl Drop for Gestures {
    fn drop(&mut self) {
        self.swipe.destroy();
        self.pinch.destroy();

        if let Some(hold) = &self.hold {
            hold.destroy();
        }
    }
}

impl WaylandState {
    // none when the compositor has no gestures
    pub fn make_gestures(&self, pointer: &WlPointer) -> Option<Gestures> {
        let manager = self.pointer_gestures.as_ref()?;

        let hold = (manager.version() >= HOLD_SINCE)
            .then(|| manager.get_hold_gesture(pointer, &self.qh, ()));

        Some(Gestures {
            swipe: manager.get_swipe_gesture(pointer, &self.qh, ()),
            pinch: manager.get_pinch_gesture(pointer, &self.qh, ()),
            hold,
        })
    }

    // the gesture goes to the window it began over, which redraws if a handler ran
    fn gesture_on(
        &mut self,
        surface: Option<WlSurface>,
        handle: impl FnOnce(&mut crate::input::Pointer, f32) -> bool,
    ) {
        if let Some(surface) = surface {
            self.gesture_surface = Some(surface);
        }

        let Some(surface) = self.gesture_surface.clone() else {
            return;
        };

        let Some(window) = self.window(&surface) else {
            return;
        };

        // gestures move in surface units, the shell measures in its own scaled pixels
        let logical = window.scale_factor.logical(1.0);

        if handle(&mut window.pointer, logical) {
            self.request_frame_on(&surface);
        }
    }
}

impl Dispatch<ZwpPointerGestureSwipeV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &ZwpPointerGestureSwipeV1,
        event: zwp_pointer_gesture_swipe_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_swipe_v1::Event::Begin {
                surface, fingers, ..
            } => state.gesture_on(Some(surface), |pointer, _| pointer.swipe_began(fingers)),

            zwp_pointer_gesture_swipe_v1::Event::Update { dx, dy, .. } => {
                state.gesture_on(None, |pointer, logical| {
                    let by = Point {
                        x: dx as f32 * logical,
                        y: dy as f32 * logical,
                    };

                    pointer.gesture_moved(by, None, Instant::now())
                });
            }

            zwp_pointer_gesture_swipe_v1::Event::End { cancelled, .. } => {
                state.gesture_on(None, |pointer, _| pointer.gesture_ended(cancelled != 0));
            }

            _ => {}
        }
    }
}

impl Dispatch<ZwpPointerGesturePinchV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin {
                surface, fingers, ..
            } => state.gesture_on(Some(surface), |pointer, _| pointer.pinch_began(fingers)),

            zwp_pointer_gesture_pinch_v1::Event::Update {
                dx,
                dy,
                scale,
                rotation,
                ..
            } => {
                state.gesture_on(None, |pointer, logical| {
                    let by = Point {
                        x: dx as f32 * logical,
                        y: dy as f32 * logical,
                    };

                    let pinch = Some((scale as f32, rotation as f32));

                    pointer.gesture_moved(by, pinch, Instant::now())
                });
            }

            zwp_pointer_gesture_pinch_v1::Event::End { cancelled, .. } => {
                state.gesture_on(None, |pointer, _| pointer.gesture_ended(cancelled != 0));
            }

            _ => {}
        }
    }
}

impl Dispatch<ZwpPointerGestureHoldV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &ZwpPointerGestureHoldV1,
        event: zwp_pointer_gesture_hold_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // only the touch matters; lifting the fingers again does nothing
        if let zwp_pointer_gesture_hold_v1::Event::Begin { surface, .. } = event {
            state.gesture_on(Some(surface), |pointer, _| pointer.hold());
        }
    }
}

// the manager has no events
impl Dispatch<ZwpPointerGesturesV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZwpPointerGesturesV1,
        _: zwp_pointer_gestures_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
