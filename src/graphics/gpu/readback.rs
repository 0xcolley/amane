use std::sync::mpsc;

use vello::wgpu::{
    BufferDescriptor, BufferUsages, COPY_BYTES_PER_ROW_ALIGNMENT, Extent3d, MapMode, PollType,
    TexelCopyBufferInfo, TexelCopyBufferLayout,
};

use crate::graphics::renderer::Command;

use super::Gpu;

impl Gpu {
    /*
     * draws the commands offscreen and reads the pixels back, top row first and
     * four premultiplied bytes each; for pictures of a window, never for showing one
     */
    pub fn picture(&mut self, commands: Vec<Command>, width: u32, height: u32) -> Option<Vec<u8>> {
        self.painting = 0;

        self.quads.start_frame();

        let canvas = self.take_canvas(width, height);

        self.run(commands, &canvas);

        // the gpu copies whole rows of its own width, the padding is cut off below
        let row = width * 4;
        let stride = row.div_ceil(COPY_BYTES_PER_ROW_ALIGNMENT) * COPY_BYTES_PER_ROW_ALIGNMENT;

        let buffer = self.device.create_buffer(&BufferDescriptor {
            label: None,
            size: u64::from(stride * height),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self.take_encoder();

        encoder.copy_texture_to_buffer(
            canvas.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.keep_encoder(encoder);

        self.submit();

        let (send, receive) = mpsc::channel();

        buffer.slice(..).map_async(MapMode::Read, move |mapped| {
            let _ = send.send(mapped);
        });

        self.device.poll(PollType::wait_indefinitely()).ok()?;

        receive.recv().ok()?.ok()?;

        let mapped = buffer.slice(..).get_mapped_range().ok()?;

        let mut pixels = Vec::with_capacity((row * height) as usize);

        for line in mapped.chunks(stride as usize) {
            pixels.extend_from_slice(&line[..row as usize]);
        }

        drop(mapped);

        self.give_back(canvas);

        self.forget_unshown();

        self.forget_unpainted();

        Some(pixels)
    }
}
