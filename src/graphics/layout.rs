use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use swash::FontRef;
use swash::shape::ShapeContext;
use swash::text::{Codepoint, Script};

use crate::graphics::font;

/*
 * a hover fade measures and draws the same few lines every frame, so they are
 * shaped once; the cache is emptied now and then instead of growing without end
 */
const MAX_LINES: usize = 2048;

// one letter set on a line
#[derive(Clone, Copy)]
pub struct Placed {
    // the font that has it, a fallback for letters the text's own font lacks
    pub font: FontRef<'static>,
    pub id: u16,

    // where it starts from the line's start and how far above the baseline, in pixels, not rounded
    pub x: f32,
    pub y: f32,

    // font units to pixels; a fallback font can measure in other units than the text's own
    pub units: f32,
}

// a line of letters shaped by its fonts, so pairs like "To" sit as close as the font means them to
pub struct Line {
    pub letters: Vec<Placed>,

    // how far the whole line moves the pen
    pub width: f32,
}

// the font, the size and the text
type Key = (u64, u32, String);

thread_local! {
    static SHAPER: RefCell<ShapeContext> = RefCell::new(ShapeContext::new());

    static LINES: RefCell<HashMap<Key, Rc<Line>>> = RefCell::new(HashMap::new());
}

pub fn lay_out(font: FontRef<'static>, content: &str, size: f32) -> Rc<Line> {
    let key = (font.key.value(), size.to_bits(), String::from(content));

    if let Some(line) = LINES.with_borrow(|lines| lines.get(&key).cloned()) {
        return line;
    }

    let line = Rc::new(shape(font, content, size));

    LINES.with_borrow_mut(|lines| {
        if lines.len() >= MAX_LINES {
            lines.clear();
        }

        lines.insert(key, line.clone());
    });

    line
}

fn shape(font: FontRef<'static>, content: &str, size: f32) -> Line {
    let mut letters = Vec::new();
    let mut pen = 0.0;

    for (run_font, run) in runs(font, content) {
        let units_per_em = run_font.metrics(font::coords(run_font)).units_per_em;
        let units = size / f32::from(units_per_em);

        SHAPER.with_borrow_mut(|context| {
            let mut shaper = context
                .builder(run_font)
                .script(script(run))
                .size(size)
                .normalized_coords(font::coords(run_font))
                .build();

            shaper.add_str(run);

            shaper.shape_with(|cluster| {
                for glyph in cluster.glyphs {
                    letters.push(Placed {
                        font: run_font,
                        id: glyph.id,
                        x: pen + glyph.x,
                        y: glyph.y,
                        units,
                    });

                    pen += glyph.advance;
                }
            });
        });
    }

    Line {
        letters,
        width: pen,
    }
}

/*
 * the text cut where the font that has its letters changes, since a shaper
 * works in one font; kerning stops at a cut, which only falls between scripts
 */
fn runs<'a>(font: FontRef<'static>, content: &'a str) -> Vec<(FontRef<'static>, &'a str)> {
    let mut runs = Vec::new();

    let mut start = 0;
    let mut current: Option<FontRef<'static>> = None;

    for (index, letter) in content.char_indices() {
        let (letter_font, _) = font::with_letter(font, letter);

        match current {
            Some(run_font) if run_font.key == letter_font.key => {}

            Some(run_font) => {
                runs.push((run_font, &content[start..index]));

                start = index;
                current = Some(letter_font);
            }

            None => current = Some(letter_font),
        }
    }

    if let Some(run_font) = current {
        runs.push((run_font, &content[start..]));
    }

    runs
}

// the run's script, from its first letter that has one of its own; spaces and digits belong to any
fn script(run: &str) -> Script {
    run.chars()
        .map(|letter| letter.script())
        .find(|script| !matches!(script, Script::Common | Script::Inherited | Script::Unknown))
        .unwrap_or(Script::Latin)
}

/*
 * a letter's place as a whole pixel and the quarter pixels past it, 0 to 3;
 * a quarter is as fine as the eye tells apart, and keeps the pictures few
 */
pub fn split_quarters(x: f32) -> (f32, u8) {
    let quarters = (x * 4.0).round();

    let whole = (quarters / 4.0).floor();

    (whole, (quarters - whole * 4.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    // the text's width from each letter's own advance, as if the font had no kerning
    fn plain_width(font: FontRef<'static>, content: &str, size: f32) -> f32 {
        content
            .chars()
            .map(|letter| {
                let (font, id) = font::with_letter(font, letter);

                let units = size / f32::from(font.metrics(font::coords(font)).units_per_em);

                font.glyph_metrics(font::coords(font)).advance_width(id) * units
            })
            .sum()
    }

    #[test]
    fn splits_into_whole_pixels_and_quarters() {
        assert_eq!(split_quarters(3.0), (3.0, 0));
        assert_eq!(split_quarters(3.26), (3.0, 1));
        assert_eq!(split_quarters(3.5), (3.0, 2));
        assert_eq!(split_quarters(3.9), (4.0, 0));
        assert_eq!(split_quarters(-0.3), (-1.0, 3));
    }

    #[test]
    fn the_width_is_where_the_pen_ends() {
        let font = font::load_weighted(None, crate::Weight::Regular);

        let line = lay_out(font, "Wi-Fi Networks", 13.0);

        let plain = plain_width(font, "Wi-Fi Networks", 13.0);

        assert_eq!(line.letters.len(), 14);

        // kerning only ever moves letters a little, never by whole letters
        assert!((line.width - plain).abs() < 13.0);
    }

    #[test]
    fn kerned_pairs_sit_closer() {
        let font = font::load_weighted(None, crate::Weight::Regular);

        let kerned = lay_out(font, "AVAVAVAV", 40.0).width;

        let plain = plain_width(font, "AVAVAVAV", 40.0);

        // the usual sans-serif fonts, like noto, dejavu and adwaita, all pull AV together
        assert!(kerned < plain);
    }
}
