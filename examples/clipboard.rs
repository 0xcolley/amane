use amane::{
    App, Button, Center, Clip, ClipKind, Clipboard, Color, Column, Image, Keyboard, LayerWindow,
    Rectangle, Row, Service, Start, Text, TextInput, Widget, children,
};

const WIDTH: f32 = 360.0;
const ROW: f32 = 44.0;
const SHOWN: usize = 8;

// the latest copies: click one to copy it again, right click to forget it; Ctrl+V pastes into the field
fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let clipboard = Clipboard::read();

    let field = Rectangle::new()
        .width(WIDTH)
        .height(40.0)
        .fill(Color::WHITE)
        .padding(10.0)
        .child(
            TextInput::new("paste")
                .size(16.0)
                .placeholder("Ctrl+V pastes here"),
        );

    let mut rows: Vec<Box<dyn Widget>> = vec![Box::new(field)];

    for clip in clipboard.clips().iter().take(SHOWN) {
        let current = clipboard.current() == Some(clip.id());

        rows.push(Box::new(row(clip, current)));
    }

    let height = 40.0 + clipboard.clips().len().min(SHOWN) as f32 * ROW;

    LayerWindow::new()
        .width(WIDTH)
        .height(height)
        .keyboard(Keyboard::OnDemand)
        .child(
            Rectangle::new()
                .width(WIDTH)
                .height(height)
                .fill(Color::from("#1e1e2e"))
                .child(Column::new(rows)),
        )
}

// the clipboard holds the current one, which is lit
fn row(clip: &Clip, current: bool) -> Rectangle {
    let id = clip.id();

    let fill = if current {
        Color::from("#313244")
    } else {
        Color::TRANSPARENT
    };

    let base = Rectangle::new()
        .width(WIDTH)
        .height(ROW)
        .fill(fill)
        .padding(10.0)
        .align_child(Start, Center)
        .on_click(move |button| match button {
            Button::Right => Clipboard::remove(id),
            _ => Clipboard::copy(id),
        });

    match clip.kind() {
        ClipKind::Text(text) => base.child(
            Text::new(text.lines().next().unwrap_or_default())
                .color(Color::from("#cdd6f4"))
                .elide(),
        ),

        ClipKind::Image {
            path,
            width,
            height,
        } => base.child(
            Row::new(children![
                Rectangle::new()
                    .width(48.0)
                    .height(32.0)
                    .fill(Image::contain(path).thumbnail(96, 64)),
                Text::new(format!("{width}×{height}")).color(Color::from("#a6adc8")),
            ])
            .gap(10.0)
            .align(Center),
        ),
    }
}
