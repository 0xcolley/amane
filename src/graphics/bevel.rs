use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::rc::Rc;
use std::sync::Arc;

use swash::FontRef;

use crate::Bevel;
use crate::graphics::image::Bitmap;
use crate::graphics::letter::{self, LetterPicture};
use crate::graphics::{Color, layout};

// how many lines of lit letters are kept; a clock needs two, the minute shown and the next
const KEPT: usize = 8;

thread_local! {
    static BAKED: RefCell<HashMap<String, Rc<Baked>>> = RefCell::new(HashMap::new());
}

// a line of lit letters as one picture
pub struct Baked {
    pub bitmap: Arc<Bitmap>,

    // where the picture's top left sits from the line's start on the baseline, in pixels
    pub left: f32,
    pub top: f32,
}

// how much of each pixel the letters cover, 0 to 1
struct Mask {
    width: usize,
    height: usize,
    values: Vec<f32>,
}

/*
 * the letters are lit once and kept, since what they say rarely changes,
 * like a clock's minute; none for a line with nothing to draw
 */
pub fn baked(
    content: &str,
    font: FontRef<'static>,
    pixel_size: f32,
    color: Color,
    bevel: &Bevel,
    scale: f32,
) -> Option<Rc<Baked>> {
    let key = format!(
        "{content}\u{0}{}:{}:{}:{color:?}:{bevel:?}",
        font.key.value(),
        pixel_size.to_bits(),
        scale.to_bits()
    );

    if let Some(baked) = BAKED.with_borrow(|baked| baked.get(&key).cloned()) {
        return Some(baked);
    }

    let (mask, left, top) = coverage(content, font, pixel_size, bevel.reach() * scale)?;

    let baked = Rc::new(Baked {
        bitmap: Arc::new(light(&mask, color, bevel, scale)),
        left,
        top,
    });

    BAKED.with_borrow_mut(|kept| {
        if kept.len() >= KEPT {
            kept.clear();
        }

        kept.insert(key, Rc::clone(&baked));
    });

    Some(baked)
}

// the line's letters drawn into one mask, with room around them for what the bevel paints outside
fn coverage(
    content: &str,
    font: FontRef<'static>,
    pixel_size: f32,
    reach: f32,
) -> Option<(Mask, f32, f32)> {
    let line = layout::lay_out(font, content, pixel_size);

    let mut pictures = Vec::new();

    for placed in &line.letters {
        let (whole, quarter) = layout::split_quarters(placed.x);

        let Some(picture) = letter::rasterize(LetterPicture {
            font: placed.font,
            id: placed.id,
            size: pixel_size,
            quarter,
        }) else {
            continue;
        };

        let x = (whole + picture.left).round() as i64;
        let y = (picture.top - placed.y.round()).round() as i64;

        pictures.push((x, y, picture));
    }

    let left = pictures.iter().map(|(x, _, _)| *x).min()?;
    let top = pictures.iter().map(|(_, y, _)| *y).min()?;
    let right = pictures
        .iter()
        .map(|(x, _, picture)| x + i64::from(picture.width))
        .max()?;
    let bottom = pictures
        .iter()
        .map(|(_, y, picture)| y + i64::from(picture.height))
        .max()?;

    let pad = reach.ceil() as i64;

    let width = (right - left + 2 * pad) as usize;
    let height = (bottom - top + 2 * pad) as usize;

    let mut values = vec![0.0_f32; width * height];

    for (x, y, picture) in &pictures {
        let start_x = (x - left + pad) as usize;
        let start_y = (y - top + pad) as usize;

        for row in 0..picture.height as usize {
            for column in 0..picture.width as usize {
                let covered =
                    f32::from(picture.coverage[row * picture.width as usize + column]) / 255.0;
                let value = &mut values[(start_y + row) * width + start_x + column];

                // letters that touch keep the fuller of the two
                *value = value.max(covered);
            }
        }
    }

    let mask = Mask {
        width,
        height,
        values,
    };

    Some((mask, (left - pad) as f32, (top - pad) as f32))
}

/*
 * the blurred mask is a height map: letters are high, the rest is low.
 * Where it slopes toward the light the letters catch it, where it slopes
 * away they fall into shade; the slope only shows near the edges, so the
 * middle keeps the letters' own color
 */
