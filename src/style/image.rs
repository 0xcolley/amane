use std::path::{Path, PathBuf};

use crate::graphics::Area;
use crate::graphics::image::{self, Bitmap};

/*
 * how much of its square an icon's drawing covers: a full circle's share, so
 * round icons span the square and square ones sit a little inside it, which
 * makes them look the same size
 */
const ICON_SHARE: f32 = 0.8;

pub struct Image {
    pub(crate) path: PathBuf,
    pub(crate) fit: Fit,

    // the size it is shrunk to cover when decoded, none keeps every pixel
    pub(crate) thumbnail: Option<(u32, u32)>,

    // how far the decoded copy is blurred, in its own pixels
    pub(crate) blur: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    // fills the rectangle exactly, bending the image's shape to match
    Stretch,

    // fills the whole rectangle, cutting off what spills past the edges
    Cover,

    // shows the whole image, leaving the rest of the rectangle uncovered
    Contain,

    // fits what the image draws rather than its file, sized by the drawing's shape
    Icon,
}

impl Image {
    pub fn cover(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            fit: Fit::Cover,
            thumbnail: None,
            blur: 0,
        }
    }

    pub fn contain(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            fit: Fit::Contain,
            thumbnail: None,
            blur: 0,
        }
    }

    /*
     * an app icon, sized by what it draws: the margin its file happens to
     * have is left out and round, square and odd shapes are evened out, so
     * icons from different themes look the same size side by side
     */
    pub fn icon(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            fit: Fit::Icon,
            thumbnail: None,
            blur: 0,
        }
    }

    pub fn stretch(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            fit: Fit::Stretch,
            thumbnail: None,
            blur: 0,
        }
    }

    /*
     * keeps a small copy big enough to cover width by height, for pictures
     * shown far smaller than their file, like a list of wallpapers
     */
    pub fn thumbnail(mut self, width: u32, height: u32) -> Self {
        self.thumbnail = Some((width, height));

        self
    }

    /*
     * blurs the decoded copy once, radius counted in its own pixels; with a
     * small thumbnail stretched far bigger, like a blurred backdrop, it
     * looks smooth instead of blocky and costs nothing per frame
     */
    pub fn blurred(mut self, radius: u32) -> Self {
        self.blur = radius;

        self
    }

    /*
     * starts decoding the file if nothing asked for it yet, and says whether
     * it is ready to draw; a file that can't be read never becomes ready
     */
    pub fn loaded(path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();

        image::load(path, None, 0).is_some()
    }
}

impl Fit {
    // where the whole image goes, which can be past the area for a cover or an icon
    pub fn place_image(self, area: Area, image: &Bitmap) -> Area {
        let width = image.width() as f32;
        let height = image.height() as f32;

        match (self, image.visible()) {
            (Fit::Icon, Some(visible)) => place_icon(area, width, height, visible),
            _ => self.place(area, width, height),
        }
    }

    pub fn place(self, area: Area, image_width: f32, image_height: f32) -> Area {
        let horizontal_scale = area.width / image_width;
        let vertical_scale = area.height / image_height;

        let scale = match self {
            Fit::Stretch => return area,
            Fit::Cover => f32::max(horizontal_scale, vertical_scale),
            // an icon that draws nothing, or isn't measured, is placed whole
            Fit::Contain | Fit::Icon => f32::min(horizontal_scale, vertical_scale),
        };

        let width = image_width * scale;
        let height = image_height * scale;

        let x = area.x + (area.width - width) / 2.0;
        let y = area.y + (area.height - height) / 2.0;

        Area::new(x, y, width, height)
    }
}

/*
 * scales the drawing until it covers ICON_SHARE of the area's square, but
 * never past the area, and puts its middle on the area's middle
 */
fn place_icon(area: Area, image_width: f32, image_height: f32, visible: image::Visible) -> Area {
    let side = area.width.min(area.height);

    let by_share = (ICON_SHARE * side * side / visible.area).sqrt();
    let by_width = area.width / visible.width;
    let by_height = area.height / visible.height;

    let scale = by_share.min(by_width).min(by_height);

    let middle_x = visible.x + visible.width / 2.0;
    let middle_y = visible.y + visible.height / 2.0;

    let x = area.x + area.width / 2.0 - middle_x * scale;
    let y = area.y + area.height / 2.0 - middle_y * scale;

    Area::new(x, y, image_width * scale, image_height * scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    // a size by size picture, drawn where `inside` says
    fn picture(size: u32, inside: impl Fn(f32, f32) -> bool) -> Bitmap {
        let mut pixels = Vec::new();

        for y in 0..size {
            for x in 0..size {
                let shows = inside(x as f32 + 0.5, y as f32 + 0.5);

                pixels.extend([0, 0, 0, if shows { 255 } else { 0 }]);
            }
        }

        Bitmap::new(size, size, pixels)
    }

    fn square(size: u32, from: f32, to: f32) -> Bitmap {
        picture(size, |x, y| {
            (from..to).contains(&x) && (from..to).contains(&y)
        })
    }

    // the drawn part's box once placed
    fn drawn(image: &Bitmap, area: Area) -> Area {
        let placed = Fit::Icon.place_image(area, image);
        let visible = image.visible().expect("failed to measure the test icon");

        let scale = placed.width / image.width() as f32;

        Area::new(
            placed.x + visible.x * scale,
            placed.y + visible.y * scale,
            visible.width * scale,
            visible.height * scale,
        )
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn a_margin_makes_no_difference() {
        let area = Area::new(0.0, 0.0, 48.0, 48.0);

        let full = drawn(&square(100, 0.0, 100.0), area);
        let padded = drawn(&square(100, 20.0, 80.0), area);

        assert!(near(full.width, padded.width));
        assert!(near(full.x, padded.x));
    }

    #[test]
    fn a_square_sits_inside_and_a_circle_spans_the_area() {
        let area = Area::new(0.0, 0.0, 48.0, 48.0);

        let square = drawn(&square(200, 0.0, 200.0), area);
        let circle = drawn(
            &picture(200, |x, y| (x - 100.0).hypot(y - 100.0) < 100.0),
            area,
        );

        assert!(near(square.width, 48.0 * ICON_SHARE.sqrt()));
        assert!(near(circle.width, 48.0));
    }

    #[test]
    fn a_drawing_off_to_one_side_is_centred() {
        let area = Area::new(10.0, 10.0, 48.0, 48.0);

        let corner = drawn(&square(100, 0.0, 40.0), area);

        assert!(near(corner.x + corner.width / 2.0, 34.0));
        assert!(near(corner.y + corner.height / 2.0, 34.0));
    }

    #[test]
    fn a_long_drawing_stops_at_the_edges() {
        let area = Area::new(0.0, 0.0, 48.0, 48.0);

        let long = picture(100, |_, y| (45.0..55.0).contains(&y));

        assert!(near(drawn(&long, area).width, 48.0));
    }
}
