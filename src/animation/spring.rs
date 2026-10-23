use std::f32::consts::TAU;
use std::time::{Duration, Instant};

use super::moving;

/*
 * a number pulled toward its target by a spring; unlike an Animation it
 * keeps its speed when the target changes, so a motion interrupted halfway
 * turns around smoothly, and a damping below 1 lets it overshoot and bounce
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    // where it was, and how fast it moved, when it last set off
    from: f32,
    speed: f32,
    target: f32,
    started: Instant,

    // seconds one swing would take without damping, the spring's stiffness
    response: f32,
    damping: f32,
    precision: f32,
}

impl Spring {
    // starts at rest on `value`, nothing moves until `to` is called
    pub fn new(value: f32) -> Self {
        Self {
            from: value,
            speed: 0.0,
            target: value,
            started: Instant::now(),

            response: 0.35,
            damping: 1.0,
            precision: 0.01,
        }
    }

    // quick and without overshoot, for small things like switches
    pub fn snappy(value: f32) -> Self {
        Self::new(value).response(Duration::from_millis(250))
    }

    // without overshoot, the default
    pub fn smooth(value: f32) -> Self {
        Self::new(value)
    }

    // overshoots a little and settles back
    pub fn bouncy(value: f32) -> Self {
        Self::new(value)
            .response(Duration::from_millis(400))
            .damping(0.7)
    }

    /*
     * how long one swing takes, shorter is stiffer; 350 ms by default. This
     * and damping can change while it moves: it carries on from where it is
     */
    pub fn response(mut self, response: Duration) -> Self {
        self.rebase(Instant::now());
        self.response = response.as_secs_f32().max(0.001);

        self
    }

    /*
     * 1 arrives as fast as it can without overshooting, the default; below 1
     * bounces, 0.5 noticeably; above 1 creeps in more slowly
     */
    pub fn damping(mut self, damping: f32) -> Self {
        self.rebase(Instant::now());
        self.damping = damping.max(0.0);

        self
    }

    // how close counts as arrived, in the value's own units; 0.01 by default
    pub fn precision(mut self, precision: f32) -> Self {
        self.precision = precision.abs();

        self
    }

    // a change halfway through keeps where it is and how fast it is going
    pub fn to(&mut self, target: f32) {
        self.head_for(target, Instant::now());
    }

    // heads for `target` starting at `speed` units a second, like a flick let go of
    pub fn fling(&mut self, target: f32, speed: f32) {
        self.rebase(Instant::now());
        self.speed = speed;
        self.target = target;
    }

    // puts it straight at `value`, at rest, like while it's being dragged
    pub fn jump(&mut self, value: f32) {
        self.from = value;
        self.speed = 0.0;
        self.target = value;
        self.started = Instant::now();
    }

    // read in the view, and while it is still moving the window keeps drawing frames
    pub fn value(&self) -> f32 {
        let (value, _, settled) = self.at(Instant::now());

        if !settled {
            moving::set();
        }

        value
    }

    // units a second, 0 once it has arrived
    pub fn velocity(&self) -> f32 {
        self.state(Instant::now()).1
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    // true once it has arrived and stopped
    pub fn settled(&self) -> bool {
        self.at(Instant::now()).2
    }

    fn head_for(&mut self, target: f32, now: Instant) {
        if target == self.target {
            return;
        }

        self.rebase(now);
        self.target = target;
    }

    // sets off afresh from where it is at `now`, at the speed it has, toward the same target
    fn rebase(&mut self, now: Instant) {
        let (from, speed) = self.state(now);

        self.from = from;
        self.speed = speed;
        self.started = now;
    }

    fn state(&self, now: Instant) -> (f32, f32) {
        let (value, speed, _) = self.at(now);

        (value, speed)
    }

    /*
     * where it is and how fast it moves at `now`, and whether it has arrived:
     * the closed-form solution of a damped spring from where it set off, so
     * it's the same at any frame rate and costs nothing while nothing reads it
     */
    fn at(&self, now: Instant) -> (f32, f32, bool) {
        let elapsed = now.saturating_duration_since(self.started).as_secs_f32();

        let (gap, speed) = swing(
            self.from - self.target,
            self.speed,
            elapsed,
            self.response,
            self.damping,
        );

        // the size of what's left of the swing, which only ever shrinks
        let stiffness = TAU / self.response;
        let left = f32::hypot(gap, speed / stiffness);

        if left <= self.precision || !left.is_finite() {
            return (self.target, 0.0, true);
        }

        (self.target + gap, speed, false)
    }
}

