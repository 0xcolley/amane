use swash::FontRef;
use swash::scale::{Render, Source};
use swash::zeno::{Format, Vector};

use crate::graphics::{Color, font};

// one letter turned into pixels: how much of each pixel it covers, 0 to 255
pub struct Letter {
    pub width: u32,
    pub height: u32,

    // where the pixels sit from the pen on the baseline
    pub left: f32,
    pub top: f32,

    pub coverage: Vec<u8>,
}

// which picture of a letter: its font, its glyph, its size in pixels and how many quarter pixels right it sits
#[derive(Clone, Copy)]
pub struct LetterPicture {
    pub font: FontRef<'static>,
    pub id: u16,
    pub size: f32,
    pub quarter: u8,
}

impl LetterPicture {
    // everything that changes the pixels
    pub fn key(&self) -> (u64, u16, u32, u8) {
        (
            self.font.key.value(),
            self.id,
            self.size.to_bits(),
            self.quarter,
        )
    }
}

// none for a letter without pixels, like a space
pub fn rasterize(picture: LetterPicture) -> Option<Letter> {
    let LetterPicture {
        font,
        id,
        size,
        quarter,
    } = picture;

    font::with_context(|context| {
        let mut scaler = context
            .builder(font)
            .normalized_coords(font::coords(font))
            .size(size)
            .hint(true)
            .build();

        let image = Render::new(&[Source::Outline])
            .format(Format::Alpha)
            .offset(Vector::new(f32::from(quarter) / 4.0, 0.0))
            .render(&mut scaler, id)?;

        let placement = image.placement;

        if placement.width == 0 || placement.height == 0 {
            return None;
        }

        // swash counts top upward from the baseline, amane counts it downward
        Some(Letter {
            width: placement.width,
            height: placement.height,
            left: placement.left as f32,
            top: -placement.top as f32,
            coverage: image.data,
        })
    })
}

/*
 * light letters on a dark background look thinner than dark letters on a
 * light one, since colors blend as stored and not as light adds up; light
 * letters get their soft edges lifted, closer to how they look on macOS.
 * quads.wgsl does the same for the letters the gpu draws
 */
pub fn lift(coverage: f32, color: Color) -> f32 {
    let light =
        (0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b))
            / 255.0;

    coverage.powf(1.0 - LIFT * light)
}

// how far white letters' edges are lifted, as 1 minus the power their coverage is raised to
pub const LIFT: f32 = 0.2;
