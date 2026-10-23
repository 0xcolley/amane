mod fingers;

use std::path::PathBuf;

use crate::{Button, Cursor};

use super::Target;

use fingers::{Fingers, Gesture};

#[derive(Default)]
pub struct Pointer {
    // the hit areas of the last drawn frame
    targets: Vec<Target>,

    x: f32,
    y: f32,

    // positions in targets, which stay the same across redraws of an unchanged layout
    hovered: Vec<usize>,
    pressed: Option<usize>,
    dragged: Option<usize>,

    // the target pressed on with on_press or on_release, and the button, until it's let go
    held: Option<(usize, Button)>,

    // what a run of finger scrolling turned out to be, until the fingers lift
    fingers: Option<Fingers>,

    // a swipe or pinch the compositor passed on, and the target it began over
    gesture: Option<Gesture>,
}

impl Pointer {
    // the hit areas of a new frame replace the last frame's
    pub fn set_targets(&mut self, targets: Vec<Target>) {
        self.targets = targets;
    }

    // the innermost target under the pointer that has the handler asked for
    fn find_topmost(&self, has_handler: fn(&Target) -> bool) -> Option<usize> {
        let mut topmost = None;

        // children are collected after their parents, so the last match is the innermost
        for (index, target) in self.targets.iter().enumerate() {
            if target.contains(self.x, self.y) && has_handler(target) {
                topmost = Some(index);
            }
        }

        topmost
    }

    // hover

    pub fn move_to(&mut self, x: f32, y: f32) -> bool {
        self.x = x;
        self.y = y;

        let mut inside = Vec::new();

        for (index, target) in self.targets.iter().enumerate() {
            if target.handlers.hover.is_some() && target.contains(x, y) {
                inside.push(index);
            }
        }

        self.update_hovered(inside)
    }

    pub fn leave(&mut self) -> bool {
        // a button let go outside the window is not a click
        self.pressed = None;

        // the release will not reach this window, so the press and the drag end here
        let released = self.held.is_some_and(|(_, button)| self.let_go(button));
        let ended = self.stop_drag();

        self.update_hovered(Vec::new()) || released || ended
    }

    // tells the targets the pointer entered or left, and returns whether any were told
    fn update_hovered(&mut self, inside: Vec<usize>) -> bool {
        let mut changed = false;

        for &index in &self.hovered {
            if !inside.contains(&index) {
                changed |= self.notify(index, false);
            }
        }

        for &index in &inside {
            if !self.hovered.contains(&index) {
                changed |= self.notify(index, true);
            }
        }

        self.hovered = inside;

        changed
    }

    fn notify(&self, index: usize, inside: bool) -> bool {
        // a redraw since the pointer entered may have removed this target
        let Some(target) = self.targets.get(index) else {
            return false;
        };

        let Some(hover) = &target.handlers.hover else {
            return false;
        };

        hover(inside);

        true
    }

    // motion

    // tells the innermost target under the pointer where the pointer is on it
    pub fn report_motion(&self) -> bool {
        let Some(index) = self.find_topmost(|target| target.handlers.motion.is_some()) else {
            return false;
        };

        let target = &self.targets[index];

        let Some(motion) = &target.handlers.motion else {
            return false;
        };

        let point = target.point(self.x, self.y);

        motion(point);

        true
    }

    // the look asked for by the innermost target under the pointer
    pub fn cursor(&self) -> Cursor {
        let Some(index) = self.find_topmost(|target| target.handlers.cursor.is_some()) else {
            return Cursor::Default;
        };

        self.targets[index]
            .handlers
            .cursor
            .unwrap_or(Cursor::Default)
    }

    // click

    // returns whether an on_press ran
    pub fn press(&mut self, button: Button) -> bool {
        self.pressed = self.find_topmost(|target| target.handlers.click.is_some());

        self.held = self
            .find_topmost(|target| {
                target.handlers.press.is_some() || target.handlers.release.is_some()
            })
            .map(|index| (index, button));

        let Some((index, _)) = self.held else {
            return false;
        };

        let Some(press) = &self.targets[index].handlers.press else {
            return false;
        };

        press(button);

        true
    }

