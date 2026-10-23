use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::Point;

// where a gesture is in its life; every one that begins ends or is cancelled
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Began,
    Moved,
    Ended,

    // the compositor took the fingers back, so whatever the gesture did should be undone
    Cancelled,
}

/*
 * fingers sliding sideways together on a touchpad: two fingers moving more
 * across than up or down, or three or more fingers the compositor passes on
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Swipe {
    pub phase: Phase,
    pub fingers: u32,

    // how far the fingers have moved since they began, in logical pixels; positive is right and down
    pub x: f32,
    pub y: f32,

    // how fast they were moving, in pixels a second, for a fling once they lift
    pub speed: Point,
}

// fingers spreading apart or closing in on a touchpad
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pinch {
    pub phase: Phase,
    pub fingers: u32,

    // 1 where it began, 2 once the fingers are twice as far apart
    pub scale: f32,

    // degrees turned clockwise since it began
    pub rotation: f32,

    // how far the middle of the fingers has moved, in logical pixels
    pub x: f32,
    pub y: f32,
}

// how far back movement counts toward the speed something is let go with
const RECENT: Duration = Duration::from_millis(100);

// fingers that rested this long before lifting were let go standing still
const RESTED: Duration = Duration::from_millis(60);

// recent movement, for how fast something was going when it was let go
#[derive(Debug, Clone, Default)]
pub struct Recent {
    moves: VecDeque<(Instant, f32, f32)>,
}

impl Recent {
    pub fn push(&mut self, at: Instant, x: f32, y: f32) {
        self.moves.push_back((at, x, y));

        while self
            .moves
            .front()
            .is_some_and(|(first, ..)| at.duration_since(*first) > RECENT)
        {
            self.moves.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.moves.clear();
    }

    // pixels a second over the last moments, 0 if it had stopped before `now`
    pub fn speed(&self, now: Instant) -> Point {
        let still = Point { x: 0.0, y: 0.0 };

        let (Some((first, ..)), Some((last, ..))) = (self.moves.front(), self.moves.back()) else {
            return still;
        };

        if now.saturating_duration_since(*last) > RESTED {
            return still;
        }

        // the first move happened before the span starts, so it isn't counted
        let span = last.duration_since(*first).as_secs_f32();

        if span < 0.008 {
            return still;
        }

        let (x, y) = self
            .moves
            .iter()
            .skip(1)
            .fold((0.0, 0.0), |(x, y), (_, dx, dy)| (x + dx, y + dy));

        Point {
            x: x / span,
            y: y / span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: Duration = Duration::from_millis(8);

    #[test]
    fn steady_moves_give_their_speed() {
        let start = Instant::now();
        let mut recent = Recent::default();

        // 4 pixels every 8 ms is 500 a second
        for step in 0..20 {
            recent.push(start + FRAME * step, 4.0, -2.0);
        }

        let speed = recent.speed(start + FRAME * 20);

        assert!((speed.x - 500.0).abs() < 1.0, "{speed:?}");
        assert!((speed.y + 250.0).abs() < 1.0, "{speed:?}");
    }

    #[test]
    fn a_rest_before_lifting_means_no_fling() {
        let start = Instant::now();
        let mut recent = Recent::default();

        for step in 0..10 {
            recent.push(start + FRAME * step, 4.0, 0.0);
        }

        let later = start + FRAME * 10 + Duration::from_millis(200);

        assert_eq!(recent.speed(later), Point { x: 0.0, y: 0.0 });
    }

    #[test]
    fn only_recent_moves_count() {
        let start = Instant::now();
        let mut recent = Recent::default();

        // fast at first, then slow for longer than the window
        for step in 0..10 {
            recent.push(start + FRAME * step, 40.0, 0.0);
        }

        for step in 10..40 {
            recent.push(start + FRAME * step, 1.0, 0.0);
        }

        let speed = recent.speed(start + FRAME * 40);

        assert!((speed.x - 125.0).abs() < 1.0, "{speed:?}");
    }
}
