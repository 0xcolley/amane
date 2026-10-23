use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::animation::moving;
use crate::input::{PIXELS_PER_LINE, Recent};
use crate::{Scroll, ScrollSource, Spring};

// how quickly a glide slows down, per second; after one second it has 5% of its speed left
const FRICTION: f32 = 3.0;

// in pixels a second: slower than this, a glide has stopped; let go slower, it doesn't start
const RESTING: f32 = 20.0;
const FLING: f32 = 80.0;
const FASTEST: f32 = 8000.0;

// how far past an end the content can be pulled, at most and as a share of the area
const FURTHEST_STRETCH: f32 = 160.0;
const STRETCH_SHARE: f32 = 0.4;

// how hard the pull gets: past an end, each pixel pulled moves the content less
const STRETCH_GIVE: f32 = 0.55;

// how quickly content pulled or flung past an end comes back
const BACK: Duration = Duration::from_millis(400);

thread_local! {
    // how far each scroll area has moved, kept apart from the widgets that are rebuilt each redraw
    static OFFSETS: RefCell<HashMap<&'static str, Offset>> = RefCell::new(HashMap::new());
}

// the most the content can scroll, and how far past an end it can stretch
#[derive(Debug, Clone, Copy)]
pub struct Room {
    farthest: f32,
    stretch: f32,
}

impl Room {
    pub fn new(farthest: f32, height: f32) -> Self {
        Self {
            farthest,
            stretch: (height * STRETCH_SHARE).min(FURTHEST_STRETCH),
        }
    }

    fn clamp(self, at: f32) -> f32 {
        at.clamp(0.0, self.farthest)
    }

    // the nearest end to something past one
    fn end(self, at: f32) -> f32 {
        if at < 0.0 { 0.0 } else { self.farthest }
    }

    // where content pulled to `at` shows: the further past an end, the harder the pull
    fn stretched(self, at: f32) -> f32 {
        let end = self.end(at);
        let past = (at - self.clamp(at)).abs();

        if past == 0.0 || self.stretch <= 0.0 {
            return self.clamp(at);
        }

        let shown = self.stretch * (1.0 - 1.0 / (past * STRETCH_GIVE / self.stretch + 1.0));

        end + shown * (at - end).signum()
    }

    // the pull that shows content at `shown`, the other way round
    fn unstretched(self, shown: f32) -> f32 {
        let end = self.end(shown);
        let past = (shown - self.clamp(shown)).abs().min(self.stretch * 0.99);

        if past == 0.0 || self.stretch <= 0.0 {
            return self.clamp(shown);
        }

        let pulled = self.stretch / STRETCH_GIVE * (1.0 / (1.0 - past / self.stretch) - 1.0);

        end + pulled * (shown - end).signum()
    }
}

#[derive(Debug, Clone, Default)]
struct Offset {
    // where the content sits, 0 at the top; past an end only while fingers pull it there
    at: f32,
    motion: Motion,

    // the fingers' recent moves, for how fast they let go
    recent: Recent,
}

#[derive(Debug, Clone, Copy, Default)]
enum Motion {
    #[default]
    Still,

    // fingers on the touchpad, which can pull it past an end
    Pulled,

    // gliding on after the fingers lifted, slowing down
    Gliding {
        from: f32,
        speed: f32,
        started: Instant,
    },

    // coming back from past an end
    Back(Spring),
}

// where the content shows this frame, moving it on if it glides or comes back
pub fn shown(id: &'static str, room: Room) -> f32 {
    with(id, |offset| offset.shown(room, Instant::now()))
}

pub fn scroll(id: &'static str, scroll: Scroll, room: Room) {
    with(id, |offset| offset.scroll(scroll, room, Instant::now()));
}

// fingers resting on the touchpad catch a glide where it is
pub fn hold(id: &'static str, room: Room) {
    with(id, |offset| offset.hold(room, Instant::now()));
}

fn with<T>(id: &'static str, change: impl FnOnce(&mut Offset) -> T) -> T {
    OFFSETS.with_borrow_mut(|offsets| change(offsets.entry(id).or_default()))
}

impl Offset {
    fn shown(&mut self, room: Room, now: Instant) -> f32 {
        match self.motion {
            Motion::Still => {
                // the content may have shrunk since
                self.at = room.clamp(self.at);

                self.at
            }

            Motion::Pulled => room.stretched(self.at),

            Motion::Gliding {
                from,
                speed,
                started,
            } => {
                let elapsed = now.saturating_duration_since(started).as_secs_f32();
                let (at, left) = glide(from, speed, elapsed);

                if at != room.clamp(at) {
                    self.bounce(room.end(at), from, speed, started);

                    return self.shown(room, now);
                }

                self.at = at;

                if left.abs() < RESTING {
                    self.motion = Motion::Still;
                } else {
                    moving::set();
                }

                at
            }

            Motion::Back(back) => {
                let (shown, _, settled) = back.at(now);

                if settled {
                    self.at = room.clamp(shown);
                    self.motion = Motion::Still;

                    return self.at;
                }

                moving::set();

                // a hard glide could fly further past the end than fingers can pull it
                shown.clamp(-room.stretch, room.farthest + room.stretch)
            }
        }
    }

