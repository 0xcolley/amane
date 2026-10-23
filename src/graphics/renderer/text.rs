use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use swash::FontRef;

use crate::Bevel;
use crate::graphics::{
    Area, BezierPath, Color, Corners, Outline, Renderer, Transform, bevel, font, layout,
};

use super::Command;

thread_local! {
    // each letter's outline, read from the font once; swash gives every font its own key
    static OUTLINES: RefCell<HashMap<(u64, u16), Option<BezierPath>>> =
        RefCell::new(HashMap::new());
}

impl Renderer {
    /*
     * letters are drawn as small cached pictures, which the gpu handles far
     * faster than outlines; only rotated or stretched text needs the outlines
     */
    pub fn text(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        size: f32,
        color: Color,
        area: Area,
    ) {
        match self.transform.even_scale() {
            Some(scale) => self.text_pictures(content, font, size * scale, color, area),
            None => self.text_outlines(content, font, size, color, area),
        }
    }

    /*
     * lit letters are baked into one picture, lined up on whole pixels the
     * way plain letters are; turned or stretched text draws plainly
     */
    pub fn text_bevel(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        size: f32,
        color: Color,
        area: Area,
        lit: &Bevel,
    ) {
        let (Some(scale), Some(back)) = (self.transform.even_scale(), self.transform.invert())
        else {
            return self.text(content, font, size, color, area);
        };

        let pixel_size = size * scale;

        let Some(baked) = bevel::baked(content, font, pixel_size, color, lit, scale) else {
            return;
        };

        let ascent = font.metrics(font::coords(font)).scale(pixel_size).ascent;

        let start = self.transform.map(area.x, area.y);

        let line_x = start.0.round();
        let baseline = (start.1 + ascent).round();

        let (x, y) = back.map(line_x + baked.left, baseline + baked.top);

        let placement = Area::new(
            x,
            y,
            baked.bitmap.width() as f32 / scale,
            baked.bitmap.height() as f32 / scale,
        );

        self.image(
            placement,
            Corners::from(0.0),
            Arc::clone(&baked.bitmap),
            placement,
        );
    }

    /*
     * the line starts on a whole pixel and every letter keeps its distance
     * from that start to a quarter pixel, so letters are spaced as the font
     * means them and a moving line never shimmers apart
     */
    fn text_pictures(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        pixel_size: f32,
        color: Color,
        area: Area,
    ) {
        let ascent = font.metrics(font::coords(font)).scale(pixel_size).ascent;

        let start = self.transform.map(area.x, area.y);

        let line_x = start.0.round();
        let baseline = (start.1 + ascent).round();

        let line = layout::lay_out(font, content, pixel_size);

        for letter in &line.letters {
            let (whole, quarter) = layout::split_quarters(letter.x);

            self.commands.push(Command::Glyph {
                font: letter.font,
                id: letter.id,
                size: pixel_size,
                quarter,
                color,
                x: line_x + whole,
                y: baseline - letter.y.round(),
            });
        }
    }

    fn text_outlines(
        &mut self,
        content: &str,
        font: FontRef<'static>,
        size: f32,
        color: Color,
        area: Area,
    ) {
        let ascent = font.metrics(font::coords(font)).scale(size).ascent;

        let baseline = area.y + ascent;

        let line = layout::lay_out(font, content, size);

        for letter in &line.letters {
            // a space has no outline but still moves the pen
            let Some(path) = outline(letter.font, letter.id) else {
                continue;
            };

            let face_units = letter.units;

            // fonts point y upward, the screen points it downward
            let letter_transform = Transform::from_row(
                face_units,
                0.0,
                0.0,
                -face_units,
                area.x + letter.x,
                baseline - letter.y,
            );
            let transform = letter_transform.post_concat(self.transform);

            self.commands.push(Command::Fill {
                path,
                transform,
                color,
            });
        }
    }
}

fn outline(font: FontRef<'static>, id: u16) -> Option<BezierPath> {
    let key = (font.key.value(), id);

    OUTLINES.with_borrow_mut(|outlines| {
        let path = outlines
            .entry(key)
            .or_insert_with(|| read_outline(font, id));

        path.clone()
    })
}

fn read_outline(font: FontRef<'static>, id: u16) -> Option<BezierPath> {
    font::with_context(|context| {
        // no size keeps the outline in the font's own units
        let mut scaler = context
            .builder(font)
            .normalized_coords(font::coords(font))
            .build();

        let shape = scaler.scale_outline(id)?;

        Outline::new().read(shape.path()).finish()
    })
}
