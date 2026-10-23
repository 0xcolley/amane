use crate::{Color, Shadow};

/*
 * raised, lit letters: the edges facing the light catch it, the edges
 * facing away fall into shade, a thin ring can trace the outside and a
 * shadow can fall behind. The letters' own color fills the middle
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bevel {
    // how far in from the edges the light and shade reach, in pixels before scaling
    pub(crate) depth: f32,

    // where the light comes from, in degrees: 0 from the right, 90 from above
    pub(crate) light: f32,

    pub(crate) highlight: Color,
    pub(crate) shade: Color,

    // a ring this many pixels wide just outside the letters
    pub(crate) outline: Option<(f32, Color)>,

    pub(crate) shadow: Option<Shadow>,

    // how much the letters' corners are rounded off, in pixels before scaling
    pub(crate) round: f32,
}

impl Bevel {
    pub fn new(depth: f32) -> Self {
        Self {
            depth,
            light: 100.0,
            highlight: Color::rgba(255, 255, 255, 230),
            shade: Color::rgba(0, 0, 0, 46),
            outline: None,
            shadow: None,
            round: 0.0,
        }
    }

    /*
     * like the large clock on macOS 26's lock screen: letters of frosted
     * glass with softened corners, lit around every edge, the top most and
     * the bottom nearly as much, with a faint dark line just outside
     */
    pub fn glass() -> Self {
        Self::new(1.5)
            .highlight(Color::rgba(255, 255, 255, 210))
            .shade(Color::rgba(255, 255, 255, 170))
            .outline(1.0, Color::rgba(30, 28, 40, 64))
            .round(1.5)
    }

    pub fn light(mut self, degrees: f32) -> Self {
        self.light = degrees;

        self
    }

    pub fn highlight(mut self, color: impl Into<Color>) -> Self {
        self.highlight = color.into();

        self
    }

    pub fn shade(mut self, color: impl Into<Color>) -> Self {
        self.shade = color.into();

        self
    }

    pub fn outline(mut self, width: f32, color: impl Into<Color>) -> Self {
        self.outline = Some((width, color.into()));

        self
    }

    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);

        self
    }

    // rounds off the letters' corners, about this many pixels
    pub fn round(mut self, radius: f32) -> Self {
        self.round = radius;

        self
    }

    // how far past the letters it can paint, so the picture leaves room for it
    pub(crate) fn reach(&self) -> f32 {
        let outline = self.outline.map_or(0.0, |(width, _)| width);

        let shadow = self.shadow.map_or(0.0, |shadow| {
            2.0 * shadow.blur + shadow.x.abs().max(shadow.y.abs())
        });

        outline.max(shadow) + 2.0
    }
}
