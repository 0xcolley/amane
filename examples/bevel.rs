use amane::{
    App, Bevel, Center, Color, Column, Gradient, LayerWindow, Rectangle, Shadow, Text, Weight,
    children,
};

fn main() {
    App::new().window(view).run();
}

fn view() -> LayerWindow {
    // a large clock of frosted glass, like macOS 26's lock screen, in the font's display cut
    let clock = Text::new("9:41")
        .size(120.0)
        .weight(Weight::Bold)
        .optical()
        .color(Color::rgba(242, 238, 246, 196))
        .bevel(Bevel::glass());

    // raised letters lit from the top left, falling into shade at the bottom right, with a shadow behind
    let raised = Text::new("raised")
        .size(64.0)
        .weight(Weight::Black)
        .color(Color::from("#f5c2e7"))
        .bevel(
            Bevel::new(3.0)
                .light(135.0)
                .shade(Color::rgba(60, 20, 50, 110))
                .shadow(
                    Shadow::drop(Color::BLACK)
                        .opacity(0.35)
                        .blur(6.0)
                        .offset(0.0, 3.0),
                ),
        );

    // wrapped lines, each in the middle
    let caption = Text::new("each line of a wrapped caption sits in the middle of its width")
        .size(14.0)
        .color(Color::WHITE)
        .wrap()
        .max_lines(2)
        .center();

    LayerWindow::new().width(420.0).height(360.0).child(
        Rectangle::new()
            .width(420.0)
            .height(360.0)
            .fill(Gradient::linear(
                160.0,
                [(0.0, "#7f849c"), (0.6, "#b4637a"), (1.0, "#f9a66c")],
            ))
            .align_child(Center, Center)
            .child(
                Column::new(children![
                    clock,
                    raised,
                    Rectangle::new().width(240.0).height(40.0).child(caption),
                ])
                .gap(16.0)
                .align(Center),
            ),
    )
}