// the gap to the target and the speed `elapsed` seconds after setting off
fn swing(gap: f32, speed: f32, elapsed: f32, response: f32, damping: f32) -> (f32, f32) {
    let stiffness = TAU / response;

    // close to 1 the bouncing and creeping formulas divide by almost nothing
    if (damping - 1.0).abs() < 1e-3 {
        let lead = speed + stiffness * gap;
        let decay = (-stiffness * elapsed).exp();

        return (
            (gap + lead * elapsed) * decay,
            (speed - stiffness * lead * elapsed) * decay,
        );
    }

    if damping < 1.0 {
        let fade = damping * stiffness;
        let swing = stiffness * (1.0 - damping * damping).sqrt();

        let lead = (speed + fade * gap) / swing;
        let decay = (-fade * elapsed).exp();
        let (sin, cos) = (swing * elapsed).sin_cos();

        return (
            (gap * cos + lead * sin) * decay,
            (speed * cos - (fade * lead + gap * swing) * sin) * decay,
        );
    }

    let root = (damping * damping - 1.0).sqrt();
    let slow = -stiffness * (damping - root);
    let fast = -stiffness * (damping + root);

    let quick = (speed - slow * gap) / (fast - slow);
    let lasting = gap - quick;

    let slow_decay = (slow * elapsed).exp();
    let fast_decay = (fast * elapsed).exp();

    (
        lasting * slow_decay + quick * fast_decay,
        lasting * slow * slow_decay + quick * fast * fast_decay,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: Duration = Duration::from_millis(4);

    // where the spring is every few milliseconds over two seconds
    fn path(spring: &Spring) -> Vec<f32> {
        (0..500)
            .map(|step| spring.at(spring.started + STEP * step).0)
            .collect()
    }

    fn heading_to(spring: Spring, target: f32) -> Spring {
        let mut spring = spring;

        spring.head_for(target, spring.started);

        spring
    }

    #[test]
    fn every_kind_arrives() {
        for damping in [0.3, 0.7, 1.0, 1.5, 3.0] {
            let spring = heading_to(Spring::new(0.0).damping(damping), 100.0);

            let (value, speed, settled) = spring.at(spring.started + Duration::from_secs(10));

            assert!(settled, "damping {damping} never settled");
            assert_eq!((value, speed), (100.0, 0.0));
        }
    }

    #[test]
    fn damping_of_one_or_more_never_overshoots() {
        for damping in [1.0, 1.2, 2.0] {
            let spring = heading_to(Spring::new(0.0).damping(damping), 100.0);

            let path = path(&spring);

            assert!(path.iter().all(|value| *value <= 100.0), "{damping}");
            assert!(path.windows(2).all(|pair| pair[1] >= pair[0]), "{damping}");
        }
    }

    #[test]
    fn damping_below_one_overshoots() {
        let spring = heading_to(Spring::bouncy(0.0), 100.0);

        let furthest = path(&spring).into_iter().fold(0.0, f32::max);

        assert!(furthest > 101.0, "only reached {furthest}");
        assert!(furthest < 120.0, "overshot to {furthest}");
    }

    #[test]
    fn a_new_target_keeps_the_position_and_speed() {
        for damping in [0.5, 1.0, 2.0] {
            let mut spring = heading_to(Spring::new(0.0).damping(damping), 100.0);

            let halfway = spring.started + Duration::from_millis(80);
            let (value, speed) = spring.state(halfway);

            spring.head_for(-50.0, halfway);

            let (turned_value, turned_speed) = spring.state(halfway);

            assert!((turned_value - value).abs() < 1e-3, "{damping}");
            assert!((turned_speed - speed).abs() < 1e-2, "{damping}");
            assert!(speed > 0.0, "{damping}");
        }
    }

    #[test]
    fn the_same_target_changes_nothing() {
        let mut spring = heading_to(Spring::new(0.0), 100.0);
        let before = spring;

        spring.head_for(100.0, spring.started + Duration::from_millis(50));

        assert_eq!(spring, before);
    }

    #[test]
    fn settling_takes_about_the_response() {
        let spring = heading_to(Spring::smooth(0.0).precision(0.5), 100.0);

        let settled = (0..500)
            .find(|step| spring.at(spring.started + STEP * *step).2)
            .map(|step| STEP * step)
            .expect("failed to settle in two seconds");

        assert!(settled > Duration::from_millis(200), "{settled:?}");
        assert!(settled < Duration::from_millis(600), "{settled:?}");
    }

    #[test]
    fn a_new_feel_carries_on_from_where_it_is() {
        let mut spring = Spring::new(0.0);

        spring.to(100.0);
        spring.started -= Duration::from_millis(60);

        let now = Instant::now();
        let (value, speed) = spring.state(now);

        let changed = spring.damping(0.5).response(Duration::from_millis(200));
        let (changed_value, changed_speed) = changed.state(changed.started);

        // the few microseconds between the two readings move it a little
        assert!(
            (changed_value - value).abs() < 0.05,
            "{value} became {changed_value}"
        );
        assert!(
            (changed_speed - speed).abs() < 5.0,
            "{speed} became {changed_speed}"
        );
        assert!(changed.started >= now);
    }

    #[test]
    fn a_fling_starts_at_its_speed() {
        let mut spring = Spring::new(0.0);

        spring.fling(0.0, 400.0);

        let (_, speed) = spring.state(spring.started);

        assert!((speed - 400.0).abs() < 1e-3);
        assert!(spring.at(spring.started + Duration::from_millis(50)).0 > 5.0);
    }
}
