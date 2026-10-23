use std::time::Instant;

use crate::input::Recent;
use crate::{Phase, Pinch, Point, Scroll, ScrollSource, Swipe};

use super::Pointer;

// how far fingers move before it's clear whether they scroll or swipe, in logical pixels
const DECIDE_AFTER: f32 = 4.0;

// a run of finger scrolling, from the first move until the fingers lift
pub enum Fingers {
    // not far enough yet to tell; what they've moved is kept for whichever it becomes
    Undecided {
        scroll: Scroll,
        moved: Point,
    },

    Scrolling,

    // more across than up or down, over something with on_swipe
    Swiping {
        target: usize,
        moved: Point,
        recent: Recent,
    },
}

// a gesture the compositor passed on, kept with the target it began over
pub struct Gesture {
    target: usize,
    fingers: u32,
    moved: Point,
    recent: Recent,
    kind: Kind,
}

enum Kind {
    Swipe,
    Pinch { scale: f32, rotation: f32 },
}

impl Pointer {
    // scroll

    /*
     * a wheel scrolls; fingers either scroll or, moving mostly sideways over
     * something with on_swipe, swipe it, the way two fingers go back a page
     * on macOS. `moved` is how the fingers themselves moved, which natural
     * scrolling turns the other way round from the scroll
     */
    pub fn scroll(&mut self, scroll: Scroll, moved: Point, now: Instant) -> bool {
        if scroll.source != ScrollSource::Finger {
            self.fingers = None;

            return self.send_scroll(scroll);
        }

        match self.fingers.take() {
            None if scroll.stop => self.send_scroll(scroll),

            None => self.decide(scroll, scroll, moved, now),

            Some(Fingers::Undecided {
                scroll: so_far,
                moved: before,
            }) => {
                let total = Scroll {
                    x: so_far.x + scroll.x,
                    y: so_far.y + scroll.y,
                    ..scroll
                };

                let moved = Point {
                    x: before.x + moved.x,
                    y: before.y + moved.y,
                };

                self.decide(total, scroll, moved, now)
            }

            Some(Fingers::Scrolling) => {
                if !scroll.stop {
                    self.fingers = Some(Fingers::Scrolling);
                }

                self.send_scroll(scroll)
            }

            Some(Fingers::Swiping {
                target,
                moved: before,
                mut recent,
            }) => {
                recent.push(now, moved.x, moved.y);

                let moved = Point {
                    x: before.x + moved.x,
                    y: before.y + moved.y,
                };

                let phase = if scroll.stop {
                    Phase::Ended
                } else {
                    Phase::Moved
                };

                let swiped = self.send_swipe(target, phase, 2, moved, &recent, now);

                if !scroll.stop {
                    self.fingers = Some(Fingers::Swiping {
                        target,
                        moved,
                        recent,
                    });
                }

                swiped
            }
        }
    }

    // once the fingers have moved far enough, or lifted, they're scrolling or swiping
    fn decide(&mut self, total: Scroll, latest: Scroll, moved: Point, now: Instant) -> bool {
        let far = moved.x.abs() + moved.y.abs() >= DECIDE_AFTER;

        if !far && !latest.stop {
            self.fingers = Some(Fingers::Undecided {
                scroll: total,
                moved,
            });

            return false;
        }

        let sideways = moved.x.abs() > moved.y.abs();

        let target = if sideways && !latest.stop {
            self.find_topmost(|target| target.handlers.swipe.is_some())
        } else {
            None
        };

        let Some(target) = target else {
            if !latest.stop {
                self.fingers = Some(Fingers::Scrolling);
            }

            return self.send_scroll(total);
        };

        let mut recent = Recent::default();

        recent.push(now, moved.x, moved.y);

        let swiped = self.send_swipe(target, Phase::Began, 2, moved, &recent, now);

        self.fingers = Some(Fingers::Swiping {
            target,
            moved,
            recent,
        });

        swiped
    }

