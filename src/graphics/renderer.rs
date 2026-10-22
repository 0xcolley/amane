mod command;
mod effect;
mod shape;
mod text;

use super::{Area, Corners, Transform};

pub use command::Command;

// a rounded rectangle the compositor blurs behind, in buffer pixels through its transform
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Backdrop {
    pub area: Area,
    pub radius: Corners,
    pub transform: Transform,
}
pub use effect::VALUE_ROWS;

/*
 * collects what widgets draw as commands,
 * so no part of amane outside the gpu backend knows how drawing is done
 */
pub struct Renderer {
    commands: Vec<Command>,
    transform: Transform,

    // kept apart from the commands, since the gpu draws nothing for them
    backdrops: Vec<Backdrop>,
}

impl Renderer {
    pub fn new(scale: f32) -> Self {
        // widgets measure in logical pixels, the buffer in real ones
        let horizontal_scale = scale;
        let vertical_scale = scale;

        let transform = Transform::from_scale(horizontal_scale, vertical_scale);

        Self {
            commands: Vec::new(),
            transform,
            backdrops: Vec::new(),
        }
    }

    pub fn backdrop(&mut self, area: Area, radius: Corners) {
        if area.width <= 0.0 || area.height <= 0.0 {
            return;
        }

        self.backdrops.push(Backdrop {
            area,
            radius,
            transform: self.transform,
        });
    }

    // the frame's backdrops, taken before its commands go to the gpu
    pub fn take_backdrops(&mut self) -> Vec<Backdrop> {
        std::mem::take(&mut self.backdrops)
    }

    pub fn finish(self) -> Vec<Command> {
        self.commands
    }

    pub fn group(&self) -> Self {
        Self {
            commands: Vec::new(),
            transform: self.transform,
            backdrops: Vec::new(),
        }
    }

    pub fn blend(&mut self, mut group: Renderer, opacity: f32) {
        // rendering an invisible object is a waste of resource
        if opacity <= 0.0 {
            return;
        }

        self.backdrops.append(&mut group.backdrops);

        let cuts = group
            .commands
            .iter()
            .any(|command| matches!(command, Command::Cut { .. }));

        // a plain group draws the same inline, and only a separate group costs the gpu extra
        if opacity == 1.0 && !cuts {
            self.commands.extend(group.commands);

            return;
        }

        self.commands.push(Command::Group {
            commands: group.commands,
            opacity,
        });
    }

    // what the group drew only shows inside the rounded rectangle
    pub fn clip(&mut self, mut group: Renderer, area: Area, radius: Corners) {
        // an empty rectangle shows none of the group
        if area.width <= 0.0 || area.height <= 0.0 {
            return;
        }

        // not cut to the clip, glass is meant to sit whole inside what holds it
        self.backdrops.append(&mut group.backdrops);

        self.commands.push(Command::Clip {
            area,
            radius,
            transform: self.transform,
            commands: group.commands,
        });
    }

    // everything drawn inside goes through the local transform before the renderer's own
    pub fn transformed(&mut self, local: Transform, draw: impl FnOnce(&mut Renderer)) {
        let outer = self.transform;

        self.transform = local.post_concat(outer);

        draw(self);

        self.transform = outer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleFactor;
    use crate::{Color, Parent, Rectangle};

    #[test]
    fn frames_combine_global_and_output_scales_in_drawing() {
        let root = Rectangle::new()
            .width(Parent)
            .height(Parent)
            .fill(Color::BLUE);
        let frame = crate::frame::build(&root, 300, 150, 1.25, ScaleFactor::new(1.5));
        let commands = frame.renderer.finish();
        let Command::Rectangle {
            area, transform, ..
        } = commands[0]
        else {
            panic!("expected a rectangle");
        };

        assert_eq!(area, Area::new(0.0, 0.0, 200.0, 100.0));
        assert_eq!(transform.map(area.width, area.height), (375.0, 187.5));
    }
}
