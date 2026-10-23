use std::time::Duration;

use amane::{
    App, Center, Color, Column, Full, LayerWindow, Parent, Phase, Rectangle, ScrollArea, Service,
    Spring, Stack, Swipe, Text, Widget, children,
};

const WIDTH: f32 = 360.0;
const HEIGHT: f32 = 440.0;
const PADDING: f32 = 20.0;

const COLORS: [&str; 3] = ["#89b4fa", "#a6e3a1", "#f5c2e7"];
const LISTS: [&str; 3] = ["first", "second", "third"];

// let go faster than this, in pixels a second, and the page turns however little it moved
const FLICK: f32 = 300.0;

// how far past the first or last page the fingers can pull
const PULL: f32 = 60.0;

/*
 * three pages side by side: two fingers swipe between them, each holds a
 * list that glides on after the fingers lift, and the button sinks while held
 */
struct Pager {
    page: usize,

    // how far the row of pages has moved left
    offset: Spring,

    // the button's size, smaller while it's held down
    press: Spring,
}

impl Service for Pager {
    fn new() -> Self {
        Self {
            page: 0,
            offset: Spring::bouncy(0.0),
            press: Spring::snappy(1.0),
        }
    }

    // nothing changes on its own, only through input
    fn interval() -> Duration {
        Duration::from_secs(3600)
    }
}

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let pager = Pager::read();
    let offset = pager.offset.value();
    let press = pager.press.value();

    let pages: Vec<Box<dyn Widget>> = (0..COLORS.len())
        .map(|index| Box::new(page(index, index as f32 * WIDTH - offset, press)) as Box<dyn Widget>)
        .collect();

    LayerWindow::new().width(WIDTH).height(HEIGHT).child(
        Rectangle::new()
            .width(WIDTH)
            .height(HEIGHT)
            .fill("#1e1e2e")
            .clip()
            .on_swipe(swiped)
            .child(Stack::new(pages)),
    )
}

fn page(index: usize, left: f32, press: f32) -> Rectangle {
    let rows: Vec<Box<dyn Widget>> = (1..=40)
        .map(|number| {
            let label = format!("page {} · row {number}", index + 1);

            Box::new(Text::new(label).color(Color::from("#cdd6f4"))) as Box<dyn Widget>
        })
        .collect();

    let list_height = HEIGHT - PADDING * 2.0 - 44.0 - 16.0;

    Rectangle::new()
        .width(WIDTH)
        .height(HEIGHT)
        .padding(PADDING)
        .translate(left, 0.0)
        .child(
            Column::new(children![
                button(COLORS[index], press),
                ScrollArea::new(LISTS[index], Column::new(rows).gap(10.0)).height(list_height),
            ])
            .gap(16.0),
        )
}

// sinks as soon as it's pressed, and springs back when let go, wherever the pointer is by then
fn button(color: &str, press: f32) -> Rectangle {
    Rectangle::new()
        .width(Parent)
        .height(44.0)
        .radius(Full)
        .fill(Color::from(color))
        .scale(press)
        .on_press(|_| Pager::write().press.to(0.94))
        .on_release(|_| Pager::write().press.to(1.0))
        .on_click(|_| turn(1))
        .align_child(Center, Center)
        .child(Text::new("next page").color(Color::from("#1e1e2e")))
}

// the pages follow the fingers, then settle on the nearest page, or the next one if flicked
fn swiped(swipe: Swipe) {
    let mut pager = Pager::write();

    let resting = pager.page as f32 * WIDTH;
    let last = (COLORS.len() - 1) as f32 * WIDTH;

    match swipe.phase {
        // fingers moving left bring in the page on the right
        Phase::Began | Phase::Moved => {
            pager
                .offset
                .jump((resting - swipe.x).clamp(-PULL, last + PULL));
        }

        Phase::Ended => {
            let next = swipe.x < -WIDTH / 2.0 || swipe.speed.x < -FLICK;
            let back = swipe.x > WIDTH / 2.0 || swipe.speed.x > FLICK;

            if next {
                pager.page = (pager.page + 1).min(COLORS.len() - 1);
            } else if back {
                pager.page = pager.page.saturating_sub(1);
            }

            let target = pager.page as f32 * WIDTH;

            // carries on at the fingers' speed, so it doesn't stall as they lift
            pager.offset.fling(target, -swipe.speed.x);
        }

        Phase::Cancelled => pager.offset.to(resting),
    }
}

fn turn(by: usize) {
    let mut pager = Pager::write();

    pager.page = (pager.page + by) % COLORS.len();

    let target = pager.page as f32 * WIDTH;

    pager.offset.to(target);
}