    // the target pressed on hears the button go up, wherever the pointer is by then
    pub fn let_go(&mut self, button: Button) -> bool {
        let Some((index, held)) = self.held else {
            return false;
        };

        if held != button {
            return false;
        }

        self.held = None;

        // a redraw since the press may have removed this target
        let Some(release) = self
            .targets
            .get(index)
            .and_then(|target| target.handlers.release.clone())
        else {
            return false;
        };

        release(button);

        true
    }

    // a click only counts when the button is let go over the widget it was pressed on
    pub fn release(&mut self, button: Button) -> bool {
        let Some(pressed) = self.pressed.take() else {
            return false;
        };

        let released = self.find_topmost(|target| target.handlers.click.is_some());

        if released != Some(pressed) {
            return false;
        }

        let Some(click) = &self.targets[pressed].handlers.click else {
            return false;
        };

        click(button);

        true
    }

    // drag

    pub fn start_drag(&mut self) -> bool {
        self.dragged = self.find_topmost(|target| target.handlers.drag.is_some());

        self.drag()
    }

    // returns whether an on_drag_end ran
    pub fn stop_drag(&mut self) -> bool {
        let Some(index) = self.dragged.take() else {
            return false;
        };

        let Some(drag_end) = self
            .targets
            .get(index)
            .and_then(|target| target.handlers.drag_end.clone())
        else {
            return false;
        };

        drag_end();

        true
    }

    // keeps reporting to the target pressed on, even after the pointer leaves it
    pub fn drag(&self) -> bool {
        let Some(index) = self.dragged else {
            return false;
        };

        // a redraw since the press may have removed this target
        let Some(target) = self.targets.get(index) else {
            return false;
        };

        let Some(drag) = &target.handlers.drag else {
            return false;
        };

        let point = target.point(self.x, self.y);

        drag(point);

        true
    }

    // drop

