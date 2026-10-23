use std::time::Duration;

use amane::{App, Button, Center, Color, Full, Idle, LayerWindow, Rectangle, Service, Text};

// a pill that notices when nobody's been here for five seconds, and a click that keeps the screen awake
fn main() {
    Idle::watch("away", Some(Duration::from_secs(5)), changed);

    App::new().window(view).run();
}

// runs on the event loop: fine for starting things, like a lock or a command
fn changed(idle: bool) {
    println!("idle: {idle}");
}

fn view() -> LayerWindow {
    let idle = Idle::read();

    let (label, fill) = if idle.awake() {
        ("kept awake", "#f9e2af")
    } else if idle.idle("away") {
        ("nobody's here", "#45475a")
    } else {
        ("here", "#a6e3a1")
    };

    LayerWindow::new().width(220.0).height(48.0).child(
        Rectangle::new()
            .width(220.0)
            .height(48.0)
            .radius(Full)
            .fill(Color::from(fill))
            .align_child(Center, Center)
            .on_click(toggled)
            .child(Text::new(label).color(Color::from("#1e1e2e"))),
    )
}

fn toggled(_: Button) {
    let awake = Idle::read().awake();

    Idle::keep_awake(!awake);
}