    // a glide ran into an end: from the moment it got there, it flies on past and springs back
    fn bounce(&mut self, end: f32, from: f32, speed: f32, started: Instant) {
        let crossed = crossing(from, speed, end);
        let (_, speed_there) = glide(from, speed, crossed);

        let mut back = Spring::new(end).response(BACK).precision(0.5);

        back.fling_at(end, speed_there, started + Duration::from_secs_f32(crossed));

        self.at = end;
        self.motion = Motion::Back(back);
    }

    fn scroll(&mut self, scroll: Scroll, room: Room, now: Instant) {
        let moved = scroll.y * PIXELS_PER_LINE;

        if scroll.source != ScrollSource::Finger {
            // a wheel steps from wherever it is, even mid-glide
            self.at = room.clamp(self.shown(room, now) + moved);
            self.motion = Motion::Still;

            return;
        }

        if !matches!(self.motion, Motion::Pulled) {
            // touching again catches it where it shows, even past an end
            self.at = room.unstretched(self.shown(room, now));
            self.motion = Motion::Pulled;
            self.recent.clear();
        }

        if moved != 0.0 {
            self.at += moved;
            self.recent.push(now, 0.0, moved);
        }

        if scroll.stop {
            self.let_go(room, now);
        }
    }

    // the fingers lifted: back from past an end, glide on, or stay
    fn let_go(&mut self, room: Room, now: Instant) {
        let speed = self.recent.speed(now).y.clamp(-FASTEST, FASTEST);

        self.recent.clear();

        let shown = room.stretched(self.at);

        if shown != room.clamp(shown) {
            let mut back = Spring::new(shown).response(BACK).precision(0.5);

            back.fling_at(room.end(shown), 0.0, now);

            self.motion = Motion::Back(back);
        } else if speed.abs() > FLING {
            self.motion = Motion::Gliding {
                from: self.at,
                speed,
                started: now,
            };
        } else {
            self.motion = Motion::Still;
        }
    }

    fn hold(&mut self, room: Room, now: Instant) {
        // coming back from past an end goes on, so it never stays stretched
        if matches!(self.motion, Motion::Gliding { .. }) {
            self.at = room.clamp(self.shown(room, now));
            self.motion = Motion::Still;
        }
    }
}

// where a glide is, and how fast it still goes, `elapsed` seconds after setting off
fn glide(from: f32, speed: f32, elapsed: f32) -> (f32, f32) {
    let left = (-FRICTION * elapsed).exp();

    (from + speed / FRICTION * (1.0 - left), speed * left)
}

