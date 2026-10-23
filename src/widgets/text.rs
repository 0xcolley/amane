mod lines;

use swash::FontRef;

use crate::graphics::{Area, Renderer, font};
use crate::{Bevel, Color, Size, Weight};

use super::Widget;

use lines::Rules;

pub struct Text {
    content: String,
    size: f32,
    color: Color,
    font: Option<String>,
    weight: Weight,
    rules: Rules,

    // sized by the letters' own shapes instead of their advance, so an icon centers exactly
    tight: bool,

    bevel: Option<Bevel>,

    // drawn with the font's optical size for this size, if it has one
    optical: bool,

    // each line in the middle of the width, instead of at its left
    centered: bool,
}

impl Text {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            size: 16.0,
            color: Color::BLACK,
            font: None,
            weight: Weight::Regular,
            rules: Rules::default(),
            tight: false,
            bevel: None,
            optical: false,
            centered: false,
        }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;

        self
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;

        self
    }

    pub fn font(mut self, family: &str) -> Self {
        self.font = Some(String::from(family));

        self
    }

    pub fn weight(mut self, weight: impl Into<Weight>) -> Self {
        self.weight = weight.into();

        self
    }

    pub fn wrap(mut self) -> Self {
        self.rules.wrap = true;

        self
    }

    pub fn elide(mut self) -> Self {
        self.rules.elide = true;

        self
    }

    pub fn max_lines(mut self, count: usize) -> Self {
        self.rules.max_lines = Some(count);

        self
    }

    pub fn tight(mut self) -> Self {
        self.tight = true;

        self
    }

    // large text drawn with the font's display cut, as macOS draws big titles in SF Pro Display
    pub fn optical(mut self) -> Self {
        self.optical = true;

        self
    }

    // raised, lit letters, drawn as one picture; best for large text that rarely changes, like a clock
    // each line in the middle of the width it's given, for wrapped captions
    pub fn center(mut self) -> Self {
        self.centered = true;

        self
    }

    pub fn bevel(mut self, bevel: Bevel) -> Self {
        self.bevel = Some(bevel);

        self
    }

    // how tall it is once laid out in this width, for wrapped text whose height depends on it
    pub fn height_in(&self, width: f32) -> f32 {
        let font = self.face();

        let arranged = lines::arrange(&self.content, font, self.size, width, self.rules);

        lines::stack_height(font, self.size, arranged.len())
    }

    fn face(&self) -> FontRef<'static> {
        let face = font::load_weighted(self.font.as_deref(), self.weight);

        if self.optical {
            font::optical(face, self.size)
        } else {
            face
        }
    }
}

impl Widget for Text {
    fn width(&self) -> Size {
        // wrapped or elided text takes the width it is given and fits itself into it
        if self.rules.wrap || self.rules.elide {
            return Size::Parent;
        }

        if self.tight {
            let (left, right) = lines::ink(&self.content, self.face(), self.size);

            return Size::Fixed(right - left);
        }

        Size::Fixed(lines::measure(&self.content, self.face(), self.size))
    }

    fn height(&self) -> Size {
        /*
         * the width is only known when drawing,
         * so wrapped text without a line limit cannot know its height yet
         */
        let count = match (self.rules.wrap, self.rules.max_lines) {
            (false, _) => 1,
            (true, Some(count)) => count,
            (true, None) => return Size::Parent,
        };

        Size::Fixed(lines::stack_height(self.face(), self.size, count))
    }

    fn draw(&self, renderer: &mut Renderer, area: Area) {
        let font = self.face();

        // the ink starts at the area's left edge, not the pen
        let area = if self.tight {
            let (left, _) = lines::ink(&self.content, font, self.size);

            Area::new(area.x - left, area.y, area.width, area.height)
        } else {
            area
        };

        let arranged = lines::arrange(&self.content, font, self.size, area.width, self.rules);

        let line_height = lines::height(font, self.size);
        let spacing = lines::spacing(font, self.size);

        let mut top = area.y;

        for line in arranged {
            let left = if self.centered {
                (area.width - lines::measure(&line, font, self.size)).max(0.0) / 2.0
            } else {
                0.0
            };

            let line_area = Area::new(area.x + left, top, area.width - left, line_height);

            match &self.bevel {
                Some(bevel) => {
                    renderer.text_bevel(&line, font, self.size, self.color, line_area, bevel)
                }
                None => renderer.text(&line, font, self.size, self.color, line_area),
            }

            top += spacing;
        }
    }
}
