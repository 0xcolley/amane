# Scroll Area

`ScrollArea` shows part of a widget that's too tall for its space, and scrolls it with the mouse wheel or touchpad. Fingers on a touchpad can flick it, and it glides on after they lift.

## A scrolling list

```rust
use amane::{App, Column, LayerWindow, Rectangle, ScrollArea, Text, Widget};

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    let mut rows: Vec<Box<dyn Widget>> = Vec::new();

    for number in 1..=50 {
        rows.push(Box::new(Text::new(format!("item {number}"))));
    }

    LayerWindow::new().width(300.0).height(400.0).child(
        Rectangle::new()
            .width(300.0)
            .height(400.0)
            .fill("#ffffff")
            .child(ScrollArea::new("items", Column::new(rows))),
    )
}
```

## How scrolling works

- A `ScrollArea` is `Parent`-sized by default, so it takes all the room its parent gives it. Here, that's 300 by 400 pixels.
- Its child, the column, is as tall as its 50 rows. Only the part that fits inside the area is shown.
- The view is rebuilt on every frame, so the scroll area can't remember how far it's scrolled. Amane keeps the position for you under the name you pass, here `"items"`. Give each scroll area its own name. Two areas with the same name scroll together.

## Options

```rust,ignore
ScrollArea::new("apps", Column::new(rows))
    .width(300.0)
    .height(Parent)
```

`width` and `height` default to `Parent`.

Scrolling is vertical only.

## Wheels and fingers

A mouse wheel moves the content a step at a time and stops at the ends.

Fingers on a touchpad move it with them, and then:

- **Flick and lift:** the content glides on at the speed the fingers were moving and slows to a stop, about a second later for a fast flick. A glide never asks for frames once it has stopped.
- **Rest before lifting:** it stays where it is.
- **Touch while it glides:** fingers resting on the touchpad catch it where it is, as does scrolling again. Catching needs a compositor that sends hold gestures; niri does.
- **Pull past an end:** the content stretches, harder the further it goes, up to 40% of the area's height or 160 pixels. Let go and it springs back.
- **Glide into an end:** it runs a little past, by how fast it was going, and springs back.

All of this happens inside the scroll area. Nothing needs setting up, and the content never stays past an end.

`ScrollSource::Continuous` scrolling, like a trackpoint's, moves the content as it goes and stops at the ends, but never glides or stretches.

## Rounded corners

Put the scroll area inside a rounded rectangle with `clip`, so rows scrolling past the corners are cut off along the curve:

```rust,ignore
Rectangle::new()
    .width(300.0)
    .height(400.0)
    .radius(24.0)
    .fill(Color::WHITE)
    .clip()
    .child(ScrollArea::new("items", Column::new(rows)))
```

## Rows that take input

Rows inside a scroll area take clicks like anywhere else. Rows scrolled out of view can't be clicked.

A row with its own `on_scroll`, like a volume slider, takes the scroll while the pointer is over it, and the area doesn't move.

A row with `on_swipe` takes fingers moving sideways, and the area still scrolls when they move up or down ([Input](input.md#swipes)).