fn light(mask: &Mask, color: Color, bevel: &Bevel, scale: f32) -> Bitmap {
    let rounded = (bevel.round > 0.0)
        .then(|| rounded(mask, (bevel.round * scale / 2.0).round().max(1.0) as usize));
    let mask = rounded.as_ref().unwrap_or(mask);

    let Mask { width, height, .. } = *mask;

    // three box blurs make a gaussian about this wide
    let radius = (bevel.depth * scale / 2.0).round().max(1.0) as usize;
    let spread = ((radius * (radius + 1)) as f32).sqrt();

    let heights = blur(mask, radius);

    // the steepest a blurred edge gets, so a full edge lights fully
    let steepest = 1.0 / (TAU.sqrt() * spread);

    let angle = bevel.light.to_radians();
    let toward = (angle.cos(), -angle.sin());

    let ring = bevel.outline.map(|(outline, _)| {
        let grown = grow(mask, (outline * scale).round().max(1.0) as usize);

        grown
            .iter()
            .zip(&mask.values)
            .map(|(grown, covered)| (grown - covered).max(0.0))
            .collect::<Vec<f32>>()
    });

    let shadow = bevel.shadow.map(|shadow| {
        let moved = shift(
            mask,
            (shadow.x * scale).round() as i64,
            (shadow.y * scale).round() as i64,
        );

        (
            blur(&moved, (shadow.blur * scale / 2.0).round() as usize).values,
            shadow.tint(),
        )
    });

    let mut pixels = vec![0; width * height * 4];

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let covered = mask.values[index];

            let mut pixel = [0.0; 4];

            if let Some((shadow, tint)) = &shadow {
                over(&mut pixel, *tint, shadow[index]);
            }

            if let (Some(ring), Some((_, outline))) = (&ring, bevel.outline) {
                over(&mut pixel, outline, ring[index]);
            }

            over(&mut pixel, color, covered);

            let height_at = |x: usize, y: usize| heights.values[y * width + x];

            let slope_x =
                (height_at((x + 1).min(width - 1), y) - height_at(x.saturating_sub(1), y)) / 2.0;
            let slope_y =
                (height_at(x, (y + 1).min(height - 1)) - height_at(x, y.saturating_sub(1))) / 2.0;

            // the edge faces down its slope, outward from the letters
            let facing = -(slope_x * toward.0 + slope_y * toward.1) / steepest;

            over(&mut pixel, bevel.shade, (-facing).clamp(0.0, 1.0) * covered);
            over(
                &mut pixel,
                bevel.highlight,
                facing.clamp(0.0, 1.0) * covered,
            );

            let alpha = pixel[3];

            if alpha <= 0.0 {
                continue;
            }

            // the bitmap keeps plain alpha
            for channel in 0..3 {
                pixels[index * 4 + channel] =
                    (pixel[channel] / alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            }

            pixels[index * 4 + 3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    Bitmap::new(width as u32, height as u32, pixels)
}

/*
 * the letters with their corners rounded off: blurred, then cut again where
 * they're half covered, which keeps straight edges where they were and pulls
 * corners in. The cut keeps a pixel's worth of smoothing
 */
fn rounded(mask: &Mask, radius: usize) -> Mask {
    let soft = blur(mask, radius);

    let spread = ((radius * (radius + 1)) as f32).sqrt();
    let steepest = 1.0 / (TAU.sqrt() * spread);

    let values = soft
        .values
        .iter()
        .map(|value| ((value - 0.5) / steepest + 0.5).clamp(0.0, 1.0))
        .collect();

    Mask {
        width: mask.width,
        height: mask.height,
        values,
    }
}

// lays the color over the pixel, kept premultiplied, as much as the amount says
fn over(pixel: &mut [f32; 4], color: Color, amount: f32) {
    let alpha = f32::from(color.a) / 255.0 * amount;

    if alpha <= 0.0 {
        return;
    }

    let keep = 1.0 - alpha;

    pixel[0] = f32::from(color.r) / 255.0 * alpha + pixel[0] * keep;
    pixel[1] = f32::from(color.g) / 255.0 * alpha + pixel[1] * keep;
    pixel[2] = f32::from(color.b) / 255.0 * alpha + pixel[2] * keep;
    pixel[3] = alpha + pixel[3] * keep;
}

// three box blurs along rows and columns; past the edges is empty
fn blur(mask: &Mask, radius: usize) -> Mask {
    let Mask { width, height, .. } = *mask;

    let mut values = mask.values.clone();

    if radius == 0 {
        return Mask {
            width,
            height,
            values,
        };
    }

    let mut line = Vec::new();

    for _ in 0..3 {
        for y in 0..height {
            line.clear();
            line.extend_from_slice(&values[y * width..(y + 1) * width]);

            average(&line, radius, |x, value| values[y * width + x] = value);
        }

        for x in 0..width {
            line.clear();
            line.extend((0..height).map(|y| values[y * width + x]));

            average(&line, radius, |y, value| values[y * width + x] = value);
        }
    }

    Mask {
        width,
        height,
        values,
    }
}

// each value becomes the average of those up to radius away, kept as a running sum
fn average(line: &[f32], radius: usize, mut write: impl FnMut(usize, f32)) {
    let across = (2 * radius + 1) as f32;

    let mut sum: f32 = line.iter().take(radius + 1).sum();

    for index in 0..line.len() {
        write(index, sum / across);

        if let Some(entering) = line.get(index + radius + 1) {
            sum += entering;
        }

        if index >= radius {
            sum -= line[index - radius];
        }
    }
}

// each value becomes the largest of those up to radius away, along rows and then columns
fn grow(mask: &Mask, radius: usize) -> Vec<f32> {
    let Mask { width, height, .. } = *mask;

    let mut rows = vec![0.0; width * height];

    for y in 0..height {
        for x in 0..width {
            let from = x.saturating_sub(radius);
            let to = (x + radius).min(width - 1);

            rows[y * width + x] = (from..=to)
                .map(|x| mask.values[y * width + x])
                .fold(0.0, f32::max);
        }
    }

    let mut grown = vec![0.0; width * height];

    for y in 0..height {
        for x in 0..width {
            let from = y.saturating_sub(radius);
            let to = (y + radius).min(height - 1);

            grown[y * width + x] = (from..=to).map(|y| rows[y * width + x]).fold(0.0, f32::max);
        }
    }

    grown
}

// the mask moved by whole pixels, what moves past the edge is lost
fn shift(mask: &Mask, by_x: i64, by_y: i64) -> Mask {
    let Mask { width, height, .. } = *mask;

    let mut values = vec![0.0; width * height];

    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let (to_x, to_y) = (x + by_x, y + by_y);

            if (0..width as i64).contains(&to_x) && (0..height as i64).contains(&to_y) {
                values[to_y as usize * width + to_x as usize] =
                    mask.values[y as usize * width + x as usize];
            }
        }
    }

    Mask {
        width,
        height,
        values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // a filled square with empty room around it
    fn square() -> Mask {
        let (width, height) = (40, 40);

        let values = (0..width * height)
            .map(|index| {
                let (x, y) = (index % width, index / width);

                if (10..30).contains(&x) && (10..30).contains(&y) {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();

        Mask {
            width,
            height,
            values,
        }
    }

    fn brightness(bitmap: &Bitmap, x: usize, y: usize) -> u32 {
        let index = (y * bitmap.width() as usize + x) * 4;

        bitmap.pixels[index..index + 3]
            .iter()
            .map(|channel| u32::from(*channel))
            .sum()
    }

    #[test]
    fn the_edges_facing_the_light_are_brighter_than_those_facing_away() {
        let bevel = Bevel::new(4.0).light(135.0);

        let lit = light(&square(), Color::rgb(128, 128, 128), &bevel, 1.0);

        // just inside the top left corner, and just inside the bottom right one
        let near = brightness(&lit, 11, 11);
        let far = brightness(&lit, 28, 28);
        let middle = brightness(&lit, 20, 20);

        assert!(near > middle && middle > far, "{near} {middle} {far}");
        assert_eq!(middle, 3 * 128);
    }

    #[test]
    fn an_outline_rings_the_outside_only() {
        let bevel = Bevel::new(2.0).outline(1.0, Color::rgb(255, 0, 0));

        let lit = light(&square(), Color::WHITE, &bevel, 1.0);

        let pixel = |x: usize, y: usize| {
            let index = (y * lit.width() as usize + x) * 4;

            [lit.pixels[index], lit.pixels[index + 3]]
        };

        // just outside the left edge is the ring, two pixels out is empty
        assert_eq!(pixel(9, 20), [255, 255]);
        assert_eq!(pixel(8, 20)[1], 0);
    }

    #[test]
    fn rounding_takes_the_corners_and_leaves_the_edges() {
        let rounded = rounded(&square(), 2);

        let at = |x: usize, y: usize| rounded.values[y * 40 + x];

        // the corner pixel is mostly gone, the middle of an edge and the inside stay
        assert!(at(10, 10) < 0.5, "{}", at(10, 10));
        assert!(at(10, 20) > 0.9 && at(20, 20) > 0.99);
        assert!(at(9, 20) < 0.1);
    }

    #[test]
    fn a_running_average_matches_the_plain_one() {
        let line = [0.0, 1.0, 1.0, 0.0, 0.0, 1.0];

        let mut averaged = vec![0.0; line.len()];

        average(&line, 1, |index, value| averaged[index] = value);

        assert_eq!(
            averaged,
            [
                1.0 / 3.0,
                2.0 / 3.0,
                2.0 / 3.0,
                1.0 / 3.0,
                1.0 / 3.0,
                1.0 / 3.0
            ]
        );
    }
}
