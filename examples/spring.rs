use std::time::Duration;

use amane::{App, Button, Color, Column, Full, LayerWindow, Rectangle, Service, Spring, children};

// three bars that spring to the other side on every click, each with its own feel
struct Bars {
    right: bool,

    snappy: Spring,
    smooth: Spring,
    bouncy: Spring,
}

const LEFT: f32 = 60.0;
const RIGHT: f32 = 380.0;

impl Service for Bars {
    fn new() -> Self {
        Self {
            right: false,

            snappy: Spring::snappy(LEFT),
            smooth: Spring::smooth(LEFT),
            bouncy: Spring::bouncy(LEFT),
        }
    }

    // nothing changes on its own, only through clicks
    fn interval() -> Duration {
        Duration::from_secs(3600)
    }
}

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let bars = Bars::read();

    LayerWindow::new().width(440.0).height(160.0).child(
        Rectangle::new()
            .width(440.0)
            .height(160.0)
            .padding(20.0)
            .fill("#1e1e2e")
            .on_click(clicked)
            .child(
                Column::new(children![
                    bar(bars.snappy.value(), "#a6e3a1"),
                    bar(bars.smooth.value(), "#89b4fa"),
                    bar(bars.bouncy.value(), "#f5c2e7"),
                ])
                .gap(20.0),
            ),
    )
}

fn bar(width: f32, color: &str) -> Rectangle {
    Rectangle::new()
        .width(width)
        .height(26.0)
        .radius(Full)
        .fill(Color::from(color))
}

// a click while they're still moving turns them around without a jerk
fn clicked(_: Button) {
    let mut bars = Bars::write();

    bars.right = !bars.right;

    let target = if bars.right { RIGHT } else { LEFT };

    bars.snappy.to(target);
    bars.smooth.to(target);
    bars.bouncy.to(target);
}
