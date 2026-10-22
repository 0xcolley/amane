use crate::graphics::{self, PathBuilder, Segment};

use super::{Shape, Style};

// a free-form outline drawn step by step in canvas coordinates
pub struct Path {
    pub(crate) steps: PathBuilder,
    pub(crate) style: Style,

    // the part of the path's own coordinates that fills the canvas, none for canvas coordinates
    pub(crate) view_box: Option<[f32; 4]>,
}

impl Path {
    pub fn new() -> Self {
        Self {
            steps: PathBuilder::new(),
            style: Style::default(),
            view_box: None,
        }
    }

    // lifts the pen and puts it down here, without drawing
    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.steps.move_to(x, y);

        self
    }

    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.steps.line_to(x, y);

        self
    }

    // a curve bent toward the handle point
    pub fn quad_to(mut self, handle_x: f32, handle_y: f32, x: f32, y: f32) -> Self {
        self.steps.quad_to(handle_x, handle_y, x, y);

        self
    }

    // a curve bent by two handle points, the first near the start and the second near the end
    pub fn cubic_to(
        mut self,
        first_x: f32,
        first_y: f32,
        second_x: f32,
        second_y: f32,
        x: f32,
        y: f32,
    ) -> Self {
        self.steps
            .cubic_to(first_x, first_y, second_x, second_y, x, y);

        self
    }

    // same angles as Arc: degrees, 0 straight up, growing clockwise
    pub fn arc(
        mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        start: f32,
        sweep: f32,
    ) -> Self {
        self.steps.arc(center_x, center_y, radius, start, sweep);

        self
    }

    // a straight line back to where this piece started
    pub fn close(mut self) -> Self {
        self.steps.close();

        self
    }

    // the steps an svg path's d attribute describes, like an icon's; data that can't be read adds nothing
    pub fn svg(mut self, data: &str) -> Self {
        self.steps.svg(data);

        self
    }

    /*
     * draws this part of the path's own coordinates as big as fits the canvas
     * and centered, like an svg's viewBox, so an icon drawn on a 24 grid fills
     * a canvas of any size
     */
    pub fn view_box(mut self, x: f32, y: f32, width: f32, height: f32) -> Self {
        self.view_box = Some([x, y, width, height]);

        self
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

impl Shape for Path {
    fn trace(&self, width: f32, height: f32) -> Option<graphics::BezierPath> {
        let path = self.steps.clone().finish()?;

        let Some([x, y, view_width, view_height]) = self.view_box else {
            return Some(path);
        };

        // an empty view box has nothing to stretch over the canvas
        if view_width <= 0.0 || view_height <= 0.0 {
            return None;
        }

        let scale = f32::min(width / view_width, height / view_height);

        let left = (width - view_width * scale) / 2.0 - x * scale;
        let top = (height - view_height * scale) / 2.0 - y * scale;

        let place = |px: f32, py: f32| (left + px * scale, top + py * scale);

        let segments = path
            .segments
            .into_iter()
            .map(|segment| match segment {
                Segment::MoveTo(px, py) => {
                    let (x, y) = place(px, py);

                    Segment::MoveTo(x, y)
                }

                Segment::LineTo(px, py) => {
                    let (x, y) = place(px, py);

                    Segment::LineTo(x, y)
                }

                Segment::QuadTo(hx, hy, px, py) => {
                    let (handle_x, handle_y) = place(hx, hy);
                    let (x, y) = place(px, py);

                    Segment::QuadTo(handle_x, handle_y, x, y)
                }

                Segment::CubicTo(ax, ay, bx, by, px, py) => {
                    let (first_x, first_y) = place(ax, ay);
                    let (second_x, second_y) = place(bx, by);
                    let (x, y) = place(px, py);

                    Segment::CubicTo(first_x, first_y, second_x, second_y, x, y)
                }

                Segment::Close => Segment::Close,
            })
            .collect();

        Some(graphics::BezierPath { segments })
    }

    fn style(&self) -> &Style {
        &self.style
    }

    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_box_fills_the_canvas() {
        let path = Path::new()
            .svg("M 0 0 L 56 56")
            .view_box(0.0, 0.0, 56.0, 56.0);

        let traced = path
            .trace(16.0, 16.0)
            .expect("failed to trace the test path");

        assert_eq!(
            traced.segments,
            vec![Segment::MoveTo(0.0, 0.0), Segment::LineTo(16.0, 16.0)]
        );
    }

    #[test]
    fn view_box_keeps_its_shape_and_centers() {
        // twice as wide as tall, on a square canvas: it fits the width and sits in the middle
        let path = Path::new()
            .svg("M 10 0 L 30 10")
            .view_box(10.0, 0.0, 20.0, 10.0);

        let traced = path
            .trace(20.0, 20.0)
            .expect("failed to trace the test path");

        assert_eq!(
            traced.segments,
            vec![Segment::MoveTo(0.0, 5.0), Segment::LineTo(20.0, 15.0)]
        );
    }

    #[test]
    fn without_a_view_box_it_stays_in_canvas_coordinates() {
        let path = Path::new().svg("M 1 2 L 3 4");

        let traced = path
            .trace(100.0, 100.0)
            .expect("failed to trace the test path");

        assert_eq!(
            traced.segments,
            vec![Segment::MoveTo(1.0, 2.0), Segment::LineTo(3.0, 4.0)]
        );
    }
}