    // returns whether a handler ran
    pub fn drop_files(&mut self, x: f32, y: f32, files: Vec<PathBuf>) -> bool {
        self.x = x;
        self.y = y;

        let Some(index) = self.find_topmost(|target| target.handlers.drop.is_some()) else {
            return false;
        };

        let Some(drop) = &self.targets[index].handlers.drop else {
            return false;
        };

        drop(files);

        true
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::graphics::Area;
    use crate::input::Handlers;
    use crate::{Point, Scroll, ScrollSource, Swipe};

    type Log = Rc<RefCell<Vec<String>>>;

    fn note(log: &Log, entry: impl Into<String>) {
        log.borrow_mut().push(entry.into());
    }

    // a button on the left that sinks while held, and a list on the right that scrolls and swipes
    fn pointer(log: &Log) -> Pointer {
        let (press, release, click, ended) = (log.clone(), log.clone(), log.clone(), log.clone());
        let (scroll, swipe) = (log.clone(), log.clone());

        let button = Handlers {
            press: Some(Rc::new(move |_| note(&press, "press"))),
            release: Some(Rc::new(move |_| note(&release, "release"))),
            click: Some(Rc::new(move |_| note(&click, "click"))),
            drag: Some(Rc::new(|_| {})),
            drag_end: Some(Rc::new(move || note(&ended, "drag end"))),
            ..Handlers::default()
        };

        let list = Handlers {
            scroll: Some(Rc::new(move |moved: Scroll| {
                note(&scroll, format!("scroll {} {}", moved.y, moved.stop));
            })),
            swipe: Some(Rc::new(move |moved: Swipe| {
                note(&swipe, format!("swipe {:?} {}", moved.phase, moved.x));
            })),
            ..Handlers::default()
        };

        let mut pointer = Pointer::default();

        pointer.set_targets(vec![
            Target::new(Area::new(0.0, 0.0, 100.0, 100.0), button),
            Target::new(Area::new(100.0, 0.0, 100.0, 100.0), list),
        ]);

        pointer
    }

    // the pointer's own button handling, the way the wayland side calls it
    fn button_down(pointer: &mut Pointer) {
        pointer.press(Button::Left);
        pointer.start_drag();
    }

    fn button_up(pointer: &mut Pointer) {
        pointer.stop_drag();
        pointer.let_go(Button::Left);
        pointer.release(Button::Left);
    }

    fn fingers(x: f32, y: f32, stop: bool) -> (Scroll, Point) {
        let scroll = Scroll {
            x: x / 15.0,
            y: y / 15.0,
            source: ScrollSource::Finger,
            stop,
        };

        // natural scrolling: the content follows the fingers, so they move the other way
        (scroll, Point { x: -x, y: -y })
    }

    #[test]
    fn a_press_and_release_on_the_same_spot_clicks() {
        let log = Log::default();
        let mut pointer = pointer(&log);

        pointer.move_to(50.0, 50.0);
        button_down(&mut pointer);
        button_up(&mut pointer);

        assert_eq!(*log.borrow(), ["press", "drag end", "release", "click"]);
    }

    #[test]
    fn the_release_reaches_the_pressed_target_after_the_pointer_leaves_it() {
        let log = Log::default();
        let mut pointer = pointer(&log);

        pointer.move_to(50.0, 50.0);
        button_down(&mut pointer);
        pointer.move_to(150.0, 50.0);
        button_up(&mut pointer);

        // let go elsewhere: no click, but the button still springs back
        assert_eq!(*log.borrow(), ["press", "drag end", "release"]);
    }

    #[test]
    fn leaving_the_window_lets_go() {
        let log = Log::default();
        let mut pointer = pointer(&log);

        pointer.move_to(50.0, 50.0);
        button_down(&mut pointer);
        pointer.leave();
        button_up(&mut pointer);

        assert_eq!(*log.borrow(), ["press", "release", "drag end"]);
    }

    #[test]
    fn fingers_moving_sideways_over_a_swipe_target_swipe() {
        let log = Log::default();
        let mut pointer = pointer(&log);
        let start = Instant::now();
        let frame = Duration::from_millis(8);

        pointer.move_to(150.0, 50.0);

        // too little to tell at first, then clearly sideways
        for step in 0..4 {
            let (scroll, moved) = fingers(3.0, 0.5, false);

            pointer.scroll(scroll, moved, start + frame * step);
        }

        let (scroll, moved) = fingers(0.0, 0.0, true);

        pointer.scroll(scroll, moved, start + frame * 4);

        assert_eq!(
            *log.borrow(),
            [
                "swipe Began -6",
                "swipe Moved -9",
                "swipe Moved -12",
                "swipe Ended -12"
            ]
        );
    }

    #[test]
    fn fingers_moving_up_and_down_scroll_and_stop() {
        let log = Log::default();
        let mut pointer = pointer(&log);
        let now = Instant::now();

        pointer.move_to(150.0, 50.0);

        for _ in 0..2 {
            let (scroll, moved) = fingers(0.0, 15.0, false);

            pointer.scroll(scroll, moved, now);
        }

        let (scroll, moved) = fingers(0.0, 0.0, true);

        pointer.scroll(scroll, moved, now);

        assert_eq!(
            *log.borrow(),
            ["scroll 1 false", "scroll 1 false", "scroll 0 true"]
        );
    }

    #[test]
    fn a_wheel_only_ever_scrolls() {
        let log = Log::default();
        let mut pointer = pointer(&log);

        pointer.move_to(150.0, 50.0);

        let sideways = Scroll {
            x: 3.0,
            ..Scroll::default()
        };

        pointer.scroll(sideways, Point { x: -45.0, y: 0.0 }, Instant::now());

        assert_eq!(*log.borrow(), ["scroll 0 false"]);
    }
}
