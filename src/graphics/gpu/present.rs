use vello::wgpu::{CurrentSurfaceTexture, Surface, SurfaceConfiguration, SurfaceTexture};

use crate::graphics::renderer::Command;

use super::pass::Pass;
use super::{Gpu, texture};

// a window's swapchain, and the pass that copies the finished canvas into it
pub(super) struct Presenter {
    pub surface: Surface<'static>,
    pub config: SurfaceConfiguration,
    pub pass: Pass,
}

impl Gpu {
    // false when the frame was skipped, so nothing was committed; an offscreen gpu skips every frame
    pub fn draw(&mut self, commands: Vec<Command>, width: u32, height: u32) -> bool {
        let Some(presenter) = &mut self.presenter else {
            return false;
        };

        if presenter.config.width != width || presenter.config.height != height {
            presenter.config.width = width;
            presenter.config.height = height;

            presenter.surface.configure(&self.device, &presenter.config);

            // spares and kept results of the old size would never fit again
            self.spare.clear();
            self.painted.clear();
        }

        let Some(frame) = self.next_frame() else {
            return false;
        };

        self.painting = 0;

        self.quads.start_frame();

        let canvas = self.take_canvas(width, height);

        self.run(commands, &canvas);

        let target = frame.texture.create_view(&Default::default());

        let mut encoder = self.take_encoder();

        // there since the top, where a gpu without one already stopped
        if let Some(presenter) = &self.presenter {
            presenter.pass.run(
                &self.device,
                &mut encoder,
                &texture::view(&canvas),
                &target,
                [1.0, 0.0, 0.0, 0.0],
            );
        }

        self.keep_encoder(encoder);

        self.submit();

        self.give_back(canvas);

        self.forget_unshown();

        self.forget_unpainted();

        // presenting attaches the frame to the wayland surface and commits it
        self.queue.present(frame);

        true
    }

    fn next_frame(&mut self) -> Option<SurfaceTexture> {
        let presenter = self.presenter.as_ref()?;

        match presenter.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => Some(frame),

            CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),

            // the swapchain no longer fits the surface, a fresh one takes its place
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                presenter.surface.configure(&self.device, &presenter.config);

                match presenter.surface.get_current_texture() {
                    CurrentSurfaceTexture::Success(frame) => Some(frame),

                    _ => None,
                }
            }

            // a hidden or busy window skips this frame, the next redraw catches up
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => None,

            // already reported to the gpu error log, and the next redraw tries again
            CurrentSurfaceTexture::Validation => None,
        }
    }
}