    fn send_scroll(&self, scroll: Scroll) -> bool {
        let Some(topmost) = self.find_topmost(|target| target.handlers.scroll.is_some()) else {
            return false;
        };

        let Some(handler) = &self.targets[topmost].handlers.scroll else {
            return false;
        };

        handler(scroll);

        true
    }

    fn send_swipe(
        &self,
        target: usize,
        phase: Phase,
        fingers: u32,
        moved: Point,
        recent: &Recent,
        now: Instant,
    ) -> bool {
        // a redraw since it began may have removed this target
        let Some(swipe) = self
            .targets
            .get(target)
            .and_then(|target| target.handlers.swipe.clone())
        else {
            return false;
        };

        let speed = if phase == Phase::Ended {
            recent.speed(now)
        } else {
            Point { x: 0.0, y: 0.0 }
        };

        swipe(Swipe {
            phase,
            fingers,
            x: moved.x,
            y: moved.y,
            speed,
        });

        true
    }

    // gestures the compositor passes on

    // fingers resting on the touchpad stop whatever scroll area they're over from gliding
    pub fn hold(&self) -> bool {
        let Some(index) = self.find_topmost(|target| target.handlers.hold.is_some()) else {
            return false;
        };

        let Some(hold) = &self.targets[index].handlers.hold else {
            return false;
        };

        hold();

        true
    }

    pub fn swipe_began(&mut self, fingers: u32) -> bool {
        self.gesture = self
            .find_topmost(|target| target.handlers.swipe.is_some())
            .map(|target| Gesture {
                target,
                fingers,
                moved: Point { x: 0.0, y: 0.0 },
                recent: Recent::default(),
                kind: Kind::Swipe,
            });

        self.report_gesture(Phase::Began, Instant::now())
    }

    pub fn pinch_began(&mut self, fingers: u32) -> bool {
        self.gesture = self
            .find_topmost(|target| target.handlers.pinch.is_some())
            .map(|target| Gesture {
                target,
                fingers,
                moved: Point { x: 0.0, y: 0.0 },
                recent: Recent::default(),
                kind: Kind::Pinch {
                    scale: 1.0,
                    rotation: 0.0,
                },
            });

        self.report_gesture(Phase::Began, Instant::now())
    }

    // the fingers moved by this much since the last update; a pinch also gives its scale and turn
    pub fn gesture_moved(&mut self, by: Point, pinch: Option<(f32, f32)>, now: Instant) -> bool {
        let Some(gesture) = &mut self.gesture else {
            return false;
        };

        gesture.moved.x += by.x;
        gesture.moved.y += by.y;
        gesture.recent.push(now, by.x, by.y);

        if let (Kind::Pinch { scale, rotation }, Some((new_scale, turned))) =
            (&mut gesture.kind, pinch)
        {
            *scale = new_scale;
            *rotation += turned;
        }

        self.report_gesture(Phase::Moved, now)
    }

    pub fn gesture_ended(&mut self, cancelled: bool) -> bool {
        let phase = if cancelled {
            Phase::Cancelled
        } else {
            Phase::Ended
        };

        let reported = self.report_gesture(phase, Instant::now());

        self.gesture = None;

        reported
    }

    fn report_gesture(&self, phase: Phase, now: Instant) -> bool {
        let Some(gesture) = &self.gesture else {
            return false;
        };

        match gesture.kind {
            Kind::Swipe => self.send_swipe(
                gesture.target,
                phase,
                gesture.fingers,
                gesture.moved,
                &gesture.recent,
                now,
            ),

            Kind::Pinch { scale, rotation } => {
                let Some(pinch) = self
                    .targets
                    .get(gesture.target)
                    .and_then(|target| target.handlers.pinch.clone())
                else {
                    return false;
                };

                pinch(Pinch {
                    phase,
                    fingers: gesture.fingers,
                    scale,
                    rotation,
                    x: gesture.moved.x,
                    y: gesture.moved.y,
                });

                true
            }
        }
    }
}
