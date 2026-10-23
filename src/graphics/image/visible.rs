use super::Bitmap;

// fainter pixels are a soft shadow or a stray edge, not the picture itself
const SHOWS: u8 = 40;

/*
 * where a picture actually draws, in its own pixels: the box around the
 * pixels that show, and how much of it they take up. An icon file can have
 * any margin around its drawing, so icons are sized by this instead
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Visible {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,

    // from each row's first showing pixel to its last, so holes inside count as filled
    pub area: f32,
}

pub fn measure(image: &Bitmap) -> Option<Visible> {
    let row_length = image.width as usize * 4;

    if row_length == 0 {
        return None;
    }

    let shows = |pixel: &[u8; 4]| pixel[3] >= SHOWS;

    let mut top = None;
    let mut bottom = 0;
    let mut left = usize::MAX;
    let mut right = 0;
    let mut area = 0;

    for (y, row) in image.pixels.chunks_exact(row_length).enumerate() {
        let (pixels, _) = row.as_chunks::<4>();

        let Some(first) = pixels.iter().position(shows) else {
            continue;
        };

        let last = pixels.iter().rposition(shows).unwrap_or(first);

        top.get_or_insert(y);
        bottom = y;
        left = left.min(first);
        right = right.max(last);
        area += last - first + 1;
    }

    let top = top?;

    Some(Visible {
        x: left as f32,
        y: top as f32,
        width: (right - left + 1) as f32,
        height: (bottom - top + 1) as f32,
        area: area as f32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // a size by size picture with a filled square from `from` up to `to`
    fn square(size: u32, from: u32, to: u32) -> Bitmap {
        let mut pixels = Vec::new();

        for y in 0..size {
            for x in 0..size {
                let inside = (from..to).contains(&x) && (from..to).contains(&y);

                pixels.extend([255, 255, 255, if inside { 255 } else { 0 }]);
            }
        }

        Bitmap::new(size, size, pixels)
    }

    #[test]
    fn finds_the_drawing_inside_its_margin() {
        let visible = measure(&square(10, 2, 7)).expect("failed to find the square");

        assert_eq!((visible.x, visible.y), (2.0, 2.0));
        assert_eq!((visible.width, visible.height), (5.0, 5.0));
        assert_eq!(visible.area, 25.0);
    }

    #[test]
    fn a_clear_picture_shows_nothing() {
        assert_eq!(measure(&square(4, 0, 0)), None);
    }

    #[test]
    fn faint_pixels_dont_count() {
        let mut pixels = vec![0; 4 * 4 * 4];

        // one faint pixel, like the edge of a shadow
        pixels[3] = 10;

        assert_eq!(measure(&Bitmap::new(4, 4, pixels)), None);
    }
}