// how many seconds a glide takes to reach `to`, which it must get to
fn crossing(from: f32, speed: f32, to: f32) -> f32 {
    let share = 1.0 - (to - from) * FRICTION / speed;

    if share <= 0.0 || !share.is_finite() {
        return 0.0;
    }

    (-share.ln() / FRICTION).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(8);

    fn fingers(y: f32, stop: bool) -> Scroll {
        Scroll {
            x: 0.0,
            y: y / PIXELS_PER_LINE,
            source: ScrollSource::Finger,
            stop,
        }
    }

    fn wheel(steps: f32) -> Scroll {
        Scroll {
            y: steps,
            ..Scroll::default()
        }
    }

    // a quick flick down: 20 pixels every frame for a tenth of a second, then lift
    fn flick(offset: &mut Offset, room: Room, start: Instant) -> Instant {
        for step in 0..12 {
            offset.scroll(fingers(20.0, false), room, start + FRAME * step);
        }

        let lifted = start + FRAME * 12;

        offset.scroll(fingers(0.0, true), room, lifted);

        lifted
    }

    fn close(a: f32, b: f32, within: f32) -> bool {
        (a - b).abs() <= within
    }

    #[test]
    fn a_flick_glides_on_and_slows_to_a_stop() {
        let room = Room::new(5000.0, 400.0);
        let mut offset = Offset::default();
        let start = Instant::now();

        let lifted = flick(&mut offset, room, start);

        assert!(close(offset.shown(room, lifted), 240.0, 0.01));

        let soon = offset.shown(room, lifted + Duration::from_millis(100));
        let later = offset.shown(room, lifted + Duration::from_millis(400));
        let stopped = offset.shown(room, lifted + Duration::from_secs(3));

        assert!(soon > 400.0, "{soon}");
        assert!(later > soon);
        assert!(stopped > later);
        assert!(matches!(offset.motion, Motion::Still));

        // 2500 pixels a second over the friction, from where it was let go
        assert!(close(stopped, 240.0 + 2500.0 / FRICTION, 10.0), "{stopped}");
    }

    #[test]
    fn a_slow_lift_stays_put() {
        let room = Room::new(5000.0, 400.0);
        let mut offset = Offset::default();
        let start = Instant::now();

        for step in 0..10 {
            offset.scroll(fingers(5.0, false), room, start + FRAME * step);
        }

        // resting a moment before lifting
        let lifted = start + FRAME * 10 + Duration::from_millis(200);

        offset.scroll(fingers(0.0, true), room, lifted);

        assert!(matches!(offset.motion, Motion::Still));
        assert!(close(offset.shown(room, lifted), 50.0, 0.01));
    }

    #[test]
    fn touching_or_holding_catches_a_glide() {
        let room = Room::new(5000.0, 400.0);
        let mut offset = Offset::default();
        let start = Instant::now();

        let lifted = flick(&mut offset, room, start);
        let held = lifted + Duration::from_millis(100);
        let shown = offset.shown(room, held);

        offset.hold(room, held);

        assert!(close(
            offset.shown(room, held + Duration::from_secs(1)),
            shown,
            0.01
        ));

        let lifted = flick(&mut offset, room, held);
        let touched = lifted + Duration::from_millis(50);
        let shown = offset.shown(room, touched);

        offset.scroll(fingers(1.0, false), room, touched);

        let after = offset.shown(room, lifted + Duration::from_secs(1));

        assert!(close(after, shown + 1.0, 0.01), "{shown} {after}");
    }

    #[test]
    fn pulling_past_an_end_stretches_and_springs_back() {
        let room = Room::new(1000.0, 400.0);
        let mut offset = Offset::default();
        let start = Instant::now();
        let step = Duration::from_millis(40);

        // pulled down 300 pixels at the top, slowly
        for moved in 0..30 {
            offset.scroll(fingers(-10.0, false), room, start + step * moved);
        }

        let pulled = start + step * 30;
        let shown = offset.shown(room, pulled);

        assert!(shown < -50.0, "{shown}");
        assert!(shown > -room.stretch, "{shown}");

        offset.scroll(fingers(0.0, true), room, pulled);

        assert!(matches!(offset.motion, Motion::Back(_)));

        let coming = offset.shown(room, pulled + Duration::from_millis(100));

        assert!(coming > shown && coming < 0.0, "{coming}");
        assert_eq!(offset.shown(room, pulled + Duration::from_secs(3)), 0.0);
    }

    #[test]
    fn a_glide_into_an_end_bounces_off_it() {
        let room = Room::new(300.0, 400.0);
        let mut offset = Offset::default();
        let start = Instant::now();

        let lifted = flick(&mut offset, room, start);

        let furthest = (1..200)
            .map(|step| offset.shown(room, lifted + FRAME * step))
            .fold(0.0, f32::max);

        assert!(furthest > 320.0, "{furthest}");
        assert!(furthest <= 300.0 + room.stretch, "{furthest}");
        assert_eq!(offset.shown(room, lifted + Duration::from_secs(3)), 300.0);
    }

    #[test]
    fn the_bounce_starts_where_and_when_the_glide_reaches_the_end() {
        let (from, speed) = (0.0, 1200.0);
        let crossed = crossing(from, speed, 100.0);

        assert!(close(glide(from, speed, crossed).0, 100.0, 0.01));
        assert_eq!(crossing(from, speed, -5.0), 0.0);
    }

    #[test]
    fn a_wheel_steps_and_stays_inside() {
        let room = Room::new(100.0, 400.0);
        let mut offset = Offset::default();
        let now = Instant::now();

        offset.scroll(wheel(3.0), room, now);
        assert_eq!(offset.shown(room, now), 45.0);

        offset.scroll(wheel(10.0), room, now);
        assert_eq!(offset.shown(room, now), 100.0);

        offset.scroll(wheel(-20.0), room, now);
        assert_eq!(offset.shown(room, now), 0.0);
    }

    #[test]
    fn stretching_turns_back_round() {
        let room = Room::new(500.0, 400.0);

        for at in [-300.0, -20.0, 0.0, 250.0, 520.0, 900.0] {
            let shown = room.stretched(at);

            assert!(close(room.unstretched(shown), at, 0.01), "{at} {shown}");
        }
    }
}
